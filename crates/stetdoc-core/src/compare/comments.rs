//! Comments in the comparison (decision 0022): which comment of the original a revised comment is, and the differences of a
//! comment that no tracked change records.
//!
//! A comment pairs by its `w:commentReference`: the references of two comments are the same place in an unchanged stretch of two
//! paired body paragraphs. Several comments at one place (comments on the same text) pair by content and never by id, which
//! Word renumbers on save: the same author, initials and text first (the id orders otherwise identical comments), then the
//! same author and initials when that is unique on both sides. One comment on each side pairs as it is. Reply links
//! (`commentsExtended`) are not read. Comments left on both sides of such a
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

/// The comments a notice leaves out of the comparison: the original's ids and the revised comment's id.
pub(super) type LeftOut = (Vec<i64>, i64);

/// A pass of the pairing at one place: whether a match must be unique on both sides, and the key it matches.
type Pass = (bool, fn(&Fact) -> String);

/// The key that pairs two comments at one place first: author, initials and text.
fn full_key(f: &Fact) -> String {
    format!("{}\u{1f}{}", who_key(f), f.text)
}

/// The key that pairs them next, when unique on both sides: author and initials.
fn who_key(f: &Fact) -> String {
    format!(
        "{}\u{1f}{}",
        f.author.as_deref().unwrap_or(""),
        f.initials.as_deref().unwrap_or("")
    )
}

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
    /// The revised comment's first paragraph, the message, and for a comment left out of the comparison the original's
    /// comments it was not compared with and its own id.
    pub notices: Vec<(BlockKey, String, Option<LeftOut>)>,
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
    let mut notice = |found: &mut Found, id: i64, message: String, left_out: Option<Vec<i64>>| {
        if let Some(first) = fb.get(&id).and_then(|f| f.first)
            && noticed.insert(id)
        {
            found.notices.push((
                BlockKey::new(Arc::clone(part_b), first),
                message,
                left_out.map(|a| (a, id)),
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
                    // by content, never by id, which Word renumbers on save: the same author, initials and text, then the same
                    // author and initials when that is unique in the group; the id only orders otherwise identical comments
                    ga.sort_unstable();
                    gb.sort_unstable();
                    let passes: [Pass; 2] = [(false, full_key), (true, who_key)];
                    for (unique_only, key) in passes {
                        let keys_a: Vec<Option<String>> =
                            ga.iter().map(|a| fa.get(a).map(key)).collect();
                        let keys_b: Vec<Option<String>> =
                            gb.iter().map(|b| fb.get(b).map(key)).collect();
                        let mut matched: Vec<(i64, i64)> = Vec::new();
                        for (ia, &a) in ga.iter().enumerate() {
                            let Some(Some(k)) = keys_a.get(ia) else {
                                continue;
                            };
                            let count_a = keys_a.iter().filter(|x| x.as_ref() == Some(k)).count();
                            let count_b = keys_b.iter().filter(|x| x.as_ref() == Some(k)).count();
                            if unique_only && (count_a != 1 || count_b != 1) {
                                continue;
                            }
                            let free = gb.iter().zip(&keys_b).find(|(b, kb)| {
                                kb.as_ref() == Some(k) && !matched.iter().any(|m| m.1 == **b)
                            });
                            if let Some((&b, _)) = free {
                                matched.push((a, b));
                            }
                        }
                        found.pairs.extend(matched);
                        ga.retain(|a| !found.pairs.iter().any(|p| p.0 == *a));
                        gb.retain(|b| !found.pairs.iter().any(|p| p.1 == *b));
                    }
                    if !ga.is_empty() {
                        for &b in &gb {
                            notice(&mut found, b, "this comment shares its place with other comments and none of the original's comments there could be matched to it, so it was not compared; the redline keeps it as the revised document has it".to_owned(), Some(ga.clone()));
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
                None,
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
            notice(&mut found, b, "this comment's reference moved, so it was not compared with the original's comment, which reads differently; the redline keeps the revised comment and rejecting does not bring back the original's".to_owned(), Some(vec![b]));
        }
    }
    Ok(found)
}

/// True if two comments have the same author, initials and text.
fn same_comment(x: &Fact, y: &Fact) -> bool {
    x.author == y.author && x.initials == y.initials && x.text == y.text
}
