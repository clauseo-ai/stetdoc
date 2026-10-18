//! Tests of the map operations: `locate`, `gap`, `offset_of`, `find_all` and the UTF-16 helpers.
//! Known answers first (the three examples of `docs/design/wordml.md` 5.3), then the invariants of 5.2
//! checked over a set of paragraphs in every mode, including agreement with the edit primitive.

use super::testing::document;
use super::tests_fields::{begin, end, instr, separate, text};
use super::tests_text::{FINAL, ORIGINAL, REDLINE, kinds, para};
use super::*;
use crate::xml::SpanTree;
use std::sync::Arc;

fn run(t: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{t}</w:t></w:r>")
}

fn del(t: &str) -> String {
    format!(
        "<w:del w:id=\"9\"><w:r><w:delText xml:space=\"preserve\">{t}</w:delText></w:r></w:del>"
    )
}

fn ins(t: &str) -> String {
    format!("<w:ins w:id=\"8\">{}</w:ins>", run(t))
}

// ---- known answers ------------------------------------------------------------------------------

#[test]
fn a_range_across_runs_is_one_span_in_each() {
    let m = para(&format!(
        "{}{}",
        run("Sell"),
        "<w:r><w:rPr><w:b/></w:rPr><w:t>er</w:t></w:r>"
    ));
    let at = m.locate(FINAL, 2..6).unwrap();
    assert_eq!(at.spans.len(), 2);
    let (a, b) = (&at.spans[0], &at.spans[1]);
    assert_eq!((a.start, a.end, a.whole), (2, 4, false));
    assert_eq!((b.start, b.end, b.whole), (0, 2, true));
    assert_ne!(a.node, b.node);
    assert_eq!(at.items, 0..2);
}

#[test]
fn a_range_across_a_field_has_the_whole_result_and_the_marks_between() {
    let m = para(&format!(
        "{}{}{}{}{}{}{}",
        text("see section "),
        begin(),
        instr(" REF _Ref1 "),
        separate(),
        text("4.2"),
        end(),
        text(" above")
    ));
    let at = m.locate(FINAL, 4..16).unwrap();
    assert_eq!(at.spans.len(), 3);
    assert_eq!((at.spans[0].start, at.spans[0].end), (4, 12));
    assert!(!at.spans[0].whole && at.spans[1].whole && !at.spans[2].whole);
    assert_eq!((at.spans[2].start, at.spans[2].end), (0, 1));
    assert_eq!(
        m.items()[at.spans[1].item].flags & FIELD_RESULT,
        FIELD_RESULT
    );
    let between: Vec<ItemKind> = m.items()[at.items.clone()].iter().map(|i| i.kind).collect();
    use ItemKind::*;
    assert_eq!(
        between,
        [
            Text,
            FieldBegin,
            FieldCode,
            FieldSeparate,
            Text,
            FieldEnd,
            Text
        ]
    );
}

#[test]
fn across_a_revision_boundary_the_gap_sees_the_deleted_run() {
    let m = para(&format!(
        "{}{}{}{}{}",
        run("A "),
        ins("B"),
        run(" "),
        del("C"),
        run("D")
    ));
    assert_eq!(m.text(FINAL), "A B D");
    assert_eq!(m.text(ORIGINAL), "A  CD");
    assert_eq!(m.text(REDLINE), "A B CD");
    let g = m.gap(FINAL, 4).unwrap();
    assert_eq!((g.left, g.right), (Some(2), Some(4)));
    assert_eq!(g.between, 3..4, "the deleted C is between the space and D");
    assert_eq!(m.items()[3].kind, ItemKind::Text);
    let b = m.locate(FINAL, 2..3).unwrap();
    assert_eq!(b.spans.len(), 1);
    assert_eq!(
        m.context(m.items()[b.spans[0].item].ctx)
            .next()
            .unwrap()
            .kind,
        WrapperKind::Ins
    );
    // in Original the inserted B is not there and the deleted C is
    let c = m.locate(ORIGINAL, 3..4).unwrap();
    assert_eq!(c.spans[0].item, 3);
}

