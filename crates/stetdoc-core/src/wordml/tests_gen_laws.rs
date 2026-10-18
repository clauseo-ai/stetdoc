//! The invariants of `docs/design/wordml.md` 5.2 as properties, checked on generated paragraphs
//! (`tests_gen.rs`) and, through `check_laws`, on any paragraph of any tree: items 1 to 7 and 10 to
//! 12 and the UTF-16 round trip. `check_invariants` of `tests_text.rs` does the checks that need no
//! tree (partition, mode text, field code); this file adds the ones that read the XML back.
//!
//! Items 8 and 9 (edit agreement and insensitivity) are in `tests_meta.rs`.

use super::testing::document;
use super::tests_gen::{Gen, mode_index};
use super::tests_gen_data::{HIDDEN, Rng};
use super::tests_gen_oracle::{is_w, nearest_run, wrappers_above};
use super::tests_text::{FINAL, ORIGINAL, REDLINE, check_invariants};
use super::*;
use crate::xml::{NodeKind, NodeRef, SpanTree};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// The first and last pre-order position of every node at or below a paragraph.
fn positions(tree: &SpanTree, p: NodeId) -> HashMap<NodeId, (usize, usize)> {
    let mut pos = HashMap::new();
    pos.insert(p, (0, 0));
    let mut n = 1usize;
    let mut stack = vec![(p, tree.node_ref(p).children())];
    while let Some((id, it)) = stack.last_mut() {
        if let Some(c) = it.next() {
            pos.insert(c.id(), (n, n));
            n += 1;
            stack.push((c.id(), c.children()));
        } else {
            let id = *id;
            stack.pop();
            if let Some(e) = pos.get_mut(&id) {
                e.1 = n - 1;
            }
        }
    }
    pos
}

/// The element names an item kind may be made from.
fn names_of(kind: ItemKind) -> &'static [&'static str] {
    use ItemKind::*;
    match kind {
        Tab => &["tab", "ptab"],
        LineBreak => &["br", "cr"],
        PageBreak | ColumnBreak => &["br"],
        Hyphen => &["noBreakHyphen"],
        SoftHyphen => &["softHyphen"],
        Symbol => &["sym", "symEx"],
        Object => &["drawing", "pict", "object"],
        NoteRef => &["footnoteReference", "endnoteReference"],
        NoteMark => &["footnoteRef", "endnoteRef"],
        Math => &["oMath", "oMathPara"],
        Auto => &[
            "pgNum",
            "dayShort",
            "dayLong",
            "monthShort",
            "monthLong",
            "yearShort",
            "yearLong",
        ],
        FieldValue | FieldBegin | FieldEnd => &["fldChar", "fldSimple"],
        FieldSeparate => &["fldChar"],
        BookmarkStart => &["bookmarkStart"],
        BookmarkEnd => &["bookmarkEnd"],
        CommentStart => &["commentRangeStart"],
        CommentEnd => &["commentRangeEnd"],
        CommentRef => &["commentReference"],
        PermStart => &["permStart"],
        PermEnd => &["permEnd"],
        MoveRangeStart => &["moveFromRangeStart", "moveToRangeStart"],
        MoveRangeEnd => &["moveFromRangeEnd", "moveToRangeEnd"],
        Text | FieldCode => &[],
    }
}

/// Items 1 and 3 and 12: order, uniqueness, what each item is made from, and the wrappers.
pub(super) fn check_items(tree: &SpanTree, p: NodeId, m: &TextMap) {
    let pos = positions(tree, p);
    let mut seen = HashSet::new();
    let mut last = 0usize;
    for item in m.items() {
        assert!(seen.insert((item.kind, item.node)), "duplicate {item:?}");
        let node = tree.node_ref(item.node);
        let (first, end) = *pos
            .get(&item.node)
            .unwrap_or_else(|| panic!("{item:?} is outside the paragraph"));
        // a field value that field code demoted to a mark keeps the element, so it exits with it
        let exits =
            node.is_element() && is_w(&node, "fldSimple") && item.kind != ItemKind::FieldBegin;
        let key = if exits { end } else { first };
        assert!(key >= last, "items out of document order at {item:?}");
        last = key;
        if item.kind == ItemKind::Text {
            let cdata = node.kind() == NodeKind::CData;
            assert!(
                matches!(node.kind(), NodeKind::Text | NodeKind::CData),
                "{item:?}"
            );
            assert_eq!(item.flags & CDATA != 0, cdata, "{item:?}");
            let decoded: String = node
                .text()
                .chars()
                .skip(item.from as usize)
                .take(item.len as usize)
                .collect();
            assert_eq!(decoded, m.slice(item), "faithful: {item:?}");
        } else if item.kind != ItemKind::FieldCode {
            assert!(
                names_of(item.kind).contains(&node.local_name()),
                "{:?} made from {}",
                item.kind,
                node.local_name()
            );
        }
        // 12: the wrappers decide the modes, and the context chain is the chain of wrappers
        let above = wrappers_above(tree, item);
        let chain: Vec<WrapperKind> = m.context(item.ctx).map(|c| c.kind).collect();
        assert_eq!(chain, above, "the context chain of {item:?}");
        assert_eq!(
            item.vis.in_mode(FINAL),
            !above.iter().any(|k| k.is_deletion()),
            "{item:?}"
        );
        assert_eq!(
            item.vis.in_mode(ORIGINAL),
            !above.iter().any(|k| k.is_insertion()),
            "{item:?}"
        );
        assert!(item.vis.in_mode(REDLINE));
        assert_eq!(item.run, nearest_run(tree, item), "run of {item:?}");
    }
}

