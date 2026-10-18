//! Exact splits and merges: law T7 (design 6.4, step 6).

use super::split::{Merge, Split, final_width, match_merge, match_split};
use super::testing::*;
use crate::api::error::ErrorCode;
use crate::api::view::WarningCode;
use crate::projection::draft::DraftBlock;
use crate::projection::keys::{Key, RevRef};
use crate::projection::model::*;
use crate::projection::testing::*;

fn doc() -> Projection {
    plain_doc(&[
        "Alpha beta gamma delta.",
        "Second paragraph here.",
        "Third one.",
    ])
}

fn block(texts: &[&str]) -> DraftBlock {
    DraftBlock {
        texts: texts.iter().map(|t| (*t).to_string()).collect(),
        ..DraftBlock::plain(None, "")
    }
}

// the comparison of drafts

#[test]
fn a_cut_is_after_the_whitespace_and_the_pieces_are_the_text_without_it() {
    let p = block(&["Alpha beta"]);
    assert_eq!(
        match_split(&p, &[&block(&["Alpha"]), &block(&["beta"])]),
        Split::Yes(vec![super::split::Cut {
            slot: 0,
            within: 6,
            at: 6,
            piece: 1
        }])
    );
    // a cut inside a word has no whitespace to leave out
    assert!(
        matches!(match_split(&p, &[&block(&["Alp"]), &block(&["ha beta"])]), Split::Yes(c) if c[0].at == 3)
    );
    // two spaces are one run
    let two = block(&["a  b"]);
    assert!(
        matches!(match_split(&two, &[&block(&["a"]), &block(&["b"])]), Split::Yes(c) if c[0].at == 3)
    );
    // a second piece that starts with a space is not the old text
    assert_eq!(
        match_split(&two, &[&block(&["a"]), &block(&[" b"])]),
        Split::No
    );
    // tabs count as whitespace
    let tab = block(&["a\tb"]);
    assert!(
        matches!(match_split(&tab, &[&block(&["a"]), &block(&["b"])]), Split::Yes(c) if c[0].at == 2)
    );
}

#[test]
fn any_difference_is_not_a_split() {
    let p = block(&["Alpha beta"]);
    assert_eq!(
        match_split(&p, &[&block(&["Alpha"]), &block(&["Beta"])]),
        Split::No
    );
    assert_eq!(
        match_split(&p, &[&block(&["Alpha"]), &block(&["beta."])]),
        Split::No
    );
    assert_eq!(
        match_split(&p, &[&block(&["Alpha"])]),
        Split::No,
        "one piece is no split"
    );
    assert_eq!(
        match_split(&p, &[&block(&["Alpha beta"]), &block(&["x"])]),
        Split::No
    );
}

#[test]
fn seams_must_be_the_old_ones_in_order_and_each_piece_is_a_draft() {
    let mut p = block(&["see ", " here"]);
    p.seams = vec![crate::projection::draft::Seam {
        key: Key::PageBreak,
        width: 1,
    }];
    let mut left = block(&["see ", ""]);
    left.seams = p.seams.clone();
    assert!(
        matches!(match_split(&p, &[&left, &block(&["here"])]), Split::Yes(c) if c[0].at == 6 && c[0].slot == 1)
    );
    let mut wrong = left.clone();
    wrong.seams[0].key = Key::ColumnBreak;
    assert_eq!(match_split(&p, &[&wrong, &block(&["here"])]), Split::No);
}

#[test]
fn a_cut_next_to_a_seam_that_takes_no_room_is_ambiguous() {
    let rev = RevRef {
        id: 1,
        author: "JD".into(),
        peer: None,
    };
    let deleted = Key::Deleted {
        moved: false,
        rev,
        text: "x".into(),
    };
    let mut p = block(&["abc", " def"]);
    p.seams = vec![crate::projection::draft::Seam {
        key: deleted.clone(),
        width: 0,
    }];
    // `abc` | `{--x--} def`: the cut is right before the deletion
    let mut right = block(&["", " def"]);
    right.seams = p.seams.clone();
    assert!(
        matches!(match_split(&p, &[&block(&["abc"]), &right]), Split::Ambiguous { seam, .. } if seam == deleted)
    );
    // `abc{--x--}` | `def`: the cut is after the space that follows the deletion, so it is not next to it
    let mut left = block(&["abc", ""]);
    left.seams = p.seams.clone();
    assert!(matches!(match_split(&p, &[&left, &block(&["def"])]), Split::Yes(c) if c[0].at == 4));
}

