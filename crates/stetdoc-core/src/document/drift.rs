//! The cross-references an edit moved: a typed mention such as `Clause 16.2(b)` reads the same after an edit that renumbered
//! the clauses, and names another block, or none. The receipt warns about each one (`ref_retargeted`, `ref_unresolved`), and
//! about a definition the edit removed from a term the document still uses (`definition_removed`).
//!
//! The check compares the prose edges of the state the edit started from with those of the state it commits, both over
//! `Final` text and numbers, by the ref of the citing block and the mention as written. A target is the ref of a block,
//! never its label, so a split, a merge or a rebind that the batch declared keeps its identity, and a block the batch adds is
//! "a block this batch adds" in a dry run and in the real edit alike: no message names a ref the batch mints (law H14). A dry
//! run reads the state its commit made and then drops, so the two read the same state.
//!
//! A mention whose citing block or text the batch changed is skipped: the agent edited the reference. A change that puts a
//! reference back where it pointed before any pending change (a rejected deletion, a deletion of one's own insertion) is not
//! drift, which the `Original` view of the starting state tells; a state with no tracked change needs no such view.
//!
//! **Memory.** The index and the edges of a whole document are built three times at most (before, after, and the `Original`
//! view before), and each is built in a transaction of its own that nothing commits, then dropped, so none stays in a store
//! and no two are held at once. What is kept between them is small: `Before` holds the resolved mentions and the definitions,
//! and the after pass keeps only its warnings.

use crate::api::error::StetError;
use crate::api::receipt::{RefDrift, RefTarget};
use crate::api::view::{Mode, Warning, WarningCode};
use crate::index::{self, Edge, EdgeKind, Edges, Index, State as Ended};
use crate::ops::Applied;
use crate::state::{PartsView, State};
use crate::wordml::{BlockKey, StoryKind, revision_at};
use std::collections::{BTreeMap, HashMap, HashSet};

/// How a block the batch adds is written in a message.
const NEW_BLOCK: &str = "a block this batch adds";

/// The most refs a message lists before it says how many more there are.
const MOST_LISTED: usize = 5;

/// What the check found: the warnings in order, and one `RefDrift` for each warning about a reference, in the same order.
#[derive(Debug, Default)]
pub(super) struct Drift {
    pub warnings: Vec<Warning>,
    pub refs: Vec<RefDrift>,
}

/// A mention: the ref of the citing block and the text as written.
type Mention = (String, String);

/// The block a mention resolved to before the edit: its key, its ref and its label.
struct Was {
    key: BlockKey,
    r#ref: String,
    label: String,
}

/// A definition before the edit.
struct Def {
    block: BlockKey,
    pointer: bool,
    /// The prose mentions that cited its block: the citing block and the text.
    cited_by: Vec<(BlockKey, String)>,
}

/// What the check reads of the state an edit starts from, read before the edit opens its transaction.
#[derive(Default)]
pub(super) struct Before {
    /// The resolved prose mentions, by mention, in document order; none for a mention that named two blocks.
    mentions: Vec<(Mention, Option<Was>)>,
    /// The definitions, by term, in the order of the terms.
    defs: BTreeMap<String, Vec<Def>>,
}

