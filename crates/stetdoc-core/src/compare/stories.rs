//! Which stories of the two documents are compared, and how their blocks pair (design `compare.md`, 4.7).
//!
//! The body pairs with the body. A header or footer pairs through its slot: section k of one package corresponds to the
//! section of the other whose ending paragraph was paired in the body (the last section to the last), and a slot is the
//! effective part of a type (default, first, even), inheritance applied. The pairing of parts is one to one. A slot that
//! exists on one side only cannot be shown as a revision (a header reference is not tracked), so it is skipped and
//! counted. A note follows its reference: two notes pair when their references are the same character of an unchanged
//! stretch of two paired paragraphs, and every other note is new or deleted with its reference.

use super::edits::{map_unchanged, word_edits};
use super::model::{Atoms, Body, StoryModel, root_of, story_model};
use super::regroup::{Regrouped, regroup};
use super::tables::{StoryPairing, pair_containers};
use crate::api::error::StetError;
use crate::state::{PartId, State};
use crate::wordml::{NodeId, StoryKind};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

/// One paired root: a whole story, or one note against its partner.
#[derive(Debug)]
pub(super) struct RootPair {
    /// The note's id in the original, or `None` for a story that is not a note part.
    pub note_a: Option<i64>,
    /// The note's id in the revised document.
    pub note_b: Option<i64>,
    /// The pairing of the two roots' blocks.
    pub pairing: StoryPairing,
    /// The moves, splits and merges found in it.
    pub regrouped: Regrouped,
}

/// Two stories that are compared.
#[derive(Debug)]
pub(super) struct StoryPair {
    /// What the story is.
    pub kind: StoryKind,
    /// The original's part name.
    pub part_a: Arc<str>,
    /// The revised part name.
    pub part_b: Arc<str>,
    /// The original's model.
    pub model_a: StoryModel,
    /// The revised model.
    pub model_b: StoryModel,
    /// The paired roots: one for a body, a header or a footer, one for each paired note.
    pub roots: Vec<RootPair>,
}

/// Everything compare pairs in two documents.
#[derive(Debug, Default)]
pub(super) struct Pairings {
    /// The stories, the body first.
    pub stories: Vec<StoryPair>,
    /// How many header and footer slots were skipped, because they exist on one side only or have no one-to-one partner.
    pub unmatched_slots: usize,
    /// How many rows had different cell shapes and were left unpaired, over all stories.
    pub table_replaced: usize,
}

