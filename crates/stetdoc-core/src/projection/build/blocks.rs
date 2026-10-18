//! Blocks: paragraphs, tables, rows, cells, section lines and the blocks that cannot be shown.
//!
//! A paragraph's text is `inlines.rs`; this file reads everything else about a block: its style, its list
//! label, its mark, the changes to its properties, and why it may not be edited. A table is read through
//! `wordml`'s block views, so that its rows and cells are found the way the scan found them (design 3.3).

use super::cx::StoryCx;
use super::facts::style_name;
use super::inlines::Para;
use super::layout::{Collapse, Top};
use super::warnings;
use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::numbering::{ItemKind as ListKind, ListItem, Note};
use crate::projection::model::{
    Block, Cell, CellChange, CellRev, ChangeKind, Label, MarkKind, MarkRev, OpaqueBlock, ParaFlags,
    Paragraph, PropRev, ReadOnly, Row, RowChange, RowRev, SectionLine, Table,
};
use crate::styles::{HiddenReason as StyleHidden, MarkEffect};
use crate::wordml::{
    AnomalyKind, BlockKind, Item, NodeId, OpaqueKind, Paragraph as WPara, RevKind as WordRev,
    Revision, Row as WRow, SECTION_BREAK, Table as WTable, control, enclosing_controls, math_text,
    revision_at,
};
use std::ops::Range;

/// What a paragraph in a deleted row or cell inherits.
type Inherited = Option<ReadOnly>;

/// Anomalies of a paragraph's text that change what is shown.
const SHOWN_ANOMALIES: [AnomalyKind; 3] = [
    AnomalyKind::UnknownElement,
    AnomalyKind::NestedBlock,
    AnomalyKind::NoBranch,
];

impl<'a> StoryCx<'a> {
    /// The model blocks of one top: a paragraph and its section line, a table, or a collapsed field. A table
    /// can be cut to some of its rows, for a page that does not hold it whole.
    pub(super) fn top_blocks(
        &mut self,
        top: &Top,
        rows: Option<Range<usize>>,
    ) -> Result<Vec<Block>, StetError> {
        if let Some(collapse) = &top.collapse {
            return Ok(vec![self.collapsed(top, collapse)]);
        }
        let Some(entry) = self.entry(top.first) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        match entry.kind {
            BlockKind::Paragraph => {
                if let Some((p, after)) = self.paragraph(top.first, None, true)? {
                    out.push(Block::Paragraph(p));
                    out.extend(after);
                }
            }
            BlockKind::Table => {
                if let Some(t) = self.table(top.first, rows)? {
                    out.push(Block::Table(t));
                }
            }
            BlockKind::Row => {}
        }
        Ok(out)
    }

    /// The opaque blocks that stand after the entries `top` covers.
    pub(super) fn opaque_after(&mut self, top: &Top) -> Vec<Block> {
        let story = self.story.clone();
        let mut out = Vec::new();
        for o in story.opaque() {
            if o.after.is_some_and(|a| a >= top.first && a < top.end) {
                out.push(self.opaque_block(o.kind, o.node, o.after));
            }
        }
        out
    }

    /// The opaque blocks that stand before the first entry.
    pub(super) fn opaque_first(&mut self) -> Vec<Block> {
        let story = self.story.clone();
        let mut out = Vec::new();
        for o in story.opaque().iter().filter(|o| o.after.is_none()) {
            out.push(self.opaque_block(o.kind, o.node, None));
        }
        out
    }

    fn opaque_block(&mut self, kind: OpaqueKind, node: NodeId, after: Option<usize>) -> Block {
        let (what, block) = match kind {
            OpaqueKind::AltChunk => ("an imported document part", OpaqueBlock::AltChunk),
            OpaqueKind::Math => (
                "an equation",
                OpaqueBlock::Math(math_text(self.tree, node).into_boxed_str()),
            ),
            OpaqueKind::Unknown => ("an element the reader does not know", OpaqueBlock::Unknown),
        };
        let after_ref = after.and_then(|a| self.ref_at(a)).map(|r| r.to_string());
        let warning = warnings::opaque(self.story_label(), what, after_ref);
        self.warnings.push(warning);
        Block::Opaque(block)
    }

    fn collapsed(&self, top: &Top, c: &Collapse) -> Block {
        Block::Opaque(OpaqueBlock::Collapsed {
            keyword: c.keyword.clone(),
            first: self.ref_at(top.first),
            last: self.ref_at(c.last),
            blocks: c.blocks,
        })
    }