impl Before {
    /// Reads the resolved mentions and the definitions of a state, in a transaction that is dropped after, so that the index
    /// and the edges it builds stay in no store.
    pub(super) fn read(state: &State) -> Result<Before, StetError> {
        let scratch = state.begin();
        let parts = scratch.parts();
        let edges = parts.edges()?;
        let index = parts.doc_index()?;
        let mut out = Before::default();
        let mut at: HashMap<Mention, usize> = HashMap::new();
        for e in edges
            .all()
            .iter()
            .filter(|e| e.kind == EdgeKind::Prose && e.state == Ended::Resolved)
        {
            let (Some(source), Some(to)) = (parts.ref_of(&e.from), e.to.first()) else {
                continue;
            };
            let Some(to_ref) = parts.ref_of(to).map(|r| r.to_string()) else {
                continue;
            };
            let mention = (source.to_string(), e.text.clone());
            match at.get(&mention) {
                Some(i) => {
                    if let Some(slot) = out.mentions.get_mut(*i)
                        && slot.1.as_ref().is_some_and(|w| w.r#ref != to_ref)
                    {
                        slot.1 = None;
                    }
                }
                None => {
                    at.insert(mention.clone(), out.mentions.len());
                    out.mentions.push((
                        mention,
                        Some(Was {
                            key: to.clone(),
                            r#ref: to_ref,
                            label: label(&index, to),
                        }),
                    ));
                }
            }
        }
        for term in index.terms() {
            let defs = index
                .definitions(term)
                .iter()
                .map(|d| Def {
                    block: d.block.clone(),
                    pointer: d.pointer,
                    cited_by: edges
                        .inbound(&d.block)
                        .into_iter()
                        .filter(|e| e.kind == EdgeKind::Prose)
                        .map(|e| (e.from.clone(), e.text.clone()))
                        .collect(),
                })
                .collect();
            out.defs.insert(term.to_string(), defs);
        }
        Ok(out)
    }
}

/// What a mention came to after the edit, when that is drift.
enum Now {
    /// One block, which is not the one it named before: its ref, none for a block the batch adds.
    To(Option<String>),
    /// No block.
    Nothing,
    /// Several blocks.
    Several,
}

/// A mention that names another block after the edit, or none, with what the receipt says of it.
struct Moved<'b> {
    mention: &'b Mention,
    now: Now,
    warning: Warning,
    drift: RefDrift,
}

/// The warnings about the cross-references and the definitions that the edit from `base`, read into `before`, to `next` changed.
/// `next` is the committed state for a real edit and the state a dry run committed and drops. Nothing when no operation changed
/// the document.
pub(super) fn find(
    before: &Before,
    base: &State,
    next: &State,
    applied: &Applied,
) -> Result<Drift, StetError> {
    let mut out = Drift::default();
    if !applied.ops.iter().any(|o| o.changed)
        || (before.mentions.is_empty() && before.defs.is_empty())
    {
        return Ok(out);
    }
    let made: HashSet<BlockKey> = applied.new_blocks().into_iter().collect();
    let (mut moved, removed) = {
        let scratch = next.begin();
        let ctx = Ctx {
            after: scratch.parts(),
            made: &made,
        };
        let edges = ctx.after.edges()?;
        let index = ctx.after.doc_index()?;
        let moved = ctx.moved(before, &edges, &index)?;
        let removed = ctx.definitions_removed(base, before, &edges, &index)?;
        (moved, removed)
    };
    if !moved.is_empty() && has_tracked_changes(base)? {
        let scratch = base.begin();
        let parts = scratch.parts();
        let original = parts.edges_in(Mode::Original)?;
        let back = first_by_mention(parts, &original);
        moved.retain(|m| !puts_back(m, parts, back.get(m.mention).copied()));
    }
    for m in moved {
        out.warnings.push(m.warning);
        out.refs.push(m.drift);
    }
    out.warnings.extend(removed);
    Ok(out)
}

/// True if a story of the state has a tracked change: without one, the `Original` view is the `Final` view.
fn has_tracked_changes(state: &State) -> Result<bool, StetError> {
    let parts = state.parts();
    let list = parts.story_specs()?;
    for spec in list.0.iter().filter(|s| s.kind != StoryKind::Comments) {
        let Some(part) = parts.id(&spec.part) else {
            continue;
        };
        let tree = parts.tree(part)?;
        let mut stack = vec![tree.root().children()];
        while let Some(top) = stack.last_mut() {
            let Some(n) = top.next() else {
                stack.pop();
                continue;
            };
            if !n.is_element() {
                continue;
            }
            if revision_at(&n).is_some() {
                return Ok(true);
            }
            stack.push(n.children());
        }
    }
    Ok(false)
}

