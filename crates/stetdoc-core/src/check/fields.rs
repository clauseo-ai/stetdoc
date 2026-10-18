//! W8: the pieces of a field are all in a deletion or all outside one.
//!
//! Word reads a field whose `begin` is deleted and whose `end` is not as a field that was cut in
//! two, and a tracked deletion that does that to a field is a bug of the compiler. The pieces are
//! `wordml`'s (`FieldInfo`): the `begin`, the `separate` and the `end` of a complex field. A
//! `w:fldSimple` has one piece and is not read.
//!
//! A field belongs to the unit that holds its `begin`, so a field that was already split cannot
//! hide a new split of another field in another paragraph: the finding is of the paragraph, and the
//! paragraph is compared with its origin like any other unit.

use super::census::Census;
use super::document::SchemaPart;
use super::found::Found;
use super::origins::Origins;
use super::rules::is_word;
use super::verdict::Verdict;
use crate::api::check::FindingCode;
use crate::api::error::StetError;
use crate::state::{Dep, PartsView};
use crate::wordml::{Story, StoryKind};
use crate::xml::{NodeId, SpanTree};
use std::collections::HashSet;
use std::sync::Arc;

/// The variant of the cache that holds the paragraphs of the base that hold a cut field.
const SPLIT: u64 = 3;

/// The paragraphs that hold the begin of a cut field, as the base has them.
pub(super) struct Split(HashSet<NodeId>);

/// What kind of story a part is, or `None` when the part is not in the list of stories, as the
/// glossary is not.
fn story_kind(view: &PartsView<'_>, part: &SchemaPart) -> Result<Option<StoryKind>, StetError> {
    Ok(view
        .story_specs()?
        .0
        .iter()
        .find(|spec| *spec.part == *part.name)
        .map(|spec| spec.kind))
}

/// The paragraphs of the base that hold the begin of a cut field, from the cache while the part
/// has not changed.
fn base_split(view: &PartsView<'_>, part: &SchemaPart) -> Result<Arc<Split>, StetError> {
    let build = |_: Option<&Split>| -> Result<Split, StetError> {
        let story = view.story(part.id)?;
        let tree = view.tree(part.id)?;
        Ok(Split(split_paragraphs(&story, tree).into_iter().collect()))
    };
    view.derived(SPLIT, &[Dep::Part(part.id)], &build)
}

/// Whether the node is inside a `w:del` of its own paragraph.
fn in_deletion(tree: &SpanTree, node: NodeId) -> bool {
    let mut at = tree.node_ref(node).parent();
    while let Some(n) = at {
        if is_word(&n, "p") {
            return false;
        }
        if is_word(&n, "del") {
            return true;
        }
        at = n.parent();
    }
    false
}

/// The paragraph that holds the `begin` of each field of the story that a deletion cuts, once for
/// each such field, in document order. `story` must be current for `tree`.
pub(super) fn split_paragraphs(story: &Story, tree: &SpanTree) -> Vec<NodeId> {
    story
        .fields(tree)
        .iter()
        .filter(|field| !field.simple)
        .filter(|field| {
            let mut pieces = [Some(field.begin), field.separate, field.end]
                .into_iter()
                .flatten()
                .map(|piece| in_deletion(tree, piece));
            pieces
                .next()
                .is_some_and(|first| pieces.any(|inside| inside != first))
        })
        .map(|field| field.paragraph_begin)
        .collect()
}

/// The finding for a field that a deletion cuts, in the paragraph that holds its `begin`.
pub(super) fn finding(paragraph: NodeId) -> Found {
    Found::rule(FindingCode::FieldSplitByDeletion, "", Some(paragraph))
}

/// The cut fields of a changed story part. The part is scanned again for the fields, because the
/// story that the transaction would give is a refresh from the nodes the compiler declared it
/// touched, and a declaration is not what `check` trusts. A field that was cut in the base, or in a
/// paragraph that the edit declared this one was made from, is carried.
pub(super) fn judge(
    verdict: &mut Verdict,
    origins: &mut Origins<'_>,
    (before, after): (&PartsView<'_>, &PartsView<'_>),
    (at, part): (usize, &SchemaPart),
    tree: &SpanTree,
    (now, was): (&Census, Option<&Census>),
) -> Result<(), StetError> {
    if !now.has_fields() && !was.is_some_and(Census::has_fields) {
        return Ok(());
    }
    let Some(kind) = story_kind(after, part)? else {
        return Ok(());
    };
    let cut = split_paragraphs(&Story::scan(&part.name, kind, tree)?, tree);
    if cut.is_empty() {
        return Ok(());
    }
    let old: HashSet<NodeId> = match was {
        Some(_) => base_split(before, part)?.0.clone(),
        None => HashSet::new(),
    };
    for paragraph in cut {
        let inherited = old.contains(&paragraph)
            || origins
                .sources(part, paragraph)?
                .iter()
                .any(|source| old.contains(source));
        if inherited {
            verdict.carry(Some(at), finding(paragraph));
        } else {
            verdict.add_new(Some(at), finding(paragraph));
        }
    }
    Ok(())
}
