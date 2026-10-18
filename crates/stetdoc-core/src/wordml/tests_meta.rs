//! Edit agreement and insensitivity (`docs/design/wordml.md` 5.2, items 8 and 9) as metamorphic
//! properties through the real `xml::Editor`, on the paragraphs of `tests_gen.rs`.
//!
//! Agreement: deleting a range of `Final` by its spans, and inserting text at a gap, give the text
//! the map predicted in all three modes, and every item that was not touched keeps its place.
//! Insensitivity: a rewrite that changes nothing Word shows changes no mode's text. Splitting a run,
//! merging two, adding and removing marks and attributes, wrapping runs. Each edit is made on a
//! clone and read again through a fresh scan, and the refresh of the old story must equal that scan.

use super::tests_gen_data::Rng;
use super::tests_gen_laws::for_each_paragraph;
use super::tests_text::FINAL;
use super::*;
use crate::xml::SpanTree;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

type Digest = Vec<(ItemKind, NodeId, u32, u32, Vis, u32, u8, Vec<WrapperKind>)>;

/// Everything about a map that the edits below must not disturb, one row for each item.
pub(super) fn digest(m: &TextMap) -> Digest {
    m.items()
        .iter()
        .map(|i| {
            let chain = m.context(i.ctx).map(|c| c.kind).collect();
            (i.kind, i.node, i.at, i.len, i.vis, i.field, i.flags, chain)
        })
        .collect()
}

pub(super) fn scan(tree: &SpanTree) -> Story {
    Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, tree).unwrap()
}

/// The map of paragraph `p` in a story of `tree`.
pub(super) fn map_of(story: &Story, tree: &SpanTree, p: NodeId) -> TextMap {
    let i = story.index_of(p).expect("the paragraph is still a block");
    story.paragraph(tree, i).unwrap().map()
}

/// Where each item starts in a mode's text, with the length as the last element.
pub(super) fn starts(m: &TextMap, mode: Mode) -> Vec<usize> {
    let mut out = Vec::new();
    let mut at = 0usize;
    for item in m.items() {
        out.push(at);
        if item.vis.in_mode(mode) {
            at += item.len as usize;
        }
    }
    out.push(at);
    out
}

/// Each item that is the first made from its node: a `w:fldSimple` makes a begin and an end.
pub(super) fn firsts(m: &TextMap) -> Vec<(usize, Item)> {
    let mut seen = HashSet::new();
    (0..m.items().len())
        .filter(|&j| seen.insert(m.items()[j].node))
        .map(|j| (j, m.items()[j]))
        .collect()
}

pub(super) fn texts(m: &TextMap) -> Vec<String> {
    Mode::ALL.map(|mode| m.text(mode).into_owned()).to_vec()
}

/// `text` without the character ranges, which are in order and do not overlap.
fn without(text: &str, ranges: &[Range<usize>]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    for r in ranges {
        out.extend(&chars[at..r.start]);
        at = r.end;
    }
    out.extend(&chars[at..]);
    out
}

/// Where an offset of the old text is once the ranges are removed.
fn shifted(offset: usize, ranges: &[Range<usize>]) -> usize {
    offset
        - ranges
            .iter()
            .map(|r| offset.min(r.end).saturating_sub(r.start.min(offset)))
            .sum::<usize>()
}

/// The refresh of the old story must be the scan of the edited tree, and must say the same of fields.
pub(super) fn check_refresh(old: &Story, edited: &SpanTree, p: NodeId, scanned: &Story) {
    let refreshed = Story::refresh(old, edited, &[p]).unwrap().story;
    assert!(refreshed.same_as(scanned));
    assert_eq!(refreshed.field_index(edited), scanned.field_index(edited));
}

// ---- edit agreement -----------------------------------------------------------------------------