/// True if the mention now names what it named in the `Original` view of the starting state: the edit undid a pending change,
/// and the reference is back where it was.
fn puts_back(m: &Moved<'_>, before: PartsView<'_>, original: Option<&Edge>) -> bool {
    let Some(edge) = original else {
        return false;
    };
    match (&m.now, edge.state) {
        (Now::To(Some(now)), Ended::Resolved | Ended::Relative) => edge
            .to
            .first()
            .and_then(|k| before.ref_of(k))
            .is_some_and(|was| was.to_string() == *now),
        (Now::Nothing, Ended::Unresolved) => true,
        _ => false,
    }
}

/// What the check reads of the state after the edit.
struct Ctx<'a> {
    after: PartsView<'a>,
    made: &'a HashSet<BlockKey>,
}

impl Ctx<'_> {
    /// The ref of a block after the edit, or none for a block the batch adds, whatever ref the commit gave it.
    fn ref_after(&self, key: &BlockKey) -> Option<String> {
        if self.made.contains(key) {
            return None;
        }
        self.after.ref_of(key).map(|r| r.to_string())
    }

    /// The mentions that name another block after the edit, or none, in the order of the starting document, each with its
    /// warning. A mention with no edge after the edit is one the agent changed, or whose block went, and is not drift.
    fn moved<'b>(
        &self,
        before: &'b Before,
        edges: &Edges,
        index: &Index,
    ) -> Result<Vec<Moved<'b>>, StetError> {
        let now = first_by_mention(self.after, edges);
        let mut out = Vec::new();
        for (mention, was) in &before.mentions {
            let (Some(was), Some(edge)) = (was, now.get(mention)) else {
                continue;
            };
            let (code, message, next, now, after) = match edge.state {
                Ended::Resolved | Ended::Relative => {
                    let Some(to) = edge.to.first() else {
                        continue;
                    };
                    let target = RefTarget {
                        r#ref: self.ref_after(to),
                        label: label(index, to),
                    };
                    if target.r#ref.as_ref() == Some(&was.r#ref) {
                        continue;
                    }
                    let at = target
                        .r#ref
                        .clone()
                        .unwrap_or_else(|| NEW_BLOCK.to_string());
                    (
                        WarningCode::RefRetargeted,
                        format!("now {at} ({}).", target.label),
                        "update the reference in this batch, or confirm it is meant to move.",
                        Now::To(target.r#ref.clone()),
                        Some(target),
                    )
                }
                Ended::Unresolved | Ended::Ambiguous => {
                    let deleted = if self.gone(&was.key)? {
                        ", which this edit deletes,"
                    } else {
                        ""
                    };
                    let (said, now) = if edge.state == Ended::Ambiguous {
                        let names = self.names(&edge.to);
                        (
                            format!(
                                "now matches {} blocks ({})",
                                edge.to.len(),
                                listed(&names, false)
                            ),
                            Now::Several,
                        )
                    } else {
                        ("now resolves to nothing".to_string(), Now::Nothing)
                    };
                    (
                        WarningCode::RefUnresolved,
                        format!("{deleted} and {said}."),
                        "update the reference in this batch, or remove it.",
                        now,
                        None,
                    )
                }
                Ended::Outside => continue,
            };
            let (source, text) = mention;
            let was_said = format!("\"{text}\" pointed at {} ({})", was.r#ref, was.label);
            let message = if code == WarningCode::RefRetargeted {
                format!("{was_said}, {message}")
            } else {
                format!("{was_said}{message}")
            };
            out.push(Moved {
                mention,
                now,
                warning: Warning {
                    code,
                    block: Some(source.clone()),
                    story: None,
                    message,
                    next: Some(next.to_string()),
                },
                drift: RefDrift {
                    source: source.clone(),
                    text: text.clone(),
                    before: RefTarget {
                        r#ref: Some(was.r#ref.clone()),
                        label: was.label.clone(),
                    },
                    after,
                },
            });
        }
        Ok(out)
    }

    /// The refs of blocks as a message lists them, a block the batch adds written once as one.
    fn names(&self, keys: &[BlockKey]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for key in keys {
            let name = self.ref_after(key).unwrap_or_else(|| NEW_BLOCK.to_string());
            if !out.contains(&name) {
                out.push(name);
            }
        }
        out
    }

    /// True if a block has no `Final` text after the edit, or is no longer in the document.
    fn gone(&self, key: &BlockKey) -> Result<bool, StetError> {
        let Some(part) = self.after.id(key.part()) else {
            return Ok(true);
        };
        let story = self.after.story(part)?;
        let Some(entry) = story.index_of(key.node()) else {
            return Ok(true);
        };
        let facts = self.after.story_facts(part)?;
        Ok(facts.lens.get(entry).is_none_or(|l| l[0] == 0))
    }

    /// A warning for each term that the edit left with no definition while the document still uses it, in the order of the
    /// terms. A definition paragraph that points elsewhere (`has the meaning given in Clause 16.2(b)`) does not stand in for
    /// the definition it points at.
    fn definitions_removed(
        &self,
        base: &State,
        before: &Before,
        edges: &Edges,
        index: &Index,
    ) -> Result<Vec<Warning>, StetError> {
        let mut lost: Vec<(&str, &Def)> = Vec::new();
        for (term, was) in &before.defs {
            let now = index.definitions(term);
            let first_real = was.iter().find(|d| !d.pointer);
            let gone = if now.is_empty() {
                first_real.or(was.first())
            } else if now.iter().all(|d| d.pointer) {
                first_real
            } else {
                None
            };
            if let Some(def) = gone {
                lost.push((term, def));
            }
        }
        if lost.is_empty() {
            return Ok(Vec::new());
        }
        let terms: Vec<&str> = lost.iter().map(|(t, _)| *t).collect();
        let used = index::blocks_using(&self.after, &terms)?;
        let mut out = Vec::new();
        for ((term, def), uses) in lost.into_iter().zip(used) {
            let (Some(defined), false) = (base.parts().ref_of(&def.block), uses.is_empty()) else {
                continue;
            };
            let users = self.names(&uses);
            let mut citing: Vec<String> = Vec::new();
            for (from, text) in &def.cited_by {
                let still = edges
                    .outbound(from)
                    .iter()
                    .any(|a| a.kind == EdgeKind::Prose && a.text == *text);
                if let Some(name) = still.then(|| self.ref_after(from)).flatten()
                    && !citing.contains(&name)
                {
                    citing.push(name);
                }
            }
            let cites = match citing.len() {
                0 => String::new(),
                1 => format!(", and {} cites this block", listed(&citing, true)),
                _ => format!(", and {} cite this block", listed(&citing, true)),
            };
            out.push(Warning {
                code: WarningCode::DefinitionRemoved,
                block: Some(defined.to_string()),
                story: None,
                message: format!(
                    "defined \"{term}\", and no block defines it now. It is still used in {}{cites}.",
                    listed(&users, true)
                ),
                next: Some(
                    "move the definition to a block that stays, or update the uses.".to_string(),
                ),
            });
        }
        Ok(out)
    }
}

