//! The width of inlines and their cut into slots and seams (laws M1 and the seam rules of design 3.7).

use super::draft::Seam;
use super::keys::{Edge, Key, Span};
use super::model::*;
use super::model_region::{regions, rev_ref};
use super::testing::{authors, del, ins, tag, text};
use super::testing_gen::Gen;
use super::testing_gen_blocks::generate;

fn field(text: &str, width: u32) -> Inline {
    Inline::Field(Field {
        keyword: "REF".into(),
        text: text.into(),
        width,
        target: None,
        opens: true,
        closes: true,
        dirty: false,
    })
}

#[test]
fn a_deletion_takes_no_room_and_an_atom_one() {
    assert_eq!(del(1, "gone").final_width(), 0);
    assert_eq!(ins(2, vec![text("héllo")]).final_width(), 5);
    assert_eq!(Inline::Atom(Atom::PageBreak).final_width(), 1);
    assert_eq!(field("4.2", 3).final_width(), 3);
    assert_eq!(Inline::Mark(Mark::CommentStart(1)).final_width(), 0);
    assert_eq!(
        text("a\u{1f600}b").final_width(),
        3,
        "an astral character is one scalar value"
    );
    let hidden = Inline::Hidden {
        reason: HiddenReason::Vanish,
        inlines: vec![text("ab"), del(3, "xyz")],
    };
    assert_eq!(
        hidden.final_width(),
        2,
        "a deletion inside hidden text is still no room"
    );
}

#[test]
fn the_design_example_is_cut_into_four_slots_and_three_seams() {
    // `In this Agreement, {--"Losses"--}{++"Loss"++} means any \tliability.`
    let inlines = vec![
        text("In this Agreement, "),
        del(4, "\"Losses\""),
        ins(5, vec![text("\"Loss\"")]),
        text(" means any \tliability."),
    ];
    let cut = regions(&inlines, &authors());
    assert_eq!(
        cut.texts,
        vec![
            "In this Agreement, ",
            "",
            "\"Loss\"",
            " means any \tliability."
        ]
    );
    assert_eq!(cut.seams.len(), 3);
    assert!(
        matches!(&cut.seams[0].key, Key::Deleted { moved: false, text, .. } if &**text == "\"Losses\"")
    );
    assert_eq!(cut.seams[0].width, 0);
    assert!(
        matches!(&cut.seams[1].key, Key::Open(Span::Ins(rev)) if rev.id == 5 && &*rev.author == "JD")
    );
    assert!(matches!(&cut.seams[2].key, Key::Close(Span::Ins(rev)) if rev.id == 5));
}

#[test]
fn adjacent_texts_join_and_adjacent_seams_leave_an_empty_slot() {
    let inlines = vec![
        text("a"),
        text("b"),
        Inline::Mark(Mark::CommentStart(7)),
        Inline::Mark(Mark::CommentEnd(7)),
        text("c"),
    ];
    let cut = regions(&inlines, &authors());
    assert_eq!(cut.texts, vec!["ab", "", "c"]);
    assert!(matches!(
        &cut.seams[0].key,
        Key::Comment {
            id: 7,
            edge: Edge::Start
        }
    ));
    assert!(matches!(
        &cut.seams[1].key,
        Key::Comment {
            id: 7,
            edge: Edge::End
        }
    ));
    let empty = regions(&[], &authors());
    assert_eq!(empty.texts, vec![String::new()]);
    assert!(empty.seams.is_empty());
}

#[test]
fn a_seam_holds_what_it_shows_and_a_read_only_thing_is_one_seam() {
    let control = Inline::Control {
        info: ControlInfo {
            kind: crate::wordml::ControlKind::PlainText,
            alias: Some("Name".into()),
            tag: None,
        },
        state: ControlState::Locked,
        inlines: vec![text("John Smith")],
    };
    let cut = regions(&[text("a "), control, text(" b")], &authors());
    assert_eq!(cut.texts, vec!["a ", " b"]);
    assert_eq!(cut.seams.len(), 1);
    assert_eq!(
        cut.seams[0].width, 10,
        "a locked control's text is in the Final text"
    );
    assert!(
        matches!(&cut.seams[0].key, Key::Control { text, state: ControlState::Locked, .. } if &**text == "John Smith")
    );
    // an editable control is text between an open and a close
    let editable = Inline::Control {
        info: ControlInfo {
            kind: crate::wordml::ControlKind::Date,
            alias: None,
            tag: None,
        },
        state: ControlState::Editable,
        inlines: vec![text("2026-10-01")],
    };
    let cut = regions(&[editable], &authors());
    assert_eq!(cut.texts, vec!["", "2026-10-01", ""]);
    assert!(matches!(&cut.seams[0].key, Key::Open(Span::Control(_))));
    assert!(matches!(&cut.seams[1].key, Key::Close(Span::Control(_))));
}

