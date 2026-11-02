//! The cross-references an edit moved: a typed mention such as `Clause 16.2(b)` reads the same after an edit that renumbered
//! the clauses, and names another block, or none. The receipt warns about each one (`ref_retargeted`, `ref_unresolved`), and
//! about a definition the edit removed from a term the document still uses (`definition_removed`).
//!
//! The check compares the prose edges of the state the edit started from with those of the state the receipt describes, both
//! over `Final` text and numbers, by the ref of the citing block and the mention as written. A target is the ref of a block,
//! never its label, so a split, a merge or a rebind that the batch declared keeps its identity, and a block the batch adds is
//! "a block this batch adds" in a dry run and in the real edit alike: no message names a ref the batch mints (law H14).
//!
//! A mention whose citing block or text the batch changed is skipped: the agent edited the reference. A change that puts a
//! reference back where it pointed before any pending change (a rejected deletion, a deletion of one's own insertion) is not
//! drift, which the `Original` view of the starting state tells.

use crate::api::error::StetError;
use crate::api::receipt::{RefDrift, RefTarget};
use crate::api::view::{Mode, Warning, WarningCode};
use crate::index::{self, Edge, EdgeKind, Edges, Index, State};
use crate::ops::Applied;
use crate::state::PartsView;
use crate::wordml::BlockKey;
use std::collections::{HashMap, HashSet};

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

/// What a mention came to after the edit, when that is drift.
enum Now {
    /// One block, which is not the one it named before.
    To(BlockKey),
    /// No block.
    Nothing,
    /// Several blocks.
    Several(Vec<BlockKey>),
}

/// A mention that names another block after the edit, or none.
struct Moved {
    mention: Mention,
    before: BlockKey,
    before_ref: String,
    now: Now,
}

