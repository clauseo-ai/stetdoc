//! Comments in the comparison (decision 0022): which comment of the original a revised comment is, and the differences of a
//! comment that no tracked change records.
//!
//! A comment pairs by its `w:commentReference`: the references of two comments are the same place in an unchanged stretch of two
//! paired body paragraphs. Several comments at one place (comments on the same text) pair by id first, then by author, initials
//! and text, each match unique on both sides; one comment on each side pairs as it is. Comments left on both sides of such a
//! place are ambiguous and are not compared, with a notice, rather than pairing the wrong two. A paired comment is compared like
//! a note. A notice also names a paired comment whose author or initials changed, and a comment whose reference moved, which
//! is compared with nothing while the original has a comment of the same id that reads differently.

use super::edits::{map_unchanged, word_edits};
use super::model::Body;
use super::stories::StoryPair;
use crate::api::error::StetError;
use crate::state::State;
use crate::wordml::{BlockKey, BlockKind, Mode, NodeId, Paragraph};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// What a comment is, for pairing and notices.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Fact {
    author: Option<Box<str>>,
    initials: Option<Box<str>>,
    text: String,
    first: Option<NodeId>,
}

/// The comments that pair and the notices about the others.
#[derive(Default)]
pub(super) struct Found {
    /// (id in the original, id in the revised document).
    pub pairs: Vec<(i64, i64)>,
    /// The revised comment's first paragraph, the message, and whether the comment was left out of the comparison.
    pub notices: Vec<(BlockKey, String, bool)>,
}

/// The comments of a comments part by id.
fn facts(state: &State, part: &str) -> Result<HashMap<i64, Fact>, StetError> {
    let view = state.parts();
    let Some(id) = view.id(part) else {
        return Ok(HashMap::new());
    };
    let (story, tree) = (view.story(id)?, view.tree(id)?);
    let mut out: HashMap<i64, Fact> = HashMap::new();
    for (i, u) in story.units().iter().enumerate() {
        let Some(cid) = u.id else { continue };
        let mut f = Fact {
            author: u.author.clone(),
            initials: u.initials.clone(),
            ..Fact::default()
        };
        for e in story.blocks().iter().filter(|e| e.unit as usize == i) {
            f.first.get_or_insert(e.node);
            if e.kind == BlockKind::Paragraph
                && let Some(p) = Paragraph::new(tree, e.node)
            {
                if !f.text.is_empty() {
                    f.text.push('\n');
                }
                f.text.push_str(&p.text(Mode::Final));
            }
        }
        out.insert(cid, f);
    }
    Ok(out)
}