/// Deletes a random range of `Final` by its spans. `None` if the range cannot be edited in place.
fn delete_range(story: &Story, tree: &SpanTree, i: usize, rng: &mut Rng) -> Option<()> {
    let para = story.paragraph(tree, i)?;
    let m = para.map();
    let total = m.len(FINAL);
    // The result of a `w:fldSimple` that Word shows even when empty (`PAGE`, `QUOTE`) gets a value
    // atom once its last item is gone (4.4), so deleting the whole of it adds a character. Such a
    // result is the caller's policy, as the field value atom is, and the sweep leaves it alone.
    let simple = m.fields().iter().any(|f| f.simple);
    let editable = |s: &Span| {
        let item = &m.items()[s.item];
        item.flags & CDATA == 0
            && item.kind != ItemKind::FieldValue
            && !(simple && item.flags & FIELD_RESULT != 0)
    };
    // a few tries for a range that can be edited in place
    let (a, b, located) = (0..6).find_map(|_| {
        let a = rng.below(total + 1);
        let b = a + rng.below((total - a).min(14) + 1);
        let located = m.locate(FINAL, a..b)?;
        (!located.spans.is_empty() && located.spans.iter().all(editable)).then_some((a, b, located))
    })?;
    let mut edited = tree.clone();
    {
        let mut ed = edited.edit();
        for s in located.spans.iter().rev() {
            if m.items()[s.item].kind == ItemKind::Text {
                ed.replace_text_range(s.node, s.start as usize, s.end as usize, "")
                    .unwrap();
            } else {
                ed.detach(s.node).unwrap();
            }
        }
    }
    let scanned = scan(&edited);
    let after = map_of(&scanned, &edited, para.id());
    for mode in Mode::ALL {
        let at = starts(&m, mode);
        let ranges: Vec<Range<usize>> = located
            .spans
            .iter()
            .filter(|s| m.items()[s.item].vis.in_mode(mode))
            .map(|s| {
                let from = at[s.item] + (s.start - m.items()[s.item].from) as usize;
                from..from + (s.end - s.start) as usize
            })
            .collect();
        assert_eq!(
            after.text(mode),
            without(&m.text(mode), &ranges),
            "{mode:?} delete {a}..{b}"
        );
        // an item that was not touched is where the deletion leaves it
        let touched: HashSet<NodeId> = located.spans.iter().map(|s| s.node).collect();
        for (j, item) in firsts(&m)
            .into_iter()
            .filter(|(_, it)| !touched.contains(&it.node))
        {
            let want = shifted(at[j], &ranges);
            assert_eq!(
                after.offset_of(mode, item.node, 0),
                Some(want),
                "{mode:?} {item:?}"
            );
        }
    }
    assert_eq!(fields_of(story, tree), fields_of(&scanned, &edited));
    check_refresh(story, &edited, para.id(), &scanned);
    Some(())
}

fn fields_of(
    story: &Story,
    tree: &SpanTree,
) -> Vec<(NodeId, Option<NodeId>, Option<NodeId>, String)> {
    story
        .fields(tree)
        .iter()
        .map(|f| (f.begin, f.separate, f.end, f.instr.clone()))
        .collect()
}

/// Inserts text at a random gap of `Final`, into the text item beside it. `None` if there is none.
fn insert_at_gap(story: &Story, tree: &SpanTree, i: usize, rng: &mut Rng) -> Option<()> {
    let para = story.paragraph(tree, i)?;
    let m = para.map();
    let total = m.len(FINAL);
    let a = rng.below(total + 1);
    let left = (a > 0).then(|| m.locate(FINAL, a - 1..a).unwrap().spans[0]);
    let right = (a < total).then(|| m.locate(FINAL, a..a + 1).unwrap().spans[0]);
    let usable = |s: &Span| {
        let item = &m.items()[s.item];
        item.kind == ItemKind::Text && item.flags & CDATA == 0
    };
    let (target, at_in_node) = match (left, right) {
        (Some(l), _) if usable(&l) => (l, l.end),
        (_, Some(r)) if usable(&r) => (r, r.start),
        _ => return None,
    };
    let payload = *rng.pick(&["XY", "\u{1F600}&<z", " q\tr", "\u{10000}"]);
    let mut edited = tree.clone();
    edited
        .edit()
        .replace_text_range(
            target.node,
            at_in_node as usize,
            at_in_node as usize,
            payload,
        )
        .unwrap();
    let scanned = scan(&edited);
    let after = map_of(&scanned, &edited, para.id());
    let added = payload.chars().count();
    for mode in Mode::ALL {
        let at = starts(&m, mode);
        let item = &m.items()[target.item];
        let mut chars: Vec<char> = m.text(mode).chars().collect();
        let included = item.vis.in_mode(mode);
        if included {
            let pos = at[target.item] + (at_in_node - item.from) as usize;
            chars.splice(pos..pos, payload.chars());
        }
        assert_eq!(
            after.text(mode),
            chars.iter().collect::<String>(),
            "{mode:?} insert at {a}"
        );
        // items before the target in XML order keep their place, and those after it move by the text
        for (j, other) in firsts(&m) {
            let want = at[j]
                + if included && j > target.item {
                    added
                } else {
                    0
                };
            assert_eq!(
                after.offset_of(mode, other.node, 0),
                Some(want),
                "{mode:?} {other:?}"
            );
        }
    }
    check_refresh(story, &edited, para.id(), &scanned);
    Some(())
}

// ---- the tests ----------------------------------------------------------------------------------

pub(super) type Op = fn(&Story, &SpanTree, usize, &mut Rng) -> Option<()>;

/// Runs an operation on generated paragraphs and returns how many it applied to.
pub(super) fn sweep(seeds: Range<u64>, paragraphs: usize, tries: usize, op: Op) -> usize {
    let mut applied = 0;
    for_each_paragraph(seeds, paragraphs, |story, tree, i, _, xml| {
        let mut rng = Rng::new(i as u64 * 7919 + xml.len() as u64);
        for _ in 0..tries {
            applied += usize::from(op(story, tree, i, &mut rng).is_some());
        }
    });
    applied
}

#[test]
fn deleting_the_spans_of_a_random_range_removes_that_text_in_every_mode() {
    let applied = sweep(1..16, 100, 3, delete_range);
    assert!(applied > 1500, "applied {applied} times");
}

#[test]
fn inserting_at_a_random_gap_puts_the_text_there_in_every_mode() {
    let applied = sweep(20..35, 100, 3, insert_at_gap);
    assert!(applied > 1500, "applied {applied} times");
}