    /// The paragraph at entry `i` and the section line after it, if it ends a section. `None` for a
    /// paragraph that does not exist in this mode: a top-level one whose mark is gone and that has nothing
    /// else.
    pub(super) fn paragraph(
        &mut self,
        i: usize,
        inherited: Inherited,
        top_level: bool,
    ) -> Result<Option<(Paragraph, Vec<Block>)>, StetError> {
        let Some(entry) = self.entry(i) else {
            return Ok(None);
        };
        let story = self.story.clone();
        let Some(p) = story.paragraph(self.tree, i) else {
            return Ok(None);
        };
        let map = p.map();
        if top_level && p.joins_next(self.mode) && super::scan::shows_nothing(&map, self.mode) {
            return Ok(None);
        }
        let r#ref = self.ref_at(i);
        let block_text = r#ref.map(|r| r.to_string());
        let (style_id, mark_hidden, spec_vanish) = {
            let site = self.sites.site_for(entry.node, None)?;
            (
                site.style.map(str::to_string),
                matches!(self.cascade.hidden(&site), Some(StyleHidden::Vanish)),
                matches!(self.cascade.mark_effect(&site), MarkEffect::JoinsNext),
            )
        };
        let (label, notes) = match self.sites.numbers() {
            Some(numbers) => (
                numbers.item(entry.node).and_then(label_of),
                numbers.notes_at(entry.node).cloned().collect::<Vec<Note>>(),
            ),
            None => (None, Vec::new()),
        };
        let para = Para {
            node: entry.node,
            block: block_text.as_deref(),
            p,
        };
        let built = self.inlines(&para, &map)?;
        let mark = self.mark_of(&p);
        let revs = self.prop_revs(&p, &built.runs);
        let name = style_name(&self.defs, &self.cascade, style_id.as_deref());
        let style = (self.facts.common_style.as_deref() != Some(&*name)).then_some(name);
        let readonly = self.readonly_of(&p, &map, inherited);
        if readonly.is_some() {
            self.readonly += 1;
        }
        let label_story = self.story_label();
        for note in &notes {
            let w = warnings::numbering(label_story, block_text.clone(), note);
            self.warnings.push(w);
        }
        for kind in SHOWN_ANOMALIES {
            let count = map.anomalies().iter().filter(|a| a.kind == kind).count();
            if count > 0
                && let Some(w) =
                    warnings::read_anomaly(label_story, block_text.clone(), kind, count)
            {
                self.warnings.push(w);
            }
        }
        let hidden = mark_hidden && (built.text_runs == 0 || built.hidden_runs == built.text_runs);
        let after = if entry.has(SECTION_BREAK) {
            self.section_line(i, mark.clone()).into_iter().collect()
        } else {
            Vec::new()
        };
        let paragraph = Paragraph {
            r#ref,
            style,
            label,
            mark,
            revs,
            flags: ParaFlags {
                hidden,
                joins_next: spec_vanish || p.joins_next(self.mode),
                readonly,
            },
            inlines: built.inlines,
            final_text: None,
        };
        Ok(Some((paragraph, after)))
    }

    /// The revision on the paragraph mark, as the model shows it. Only `Redline` shows revisions.
    fn mark_of(&self, p: &WPara<'_>) -> Option<MarkRev> {
        if self.mode != Mode::Redline {
            return None;
        }
        // A mark that one author inserted and another deleted holds both, and the deletion is what an edit
        // must respect, so it is the one shown.
        let mut found: Vec<(u8, MarkRev)> = p
            .mark_props()?
            .iter()
            .filter_map(|c| {
                let rev = revision_at(&c)?;
                let (rank, kind) = match rev.kind {
                    WordRev::MarkDel => (0, MarkKind::Deleted),
                    WordRev::MarkMoveFrom => (1, MarkKind::MovedFrom),
                    WordRev::MarkIns => (2, MarkKind::Inserted),
                    WordRev::MarkMoveTo => (3, MarkKind::MovedTo),
                    _ => return None,
                };
                Some((
                    rank,
                    MarkRev {
                        kind,
                        tag: self.tag_of(&rev),
                    },
                ))
            })
            .collect();
        found.sort_by_key(|(rank, _)| *rank);
        found.into_iter().next().map(|(_, mark)| mark)
    }

    /// The changes to the properties of a paragraph, its mark and its runs: at most one of each kind by each
    /// author, which is what a reader needs to know and the ledger lists the rest.
    fn prop_revs(&self, p: &WPara<'_>, runs: &[Item]) -> Vec<PropRev> {
        let mut out: Vec<PropRev> = Vec::new();
        if self.mode != Mode::Redline {
            return out;
        }
        let mut add = |kind: ChangeKind, rev: Option<Revision>| {
            let Some(rev) = rev else { return };
            let tag = self.tag_of(&rev);
            if !out
                .iter()
                .any(|r| r.kind == kind && r.tag.author == tag.author)
            {
                out.push(PropRev { kind, tag });
            }
        };
        if let Some(ppr) = p.ppr() {
            add(
                ChangeKind::Paragraph,
                ppr.get("pPrChange").and_then(|n| revision_at(&n)),
            );
            if let Some(num) = ppr.get("numPr") {
                for c in num.child_elements() {
                    if let Some(rev) = revision_at(&c)
                        && rev.kind == WordRev::NumPrIns
                    {
                        add(ChangeKind::Numbering, Some(rev));
                    }
                }
            }
        }
        if let Some(mark) = p.mark_props() {
            add(
                ChangeKind::Run,
                mark.get("rPrChange").and_then(|n| revision_at(&n)),
            );
        }
        for item in runs {
            if let Some(props) = p.run_props(item) {
                add(
                    ChangeKind::Run,
                    props.get("rPrChange").and_then(|n| revision_at(&n)),
                );
            }
        }
        out
    }