#[test]
fn locate_refuses_a_bad_range_and_an_empty_range_has_no_spans() {
    let m = para(&run("abc"));
    let (a, b) = (3usize, 2usize);
    assert!(m.locate(FINAL, a..b).is_none());
    assert!(m.locate(FINAL, 0..4).is_none());
    assert!(m.locate(FINAL, 4..4).is_none());
    let empty = m.locate(FINAL, 1..1).unwrap();
    assert!(empty.spans.is_empty() && empty.items.is_empty());
    let whole = m.locate(FINAL, 0..3).unwrap();
    assert!(whole.spans[0].whole);
    let none = para("");
    assert!(none.locate(FINAL, 0..0).unwrap().spans.is_empty());
    assert!(none.gap(FINAL, 0).is_some() && none.gap(FINAL, 1).is_none());
}

#[test]
fn locate_covers_atoms_with_zero_to_one_and_skips_marks() {
    let m = para(&format!(
        "{}<w:r><w:tab/></w:r><w:bookmarkStart w:id=\"1\" w:name=\"b\"/>{}",
        run("a"),
        run("b")
    ));
    assert_eq!(m.text(FINAL), "a\tb");
    let at = m.locate(FINAL, 1..2).unwrap();
    assert_eq!(at.spans.len(), 1);
    assert_eq!(
        (at.spans[0].start, at.spans[0].end, at.spans[0].whole),
        (0, 1, true)
    );
    assert_eq!(m.items()[at.spans[0].item].kind, ItemKind::Tab);
    let g = m.gap(FINAL, 2).unwrap();
    assert_eq!(kinds(&m)[g.between.start], ItemKind::BookmarkStart);
    assert_eq!(g.between.len(), 1);
}

#[test]
fn a_gap_inside_one_item_has_the_same_item_on_both_sides_and_nothing_between() {
    let m = para(&run("abc"));
    let g = m.gap(FINAL, 1).unwrap();
    assert_eq!((g.left, g.right, g.between.len()), (Some(0), Some(0), 0));
    let start = m.gap(FINAL, 0).unwrap();
    assert_eq!((start.left, start.right), (None, Some(0)));
    let finish = m.gap(FINAL, 3).unwrap();
    assert_eq!((finish.left, finish.right), (Some(0), None));
    assert!(m.gap(FINAL, 4).is_none());
}

#[test]
fn offset_of_goes_back_from_a_node_and_for_an_excluded_item_gives_where_it_would_sit() {
    let m = para(&format!("{}{}{}", run("ab"), del("XY"), run("cd")));
    let nodes: Vec<NodeId> = m.items().iter().map(|i| i.node).collect();
    assert_eq!(m.offset_of(FINAL, nodes[0], 1), Some(1));
    assert_eq!(m.offset_of(FINAL, nodes[2], 1), Some(3));
    assert_eq!(
        m.offset_of(FINAL, nodes[1], 0),
        Some(2),
        "the deleted run sits at 2 in Final"
    );
    assert_eq!(m.offset_of(FINAL, nodes[1], 1), Some(2));
    assert_eq!(m.offset_of(ORIGINAL, nodes[1], 1), Some(3));
    assert_eq!(m.offset_of(REDLINE, nodes[2], 0), Some(4));
    assert_eq!(
        m.offset_of(FINAL, nodes[0], 99),
        Some(2),
        "past the end of a node clamps to its end"
    );
    let tree = document("<w:p/>");
    assert_eq!(m.offset_of(FINAL, tree.root().id(), 0), None);
}

#[test]
fn find_all_returns_non_overlapping_character_ranges() {
    let m = para(&format!("{}{}", run("aaaa \u{1f600}ab"), run("ab")));
    assert_eq!(m.find_all(FINAL, "aa"), [0..2, 2..4]);
    assert_eq!(m.find_all(FINAL, "ab"), [6..8, 8..10]);
    assert_eq!(m.find_all(FINAL, "\u{1f600}"), vec![5..6]);
    assert!(m.find_all(FINAL, "").is_empty());
    assert!(m.find_all(FINAL, "zz").is_empty());
}

