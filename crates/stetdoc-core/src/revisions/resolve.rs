//! `resolve`: accept and reject (design `revisions.md`, 6).
//!
//! A step names an action and a selector. The revisions it selects are found in every part that can hold
//! them. For each part the selected elements are resolved inside out, so a nested element goes before the
//! one that holds it, then the rows that go, then the paragraph marks that go (joining paragraphs, last to
//! first), then the cells. What a resolution emptied is pruned, and every existing block whose bytes
//! changed is touched. Resolution never writes a revision.

use super::index::ChangeIndex;
use super::resolve_table::{CellOp, RowOp};
use super::types::{Action, Step};
use crate::api::error::StetError;
use crate::state::Tx;
use crate::wordml::BlockKey;
use crate::wordml::{NodeId, RevKind, Revision};
use std::collections::HashSet;
use std::sync::Arc;

/// A block that left the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Removed {
    /// The block.
    pub key: BlockKey,
    /// True if it was an insertion that was rejected.
    pub rejected: bool,
}

/// One revision element that a step resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Resolved {
    /// What it recorded.
    pub kind: RevKind,
    /// Its `w:id`.
    pub id: Option<i64>,
    /// Its author.
    pub author: Option<String>,
    /// Its date as written.
    pub date: Option<String>,
}

/// What one step did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Resolution {
    /// Accepted or rejected.
    pub action: Action,
    /// The revision elements resolved, in document order, part by part.
    pub resolved: Vec<Resolved>,
    /// The ids of revisions that went with an element that was removed: a pending deletion of another
    /// author inside an insertion that was rejected.
    pub swallowed: Vec<i64>,
    /// The blocks that left the tree.
    pub removed: Vec<Removed>,
    /// Pairs of paragraphs that were joined: the first, which survives, and the second.
    pub joined: Vec<(BlockKey, BlockKey)>,
    /// Pairs of paragraphs where the second survives and took over what the first, which left the tree, still had: its
    /// attributes and its empty runs, as the rejection of a Y paragraph's new half gives them back (design 6.2).
    pub inherited: Vec<(BlockKey, BlockKey)>,
    /// The notes that were removed because the step took out their last reference (3.6.4).
    pub notes: Vec<super::notes::RemovedNote>,
    /// Things worth telling that are not errors.
    pub warnings: Vec<String>,
}

impl Resolution {
    /// The origins of the step, which `document` converts into `check`'s `Declared`: a paragraph that a join
    /// made the later one's content part of came from both, and so did one that took over the attributes of the first.
    pub(crate) fn origins(&self) -> Vec<super::outcome::Origin> {
        let joined = self
            .joined
            .iter()
            .map(|(first, second)| super::outcome::Origin {
                block: first.clone(),
                from: vec![first.clone(), second.clone()],
            });
        let inherited = self
            .inherited
            .iter()
            .map(|(first, second)| super::outcome::Origin {
                block: second.clone(),
                from: vec![second.clone(), first.clone()],
            });
        joined.chain(inherited).collect()
    }
}

/// The work on one part.
pub(super) struct Engine<'t, 's> {
    pub tx: &'t mut Tx<'s>,
    pub part: Arc<str>,
    pub accept: bool,
    pub swallowed: Vec<i64>,
    pub removed: Vec<Removed>,
    pub joined: Vec<(BlockKey, BlockKey)>,
    pub inherited: Vec<(BlockKey, BlockKey)>,
    pub warnings: Vec<String>,
    /// Nodes whose container changed: pruned afterwards, and their blocks touched.
    pub dirty: Vec<NodeId>,
    /// Paragraphs whose mark goes, last to first.
    pub joins: Vec<NodeId>,
    pub rows: Vec<RowOp>,
    pub cells: Vec<CellOp>,
    /// The paragraphs that came right after a table this step removed. A join does not reach across the
    /// table that was there, as the text model does not: it reads the paragraph before it and the one after
    /// as two.
    pub gap_nexts: HashSet<NodeId>,
}

