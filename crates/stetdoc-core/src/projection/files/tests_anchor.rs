//! The anchor of a run of new lines: one test for each row of the table of design 6.4.

use super::testing::*;
use crate::api::error::ErrorCode;
use crate::projection::model::*;
use crate::projection::testing::*;
use crate::wordml::StoryKind;

fn doc() -> Projection {
    sample()
}

#[test]
fn after_a_paragraph_in_the_story_the_run_goes_after_it_in_the_style_word_gives() {
    let plan = plan(&doc(), |t| {
        swap(
            t,
            "p1 {style \"Heading 1\"} {§\"1.\"} DEFINITIONS\n",
            "p1 {style \"Heading 1\"} {§\"1.\"} DEFINITIONS\nFirst new\nSecond new\n",
        )
    })
    .unwrap();
    assert_eq!(
        show(&plan),
        vec!["insert After p1: \"First new\" [BodyText], \"Second new\""]
    );
    assert_eq!(plan.origins[0].block.as_deref(), Some("p1"));
    assert_eq!(plan.origins[0].line, 5, "the line of the first new line");
}

#[test]
fn after_a_paragraph_with_the_body_style_the_copy_rule_applies() {
    let plan = plan(&doc(), |t| {
        swap(t, "\\tliability\n", "\\tliability\nA new paragraph\n")
    })
    .unwrap();
    assert_eq!(show(&plan), vec!["insert After p2: \"A new paragraph\""]);
}

#[test]
fn after_the_end_of_a_table_the_run_goes_after_the_table() {
    let plan = plan(&doc(), |t| {
        swap(t, "{/table}\n", "{/table}\nAfter the table\n")
    })
    .unwrap();
    assert_eq!(show(&plan), vec!["insert After t1: \"After the table\""]);
}

#[test]
fn between_two_paragraphs_of_one_cell_it_goes_after_the_first_in_the_cell() {
    let plan = plan(&doc(), |t| swap(t, "p4 Price\n", "p4 Price\nBetween\n")).unwrap();
    assert_eq!(show(&plan), vec!["insert After p4: \"Between\""]);
}