#[test]
fn a_merge_is_the_parts_joined_with_what_the_first_hides() {
    let mut first = block(&["Alpha"]);
    let second = block(&["beta"]);
    // nothing hidden: any whitespace run is the join, and it is inserted
    assert_eq!(
        match_merge(&[&first, &second], &block(&["Alpha beta"])),
        Merge::Yes {
            insert: Some((5, " ".to_string()))
        }
    );
    assert_eq!(
        match_merge(&[&first, &second], &block(&["Alphabeta"])),
        Merge::Yes { insert: None }
    );
    assert_eq!(
        match_merge(&[&first, &second], &block(&["Alpha  \tbeta"])),
        Merge::Yes {
            insert: Some((5, "  \t".to_string()))
        }
    );
    assert_eq!(
        match_merge(&[&first, &second], &block(&["Alpha, beta"])),
        Merge::No
    );
    // the first hides a space: the join is that space and nothing else
    first.facts.trailing = 1;
    assert_eq!(
        match_merge(&[&first, &second], &block(&["Alpha beta"])),
        Merge::Yes { insert: None }
    );
    assert_eq!(
        match_merge(&[&first, &second], &block(&["Alphabeta"])),
        Merge::No
    );
    assert_eq!(
        match_merge(&[&first, &second], &block(&["Alpha  beta"])),
        Merge::No
    );
}

#[test]
fn a_join_after_a_seam_that_takes_no_room_goes_into_the_second_part_when_that_starts_with_text() {
    let rev = RevRef {
        id: 1,
        author: "JD".into(),
        peer: None,
    };
    let deleted = Key::Deleted {
        moved: false,
        rev,
        text: "x".into(),
    };
    let mut first = block(&["abc", ""]);
    first.seams = vec![crate::projection::draft::Seam {
        key: deleted.clone(),
        width: 0,
    }];
    let mut line = block(&["abc", " def"]);
    line.seams = first.seams.clone();
    assert_eq!(
        match_merge(&[&first, &block(&["def"])], &line),
        Merge::YesInSecond {
            text: " ".to_string()
        }
    );
    // a second part that starts with a seam as well has no side to put it on
    let mut second = block(&["", "tail"]);
    second.seams = first.seams.clone();
    let mut both = block(&["abc", " ", "tail"]);
    both.seams = vec![first.seams[0].clone(), first.seams[0].clone()];
    assert_eq!(
        match_merge(&[&first, &second], &both),
        Merge::Ambiguous(deleted)
    );
}

#[test]
fn a_merge_of_three_needs_plain_joins_and_a_first_part_with_no_text_inserts_nothing() {
    let mut a = block(&["a"]);
    let mut b = block(&["b"]);
    let c = block(&["c"]);
    assert_eq!(
        match_merge(&[&a, &b, &c], &block(&["a b c"])),
        Merge::No,
        "no hidden space: not a plain join"
    );
    a.facts.trailing = 1;
    b.facts.trailing = 1;
    assert_eq!(
        match_merge(&[&a, &b, &c], &block(&["a b c"])),
        Merge::Yes { insert: None }
    );
    let empty = block(&[""]);
    assert_eq!(
        match_merge(&[&empty, &block(&["x"])], &block(&[" x"])),
        Merge::No,
        "an empty first part is a deleted block: no join is inserted"
    );
    assert_eq!(
        match_merge(&[&empty, &block(&["x"])], &block(&["x"])),
        Merge::Yes { insert: None }
    );
}

#[test]
fn the_width_of_a_paragraph_counts_what_it_hides() {
    let mut p = block(&["abc"]);
    p.facts.trailing = 2;
    assert_eq!(final_width(&p), 5);
}

// edits of rendered files

#[test]
fn a_line_break_in_the_text_is_one_split() {
    let plan = plan(&doc(), |t| {
        swap(
            t,
            "p1 Alpha beta gamma delta.",
            "p1 Alpha beta\ngamma delta.",
        )
    })
    .unwrap();
    assert_eq!(show(&plan), vec!["split p1 11"]);
    assert_eq!(plan.origins[0].block.as_deref(), Some("p1"));
    assert_eq!(
        plan.origins[0].line, 4,
        "the line of the piece after the cut"
    );
}

#[test]
fn k_pieces_are_k_minus_one_splits_from_the_last_cut_to_the_first() {
    let plan = plan(&doc(), |t| {
        swap(
            t,
            "p1 Alpha beta gamma delta.",
            "p1 Alpha\nbeta\ngamma delta.",
        )
    })
    .unwrap();
    assert_eq!(show(&plan), vec!["split p1 11", "split p1 6"]);
}

#[test]
fn a_paragraph_written_again_as_lines_with_the_ref_dropped_is_still_a_split() {
    let plan = plan(&doc(), |t| {
        swap(
            t,
            "p1 Alpha beta gamma delta.\n",
            "Alpha beta\ngamma delta.\n",
        )
    })
    .unwrap();
    assert_eq!(show(&plan), vec!["split p1 11"]);
}

