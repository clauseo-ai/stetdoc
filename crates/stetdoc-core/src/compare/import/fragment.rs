//! A range of an original paragraph as runs (design `compare.md`, 7.2).
//!
//! The range is read from the original's text map: which text nodes it covers, and how much of each, and which other pieces of
//! content (a tab, a break, a picture, the characters of a field) lie inside it. The paragraph is then read as a [`Snip`] and
//! pruned to what the range covers: each run keeps its properties and what is covered of its content, a wrapper such as a
//! hyperlink is kept around the runs that remain in it, and everything else (the paragraph's properties, bookmarks, comment
//! ranges) is left out.

use super::snip::{Kid, Snip, W, W_STRICT};
use crate::api::error::{ErrorCode, StetError};
use crate::state::State;
use crate::wordml::{ItemKind, Mode, NodeId, Paragraph};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

/// What a range of a paragraph covers.
#[derive(Default)]
pub(super) struct Cover {
    /// Text nodes: the characters, as positions in the node's own text.
    text: HashMap<NodeId, (usize, usize)>,
    /// Elements that are covered whole: atoms and the marks of fields.
    atoms: HashSet<NodeId>,
    /// How many equations the range covers: they are left out.
    pub(super) math: usize,
}

fn internal(message: &str) -> StetError {
    StetError::new(ErrorCode::Internal, message)
}

/// Finds what the characters `range` of `Final` of the paragraph `para` cover.
pub(super) fn select(
    orig: &State,
    part: &Arc<str>,
    para: NodeId,
    range: Range<usize>,
) -> Result<Cover, StetError> {
    let view = orig.parts();
    let id = view
        .id(part)
        .ok_or_else(|| internal("the original has no such part"))?;
    let tree = view.tree(id)?;
    let p = Paragraph::new(tree, para)
        .ok_or_else(|| internal("an original block is not a paragraph"))?;
    let map = p.map();
    let mut cover = Cover::default();
    let mut at = 0usize;
    let mut depth = 0i64;
    for item in map.items() {
        if !item.vis.in_mode(Mode::Final) {
            continue;
        }
        let from = at;
        let to = at + item.len as usize;
        if item.len > 0 {
            let (a, b) = (from.max(range.start), to.min(range.end));
            if a < b {
                if item.kind == ItemKind::Text {
                    cover.text.insert(
                        item.node,
                        (
                            item.from as usize + (a - from),
                            item.from as usize + (b - from),
                        ),
                    );
                } else {
                    if item.kind == ItemKind::Math {
                        cover.math += 1;
                    }
                    cover.atoms.insert(item.node);
                }
            }
        } else {
            let inside = from > range.start && from < range.end;
            let opens = matches!(
                item.kind,
                ItemKind::FieldBegin | ItemKind::FieldCode | ItemKind::FieldSeparate
            );
            let closes = item.kind == ItemKind::FieldEnd;
            let is_field = opens || closes;
            let take = is_field
                && (inside
                    || (from == range.start && opens)
                    || (from == range.end && closes && range.start < range.end));
            if take {
                cover.atoms.insert(item.node);
                match item.kind {
                    ItemKind::FieldBegin => depth += 1,
                    ItemKind::FieldEnd => depth -= 1,
                    _ => {}
                }
                if depth < 0 {
                    return Err(internal("a range ends a field it did not begin"));
                }
            }
        }
        at = to;
    }
    if depth != 0 {
        return Err(internal("a range begins a field it does not end"));
    }
    Ok(cover)
}

fn is_w(s: &Snip) -> bool {
    s.ns == W || s.ns == W_STRICT
}

/// True if the element or anything below it is covered whole.
fn holds_atom(s: &Snip, cover: &Cover) -> bool {
    let mut hit = false;
    s.walk(&mut |e| hit |= cover.atoms.contains(&e.src));
    hit
}

const WRAPPERS: &[&str] = &[
    "hyperlink",
    "smartTag",
    "fldSimple",
    "sdt",
    "sdtContent",
    "customXml",
    "ins",
    "moveTo",
    "dir",
    "bdo",
];

/// The pieces of the paragraph `para` that the cover names: runs, and wrappers around runs.
pub(super) fn prune(para: Snip, cover: &Cover) -> Vec<Snip> {
    let mut out = Vec::new();
    for k in para.kids {
        if let Kid::El(e) = k
            && let Some(kept) = keep(e, cover)
        {
            out.push(kept);
        }
    }
    out
}

fn keep(mut s: Snip, cover: &Cover) -> Option<Snip> {
    if is_w(&s) && s.local == "r" {
        return keep_run(s, cover);
    }
    if is_w(&s) && WRAPPERS.contains(&s.local.as_str()) {
        let covered_whole = cover.atoms.contains(&s.src);
        let kids = std::mem::take(&mut s.kids);
        let mut out = Vec::new();
        for k in kids {
            if let Kid::El(e) = k
                && let Some(kept) = keep(e, cover)
            {
                out.push(Kid::El(kept));
            }
        }
        if out.is_empty() && !covered_whole {
            return None;
        }
        s.kids = out;
        return Some(s);
    }
    None
}

fn keep_run(mut s: Snip, cover: &Cover) -> Option<Snip> {
    let kids = std::mem::take(&mut s.kids);
    let mut out: Vec<Kid> = Vec::new();
    let mut content = false;
    for k in kids {
        let Kid::El(e) = k else { continue };
        if is_w(&e) && e.local == "rPr" {
            out.push(Kid::El(e));
            continue;
        }
        if is_w(&e) && matches!(e.local.as_str(), "t" | "delText") {
            let mut kept = e.clone();
            kept.kids =
                e.kids
                    .iter()
                    .filter_map(|k| match k {
                        Kid::Text(id, text) => cover.text.get(id).map(|&(a, b)| {
                            Kid::Text(*id, text.chars().skip(a).take(b - a).collect())
                        }),
                        Kid::El(_) => None,
                    })
                    .collect();
            // A slice of shown text keeps the white space at its ends, which a `w:t` shows only with `preserve`.
            let ws = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r');
            let edge = kept
                .kids
                .iter()
                .any(|k| matches!(k, Kid::Text(_, t) if t.starts_with(ws) || t.ends_with(ws)));
            let space = "http://www.w3.org/XML/1998/namespace";
            if edge
                && !kept
                    .attrs
                    .iter()
                    .any(|(ns, l, _)| ns == space && l == "space")
            {
                kept.attrs
                    .push((space.to_owned(), "space".to_owned(), "preserve".to_owned()));
            }
            if !kept.kids.is_empty() {
                content = true;
                out.push(Kid::El(kept));
            }
        } else if holds_atom(&e, cover) {
            content = true;
            out.push(Kid::El(e));
        }
    }
    if !content {
        return None;
    }
    s.kids = out;
    Some(s)
}