    /// Why the whole paragraph is read-only, if it is.
    fn readonly_of(
        &self,
        p: &WPara<'_>,
        map: &crate::wordml::TextMap,
        inherited: Inherited,
    ) -> Option<ReadOnly> {
        if inherited.is_some() {
            return inherited;
        }
        let mut locked = false;
        let mut bound = false;
        let mut placeholder = false;
        for id in enclosing_controls(self.tree.node_ref(p.id())) {
            let c = control(self.tree.node_ref(id));
            locked |= c.lock.locks_content();
            bound |= c.data_bound;
            placeholder |= c.showing_placeholder;
        }
        if locked {
            return Some(ReadOnly::LockedControl);
        }
        if bound {
            return Some(ReadOnly::DataBound);
        }
        if placeholder {
            return Some(ReadOnly::Placeholder);
        }
        let revs = p.mark_revs();
        let gone = revs.deleted || revs.moved_from;
        (gone && map.len(Mode::Final) == 0).then_some(ReadOnly::DeletedBlock)
    }

    /// The line that ends a section, after the paragraph at entry `i` that holds its `w:sectPr`.
    fn section_line(&mut self, i: usize, mark: Option<MarkRev>) -> Option<Block> {
        let sections = self.sections();
        let at = sections.iter().position(|s| s.ends_at == Some(i))?;
        let this = sections.get(at)?;
        if !this.exists_in(self.mode) {
            return None;
        }
        let next = sections.get(at + 1);
        let revs = if self.mode == Mode::Redline {
            this.props(self.tree)
                .get("sectPrChange")
                .and_then(|n| revision_at(&n))
                .map(|rev| PropRev {
                    kind: ChangeKind::Section,
                    tag: self.tag_of(&rev),
                })
                .into_iter()
                .collect()
        } else {
            Vec::new()
        };
        Some(Block::Section(SectionLine {
            index: u32::try_from(at + 1).unwrap_or(u32::MAX),
            break_type: next.map_or(this.break_type, |n| n.break_type),
            title_pg: next.is_some_and(|n| n.title_pg),
            mark,
            revs,
        }))
    }

    /// The table at entry `i`, with all its rows or only those in `rows`. `None` when no row of it exists in
    /// this mode.
    pub(super) fn table(
        &mut self,
        i: usize,
        rows: Option<Range<usize>>,
    ) -> Result<Option<Table>, StetError> {
        let Some(entry) = self.entry(i) else {
            return Ok(None);
        };
        let story = self.story.clone();
        let Some(wt) = WTable::new(self.tree, entry.node) else {
            return Ok(None);
        };
        let entries = story.blocks();
        let mut row_entries: Vec<usize> = Vec::new();
        let mut j = i + 1;
        while let Some(e) = entries.get(j) {
            match e.parent {
                Some(p) if p as usize >= i => {
                    if p as usize == i && e.kind == BlockKind::Row {
                        row_entries.push(j);
                    }
                }
                _ => break,
            }
            j += 1;
        }
        let had_rows = !row_entries.is_empty();
        let wanted = match rows {
            Some(r) => row_entries
                .get(r.start.min(row_entries.len())..r.end.min(row_entries.len()))
                .unwrap_or_default()
                .to_vec(),
            None => row_entries,
        };
        let mut built = Vec::new();
        let mut widest = 0u32;
        for r in wanted {
            if let Some((row, width)) = self.row(r)? {
                widest = widest.max(width);
                built.push(row);
            }
        }
        if built.is_empty() && had_rows {
            return Ok(None);
        }
        let grid = u32::try_from(wt.grid().count()).unwrap_or(u32::MAX);
        let style = wt
            .props()
            .and_then(|p| p.val("tblStyle"))
            .map(|id| style_name(&self.defs, &self.cascade, Some(&id)));
        let mut revs = Vec::new();
        if self.mode == Mode::Redline {
            if let Some(props) = wt.props()
                && let Some(rev) = props.get("tblPrChange").and_then(|n| revision_at(&n))
            {
                revs.push(PropRev {
                    kind: ChangeKind::Table,
                    tag: self.tag_of(&rev),
                });
            }
            if let Some(rev) = wt
                .grid_element()
                .and_then(|g| g.child_elements().find_map(|c| revision_at(&c)))
                .filter(|r| r.kind == WordRev::TableGrid)
            {
                revs.push(PropRev {
                    kind: ChangeKind::Grid,
                    tag: self.tag_of(&rev),
                });
            }
        }
        Ok(Some(Table {
            r#ref: self.ref_at(i),
            style,
            cols: grid.max(widest),
            revs,
            rows: built,
        }))
    }