/// Items 5, 6 and 7 for one mode: forward, back and the round trip, and `gap`.
fn check_mode(tree: &SpanTree, m: &TextMap, mode: Mode, rng: &mut Rng) {
    let items = m.items();
    let holds = |i: &Item| i.len > 0 && i.vis.in_mode(mode);
    let mut starts = Vec::with_capacity(items.len());
    let mut at = 0usize;
    for item in items {
        starts.push(at);
        if item.vis.in_mode(mode) {
            at += item.len as usize;
        }
    }
    let total = at;
    let chars: Vec<char> = m.text(mode).chars().collect();
    assert_eq!(total, chars.len());
    assert_eq!(total, m.len(mode));
    // forward: every character comes from exactly one item and one place in it
    let mut owner = vec![None; total];
    for (i, item) in items.iter().enumerate().filter(|(_, it)| holds(it)) {
        for k in 0..item.len as usize {
            let slot = &mut owner[starts[i] + k];
            assert!(
                slot.is_none(),
                "two items produce character {}",
                starts[i] + k
            );
            *slot = Some(i);
        }
    }
    for (c, owner) in owner.iter().enumerate() {
        let i = owner.unwrap_or_else(|| panic!("no item produces character {c}"));
        let item = &items[i];
        let located = m.locate(mode, c..c + 1).unwrap();
        assert_eq!(located.spans.len(), 1, "{mode:?} {c}");
        let span = located.spans[0];
        let k = (c - starts[i]) as u32;
        assert_eq!((span.item, span.node), (i, item.node));
        assert_eq!((span.start, span.end), (item.from + k, item.from + k + 1));
        assert_eq!(span.whole, item.len == 1);
        let ch = if item.kind == ItemKind::Text {
            tree.node_ref(item.node)
                .text()
                .chars()
                .nth(span.start as usize)
        } else {
            m.slice(item).chars().next()
        };
        assert_eq!(ch, Some(chars[c]), "the character of {item:?}");
        // back, and the round trip
        assert_eq!(m.offset_of(mode, span.node, span.start as usize), Some(c));
    }
    // back from every position inside an included item, and from every boundary
    for (i, item) in items.iter().enumerate().filter(|(_, it)| holds(it)) {
        if item.kind == ItemKind::Text {
            for p in 0..=item.len {
                let want = starts[i] + p as usize;
                assert_eq!(
                    m.offset_of(mode, item.node, (item.from + p) as usize),
                    Some(want)
                );
            }
        }
    }
    let mut node_seen = HashSet::new();
    for (i, item) in items.iter().enumerate() {
        if node_seen.insert(item.node) {
            assert_eq!(
                m.offset_of(mode, item.node, 0),
                Some(starts[i]),
                "offset of {item:?}"
            );
        }
    }
    // ranges: the spans cover exactly the characters asked for
    for _ in 0..4 {
        let a = rng.below(total + 1);
        let b = a + rng.below(total - a + 1);
        let located = m.locate(mode, a..b).unwrap();
        let mut got = String::new();
        let mut width = 0usize;
        let mut prev = None;
        for span in &located.spans {
            assert!(prev.is_none_or(|q| q < span.item), "spans out of order");
            prev = Some(span.item);
            let item = &items[span.item];
            assert!(holds(item) && span.start >= item.from && span.end <= item.from + item.len);
            width += (span.end - span.start) as usize;
            let node_text: Vec<char> = if item.kind == ItemKind::Text {
                tree.node_ref(item.node).text().chars().collect()
            } else {
                m.slice(item).chars().collect()
            };
            let base = if item.kind == ItemKind::Text {
                0
            } else {
                item.from as usize
            };
            got.extend(&node_text[span.start as usize - base..span.end as usize - base]);
            assert_eq!(span.whole, span.end - span.start == item.len);
        }
        assert_eq!(width, b - a);
        assert_eq!(got, chars[a..b].iter().collect::<String>());
        match (located.spans.first(), located.spans.last()) {
            (Some(f), Some(l)) => assert_eq!(located.items, f.item..l.item + 1),
            _ => assert!(located.spans.is_empty() && a == b),
        }
    }
    assert!(m.locate(mode, 0..total + 1).is_none());
    let (hi, lo) = (total.max(1), 0);
    assert!(hi > lo && m.locate(mode, hi..lo).is_none());
    // gaps
    for c in 0..=total {
        let g = m.gap(mode, c).unwrap();
        let left = (c > 0).then(|| owner[c - 1]).flatten();
        let right = (c < total).then(|| owner[c]).flatten();
        assert_eq!((g.left, g.right), (left, right), "gap {mode:?} {c}");
        let n = items.len();
        let between = match (left, right) {
            (Some(l), Some(r)) if l == r => l + 1..l + 1,
            (Some(l), Some(r)) => l + 1..r,
            (Some(l), None) => l + 1..n,
            (None, Some(r)) => 0..r,
            (None, None) => 0..n,
        };
        assert_eq!(g.between, between, "gap {mode:?} {c}");
        assert!(
            items[between].iter().all(|i| !holds(i)),
            "an included item is between"
        );
        // the conversion to UTF-16 and back
        let u = m.char_to_utf16(mode, c).unwrap();
        let expect: usize = chars[..c].iter().map(|ch| ch.len_utf16()).sum();
        assert_eq!(u, expect);
        assert_eq!(m.utf16_to_char(mode, u), Some(c));
    }
    assert!(m.gap(mode, total + 1).is_none());
    assert!(m.char_to_utf16(mode, total + 1).is_none());
    let units: usize = chars.iter().map(|ch| ch.len_utf16()).sum();
    for u in 0..=units + 1 {
        let back = m.utf16_to_char(mode, u);
        let on_boundary = back.is_some();
        let expect =
            (0..=total).find(|&c| chars[..c].iter().map(|ch| ch.len_utf16()).sum::<usize>() == u);
        assert_eq!(back, expect, "utf16 {u}");
        assert!(u > units || on_boundary || chars.iter().any(|ch| ch.len_utf16() == 2));
    }
}