#[test]
fn after_the_last_paragraph_of_a_cell_is_ambiguous_and_refused() {
    let e = plan(&doc(), |t| {
        swap(t, "p5 {+} per unit\n", "p5 {+} per unit\nA line\n")
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::Unsupported);
    assert!(
        e.message().contains("last paragraph of a table cell"),
        "{}",
        e.message()
    );
    assert!(e.next().contains("insert"), "{}", e.next());
}

#[test]
fn inside_a_table_before_its_first_cell_is_refused() {
    let e = plan(&doc(), |t| {
        swap(t, "r1 {row header}\n", "r1 {row header}\nA line\n")
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::Unsupported);
    let e = plan(&doc(), |t| {
        swap(
            t,
            "t1 {table cols=2 style=\"Table Grid\"}\n",
            "t1 {table cols=2 style=\"Table Grid\"}\nA line\n",
        )
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::Unsupported);
}

#[test]
fn after_a_section_line_the_run_goes_before_the_next_paragraph() {
    let plan = plan(&doc(), |t| {
        swap(
            t,
            "{section 1 ends: next-page}\n",
            "{section 1 ends: next-page}\nStarts section two\n",
        )
    })
    .unwrap();
    assert_eq!(
        show(&plan),
        vec!["insert Before p6: \"Starts section two\""]
    );
}

#[test]
fn at_the_start_of_the_file_the_run_goes_before_the_first_paragraph() {
    let plan = plan(&doc(), |t| {
        swap(t, "p1 {style", "Before everything\np1 {style")
    })
    .unwrap();
    assert_eq!(show(&plan), vec!["insert Before p1: \"Before everything\""]);
}

#[test]
fn at_the_start_of_a_note_the_run_goes_before_its_first_paragraph() {
    let plan = plan_with(
        &doc(),
        &crate::projection::formats::stet::StetFormat,
        &styles(),
        |name, t| {
            if name == "notes.md" {
                swap(t, "{footnote 1}\n", "{footnote 1}\nAnother\n")
            } else {
                t.to_string()
            }
        },
    )
    .unwrap();
    assert_eq!(show(&plan), vec!["insert Before p7: \"Another\""]);
}

#[test]
fn before_a_table_after_a_section_line_is_refused() {
    let mut p = doc();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    // a section line directly before a table
    let blocks = &mut stories[0].units[0].blocks;
    let table = blocks.remove(2);
    let section = blocks.remove(2);
    blocks.insert(2, section);
    blocks.insert(3, table);
    let e = plan(&p, |t| {
        swap(
            t,
            "{section 1 ends: next-page}\n",
            "{section 1 ends: next-page}\nA line\n",
        )
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::Unsupported);
    assert!(e.message().contains("before a table"), "{}", e.message());
}

#[test]
fn after_a_collapsed_field_the_run_goes_after_its_last_ref_and_other_blocks_are_refused() {
    let mut p = doc();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    stories[0].units[0].blocks.insert(
        1,
        Block::Opaque(OpaqueBlock::Collapsed {
            keyword: "toc".into(),
            first: crate::refs::BlockRef::parse("p20").ok(),
            last: crate::refs::BlockRef::parse("p30").ok(),
            blocks: 11,
        }),
    );
    stories[0].units[0]
        .blocks
        .insert(2, Block::Opaque(OpaqueBlock::AltChunk));
    let plan = plan(&p, |t| {
        swap(
            t,
            "{opaque toc p20..p30 11 paragraphs}\n",
            "{opaque toc p20..p30 11 paragraphs}\nAfter the contents\n",
        )
    });
    // the refs p20 and p30 are not in the baseline's paragraphs, but they are the collapsed field's own
    assert_eq!(
        show(&plan.unwrap()),
        vec!["insert After p30: \"After the contents\""]
    );
    let e = self::plan(&p, |t| {
        swap(t, "{opaque altchunk}\n", "{opaque altchunk}\nA line\n")
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::Unsupported);
}

#[test]
fn a_unit_with_no_paragraph_and_comments_refuse_a_run() {
    let mut p = doc();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    stories.push(Story {
        kind: StoryKind::Header,
        part: "word/header1.xml".into(),
        uses: vec![SectionUse {
            section: 1,
            slot: Slot::Default,
        }],
        units: vec![Unit {
            kind: UnitKind::Header,
            id: None,
            ordinal: 0,
            mark: None,
            meta: UnitMeta::default(),
            blocks: Vec::new(),
        }],
    });
    let e = plan_with(
        &p,
        &crate::projection::formats::stet::StetFormat,
        &styles(),
        |name, t| {
            if name == "headers.md" {
                format!("{t}A header line\n")
            } else {
                t.to_string()
            }
        },
    )
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::Unsupported);
}

#[test]
fn a_run_of_several_lines_is_one_insert_with_the_text_as_written() {
    let plan = plan(&doc(), |t| {
        swap(
            t,
            "\\tliability\n",
            "\\tliability\n  leading and tab\\there \\u{a0}\n\\{literal\\}\n",
        )
    })
    .unwrap();
    assert_eq!(
        show(&plan),
        vec!["insert After p2: \"  leading and tab\\there \\u{a0}\", \"{literal}\""]
    );
}

#[test]
fn a_marker_or_a_tag_in_a_new_line_is_refused() {
    let e = plan(&doc(), |t| {
        swap(t, "\\tliability\n", "\\tliability\n{++new++}{#9 JD}\n")
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    assert!(e.message().contains("text only"), "{}", e.message());
    let e = plan(&doc(), |t| {
        swap(t, "\\tliability\n", "\\tliability\nsee {fn3}\n")
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
}
