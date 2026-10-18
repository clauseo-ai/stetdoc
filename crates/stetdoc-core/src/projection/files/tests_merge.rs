//! Exact merges: law T7 (design 6.4, step 6), and the merges that the compiler would refuse.

use super::testing::*;
use crate::api::error::ErrorCode;
use crate::projection::model::*;
use crate::projection::testing::*;

fn doc() -> Projection {
    plain_doc(&[
        "Alpha beta gamma delta.",
        "Second paragraph here.",
        "Third one.",
    ])
}

#[test]
fn joined_lines_are_a_merge_and_the_join_space_is_inserted_at_the_end_of_the_first() {
    let plan = plan(&doc(), |t| {
        let t = swap(
            t,
            "p1 Alpha beta gamma delta.",
            "p1 Alpha beta gamma delta. Second paragraph here.",
        );
        swap(&t, "p2 Second paragraph here.\n", "")
    })
    .unwrap();
    assert_eq!(show(&plan), vec!["merge p1 p2", "replace p1 23..23 \" \""]);
    assert_eq!(plan.origins[0].block.as_deref(), Some("p1"));
    assert_eq!(plan.origins[0].also.as_deref(), Some("p2"));
    assert_eq!(plan.origins[1].block.as_deref(), Some("p1"));
}

#[test]
fn a_merge_that_kept_the_second_ref_or_no_ref_still_keeps_the_first_paragraph() {
    let later = plan(&doc(), |t| {
        let t = swap(t, "p1 Alpha beta gamma delta.\n", "");
        swap(
            &t,
            "p2 Second paragraph here.",
            "p2 Alpha beta gamma delta. Second paragraph here.",
        )
    })
    .unwrap();
    assert_eq!(show(&later), vec!["merge p1 p2", "replace p1 23..23 \" \""]);
    let none = plan(&doc(), |t| {
        swap(
            t,
            "p1 Alpha beta gamma delta.
p2 Second paragraph here.
",
            "Alpha beta gamma delta. Second paragraph here.
",
        )
    })
    .unwrap();
    assert_eq!(show(&none), vec!["merge p1 p2", "replace p1 23..23 \" \""]);
}

#[test]
fn a_join_with_no_space_or_with_the_space_the_first_hides_is_a_plain_merge() {
    let plain = plan(&doc(), |t| {
        let t = swap(
            t,
            "p1 Alpha beta gamma delta.",
            "p1 Alpha beta gamma delta.Second paragraph here.",
        );
        swap(&t, "p2 Second paragraph here.\n", "")
    })
    .unwrap();
    assert_eq!(show(&plain), vec!["merge p1 p2"]);
    let hidden = plain_doc(&["Alpha ", "beta"]);
    let plan = plan(&hidden, |t| {
        swap(&swap(t, "p1 Alpha", "p1 Alpha beta"), "p2 beta\n", "")
    })
    .unwrap();
    assert_eq!(show(&plan), vec!["merge p1 p2"]);
}

#[test]
fn three_paragraphs_with_plain_joins_are_two_merges_in_pairs() {
    let p = plain_doc(&["a ", "b ", "c"]);
    let plan = plan(&p, |t| {
        swap(
            &swap(&swap(t, "p1 a", "p1 a b c"), "p2 b\n", ""),
            "p3 c\n",
            "",
        )
    })
    .unwrap();
    assert_eq!(show(&plan), vec!["merge p1 p2", "merge p2 p3"]);
    // three with no hidden spaces and a space between them are not plain: an ordinary edit
    let q = plain_doc(&["a", "b", "c"]);
    let plan = super::testing::plan(&q, |t| {
        swap(
            &swap(&swap(t, "p1 a", "p1 a b c"), "p2 b\n", ""),
            "p3 c\n",
            "",
        )
    })
    .unwrap();
    assert_eq!(
        show(&plan),
        vec!["replace p1 0..1 \"a b c\"", "delete p2", "delete p3"]
    );
}

#[test]
fn a_join_with_a_changed_word_is_an_ordinary_edit_and_so_is_a_removed_empty_paragraph() {
    let plan = plan(&doc(), |t| {
        let t = swap(
            t,
            "p1 Alpha beta gamma delta.",
            "p1 Alpha beta gamma delta. Second paragraph there.",
        );
        swap(&t, "p2 Second paragraph here.\n", "")
    })
    .unwrap();
    assert_eq!(
        show(&plan),
        vec![
            "replace p1 0..23 \"Alpha beta gamma delta. Second paragraph there.\"",
            "delete p2"
        ]
    );
    let empty = plain_doc(&["Alpha", ""]);
    let plan = super::testing::plan(&empty, |t| swap(t, "p2\n", "")).unwrap();
    assert_eq!(show(&plan), vec!["delete p2"]);
}

fn with_section_break_after_first() -> Projection {
    let mut p = doc();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    stories[0].units[0].blocks.insert(
        1,
        Block::Section(SectionLine {
            index: 1,
            break_type: crate::wordml::SectionBreak::NextPage,
            title_pg: false,
            mark: None,
            revs: Vec::new(),
        }),
    );
    p
}

fn merge_edit(t: &str) -> String {
    let t = swap(
        t,
        "p1 Alpha beta gamma delta.",
        "p1 Alpha beta gamma delta. Second paragraph here.",
    );
    swap(&t, "p2 Second paragraph here.\n", "")
}