/// Items 1 to 7, 11 and 12 and the UTF-16 round trip, for the map of the paragraph `p`, on any tree.
pub(super) fn check_laws(tree: &SpanTree, p: NodeId, m: &TextMap, rng: &mut Rng) {
    check_invariants(m);
    check_items(tree, p, m);
    for mode in Mode::ALL {
        check_mode(tree, m, mode, rng);
    }
}

/// `check_laws`, and that no word of the generator that must stay hidden reached a text. Only for a
/// tree the generator made and nothing moved.
pub(super) fn check_generated(tree: &SpanTree, p: NodeId, m: &TextMap, rng: &mut Rng) {
    check_laws(tree, p, m, rng);
    for mode in Mode::ALL {
        let text = m.text(mode);
        for word in HIDDEN {
            assert!(
                !text.contains(word),
                "{word} reached the {mode:?} text {text:?}"
            );
        }
    }
}

/// Item 10: the map is a function of the tree, and reading changes nothing.
pub(super) fn check_pure(story: &Story, tree: &SpanTree, i: usize) {
    let before = tree.serialize();
    let first = story.paragraph(tree, i).unwrap().map();
    let second = story.paragraph(tree, i).unwrap().map();
    assert_eq!(format!("{first:?}"), format!("{second:?}"));
    assert_eq!(tree.serialize(), before, "reading changed the tree");
    assert!(!tree.is_dirty());
    // the same bytes give the same map: a re-parse of the serialized tree
    let again = super::testing::parse("word/document.xml", &String::from_utf8_lossy(&before));
    let story2 = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &again).unwrap();
    let third = story2.paragraph(&again, i).unwrap().map();
    let digest = |m: &TextMap| {
        let items: Vec<_> = m
            .items()
            .iter()
            .map(|x| (x.kind, x.at, x.len, x.from, x.vis, x.field, x.flags))
            .collect();
        (items, Mode::ALL.map(|mode| m.text(mode).into_owned()))
    };
    assert_eq!(digest(&first), digest(&third));
}

/// Generates `paragraphs` paragraphs for each of the seeds and calls `check` on each, with the story,
/// the tree, the entry index, the expected texts and the generated XML.
pub(super) fn for_each_paragraph(
    seeds: std::ops::Range<u64>,
    paragraphs: usize,
    mut check: impl FnMut(&Story, &SpanTree, usize, &[String; 3], &str),
) {
    for seed in seeds {
        let mut generator = Gen::new(seed);
        let mut done = 0;
        while done < paragraphs {
            let body = generator.body(3);
            let tree = document(&body.xml);
            let story =
                Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
            let entries: Vec<usize> = (0..story.blocks().len())
                .filter(|&i| story.paragraph(&tree, i).is_some())
                .collect();
            assert_eq!(entries.len(), body.expect.len(), "{}", body.xml);
            for (i, expect) in entries.into_iter().zip(&body.expect) {
                let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    check(&story, &tree, i, expect, &body.xml)
                }));
                if let Err(e) = run {
                    eprintln!("seed {seed}, paragraph {i} of: {}", body.xml);
                    std::panic::resume_unwind(e);
                }
            }
            done += body.expect.len();
        }
    }
}