/// The first prose edge of each mention, by the ref of the citing block in `parts`.
fn first_by_mention<'e>(parts: PartsView<'_>, edges: &'e Edges) -> HashMap<Mention, &'e Edge> {
    let mut out: HashMap<Mention, &Edge> = HashMap::new();
    for e in edges.all().iter().filter(|e| e.kind == EdgeKind::Prose) {
        if let Some(source) = parts.ref_of(&e.from) {
            out.entry((source.to_string(), e.text.clone())).or_insert(e);
        }
    }
    out
}

/// The clause a block is, as a view prints it after the arrow of a mention.
fn label(index: &Index, key: &BlockKey) -> String {
    index
        .clause_of(key)
        .map(index::Clause::heading)
        .unwrap_or_default()
}

/// Up to `MOST_LISTED` names, then how many more: `p1, p2 and p3` in a sentence, `p1, p2, p3` in a parenthesis.
fn listed(names: &[String], sentence: bool) -> String {
    let shown: Vec<&str> = names.iter().take(MOST_LISTED).map(String::as_str).collect();
    let more = names.len().saturating_sub(MOST_LISTED);
    if more > 0 {
        return format!("{} and {more} more", shown.join(", "));
    }
    if !sentence {
        return shown.join(", ");
    }
    match shown.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        Some((last, _)) => (*last).to_string(),
        None => String::new(),
    }
}