/// Pairs the stories of `original` with those of `revised`. `atoms` is the dictionary of the two sides.
pub(super) fn pair_documents(
    original: &State,
    revised: &State,
    atoms: &mut Atoms,
) -> Result<Pairings, StetError> {
    let (va, vb) = (original.parts(), revised.parts());
    let specs_a = va.story_specs()?;
    let specs_b = vb.story_specs()?;
    let mut out = Pairings::default();
    let (Some(body_a), Some(body_b)) = (specs_a.0.first(), specs_b.0.first()) else {
        return Ok(out);
    };
    let (Some(ida), Some(idb)) = (va.id(&body_a.part), vb.id(&body_b.part)) else {
        return Ok(out);
    };
    let story_a = va.story(ida)?;
    let story_b = vb.story(idb)?;
    let model_a = story_model(&va, ida, &story_a, atoms)?;
    let model_b = story_model(&vb, idb, &story_b, atoms)?;
    let body = whole_story(
        StoryKind::Body,
        &body_a.part,
        &body_b.part,
        model_a,
        model_b,
    );
    // paired body paragraphs: original element to revised element
    let mut paragraph_pairs: HashMap<NodeId, NodeId> = HashMap::new();
    for root in &body.roots {
        for c in &root.pairing.containers {
            for pair in &c.pairs {
                if let (Some(x), Some(y)) = (
                    body.model_a
                        .containers
                        .get(c.ca)
                        .and_then(|k| k.blocks.get(pair.a)),
                    body.model_b
                        .containers
                        .get(c.cb)
                        .and_then(|k| k.blocks.get(pair.b)),
                ) {
                    paragraph_pairs.insert(x.id, y.id);
                }
            }
        }
    }
    let notes = paired_notes(&body);

    // sections, and through them the headers and footers
    let tree_a = va.tree(ida)?;
    let tree_b = vb.tree(idb)?;
    let sections_a = story_a.sections(tree_a, &*va.rels(ida)?);
    let sections_b = story_b.sections(tree_b, &*vb.rels(idb)?);
    let ends = |story: &crate::wordml::Story, s: &crate::wordml::Section| -> Option<NodeId> {
        s.ends_at
            .and_then(|i| story.blocks().get(i))
            .map(|e| e.node)
    };
    let mut section_pairs: Vec<(usize, usize)> = Vec::new();
    for (i, sa) in sections_a.iter().enumerate() {
        for (j, sb) in sections_b.iter().enumerate() {
            let paired = match (ends(&story_a, sa), ends(&story_b, sb)) {
                (Some(x), Some(y)) => paragraph_pairs.get(&x) == Some(&y),
                (None, None) => true,
                _ => false,
            };
            if paired {
                section_pairs.push((i, j));
            }
        }
    }
    let mut slots: Vec<(StoryKind, Arc<str>, Arc<str>)> = Vec::new();
    let mut used_a: BTreeSet<Arc<str>> = BTreeSet::new();
    let mut used_b: BTreeSet<Arc<str>> = BTreeSet::new();
    let mut skipped = 0usize;
    for &(i, j) in &section_pairs {
        let (Some(sa), Some(sb)) = (sections_a.get(i), sections_b.get(j)) else {
            continue;
        };
        for (kind, pa, pb) in [
            (StoryKind::Header, &sa.headers, &sb.headers),
            (StoryKind::Footer, &sa.footers, &sb.footers),
        ] {
            for (x, y) in [
                (&pa.default, &pb.default),
                (&pa.first, &pb.first),
                (&pa.even, &pb.even),
            ] {
                match (x, y) {
                    (Some(x), Some(y)) => {
                        if slots.iter().any(|(_, p, q)| p == x && q == y) {
                            continue;
                        }
                        if used_a.contains(x) || used_b.contains(y) {
                            skipped += 1;
                            continue;
                        }
                        used_a.insert(Arc::clone(x));
                        used_b.insert(Arc::clone(y));
                        slots.push((kind, Arc::clone(x), Arc::clone(y)));
                    }
                    (None, None) => {}
                    _ => skipped += 1,
                }
            }
        }
    }
    out.table_replaced += body
        .roots
        .iter()
        .map(|r| r.pairing.table_replaced)
        .sum::<usize>();
    out.stories.push(body);
    for (kind, pa, pb) in slots {
        if let Some(pair) = side_story(original, revised, kind, &pa, &pb, atoms)? {
            out.table_replaced += pair
                .roots
                .iter()
                .map(|r| r.pairing.table_replaced)
                .sum::<usize>();
            out.stories.push(pair);
        }
    }
    out.unmatched_slots = skipped;

    // notes, by reference
    for (kind, endnote) in [(StoryKind::Footnotes, false), (StoryKind::Endnotes, true)] {
        let part_a = specs_a.0.iter().find(|s| s.kind == kind);
        let part_b = specs_b.0.iter().find(|s| s.kind == kind);
        let Some(pb) = part_b else { continue };
        let Some(pa) = part_a else {
            // Only the revised document has the part: every note it references is new.
            if let Some(pair) = revised_only(revised, kind, &pb.part, atoms)? {
                out.stories.push(pair);
            }
            continue;
        };
        let wanted: Vec<(i64, i64)> = notes
            .iter()
            .filter(|n| n.2 == endnote)
            .map(|n| (n.0, n.1))
            .collect();
        if let Some(pair) =
            notes_story(original, revised, kind, &pa.part, &pb.part, &wanted, atoms)?
        {
            out.table_replaced += pair
                .roots
                .iter()
                .map(|r| r.pairing.table_replaced)
                .sum::<usize>();
            out.stories.push(pair);
        }
    }
    Ok(out)
}

/// A notes part that only the revised document has: its model, with an empty original and no paired note. The original's
/// part name is empty.
fn revised_only(
    revised: &State,
    kind: StoryKind,
    part: &Arc<str>,
    atoms: &mut Atoms,
) -> Result<Option<StoryPair>, StetError> {
    let vb = revised.parts();
    let Some(idb) = vb.id(part) else {
        return Ok(None);
    };
    let sb = vb.story(idb)?;
    Ok(Some(StoryPair {
        kind,
        part_a: Arc::from(""),
        part_b: Arc::clone(part),
        model_a: StoryModel::default(),
        model_b: story_model(&vb, idb, &sb, atoms)?,
        roots: Vec::new(),
    }))
}