impl Engine<'_, '_> {
    /// Resolves one element.
    fn apply(&mut self, rev: &Revision) -> Result<(), StetError> {
        {
            let tree = self.tx.part_mut(&self.part)?;
            if !super::cleanup::is_attached(tree, rev.node) {
                return Ok(());
            }
        }
        match rev.kind {
            RevKind::Ins | RevKind::Del | RevKind::MoveFrom | RevKind::MoveTo => {
                self.resolve_wrapper(rev.node, rev.kind)
            }
            RevKind::MarkIns | RevKind::MarkMoveTo => self.resolve_mark(rev.node, true),
            RevKind::MarkDel | RevKind::MarkMoveFrom => self.resolve_mark(rev.node, false),
            RevKind::RunProps
            | RevKind::ParagraphProps
            | RevKind::SectionProps
            | RevKind::TableProps
            | RevKind::TableGrid
            | RevKind::RowProps
            | RevKind::CellProps
            | RevKind::TableExProps
            | RevKind::NumberingChange => self.resolve_props(rev.node, rev.kind),
            RevKind::NumPrIns => self.resolve_numpr_ins(rev.node),
            RevKind::RowIns | RevKind::RowDel => {
                self.resolve_row(rev.node, rev.kind);
                Ok(())
            }
            RevKind::CellIns | RevKind::CellDel | RevKind::CellMerge => {
                self.resolve_cell(rev.node, rev.kind);
                Ok(())
            }
            RevKind::CustomXmlInsStart
            | RevKind::CustomXmlInsEnd
            | RevKind::CustomXmlDelStart
            | RevKind::CustomXmlDelEnd
            | RevKind::CustomXmlMoveFromStart
            | RevKind::CustomXmlMoveFromEnd
            | RevKind::CustomXmlMoveToStart
            | RevKind::CustomXmlMoveToEnd => self.resolve_control(rev.node, rev.kind),
        }
    }

    /// Resolves the elements of one part, in document order, and cleans up.
    fn run(&mut self, picked: &[Revision]) -> Result<(), StetError> {
        let markers = self.move_markers(picked)?;
        for rev in picked.iter().rev() {
            self.apply(rev)?;
        }
        self.drop_markers(markers)?;
        self.run_rows()?;
        self.run_joins()?;
        self.run_cells()?;
        self.fill_empty_cells()?;
        self.fill_empty_stories()?;
        self.touch_dirty()
    }

    /// Touches the existing blocks that the resolution changed.
    fn touch_dirty(&mut self) -> Result<(), StetError> {
        let part = self.part.clone();
        let mut seen: HashSet<NodeId> = HashSet::new();
        for n in std::mem::take(&mut self.dirty) {
            let tree = self.tx.part_mut(&part)?;
            if !super::cleanup::is_attached(tree, n) {
                continue;
            }
            let Some(owner) = super::cleanup::owner_block(tree, n) else {
                continue;
            };
            if !seen.insert(owner) {
                continue;
            }
            let key = BlockKey::new(part.clone(), owner);
            if self.tx.parts().ref_of(&key).is_some() {
                self.tx.touch(&key);
            }
        }
        Ok(())
    }
}

/// Resolves a list of steps in order. Each step sees what the one before it did.
pub(crate) fn resolve(
    tx: &mut Tx<'_>,
    index: &mut ChangeIndex,
    steps: &[Step],
) -> Result<Vec<Resolution>, StetError> {
    let mut out = Vec::with_capacity(steps.len());
    for step in steps {
        let picked = super::resolve_select::select(tx, index, &step.selector)?;
        let mut res = Resolution {
            action: step.action,
            resolved: Vec::new(),
            swallowed: Vec::new(),
            removed: Vec::new(),
            joined: Vec::new(),
            inherited: Vec::new(),
            notes: Vec::new(),
            warnings: Vec::new(),
        };
        // A note can be left with no reference only by a step that resolves an element that holds one, so
        // the stories are read for references, before and after, only then.
        let note_refs = picked.iter().any(|(part, revs)| {
            let view = tx.parts();
            view.id(part)
                .and_then(|id| view.tree(id).ok())
                .is_some_and(|tree| {
                    revs.iter()
                        .any(|r| super::notes::holds_note_reference(tree, r.node))
                })
        });
        let named_before = if note_refs {
            Some(super::notes::referenced_in_stories(tx)?)
        } else {
            None
        };
        for (part, revs) in picked {
            res.resolved.extend(revs.iter().map(|r| Resolved {
                kind: r.kind,
                id: r.id,
                author: r.author.clone(),
                date: r.date.clone(),
            }));
            let mut engine = Engine {
                tx,
                part,
                accept: step.action == Action::Accept,
                swallowed: Vec::new(),
                removed: Vec::new(),
                joined: Vec::new(),
                inherited: Vec::new(),
                warnings: Vec::new(),
                dirty: Vec::new(),
                joins: Vec::new(),
                rows: Vec::new(),
                cells: Vec::new(),
                gap_nexts: HashSet::new(),
            };
            engine.run(&revs)?;
            res.swallowed.extend(engine.swallowed);
            res.removed.extend(engine.removed);
            res.joined.extend(engine.joined);
            res.inherited.extend(engine.inherited);
            res.warnings.extend(engine.warnings);
        }
        if let Some(before) = &named_before {
            res.notes = super::notes::remove_notes_that_lost_their_references(
                tx,
                step.action == Action::Reject,
                before,
            )?;
        }
        res.swallowed.sort_unstable();
        res.swallowed.dedup();
        out.push(res);
    }
    Ok(out)
}