#[test]
fn a_merge_the_compiler_would_refuse_is_refused_here_and_never_written_as_delete_and_insert() {
    // a section break on the first paragraph
    let e = plan(&with_section_break_after_first(), |t| {
        let t = swap(
            t,
            "p1 Alpha beta gamma delta.",
            "p1 Alpha beta gamma delta. Second paragraph here.",
        );
        swap(&t, "p2 Second paragraph here.\n", "")
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    assert!(e.message().contains("section break"), "{}", e.message());
    // a deleted block
    let mut p = doc();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    if let Block::Paragraph(second) = &mut stories[0].units[0].blocks[1] {
        second.flags.readonly = Some(ReadOnly::DeletedBlock);
    }
    let e = plan(&p, merge_edit).unwrap_err();
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    assert!(e.message().contains("deleted paragraph"), "{}", e.message());
    // every other reason a paragraph is read-only refuses a join, as it refuses a split
    for (reason, says) in [
        (ReadOnly::LockedControl, "locked content control"),
        (ReadOnly::DataBound, "bound to data"),
        (ReadOnly::Placeholder, "placeholder"),
    ] {
        for which in [0, 1] {
            let mut p = doc();
            let Body::Stories(stories) = &mut p.body else {
                panic!("a model of files has stories");
            };
            if let Block::Paragraph(par) = &mut stories[0].units[0].blocks[which] {
                par.flags.readonly = Some(reason);
            }
            let e = plan(&p, merge_edit).unwrap_err();
            assert_eq!(e.code(), ErrorCode::ReadonlyRegion, "{reason:?} {which}");
            assert!(
                e.message().contains("cannot be joined") && e.message().contains(says),
                "{reason:?} {which}: {}",
                e.message()
            );
        }
    }
    // the first paragraph's mark is already deleted
    let mut p = doc();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    if let Block::Paragraph(first) = &mut stories[0].units[0].blocks[0] {
        first.mark = Some(MarkRev {
            kind: MarkKind::Deleted,
            tag: tag(8, 1),
        });
    }
    let e = plan(&p, |t| {
        let t = swap(
            t,
            "Alpha beta gamma delta.",
            "Alpha beta gamma delta. Second paragraph here.",
        );
        swap(&t, "p2 Second paragraph here.\n", "")
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    assert!(e.message().contains("already deleted"), "{}", e.message());
    assert!(e.next().contains("Accept or reject"), "{}", e.next());
    // a field that spans paragraphs
    let mut p = doc();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    if let Block::Paragraph(first) = &mut stories[0].units[0].blocks[0] {
        first.inlines = vec![
            text("Alpha beta gamma delta."),
            Inline::Field(Field {
                keyword: "TOC".into(),
                text: "x".into(),
                width: 1,
                target: None,
                opens: true,
                closes: false,
                dirty: false,
            }),
        ];
    }
    let e = plan(&p, |t| {
        let t = swap(
            t,
            "delta.{field TOC start \"x\"}",
            "delta.{field TOC start \"x\"} Second paragraph here.",
        );
        swap(&t, "p2 Second paragraph here.\n", "")
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion, "{}", e.message());
    assert!(e.message().contains("field"), "{}", e.message());
}

#[test]
fn paragraphs_that_are_not_neighbours_are_cross_block() {
    // the last paragraph before a table and the first one in it
    let p = sample();
    let e = plan(&p, |t| {
        let t = swap(t, "{--\"Losses\"--}{#4 JD}", "{--\"Losses\"--}{#4 JD}");
        t.to_string()
    });
    assert!(e.is_ok());
    // p2 is followed by a table: joining p2 with the first paragraph of the table's first cell
    let e = plan(&p, |t| {
        let t = swap(t, "\\tliability\n", "\\tliability  Item\n");
        swap(&t, "p3 Item\n", "")
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::CrossBlock);
    assert!(e.message().contains("not neighbours"), "{}", e.message());
}

fn ending_in_a_deletion() -> Projection {
    let mut p = doc();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    if let Block::Paragraph(first) = &mut stories[0].units[0].blocks[0] {
        first.inlines = vec![text("abc"), del(1, "x")];
    }
    p
}

#[test]
fn a_join_after_a_marker_that_takes_no_room_puts_the_space_at_the_start_of_the_second_paragraph() {
    // `abc{--x--}` and `Second paragraph here.` joined with a space: the space is after the deletion, and it goes
    // in at offset 0 of the second paragraph, before the merge, where there is no marker to be on either side of.
    let plan = plan(&ending_in_a_deletion(), |t| {
        let t = swap(
            t,
            "p1 abc{--x--}{#1 JD}",
            "p1 abc{--x--}{#1 JD} Second paragraph here.",
        );
        swap(&t, "p2 Second paragraph here.\n", "")
    })
    .unwrap();
    assert_eq!(show(&plan), vec!["replace p2 0..0 \" \"", "merge p1 p2"]);
    assert_eq!(plan.origins[0].block.as_deref(), Some("p2"));
    assert_eq!(plan.origins[1].block.as_deref(), Some("p1"));
    assert_eq!(plan.origins[1].also.as_deref(), Some("p2"));
}

#[test]
fn a_join_between_a_marker_and_a_marker_is_still_ambiguous() {
    let mut p = ending_in_a_deletion();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    if let Block::Paragraph(second) = &mut stories[0].units[0].blocks[1] {
        second.inlines = vec![del(2, "y"), text("tail")];
    }
    let e = plan(&p, |t| {
        let t = swap(
            t,
            "p1 abc{--x--}{#1 JD}",
            "p1 abc{--x--}{#1 JD} {--y--}{#2 JD}tail",
        );
        swap(&t, "p2 {--y--}{#2 JD}tail\n", "")
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::Unsupported);
    assert!(e.message().contains("takes no room"), "{}", e.message());
}