/// A story that is one root against one root.
fn whole_story(
    kind: StoryKind,
    part_a: &Arc<str>,
    part_b: &Arc<str>,
    model_a: StoryModel,
    model_b: StoryModel,
) -> StoryPair {
    let mut pairing = StoryPairing::default();
    if let (Some(ra), Some(rb)) = (root_of(&model_a, None), root_of(&model_b, None)) {
        pair_containers(&model_a, ra, &model_b, rb, &mut pairing);
    }
    let regrouped = regroup(&model_a, &model_b, &pairing);
    StoryPair {
        kind,
        part_a: Arc::clone(part_a),
        part_b: Arc::clone(part_b),
        model_a,
        model_b,
        roots: vec![RootPair {
            note_a: None,
            note_b: None,
            pairing,
            regrouped,
        }],
    }
}

fn models_of(
    original: &State,
    revised: &State,
    part_a: &str,
    part_b: &str,
    atoms: &mut Atoms,
) -> Result<Option<(StoryModel, StoryModel)>, StetError> {
    let (va, vb) = (original.parts(), revised.parts());
    let (Some(ida), Some(idb)): (Option<PartId>, Option<PartId>) = (va.id(part_a), vb.id(part_b))
    else {
        return Ok(None);
    };
    let sa = va.story(ida)?;
    let sb = vb.story(idb)?;
    Ok(Some((
        story_model(&va, ida, &sa, atoms)?,
        story_model(&vb, idb, &sb, atoms)?,
    )))
}

fn side_story(
    original: &State,
    revised: &State,
    kind: StoryKind,
    part_a: &Arc<str>,
    part_b: &Arc<str>,
    atoms: &mut Atoms,
) -> Result<Option<StoryPair>, StetError> {
    Ok(models_of(original, revised, part_a, part_b, atoms)?
        .map(|(a, b)| whole_story(kind, part_a, part_b, a, b)))
}

fn notes_story(
    original: &State,
    revised: &State,
    kind: StoryKind,
    part_a: &Arc<str>,
    part_b: &Arc<str>,
    wanted: &[(i64, i64)],
    atoms: &mut Atoms,
) -> Result<Option<StoryPair>, StetError> {
    let Some((model_a, model_b)) = models_of(original, revised, part_a, part_b, atoms)? else {
        return Ok(None);
    };
    let mut roots = Vec::new();
    for &(na, nb) in wanted {
        let (Some(ra), Some(rb)) = (root_of(&model_a, Some(na)), root_of(&model_b, Some(nb)))
        else {
            continue;
        };
        let mut pairing = StoryPairing::default();
        pair_containers(&model_a, ra, &model_b, rb, &mut pairing);
        let regrouped = regroup(&model_a, &model_b, &pairing);
        roots.push(RootPair {
            note_a: Some(na),
            note_b: Some(nb),
            pairing,
            regrouped,
        });
    }
    Ok(Some(StoryPair {
        kind,
        part_a: Arc::clone(part_a),
        part_b: Arc::clone(part_b),
        model_a,
        model_b,
        roots,
    }))
}

/// The pairs of notes (the id in the original, the id in the revised document, endnote or not) whose references are the
/// same character in an unchanged stretch of two paired body paragraphs.
fn paired_notes(body: &StoryPair) -> Vec<(i64, i64, bool)> {
    let mut out = Vec::new();
    for root in &body.roots {
        for c in &root.pairing.containers {
            for pair in &c.pairs {
                let (Some(x), Some(y)) = (
                    body.model_a
                        .containers
                        .get(c.ca)
                        .and_then(|k| k.blocks.get(pair.a)),
                    body.model_b
                        .containers
                        .get(c.cb)
                        .and_then(|k| k.blocks.get(pair.b)),
                ) else {
                    continue;
                };
                let (Body::Para(px), Body::Para(py)) = (&x.body, &y.body) else {
                    continue;
                };
                if px.notes.is_empty() || py.notes.is_empty() {
                    continue;
                }
                let edits = word_edits(&px.text, &py.text);
                for &(oa, ida, endnote) in &px.notes {
                    if let Some(ob) = map_unchanged(&edits, oa)
                        && let Some(&(_, idb, _)) =
                            py.notes.iter().find(|n| n.0 == ob && n.2 == endnote)
                    {
                        out.push((ida, idb, endnote));
                    }
                }
            }
        }
    }
    out
}
