//! A paragraph mark can hold more than one tracked change, and it is gone in every mode that one of
//! them removes it from. An insertion by one author that another deleted is a `w:ins` and a `w:del`
//! side by side, whichever comes first; the corpus has 19 such marks in 4 documents. Reading one
//! revision per mark gave the wrong answer for the second one.

use super::testing::document;
use super::tests_text::{FINAL, ORIGINAL, REDLINE};
use super::*;
use crate::xml::SpanTree;
use std::sync::Arc;

const KINDS: [&str; 4] = ["ins", "del", "moveFrom", "moveTo"];

/// The `w:pPr` of a paragraph whose mark holds these revisions, in this order, and `extra` after
/// them inside the `w:pPr`.
fn ppr(marks: &[&str], extra: &str) -> String {
    let revs: String = marks
        .iter()
        .enumerate()
        .map(|(i, m)| {
            format!(
                "<w:{m} w:id=\"{}\" w:author=\"a\" w:date=\"2020-01-01T00:00:00Z\"/>",
                i + 1
            )
        })
        .collect();
    format!("<w:pPr><w:rPr>{revs}</w:rPr>{extra}</w:pPr>")
}

fn p(marks: &[&str], text: &str) -> String {
    format!("<w:p>{}<w:r><w:t>{text}</w:t></w:r></w:p>", ppr(marks, ""))
}

fn scan(body: &str) -> (Story, SpanTree) {
    let tree = document(body);
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    (story, tree)
}

#[test]
fn a_mark_that_was_inserted_and_deleted_joins_in_both_modes_whichever_comes_first() {
    // Regression: with the insertion first, `Final` did not join, because
    // only the first revision was read; with the deletion first, `Original` did not.
    for marks in [["ins", "del"], ["del", "ins"]] {
        let (story, tree) = scan(&format!("{}{}", p(&marks, "one "), p(&[], "two")));
        assert_eq!(story.join_run(&tree, 0, FINAL), 0..2, "{marks:?}");
        assert_eq!(story.join_run(&tree, 0, ORIGINAL), 0..2, "{marks:?}");
        assert_eq!(story.join_run(&tree, 0, REDLINE), 0..1, "{marks:?}");
        for mode in [FINAL, ORIGINAL] {
            let (run, joined) = story.joined(&tree, 0, mode).unwrap();
            assert_eq!(
                (run, joined.text()),
                (0..2, "one two"),
                "{marks:?} {mode:?}"
            );
        }
    }
}

#[test]
fn a_mark_that_was_moved_away_and_moved_in_joins_in_both_modes() {
    for marks in [["moveFrom", "moveTo"], ["moveTo", "moveFrom"]] {
        let (story, tree) = scan(&format!("{}{}", p(&marks, "a"), p(&[], "b")));
        assert_eq!(story.join_run(&tree, 0, FINAL), 0..2, "{marks:?}");
        assert_eq!(story.join_run(&tree, 0, ORIGINAL), 0..2, "{marks:?}");
    }
}

#[test]
fn every_combination_of_revisions_on_a_mark_is_read_the_same_way_by_every_reader() {
    // Proves: for each subset of the four kinds, in the order written and reversed, the scan's flags,
    // the paragraph, the facts for numbering and the section all say the same, and it is the rule: a
    // mark is gone in `Final` if it is deleted or moved away, in `Original` if it is inserted or
    // moved in, and in `Redline` never.
    for subset in 0u8..16 {
        let names: Vec<&str> = KINDS
            .iter()
            .enumerate()
            .filter(|(i, _)| subset & (1 << i) != 0)
            .map(|(_, k)| *k)
            .collect();
        let mut reversed = names.clone();
        reversed.reverse();
        for marks in [names, reversed] {
            let at = format!("{marks:?}");
            let section = format!("<w:p>{}</w:p>", ppr(&marks, "<w:sectPr/>"));
            let (story, tree) = scan(&format!(
                "{}{}{section}{}<w:sectPr/>",
                p(&marks, "a"),
                p(&[], "b"),
                p(&[], "c")
            ));
            let expected = MarkRevs {
                inserted: subset & 1 != 0,
                deleted: subset & 2 != 0,
                moved_from: subset & 4 != 0,
                moved_to: subset & 8 != 0,
            };
            let gone_final = expected.deleted || expected.moved_from;
            let gone_original = expected.inserted || expected.moved_to;
            assert_eq!(expected.is_none(), subset == 0, "{at}");
            let gone = |m: &MarkRevs| (m.gone_in(FINAL), m.gone_in(ORIGINAL), m.gone_in(REDLINE));
            assert_eq!(gone(&expected), (gone_final, gone_original, false), "{at}");
            assert_eq!(story.blocks()[0].mark_revs(), expected, "{at}");
            let para = story.paragraph(&tree, 0).unwrap();
            assert_eq!(para.mark_revs(), expected, "{at}");
            assert_eq!(
                (
                    para.joins_next(FINAL),
                    para.joins_next(ORIGINAL),
                    para.joins_next(REDLINE)
                ),
                (gone_final, gone_original, false),
                "{at}"
            );
            assert_eq!(story.join_run(&tree, 0, FINAL).end == 2, gone_final, "{at}");
            assert_eq!(
                story.join_run(&tree, 0, ORIGINAL).end == 2,
                gone_original,
                "{at}"
            );
            assert_eq!(story.join_run(&tree, 0, REDLINE), 0..1, "{at}");
            let facts = story.num_facts(&tree);
            assert_eq!(facts[0].mark_revs, expected, "{at}");
            assert!(facts[0].join_applies, "{at}");
            let sections = story.sections(&tree, &Rels::default());
            assert_eq!(sections.len(), 2, "{at}");
            assert_eq!(sections[0].mark_revs, expected, "{at}");
            assert_eq!(
                (
                    sections[0].exists_in(FINAL),
                    sections[0].exists_in(ORIGINAL),
                    sections[0].exists_in(REDLINE)
                ),
                (!gone_final, !gone_original, true),
                "{at}"
            );
            assert!(
                sections[1].exists_in(FINAL) && sections[1].exists_in(ORIGINAL),
                "{at}"
            );
        }
    }
}

#[test]
fn a_mark_in_a_snapshot_of_old_properties_is_not_a_revision_of_the_mark() {
    // Proves: the revisions of the old properties inside a `w:pPrChange` are a record of what was
    // replaced, not a tracked change of this mark, as the `w:pPr` there is not the paragraph's own.
    let (story, tree) = scan(
        "<w:p><w:pPr><w:pPrChange w:id=\"1\" w:author=\"a\"><w:pPr><w:rPr><w:del w:id=\"2\"/></w:rPr>\
         </w:pPr></w:pPrChange></w:pPr><w:r><w:t>a</w:t></w:r></w:p><w:p/>",
    );
    assert!(story.blocks()[0].mark_revs().is_none());
    assert!(story.paragraph(&tree, 0).unwrap().mark_revs().is_none());
}
