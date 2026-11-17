//! Hyperlinks in the comparison (decision 0022).
//!
//! A hyperlink's address is not in a paragraph's text, so two paragraphs whose texts are the same compare equal even when a link
//! points somewhere else. Each paragraph therefore lists the ranges of its links, a `w:hyperlink` or a `HYPERLINK` field, each
//! with a key made of what the link points to (the relationship's target, the anchor, the document location, or the field's
//! instruction). Along the text that two paired paragraphs have in common, a stretch whose characters are in links with different
//! keys (or in a link on one side only) becomes a replacement, widened to the whole link on both sides: the original's link is
//! deleted and the revised one inserted, which is how a change of address reads as a tracked change, since OOXML has no change
//! element for it.

use super::edits::{Replace, cuts, merge_touching};
use super::model::ParaInfo;
use super::text::hash_bytes;
use crate::api::view::Mode;
use crate::wordml::{NO_FIELD, RelTarget, Rels, SpanTree, TextMap, WrapperKind};
use std::ops::Range;

/// The ranges of the links of a paragraph and their keys, in the paragraph's `Final` text.
pub(super) fn link_ranges(map: &TextMap, tree: &SpanTree, rels: &Rels) -> Vec<(Range<usize>, u64)> {
    let mut out: Vec<(Range<usize>, u64)> = Vec::new();
    // `w:hyperlink`: the characters of the items it wraps, the element named by its node
    let mut open: Option<(crate::wordml::NodeId, Range<usize>)> = None;
    let mut fields: Vec<(u32, Range<usize>)> = Vec::new();
    for item in map.items() {
        if item.len == 0 || !item.vis.in_mode(Mode::Final) {
            continue;
        }
        let (start, end) = (item.at as usize, (item.at + item.len) as usize);
        let link = map
            .context(item.ctx)
            .find(|c| c.kind == WrapperKind::Hyperlink)
            .map(|c| c.node);
        match (&mut open, link) {
            (Some((n, r)), Some(l)) if *n == l => r.end = end,
            _ => {
                if let Some((n, r)) = open.take() {
                    out.push((r, element_key(tree, rels, n)));
                }
                open = link.map(|l| (l, start..end));
            }
        }
        // every field the item is in, the fields around it included
        let mut cur = (item.field != NO_FIELD).then_some(item.field);
        let mut guard = 0usize;
        while let Some(id) = cur {
            guard += 1;
            match fields.iter_mut().find(|(f, _)| *f == id) {
                Some((_, r)) => *r = r.start.min(start)..r.end.max(end),
                None => fields.push((id, start..end)),
            }
            cur = map
                .fields()
                .iter()
                .find(|f| f.id == id)
                .and_then(|f| f.parent);
            if guard > map.fields().len() {
                break;
            }
        }
    }
    if let Some((n, r)) = open {
        out.push((r, element_key(tree, rels, n)));
    }
    for (id, r) in fields {
        let Some(f) = map.fields().iter().find(|f| f.id == id) else {
            continue;
        };
        if !f.begins_here || f.end.is_none() {
            continue;
        }
        let instr: Vec<&str> = f.instr.split_whitespace().collect();
        if instr
            .first()
            .is_some_and(|k| k.eq_ignore_ascii_case("HYPERLINK"))
        {
            out.push((
                r,
                hash_bytes(format!("field {}", instr.join(" ")).as_bytes()),
            ));
        }
    }
    out
}

/// The key of a `w:hyperlink`: its relationship's target, its anchor and its document location.
fn element_key(tree: &SpanTree, rels: &Rels, node: crate::wordml::NodeId) -> u64 {
    let el = tree.node_ref(node);
    let attr = |local: &str, rel: bool| {
        el.attrs()
            .find(|a| a.local_name() == local && a.namespace().ends_with("/relationships") == rel)
            .map(|a| a.value().into_owned())
            .unwrap_or_default()
    };
    let target = rels
        .get(&attr("id", true))
        .map_or(String::new(), |r| match &r.target {
            RelTarget::External(u) => format!("url {u}"),
            RelTarget::Part(p) => format!("part {p}"),
            RelTarget::Invalid => String::new(),
        });
    hash_bytes(
        format!(
            "{target}\u{1f}{}\u{1f}{}",
            attr("anchor", false),
            attr("docLocation", false)
        )
        .as_bytes(),
    )
}

/// The key of each character of a paragraph: the keys of the links it is in, combined, and zero outside every link.
fn keys(p: &ParaInfo) -> Vec<u64> {
    let mut out = vec![0u64; p.text.chars().count()];
    for (r, k) in &p.links {
        for slot in out.iter_mut().take(r.end).skip(r.start) {
            *slot = slot.wrapping_mul(0x100_0000_01b3) ^ k;
        }
    }
    out
}