/// The warnings about the cross-references and the definitions that the edit from `before` to `after` changed. Nothing when
/// no operation changed the document.
pub(super) fn find(
    before: PartsView<'_>,
    after: PartsView<'_>,
    applied: &Applied,
) -> Result<Drift, StetError> {
    let mut out = Drift::default();
    if !applied.ops.iter().any(|o| o.changed) {
        return Ok(out);
    }
    let made: HashSet<BlockKey> = applied.new_blocks().into_iter().collect();
    let before_edges = before.edges()?;
    let before_index = before.doc_index()?;
    let was = resolved_before(before, &before_edges);
    if was.is_empty() && before_index.terms().next().is_none() {
        return Ok(out);
    }
    let after_edges = after.edges()?;
    let after_index = after.doc_index()?;
    let ctx = Ctx { after, made: &made };
    let mut moved = moved(&ctx, &was, &after_edges);
    if !moved.is_empty() {
        let original = before.edges_in(Mode::Original)?;
        let back = first_by_mention(before, &original);
        moved.retain(|m| !ctx.puts_back(m, before, back.get(&m.mention).copied()));
    }
    for m in &moved {
        let (warning, drift) = ctx.say(m, &before_index, &after_index)?;
        out.warnings.push(warning);
        out.refs.push(drift);
    }
    out.warnings.extend(definitions_removed(
        &ctx,
        before,
        (&before_index, &before_edges),
        (&after_index, &after_edges),
    )?);
    Ok(out)
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

    /// True if the mention now names what it named in the `Original` view of the starting state: the edit undid a pending
    /// change, and the reference is back where it was.
    fn puts_back(&self, m: &Moved, before: PartsView<'_>, original: Option<&Edge>) -> bool {
        let Some(edge) = original else {
            return false;
        };
        match (&m.now, edge.state) {
            (Now::To(to), State::Resolved | State::Relative) => {
                let was = edge.to.first().and_then(|k| before.ref_of(k));
                was.is_some_and(|w| self.ref_after(to) == Some(w.to_string()))
            }
            (Now::Nothing, State::Unresolved) => true,
            _ => false,
        }
    }

    /// The warning and the drift entry of a moved mention.
    fn say(
        &self,
        m: &Moved,
        before_index: &Index,
        after_index: &Index,
    ) -> Result<(Warning, RefDrift), StetError> {
        let (source, text) = &m.mention;
        let before_label = label(before_index, &m.before);
        let was = format!("\"{text}\" pointed at {} ({before_label})", m.before_ref);
        let target = |key: &BlockKey| RefTarget {
            r#ref: self.ref_after(key),
            label: label(after_index, key),
        };
        let (code, message, next, after) = match &m.now {
            Now::To(to) => {
                let now = target(to);
                let at = now.r#ref.clone().unwrap_or_else(|| NEW_BLOCK.to_string());
                (
                    WarningCode::RefRetargeted,
                    format!("{was}, now {at} ({}).", now.label),
                    "update the reference in this batch, or confirm it is meant to move.",
                    Some(now),
                )
            }
            Now::Nothing | Now::Several(_) => {
                let deleted = if self.gone(&m.before)? {
                    ", which this edit deletes,"
                } else {
                    ""
                };
                let now = match &m.now {
                    Now::Several(all) => {
                        let names = self.names(all);
                        format!(
                            "now matches {} blocks ({})",
                            all.len(),
                            listed(&names, false)
                        )
                    }
                    _ => "now resolves to nothing".to_string(),
                };
                (
                    WarningCode::RefUnresolved,
                    format!("{was}{deleted} and {now}."),
                    "update the reference in this batch, or remove it.",
                    None,
                )
            }
        };
        let warning = Warning {
            code,
            block: Some(source.clone()),
            story: None,
            message,
            next: Some(next.to_string()),
        };
        let drift = RefDrift {
            source: source.clone(),
            text: text.clone(),
            before: RefTarget {
                r#ref: Some(m.before_ref.clone()),
                label: before_label,
            },
            after,
        };
        Ok((warning, drift))
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
}

/// The resolved prose mentions of the starting state, by mention, in document order, with the block each names and its ref.
/// A mention that names two blocks (the same words in one paragraph, read two ways) is left out.
fn resolved_before(
    before: PartsView<'_>,
    edges: &Edges,
) -> Vec<(Mention, Option<(BlockKey, String)>)> {
    let mut out: Vec<(Mention, Option<(BlockKey, String)>)> = Vec::new();
    let mut at: HashMap<Mention, usize> = HashMap::new();
    for e in edges
        .all()
        .iter()
        .filter(|e| e.kind == EdgeKind::Prose && e.state == State::Resolved)
    {
        let (Some(source), Some(to)) = (before.ref_of(&e.from), e.to.first()) else {
            continue;
        };
        let Some(to_ref) = before.ref_of(to).map(|r| r.to_string()) else {
            continue;
        };
        let mention = (source.to_string(), e.text.clone());
        match at.get(&mention) {
            Some(i) => {
                if let Some(slot) = out.get_mut(*i)
                    && slot.1.as_ref().is_some_and(|(_, r)| *r != to_ref)
                {
                    slot.1 = None;
                }
            }
            None => {
                at.insert(mention.clone(), out.len());
                out.push((mention, Some((to.clone(), to_ref))));
            }
        }
    }
    out
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

/// The mentions that name another block after the edit, or none, in the order of the starting document. A mention with no
/// edge after the edit is one the agent changed, or whose block went, and is not drift.
fn moved(
    ctx: &Ctx<'_>,
    was: &[(Mention, Option<(BlockKey, String)>)],
    after_edges: &Edges,
) -> Vec<Moved> {
    let now = first_by_mention(ctx.after, after_edges);
    let mut out = Vec::new();
    for (mention, target) in was {
        let (Some((before, before_ref)), Some(edge)) = (target, now.get(mention)) else {
            continue;
        };
        let now = match edge.state {
            State::Resolved | State::Relative => {
                let Some(to) = edge.to.first() else {
                    continue;
                };
                if ctx.ref_after(to).as_ref() == Some(before_ref) {
                    continue;
                }
                Now::To(to.clone())
            }
            State::Unresolved => Now::Nothing,
            State::Ambiguous => Now::Several(edge.to.clone()),
            State::Outside => continue,
        };
        out.push(Moved {
            mention: mention.clone(),
            before: before.clone(),
            before_ref: before_ref.clone(),
            now,
        });
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

/// A warning for each term that the edit left with no definition while the document still uses it, in the order of the
/// terms. A definition paragraph that points elsewhere (`has the meaning given in Clause 16.2(b)`) does not stand in for the
/// definition it points at.
fn definitions_removed(
    ctx: &Ctx<'_>,
    before: PartsView<'_>,
    (before_index, before_edges): (&Index, &Edges),
    (after_index, after_edges): (&Index, &Edges),
) -> Result<Vec<Warning>, StetError> {
    let mut lost: Vec<(&str, &BlockKey)> = Vec::new();
    for term in before_index.terms() {
        let was = before_index.definitions(term);
        let now = after_index.definitions(term);
        let real = |d: &&index::Definition| !d.pointer;
        let first_real = was.iter().find(real);
        let gone = if now.is_empty() {
            first_real.or(was.first())
        } else if !now.iter().any(|d| real(&d)) {
            first_real
        } else {
            None
        };
        if let Some(def) = gone {
            lost.push((term, &def.block));
        }
    }
    if lost.is_empty() {
        return Ok(Vec::new());
    }
    let terms: Vec<&str> = lost.iter().map(|(t, _)| *t).collect();
    let used = index::blocks_using(&ctx.after, &terms)?;
    let mut out = Vec::new();
    for ((term, block), uses) in lost.into_iter().zip(used) {
        let (Some(defined), false) = (before.ref_of(block), uses.is_empty()) else {
            continue;
        };
        let users = ctx.names(&uses);
        let mut citing: Vec<String> = Vec::new();
        for e in before_edges.inbound(block) {
            let still = e.kind == EdgeKind::Prose
                && after_edges
                    .outbound(&e.from)
                    .iter()
                    .any(|a| a.kind == EdgeKind::Prose && a.text == e.text);
            if let Some(name) = still.then(|| ctx.ref_after(&e.from)).flatten()
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