#[test]
fn a_generated_paragraph_has_the_text_it_was_written_with_in_every_mode() {
    let mut count = 0;
    for_each_paragraph(1..21, 150, |story, tree, i, expect, xml| {
        let m = story.paragraph(tree, i).unwrap().map();
        for mode in Mode::ALL {
            assert_eq!(
                &m.text(mode),
                &expect[mode_index(mode)],
                "{mode:?} of {xml}"
            );
        }
        count += 1;
    });
    assert!(count >= 3000);
}

#[test]
fn the_invariants_hold_for_generated_paragraphs() {
    for_each_paragraph(100..120, 120, |story, tree, i, _, xml| {
        let p = story.paragraph(tree, i).unwrap();
        let m = p.map();
        let mut rng = Rng::new(i as u64 + xml.len() as u64);
        check_generated(tree, p.id(), &m, &mut rng);
        check_pure(story, tree, i);
    });
}

#[test]
fn the_generator_reaches_every_shape_it_is_meant_to() {
    let mut kinds = HashSet::new();
    let mut wrappers = HashSet::new();
    let mut anomalies = HashSet::new();
    let (mut revised, mut spanning, mut no_sep, mut astral, mut cdata) = (0, 0, 0, 0, 0);
    for_each_paragraph(200..230, 100, |story, tree, i, _, _| {
        let m = story.paragraph(tree, i).unwrap().map();
        kinds.extend(m.items().iter().map(|x| x.kind));
        anomalies.extend(m.anomalies().iter().map(|a| a.kind));
        for item in m.items() {
            wrappers.extend(m.context(item.ctx).map(|c| c.kind));
            cdata += usize::from(item.flags & CDATA != 0);
        }
        revised += usize::from(m.has_revisions());
        spanning += usize::from(m.fields().iter().any(|f| !f.begins_here));
        no_sep += usize::from(
            m.fields()
                .iter()
                .any(|f| f.end.is_some() && f.separate.is_none() && !f.simple),
        );
        astral += usize::from(m.text(REDLINE).chars().any(|c| c.len_utf16() == 2));
    });
    for kind in [
        ItemKind::Text,
        ItemKind::Tab,
        ItemKind::LineBreak,
        ItemKind::PageBreak,
        ItemKind::ColumnBreak,
        ItemKind::Hyphen,
        ItemKind::SoftHyphen,
        ItemKind::Symbol,
        ItemKind::Object,
        ItemKind::NoteRef,
        ItemKind::Math,
        ItemKind::Auto,
        ItemKind::FieldValue,
        ItemKind::BookmarkStart,
        ItemKind::BookmarkEnd,
        ItemKind::CommentStart,
        ItemKind::CommentEnd,
        ItemKind::CommentRef,
        ItemKind::FieldBegin,
        ItemKind::FieldSeparate,
        ItemKind::FieldEnd,
        ItemKind::FieldCode,
        ItemKind::PermStart,
        ItemKind::PermEnd,
        ItemKind::MoveRangeStart,
        ItemKind::MoveRangeEnd,
    ] {
        assert!(kinds.contains(&kind), "the generator never makes {kind:?}");
    }
    for wrapper in [
        WrapperKind::Ins,
        WrapperKind::Del,
        WrapperKind::MoveFrom,
        WrapperKind::MoveTo,
        WrapperKind::RowIns,
        WrapperKind::RowDel,
        WrapperKind::CellIns,
        WrapperKind::CellDel,
        WrapperKind::Hyperlink,
        WrapperKind::FieldSimple,
        WrapperKind::SmartTag,
        WrapperKind::CustomXml,
        WrapperKind::Control,
        WrapperKind::Dir,
        WrapperKind::Bdo,
        WrapperKind::Alternate,
        WrapperKind::Ruby,
    ] {
        assert!(
            wrappers.contains(&wrapper),
            "the generator never makes {wrapper:?}"
        );
    }
    for anomaly in [
        AnomalyKind::UnknownElement,
        AnomalyKind::NoBranch,
        AnomalyKind::InvalidSymbol,
    ] {
        assert!(anomalies.contains(&anomaly), "no {anomaly:?}");
    }
    assert!(revised > 200 && spanning > 20 && no_sep > 20 && astral > 200 && cdata > 100);
}