    /// The row at entry `r`, and how many grid columns its cells reach. `None` for a row that does not exist
    /// in this mode.
    fn row(&mut self, r: usize) -> Result<Option<(Row, u32)>, StetError> {
        let Some(entry) = self.entry(r) else {
            return Ok(None);
        };
        let Some(wr) = WRow::new(self.tree, entry.node) else {
            return Ok(None);
        };
        if wr.revs().gone_in(self.mode) {
            return Ok(None);
        }
        // A row inserted by one author and deleted by another shows as deleted: the deletion is the later
        // change, and rejecting it leaves the insertion.
        let rev = wr
            .revisions()
            .find(|x| x.kind == WordRev::RowDel)
            .or_else(|| wr.revisions().next());
        let change = if self.mode == Mode::Redline {
            rev.as_ref().map(|x| RowRev {
                kind: if x.kind == WordRev::RowDel {
                    RowChange::Deleted
                } else {
                    RowChange::Inserted
                },
                tag: self.tag_of(x),
            })
        } else {
            None
        };
        let row_deleted = change
            .as_ref()
            .is_some_and(|c| c.kind == RowChange::Deleted);
        let props = wr.props();
        let header = props.and_then(|p| p.on_off("tblHeader")).unwrap_or(false);
        let before = props
            .and_then(|p| p.val("gridBefore"))
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(0);
        let mut revs = Vec::new();
        if self.mode == Mode::Redline
            && let Some(rev) = props
                .and_then(|p| p.get("trPrChange"))
                .and_then(|n| revision_at(&n))
        {
            revs.push(PropRev {
                kind: ChangeKind::Row,
                tag: self.tag_of(&rev),
            });
        }
        let story = self.story.clone();
        let mut cells = Vec::new();
        let mut col = before;
        for wc in wr.cells() {
            let span = wc.grid_span();
            let cell_change = if self.mode == Mode::Redline {
                wc.revisions().next().map(|x| CellRev {
                    kind: match x.kind {
                        WordRev::CellDel => CellChange::Deleted,
                        WordRev::CellMerge => CellChange::Merged,
                        _ => CellChange::Inserted,
                    },
                    tag: self.tag_of(&x),
                })
            } else {
                None
            };
            let inherited = (row_deleted
                || cell_change
                    .as_ref()
                    .is_some_and(|c| c.kind == CellChange::Deleted))
            .then_some(ReadOnly::DeletedBlock);
            let mut blocks = Vec::new();
            for b in wc.blocks() {
                let Some(j) = story.index_of(b.id()) else {
                    continue;
                };
                match story.blocks().get(j).map(|e| e.kind) {
                    Some(BlockKind::Paragraph) => {
                        if let Some((mut p, _)) = self.paragraph(j, inherited, false)? {
                            // A cell that continues a vertical merge is drawn as part of the cell that starts it, with
                            // that cell's content (ECMA-376 17.4.85 and ST_Merge): Word does not show its own text.
                            if wc.v_merge() == crate::wordml::VMerge::Continue {
                                p.flags.hidden = true;
                            }
                            blocks.push(Block::Paragraph(p));
                        }
                    }
                    Some(BlockKind::Table) => {
                        if let Some(t) = self.table(j, None)? {
                            blocks.push(Block::Table(t));
                        }
                    }
                    _ => {}
                }
            }
            cells.push(Cell {
                col,
                span,
                v_merge: wc.v_merge(),
                change: cell_change,
                blocks,
            });
            col = col.saturating_add(span);
        }
        Ok(Some((
            Row {
                r#ref: self.ref_at(r),
                change,
                header,
                revs,
                cells,
            },
            col,
        )))
    }
}

/// The label a list item shows, if it shows one.
fn label_of(item: &ListItem) -> Option<Label> {
    match &item.kind {
        ListKind::Counted { text } => Some(Label {
            text: text.as_str().into(),
            bullet: false,
            ghost: item.ghost,
            hidden: item.label_hidden,
        }),
        ListKind::Bullet { glyph } => Some(Label {
            text: glyph.as_str().into(),
            bullet: true,
            ghost: item.ghost,
            hidden: item.label_hidden,
        }),
        ListKind::Plain => None,
    }
}