#[test]
fn a_move_names_the_other_end() {
    let mut moved = tag(9, 1);
    moved.peer = Some(11);
    let inline = Inline::Rev {
        kind: RevKind::MoveFrom,
        tag: moved,
        inlines: vec![text("old")],
    };
    let cut = regions(&[inline], &authors());
    assert!(matches!(
        &cut.seams[0].key,
        Key::Deleted { moved: true, rev, .. } if rev.peer == Some(11) && &*rev.author == "EW"
    ));
}

#[test]
fn an_author_the_head_does_not_have_gets_a_marker_code() {
    assert_eq!(&*rev_ref(&tag(1, 9), &authors()).author, "?");
}

#[test]
fn flatten_shows_deleted_text_and_field_results_and_no_atoms() {
    let inlines = vec![
        text("a"),
        del(1, "b"),
        Inline::Atom(Atom::PageBreak),
        field("c", 1),
        ins(2, vec![text("d")]),
    ];
    assert_eq!(flatten(&inlines), "abcd");
}

/// Law M1 for the cut: the slots and the seams account for every character of the `Final` text.
fn assert_cut_accounts_for_width(inlines: &[Inline]) {
    let cut = regions(inlines, &authors());
    assert_eq!(cut.texts.len(), cut.seams.len() + 1);
    let total: u32 = cut.texts.iter().map(|t| count(t)).sum::<u32>()
        + cut.seams.iter().map(|s| s.width).sum::<u32>();
    assert_eq!(total, width_of(inlines), "{inlines:?}");
}

fn all_paragraphs(blocks: &[Block], out: &mut Vec<Vec<Inline>>) {
    for block in blocks {
        match block {
            Block::Paragraph(p) => out.push(p.inlines.clone()),
            Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        all_paragraphs(&cell.blocks, out);
                    }
                }
            }
            _ => {}
        }
    }
}

#[test]
fn the_cut_accounts_for_every_character_on_generated_paragraphs() {
    let mut seen = 0;
    for seed in 0..120 {
        let p = generate(seed);
        let Body::Stories(stories) = &p.body else {
            panic!("a model of files has stories");
        };
        let mut paragraphs = Vec::new();
        for story in stories {
            for unit in &story.units {
                all_paragraphs(&unit.blocks, &mut paragraphs);
            }
        }
        for inlines in &paragraphs {
            assert_cut_accounts_for_width(inlines);
            seen += 1;
        }
    }
    assert!(seen > 500, "the generator must make paragraphs: {seen}");
}

#[test]
fn the_generator_is_deterministic_and_covers_every_inline_kind() {
    assert_eq!(generate(7), generate(7));
    assert_ne!(generate(7), generate(8));
    let mut kinds = std::collections::BTreeSet::new();
    fn walk(inlines: &[Inline], kinds: &mut std::collections::BTreeSet<&'static str>) {
        for inline in inlines {
            let (name, inner): (&'static str, &[Inline]) = match inline {
                Inline::Text(_) => ("text", &[]),
                Inline::Atom(_) => ("atom", &[]),
                Inline::Rev { kind, inlines, .. } => (
                    match kind {
                        RevKind::Ins => "ins",
                        RevKind::Del => "del",
                        RevKind::MoveTo => "move_to",
                        RevKind::MoveFrom => "move_from",
                    },
                    inlines,
                ),
                Inline::Link { inlines, .. } => ("link", inlines),
                Inline::Control { inlines, .. } => ("control", inlines),
                Inline::Field(_) => ("field", &[]),
                Inline::Hidden { inlines, .. } => ("hidden", inlines),
                Inline::LooksHidden { inlines, .. } => ("looks_hidden", inlines),
                Inline::Mark(_) => ("mark", &[]),
            };
            kinds.insert(name);
            walk(inner, kinds);
        }
    }
    for seed in 0..60 {
        let mut paragraphs = Vec::new();
        let p = Gen::new(seed).projection();
        let Body::Stories(stories) = &p.body else {
            panic!("a model of files has stories");
        };
        for story in stories {
            for unit in &story.units {
                all_paragraphs(&unit.blocks, &mut paragraphs);
            }
        }
        for inlines in &paragraphs {
            walk(inlines, &mut kinds);
        }
    }
    assert_eq!(kinds.len(), 12, "{kinds:?}");
}

// A seam is a seam: the unused import above keeps `Seam` in the test vocabulary.
const _: Option<Seam> = None;