#[test]
fn find_all_sees_the_text_of_its_mode() {
    let m = para(&format!(
        "{}{}{}{}",
        run("one "),
        del("two "),
        ins("three "),
        run("four")
    ));
    assert_eq!(
        m.find_all(FINAL, "two"),
        Vec::<std::ops::Range<usize>>::new()
    );
    assert_eq!(m.find_all(ORIGINAL, "two"), vec![4..7]);
    assert_eq!(m.find_all(REDLINE, "four"), vec![14..18]);
    assert_eq!(m.find_all(FINAL, "three four"), vec![4..14]);
}

#[test]
fn utf16_offsets_count_an_astral_character_as_two() {
    let m = para(&run("a\u{1f600}b\u{10000}"));
    assert_eq!(m.len(FINAL), 4);
    let to16: Vec<Option<usize>> = (0..=5).map(|c| m.char_to_utf16(FINAL, c)).collect();
    assert_eq!(to16, [Some(0), Some(1), Some(3), Some(4), Some(6), None]);
    let to_char: Vec<Option<usize>> = (0..=7).map(|u| m.utf16_to_char(FINAL, u)).collect();
    assert_eq!(
        to_char,
        [
            Some(0),
            Some(1),
            None,
            Some(2),
            Some(3),
            None,
            Some(4),
            None
        ]
    );
    for c in 0..=m.len(FINAL) {
        assert_eq!(
            m.utf16_to_char(FINAL, m.char_to_utf16(FINAL, c).unwrap()),
            Some(c)
        );
    }
    assert_eq!(para("").char_to_utf16(FINAL, 0), Some(0));
    assert_eq!(para("").utf16_to_char(FINAL, 0), Some(0));
}

// ---- invariants over a set of paragraphs --------------------------------------------------------

fn corpus() -> Vec<String> {
    vec![
        String::new(),
        run("hello"),
        format!(
            "{}{}{}",
            run("a"),
            run("bc"),
            "<w:r><w:rPr><w:i/></w:rPr><w:t>def</w:t></w:r>"
        ),
        format!(
            "{}{}{}{}{}",
            run("A "),
            ins("B"),
            run(" "),
            del("C"),
            run("D")
        ),
        format!(
            "{}<w:r><w:tab/><w:t>x</w:t><w:br/></w:r>{}",
            run("t"),
            run("\u{1f600}\u{e9}")
        ),
        format!(
            "{}{}{}{}{}{}",
            run("see "),
            begin(),
            instr(" REF a "),
            separate(),
            text("4.2"),
            end() + &run(" ok")
        ),
        format!("{}{}", begin() + &instr(" PAGE ") + &end(), run("x")),
        format!(
            "<w:bookmarkStart w:id=\"1\" w:name=\"b\"/>{}<w:bookmarkEnd w:id=\"1\"/><w:hyperlink w:anchor=\"q\">{}</w:hyperlink>",
            run("mark"),
            run("link")
        ),
        format!("{}{}", del("gone"), del("also")),
        "<w:r><w:t><![CDATA[c<d]]></w:t><w:t>&amp;&#x41;</w:t></w:r>".to_string(),
        format!(
            "<w:moveFrom w:id=\"1\"><w:r><w:delText>m</w:delText></w:r></w:moveFrom><w:moveTo w:id=\"2\">{}</w:moveTo>",
            run("n")
        ),
    ]
}