/// The comments of the original and the revised document that pair, and the notices.
pub(super) fn pair_comments(
    body: &StoryPair,
    original: &State,
    revised: &State,
    part_a: &str,
    part_b: &Arc<str>,
) -> Result<Found, StetError> {
    let (fa, fb) = (facts(original, part_a)?, facts(revised, part_b)?);
    let mut found = Found::default();
    let mut noticed: HashSet<i64> = HashSet::new();
    let mut notice = |found: &mut Found, id: i64, message: String, uncompared: bool| {
        if let Some(first) = fb.get(&id).and_then(|f| f.first)
            && noticed.insert(id)
        {
            found.notices.push((
                BlockKey::new(Arc::clone(part_b), first),
                message,
                uncompared,
            ));
        }
    };
    let (mut referenced_a, mut referenced_b): (HashSet<i64>, HashSet<i64>) = Default::default();
    for c in body.model_a.containers.iter() {
        for n in &c.blocks {
            if let Body::Para(p) = &n.body {
                referenced_a.extend(p.comments.iter().map(|c| c.1));
            }
        }
    }
    for c in body.model_b.containers.iter() {
        for n in &c.blocks {
            if let Body::Para(p) = &n.body {
                referenced_b.extend(p.comments.iter().map(|c| c.1));
            }
        }
    }
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
                if px.comments.is_empty() || py.comments.is_empty() {
                    continue;
                }
                let edits = word_edits(&px.text, &py.text);
                // the comments of each side by the place of their reference, the original's at their revised place
                let mut places: BTreeMap<usize, (Vec<i64>, Vec<i64>)> = BTreeMap::new();
                for &(oa, id) in &px.comments {
                    if let Some(ob) = map_unchanged(&edits, oa) {
                        places.entry(ob).or_default().0.push(id);
                    }
                }
                for &(ob, id) in &py.comments {
                    if let Some(p) = places.get_mut(&ob) {
                        p.1.push(id);
                    }
                }
                for (ga, gb) in places.into_values() {
                    let taken = |found: &Found, a: i64, b: i64| {
                        found.pairs.iter().any(|(x, y)| *x == a || *y == b)
                    };
                    let mut ga: Vec<i64> = ga
                        .into_iter()
                        .filter(|a| !found.pairs.iter().any(|p| p.0 == *a))
                        .collect();
                    let mut gb: Vec<i64> = gb
                        .into_iter()
                        .filter(|b| !found.pairs.iter().any(|p| p.1 == *b))
                        .collect();
                    if let ([a], [b]) = (&ga[..], &gb[..]) {
                        if !taken(&found, *a, *b) {
                            found.pairs.push((*a, *b));
                        }
                        continue;
                    }
                    // the same id, then the same author, initials and text, each match unique on both sides
                    let same_id: Vec<i64> = ga.iter().copied().filter(|a| gb.contains(a)).collect();
                    for id in same_id {
                        found.pairs.push((id, id));
                    }
                    ga.retain(|a| !found.pairs.iter().any(|p| p.0 == *a));
                    gb.retain(|b| !found.pairs.iter().any(|p| p.1 == *b));
                    let mut matched: Vec<(i64, i64)> = Vec::new();
                    for &a in &ga {
                        let Some(f) = fa.get(&a) else { continue };
                        let on_b: Vec<i64> = gb
                            .iter()
                            .copied()
                            .filter(|b| {
                                fb.get(b) == Some(f)
                                    || fb.get(b).is_some_and(|g| same_comment(f, g))
                            })
                            .collect();
                        let on_a = ga
                            .iter()
                            .filter(|x| fa.get(x).is_some_and(|g| same_comment(f, g)))
                            .count();
                        if let ([b], 1) = (&on_b[..], on_a) {
                            matched.push((a, *b));
                        }
                    }
                    for (a, b) in matched {
                        found.pairs.push((a, b));
                    }
                    ga.retain(|a| !found.pairs.iter().any(|p| p.0 == *a));
                    gb.retain(|b| !found.pairs.iter().any(|p| p.1 == *b));
                    if !ga.is_empty() {
                        for &b in &gb {
                            notice(&mut found, b, "this comment shares its place with other comments and none of the original's comments there could be matched to it, so it was not compared; the redline keeps it as the revised document has it".to_owned(), true);
                        }
                    }
                }
            }
        }
    }
    // the author or initials of a paired comment
    for &(a, b) in &found.pairs.clone() {
        let (Some(x), Some(y)) = (fa.get(&a), fb.get(&b)) else {
            continue;
        };
        let mut what: Vec<String> = Vec::new();
        for (name, p, q) in [
            ("author", &x.author, &y.author),
            ("initials", &x.initials, &y.initials),
        ] {
            if p != q {
                what.push(format!(
                    "{name} \"{}\" became \"{}\"",
                    p.as_deref().unwrap_or(""),
                    q.as_deref().unwrap_or("")
                ));
            }
        }
        if !what.is_empty() {
            notice(
                &mut found,
                b,
                format!(
                    "this comment's {}, which no tracked change can record, so the redline keeps the revised comment's",
                    what.join(" and ")
                ),
                false,
            );
        }
    }
    // a comment compared with nothing while the original has one of the same id, referenced elsewhere, that reads differently
    let paired_a: HashSet<i64> = found.pairs.iter().map(|p| p.0).collect();
    let paired_b: HashSet<i64> = found.pairs.iter().map(|p| p.1).collect();
    let mut lone: Vec<i64> = referenced_b
        .iter()
        .copied()
        .filter(|b| !paired_b.contains(b))
        .collect();
    lone.sort_unstable();
    for b in lone {
        if paired_a.contains(&b) || !referenced_a.contains(&b) {
            continue;
        }
        if let (Some(x), Some(y)) = (fa.get(&b), fb.get(&b))
            && !same_comment(x, y)
        {
            notice(&mut found, b, "this comment's reference moved, so it was not compared with the original's comment, which reads differently; the redline keeps the revised comment and rejecting does not bring back the original's".to_owned(), true);
        }
    }
    Ok(found)
}

/// True if two comments have the same author, initials and text.
fn same_comment(x: &Fact, y: &Fact) -> bool {
    x.author == y.author && x.initials == y.initials && x.text == y.text
}