/// `reps` with a replacement of each link whose address differs along the text the two paragraphs have in common.
pub(super) fn with_link_changes(reps: Vec<Replace>, a: &ParaInfo, b: &ParaInfo) -> Vec<Replace> {
    if a.links.is_empty() && b.links.is_empty() {
        return reps;
    }
    let (ka, kb) = (keys(a), keys(b));
    let mut stretches: Vec<(Range<usize>, Range<usize>)> = Vec::new();
    let (mut a_at, mut b_at) = (0usize, 0usize);
    for r in &reps {
        stretches.push((a_at..r.a.start, b_at..r.b.start));
        (a_at, b_at) = (r.a.end, r.b.end);
    }
    stretches.push((a_at..ka.len(), b_at..kb.len()));
    let mut extra: Vec<Replace> = Vec::new();
    for (ra, rb) in stretches {
        let mut current: Option<Replace> = None;
        for (x, y) in ra.zip(rb) {
            let (Some(sa), Some(sb)) = (ka.get(x), kb.get(y)) else {
                break;
            };
            if sa == sb {
                extra.extend(current.take());
                continue;
            }
            match &mut current {
                Some(c) => {
                    c.a.end = x + 1;
                    c.b.end = y + 1;
                }
                None => {
                    current = Some(Replace {
                        a: x..x + 1,
                        b: y..y + 1,
                    });
                }
            }
        }
        extra.extend(current);
    }
    if extra.is_empty() {
        return reps;
    }
    let spans = |p: &ParaInfo| -> Vec<Range<usize>> {
        p.links
            .iter()
            .map(|(r, _)| r.clone())
            .chain(p.fields.iter().cloned())
            .collect()
    };
    let mut all = reps;
    all.extend(extra);
    grow(all, &spans(a), &spans(b))
}

/// Replacements widened until none cuts a span of either side. A widened replacement takes the other side's range from the text
/// the two sides have in common, through the other replacements, so a link whose text changed with its address is replaced as
/// a whole on both sides and no more.
pub(super) fn grow(
    mut reps: Vec<Replace>,
    spans_a: &[Range<usize>],
    spans_b: &[Range<usize>],
) -> Vec<Replace> {
    reps.sort_by_key(|r| (r.a.start, r.b.start));
    reps = merge_touching(reps);
    // each pass that changes something grows a range, and a range cannot grow past the ends of the texts
    for _ in 0..(spans_a.len() + spans_b.len() + 2) * (reps.len() + 2) {
        let mut changed = false;
        for i in 0..reps.len() {
            for flip in [false, true] {
                let spans = if flip { spans_b } else { spans_a };
                for s in spans {
                    let Some(r) = reps.get(i) else { break };
                    let own = if flip { &r.b } else { &r.a };
                    if !cuts(own, s) {
                        continue;
                    }
                    let (lo, hi) = (own.start.min(s.start), own.end.max(s.end));
                    let other_lo = other_side(&reps, i, lo, flip, true);
                    let other_hi = other_side(&reps, i, hi, flip, false);
                    if let Some(r) = reps.get_mut(i) {
                        if flip {
                            r.b = lo..hi;
                            r.a = other_lo.min(r.a.start)..other_hi.max(r.a.end);
                        } else {
                            r.a = lo..hi;
                            r.b = other_lo.min(r.b.start)..other_hi.max(r.b.end);
                        }
                    }
                    changed = true;
                }
            }
        }
        reps.sort_by_key(|r| (r.a.start, r.b.start));
        reps = merge_touching(reps);
        if !changed {
            break;
        }
    }
    reps
}

/// The offset on the other side of `at`, an offset of one side (the revised one when `flip`), read through the replacements
/// other than `skip` when `at` starts a range and through all of them when it ends one. An offset inside another replacement
/// takes that replacement's start or end on the other side.
fn other_side(reps: &[Replace], skip: usize, at: usize, flip: bool, start: bool) -> usize {
    let mut delta: isize = 0;
    for (j, r) in reps.iter().enumerate() {
        let (own, other) = if flip { (&r.b, &r.a) } else { (&r.a, &r.b) };
        if start && j == skip {
            continue;
        }
        if own.end <= at && !(start && own.start == at && own.is_empty()) {
            delta += other.len() as isize - own.len() as isize;
        } else if own.start < at && at < own.end {
            return if start { other.start } else { other.end };
        }
    }
    at.saturating_add_signed(delta)
}