#[test]
fn a_split_whose_pieces_differ_in_one_word_is_an_ordinary_edit() {
    let plan = plan(&doc(), |t| {
        swap(
            t,
            "p1 Alpha beta gamma delta.",
            "p1 Alpha beta\ngamma DELTA.",
        )
    })
    .unwrap();
    assert_eq!(
        show(&plan),
        vec![
            "replace p1 0..23 \"Alpha beta\"",
            "insert After p1: \"gamma DELTA.\""
        ]
    );
}

#[test]
fn the_ref_on_a_later_line_is_refused_and_not_turned_into_a_deletion_and_an_insertion() {
    let e = plan(&doc(), |t| {
        swap(
            t,
            "p1 Alpha beta gamma delta.",
            "Alpha beta\np1 gamma delta.",
        )
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::Unsupported);
    assert!(
        e.message().contains("keeps the ref on the first part"),
        "{}",
        e.message()
    );
    assert!(e.next().contains("p1"), "{}", e.next());
}

#[test]
fn a_split_at_a_wrap_point_is_a_split_with_a_warning() {
    let long = format!("{} tail words here", "word ".repeat(400).trim_end());
    let p = plain_doc(&[&long, "Second."]);
    let rendered = crate::projection::formats::Format::render(
        &crate::projection::formats::stet::StetFormat,
        &p,
    );
    let text = &rendered.files[0].1;
    assert!(text.contains(" \\\n"), "the paragraph is wrapped");
    let plan = plan(&p, |t| t.replacen(" \\\n", "\n", 1)).unwrap();
    let ops = show(&plan);
    assert_eq!(ops.len(), 1, "{ops:?}");
    assert!(ops[0].starts_with("split p1 "), "{ops:?}");
    assert_eq!(plan.notes.len(), 1);
    assert_eq!(plan.notes[0].code, WarningCode::SplitAtWrap);
    assert_eq!(plan.notes[0].block.as_deref(), Some("p1"));
    // a split that is not at a wrap has no warning
    let plan = super::testing::plan(&p, |t| t.replacen("word word", "word\nword", 1)).unwrap();
    assert!(plan.notes.is_empty());
}

#[test]
fn a_split_next_to_a_marker_that_takes_no_room_is_refused_as_ambiguous() {
    let mut p = doc();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    if let Block::Paragraph(first) = &mut stories[0].units[0].blocks[0] {
        first.inlines = vec![text("abc"), del(1, "x"), text(" def")];
    }
    let e = plan(&p, |t| {
        swap(t, "p1 abc{--x--}{#1 JD} def", "p1 abc\n{--x--}{#1 JD} def")
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::Unsupported);
    assert!(e.message().contains("takes no room"), "{}", e.message());
}

#[test]
fn a_read_only_paragraph_cannot_be_split() {
    let mut p = doc();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    if let Block::Paragraph(first) = &mut stories[0].units[0].blocks[0] {
        first.flags.readonly = Some(ReadOnly::LockedControl);
    }
    let e = plan(&p, |t| {
        swap(
            t,
            "p1 Alpha beta gamma delta.",
            "p1 Alpha beta\ngamma delta.",
        )
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    assert!(e.message().contains("cannot be split"), "{}", e.message());
}

#[test]
fn a_new_line_next_to_a_split_or_a_join_is_refused_so_that_the_order_of_the_two_is_never_guessed() {
    let e = plan(&doc(), |t| {
        swap(
            t,
            "p1 Alpha beta gamma delta.",
            "p1 Alpha beta\ngamma delta.\nBrand new",
        )
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::Unsupported);
    assert!(e.message().contains("splits or joins"), "{}", e.message());
    // with two edits in the file the message names the paragraph that the new line is next to, every time
    for _ in 0..8 {
        let e = plan(&doc(), |t| {
            let t = swap(
                t,
                "p1 Alpha beta gamma delta.",
                "p1 Alpha beta\ngamma delta.",
            );
            swap(&t, "p3 Third one.", "p3 Third\none.\nBrand new")
        })
        .unwrap_err();
        assert!(e.message().contains("next to p3,"), "{}", e.message());
    }
    // a new line elsewhere is fine
    let ok = plan(&doc(), |t| {
        swap(
            &swap(
                t,
                "p1 Alpha beta gamma delta.",
                "p1 Alpha beta\ngamma delta.",
            ),
            "p3 Third one.",
            "p3 Third one.\nBrand new",
        )
    })
    .unwrap();
    assert_eq!(
        show(&ok),
        vec!["split p1 11", "insert After p3: \"Brand new\""]
    );
}

#[test]
fn a_split_and_a_replace_in_other_paragraphs_keep_the_file_order() {
    let plan = plan(&doc(), |t| {
        let t = swap(
            t,
            "p1 Alpha beta gamma delta.",
            "p1 Alpha beta\ngamma delta.",
        );
        swap(&t, "p3 Third one.", "p3 Third two.")
    })
    .unwrap();
    assert_eq!(
        show(&plan),
        vec!["split p1 11", "replace p3 0..10 \"Third two.\""]
    );
}