#[test]
fn forward_back_and_round_trip_hold_in_every_mode() {
    for content in corpus() {
        let m = para(&content);
        for mode in Mode::ALL {
            let len = m.len(mode);
            let mut seen = 0;
            for c in 0..len {
                let at = m.locate(mode, c..c + 1).unwrap();
                assert_eq!(at.spans.len(), 1, "{content} {mode:?} {c}");
                let span = at.spans[0];
                assert!(span.whole == (m.items()[span.item].len == 1));
                let item = &m.items()[span.item];
                assert!(item.vis.in_mode(mode));
                let back = m.offset_of(mode, span.node, span.start as usize).unwrap();
                assert_eq!(back, c, "offset_of(locate(c)) == c: {content} {mode:?}");
                seen += 1;
            }
            assert_eq!(seen, len);
            for c in 0..=len {
                let g = m.gap(mode, c).unwrap();
                assert!(g.left.is_some() || c == 0);
                assert!(g.right.is_some() || c == len);
                if let (Some(l), Some(r)) = (g.left, g.right) {
                    assert!(l <= r && g.between.start <= g.between.end);
                    if l != r {
                        assert_eq!(g.between, l + 1..r);
                    }
                }
                let u = m.char_to_utf16(mode, c).unwrap();
                assert_eq!(m.utf16_to_char(mode, u), Some(c));
            }
            assert!(m.gap(mode, len + 1).is_none());
        }
    }
}

/// The Final text of the paragraph that is the first block of `tree`.
fn final_of(tree: &SpanTree) -> String {
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, tree).unwrap();
    story.paragraph(tree, 0).unwrap().text(FINAL)
}

#[test]
fn deleting_the_spans_of_a_range_removes_exactly_that_text_and_an_insert_lands_at_the_offset() {
    for content in corpus() {
        let tree = document(&format!("<w:p>{content}</w:p>"));
        let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
        let m = story.paragraph(&tree, 0).unwrap().map();
        if m.items().iter().any(|i| i.kind == ItemKind::FieldValue) {
            // deleting the value of a field is deleting the field, which is the caller's policy
            continue;
        }
        let editable = |span: &Span| m.items()[span.item].flags & CDATA == 0;
        let old: Vec<char> = m.text(FINAL).chars().collect();
        let len = old.len();
        for a in 0..=len {
            for b in a..=len {
                let at = m.locate(FINAL, a..b).unwrap();
                if !at.spans.iter().all(editable) {
                    continue; // replace_text_range edits a text node, not CDATA
                }
                let mut trial = tree.clone();
                let mut ed = trial.edit();
                for span in at.spans.iter().rev() {
                    let item = &m.items()[span.item];
                    if item.kind == ItemKind::Text {
                        ed.replace_text_range(
                            span.node,
                            span.start as usize,
                            span.end as usize,
                            "",
                        )
                        .unwrap();
                    } else {
                        ed.detach(span.node).unwrap();
                    }
                }
                let expected: String = old.iter().take(a).chain(old.iter().skip(b)).collect();
                assert_eq!(final_of(&trial), expected, "{content} delete {a}..{b}");
            }
            // an insert at the offset, into the text item that holds the character before or after
            let left = (a > 0).then(|| m.locate(FINAL, a - 1..a).unwrap().spans[0]);
            let right = (a < len).then(|| m.locate(FINAL, a..a + 1).unwrap().spans[0]);
            let target = match (left, right) {
                (Some(l), _) if m.items()[l.item].kind == ItemKind::Text => {
                    Some((l.node, l.end as usize))
                }
                (_, Some(r)) if m.items()[r.item].kind == ItemKind::Text => {
                    Some((r.node, r.start as usize))
                }
                _ => None,
            };
            if let Some((node, at_in_node)) =
                target.filter(|_| left.iter().chain(&right).all(editable))
            {
                let mut trial = tree.clone();
                trial
                    .edit()
                    .replace_text_range(node, at_in_node, at_in_node, "XY")
                    .unwrap();
                let expected: String = old
                    .iter()
                    .take(a)
                    .chain("XY".chars().collect::<Vec<_>>().iter())
                    .chain(old.iter().skip(a))
                    .collect();
                assert_eq!(final_of(&trial), expected, "{content} insert at {a}");
            }
        }
    }
}

#[test]
fn text_is_borrowed_and_equal_in_all_modes_when_there_is_no_revision() {
    let m = para(&run("same"));
    assert_eq!(m.find_all(FINAL, "am"), m.find_all(REDLINE, "am"));
    assert_eq!(m.locate(FINAL, 0..4), m.locate(ORIGINAL, 0..4));
}
