//! The alignment: no phantom edits, edits that stick, and no edit dropped (laws T1 to T3, design 9.1).

use super::testing::*;
use crate::api::error::ErrorCode;
use crate::api::files::PlannedOp;
use crate::projection::formats::Format;
use crate::projection::formats::json::JsonFormat;
use crate::projection::formats::stet::StetFormat;
use crate::projection::model::*;
use crate::projection::testing::*;
use crate::projection::testing_gen_blocks::generate;

// T1: the unedited files of every model give no operation.

#[test]
fn t1_unedited_files_give_no_operation() {
    for seed in 0..400 {
        let p = generate(seed);
        for format in [&StetFormat as &dyn Format, &JsonFormat] {
            let plan = plan_with(&p, format, &styles(), |_, text| text.to_string())
                .unwrap_or_else(|e| panic!("{} seed {seed}: {}", format.id(), e.message()));
            assert!(
                plan.ops.is_empty(),
                "{} seed {seed}: {:?}",
                format.id(),
                show(&plan)
            );
            assert!(plan.notes.is_empty());
        }
    }
}

#[test]
fn t1_files_that_were_rendered_again_after_an_edit_give_no_operation() {
    let mut p = sample();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    if let Block::Paragraph(first) = &mut stories[0].units[0].blocks[0] {
        first.inlines = vec![text("DEFINITIONS AND INTERPRETATION")];
    }
    assert!(plan(&p, |t| t.to_string()).unwrap().ops.is_empty());
}

// T2: edits stick.

fn doc() -> crate::projection::model::Projection {
    plain_doc(&[
        "In this Agreement, the Seller shall pay the Price.",
        "Notices go to the Buyer.",
        "Nothing else changes.",
    ])
}

#[test]
fn a_changed_slot_is_one_replace_of_the_whole_old_slot() {
    let p = doc();
    let plan = plan(&p, |t| {
        swap(
            t,
            "p2 Notices go to the Buyer.",
            "p2 Notices go to each Buyer.",
        )
    })
    .unwrap();
    assert_eq!(
        show(&plan),
        vec!["replace p2 0..24 \"Notices go to each Buyer.\""]
    );
    assert_eq!(plan.origins.len(), 1);
    assert_eq!(plan.origins[0].file, "document.md");
    assert_eq!(plan.origins[0].block.as_deref(), Some("p2"));
    assert_eq!(
        plan.origins[0].line, 4,
        "the header and the legend, then p1 on line 3 and p2 on line 4"
    );
}

#[test]
fn the_design_example_replaces_the_slot_after_the_marker_in_final_offsets() {
    // `p2 In this Agreement, {--"Losses"--}{#4 JD}{++"Loss"++}{#5 JD} means any {fn1}\tliability`
    let p = sample();
    let plan = plan(&p, |t| swap(t, "means any {fn1}", "means each {fn1}")).unwrap();
    // the slots: "In this Agreement, " (19), the deletion (0), "" , "\"Loss\"" (6), " means any " (11), the note ref (1), "\tliability"
    let start = 19 + 6;
    assert_eq!(
        show(&plan),
        vec![format!(
            "replace p2 {start}..{} \" means each \"",
            start + 11
        )]
    );
}

#[test]
fn slots_of_one_paragraph_go_from_the_last_to_the_first() {
    let p = sample();
    let plan = plan(&p, |t| {
        let t = swap(t, "In this Agreement,", "In this Deed,");
        swap(&t, "\\tliability", "\\tdebt")
    })
    .unwrap();
    let ops = show(&plan);
    assert_eq!(ops.len(), 2, "{ops:?}");
    assert!(ops[0].contains("\"\\tdebt\""), "{ops:?}");
    assert!(ops[1].contains("\"In this Deed, \""), "{ops:?}");
}

#[test]
fn a_changed_trailing_space_is_not_an_edit_but_a_changed_word_beside_it_is() {
    let p = plain_doc(&["abc   "]);
    assert!(plan(&p, |t| t.to_string()).unwrap().ops.is_empty());
    let edited = plan(&p, |t| swap(t, "p1 abc", "p1 abd")).unwrap();
    assert_eq!(
        show(&edited),
        vec!["replace p1 0..3 \"abd\""],
        "the hidden spaces stay: the range is the shown text"
    );
}

#[test]
fn removed_lines_are_deleted_in_file_order() {
    let p = doc();
    let plan = plan(&p, |t| {
        let t = swap(
            t,
            "p1 In this Agreement, the Seller shall pay the Price.\n",
            "",
        );
        swap(&t, "p3 Nothing else changes.\n", "")
    })
    .unwrap();
    assert_eq!(show(&plan), vec!["delete p1", "delete p3"]);
}

#[test]
fn a_table_or_a_row_removed_whole_is_one_delete_and_its_paragraphs_have_none() {
    let p = sample();
    let plan = plan(&p, |t| {
        swap(t, "t1 {table cols=2 style=\"Table Grid\"}\nr1 {row header}\np3 Item\np4 Price\np5 {+} per unit\n{/table}\n", "")
    })
    .unwrap();
    assert_eq!(show(&plan), vec!["delete t1"]);
    let rows = plan_row_removed();
    assert_eq!(show(&rows), vec!["row delete r2"]);
}

fn plan_row_removed() -> super::Plan {
    let mut p = sample();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    for block in &mut stories[0].units[0].blocks {
        if let Block::Table(t) = block {
            t.rows.push(Row {
                r#ref: crate::refs::BlockRef::new(crate::refs::RefKind::Row, 2),
                change: None,
                header: false,
                revs: Vec::new(),
                cells: vec![cell(vec![plain(10, "Gadget")])],
            });
        }
    }
    plan(&p, |t| swap(t, "r2 {row}\np10 Gadget\n", "")).unwrap()
}

// T3 and the refusals: nothing the agent wrote is dropped.

fn refused(
    p: &crate::projection::model::Projection,
    edit: impl Fn(&str) -> String,
) -> crate::api::error::StetError {
    plan(p, edit).expect_err("must be refused")
}

#[test]
fn a_duplicated_ref_names_both_lines() {
    let e = refused(&doc(), |t| format!("{t}p2 again\n"));
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    assert!(
        e.message().contains("p2 is on lines 4 and 6"),
        "{}",
        e.message()
    );
}

#[test]
fn a_ref_that_was_never_there_or_is_gone_or_is_in_another_file() {
    let e = refused(&doc(), |t| format!("{t}p50 text\n"));
    assert_eq!(e.code(), ErrorCode::NotFound);
    let e = refused(&doc(), |t| format!("{t}p950 text\n"));
    assert_eq!(e.code(), ErrorCode::StaleRef);
    let e = refused(&sample(), |t| format!("{t}p7 text\n"));
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    assert!(
        e.message().contains("p7 belongs to notes.md"),
        "{}",
        e.message()
    );
}

#[test]
fn a_moved_paragraph_is_unsupported() {
    let e = refused(&doc(), |t| {
        let t = swap(
            t,
            "p1 In this Agreement, the Seller shall pay the Price.\n",
            "",
        );
        format!("{t}p1 In this Agreement, the Seller shall pay the Price.\n")
    });
    assert_eq!(e.code(), ErrorCode::Unsupported);
    assert!(
        e.message().contains("p1 is before p3") || e.message().contains("moved"),
        "{}",
        e.message()
    );
}

#[test]
fn a_paragraph_moved_across_the_end_of_a_table_is_unsupported() {
    let e = refused(&sample(), |t| {
        swap(
            t,
            "p5 {+} per unit
{/table}
",
            "{/table}
p5 {+} per unit
",
        )
    });
    assert_eq!(e.code(), ErrorCode::Unsupported);
    assert!(e.message().contains("moved across"), "{}", e.message());
}

#[test]
fn structure_lines_are_read_only_and_a_table_is_removed_whole_or_not_at_all() {
    let p = sample();
    let e = refused(&p, |t| {
        swap(
            t,
            "{section 1 ends: next-page}",
            "{section 1 ends: continuous}",
        )
    });
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    let e = refused(&p, |t| swap(t, "{section 1 ends: next-page}\n", ""));
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    let e = refused(&p, |t| swap(t, "{/table}\n", ""));
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    assert!(e.message().contains("end of a table"), "{}", e.message());
    let e = refused(&p, |t| swap(t, "r1 {row header}\n", ""));
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    let e = refused(&p, |t| {
        swap(t, "t1 {table cols=2 style=\"Table Grid\"}\n", "")
    });
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    let e = refused(&p, |t| swap(t, "{/table}\n", "{/table}\n{/table}\n"));
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    let e = refused(&p, |t| {
        swap(
            t,
            "{/table}\n",
            "{/table}\nt9 {table cols=1}\nr9 {row}\np9 x\n{/table}\n",
        )
    });
    assert_eq!(
        e.code(),
        ErrorCode::NotFound,
        "an unknown ref is found first"
    );
    let e = refused(&p, |t| swap(t, "{/table}\n", "{/table}\n{table cols=1}\n"));
    assert_eq!(e.code(), ErrorCode::Unsupported);
}

#[test]
fn a_changed_tag_or_seam_is_read_only_and_the_message_says_what_and_how_to_undo() {
    let p = sample();
    let e = refused(&p, |t| {
        swap(t, "{style \"Heading 1\"}", "{style \"Heading 2\"}")
    });
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    assert!(
        e.message().contains("p1") && e.message().contains("style"),
        "{}",
        e.message()
    );
    let e = refused(&p, |t| {
        swap(t, "{--\"Losses\"--}{#4 JD}", "{--\"Loss\"--}{#4 JD}")
    });
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    assert!(
        e.message().contains("deletion") && e.next().contains("reject"),
        "{} / {}",
        e.message(),
        e.next()
    );
    let e = refused(&p, |t| swap(t, "{fn1}", "{fn2}"));
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    let e = refused(&p, |t| swap(t, "{fn1}", ""));
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    let e = refused(&p, |t| swap(t, " {§\"1.\"}", ""));
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
}

#[test]
fn a_read_only_paragraph_refuses_a_text_change_and_the_removal_of_its_line() {
    let mut p = doc();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    if let Block::Paragraph(second) = &mut stories[0].units[0].blocks[1] {
        second.flags.readonly = Some(ReadOnly::DeletedBlock);
    }
    let e = refused(&p, |t| {
        swap(t, "Notices go to the Buyer.", "Notices go to the Seller.")
    });
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    assert!(e.message().contains("deleted paragraph"), "{}", e.message());
    let e = refused(&p, |t| swap(t, "p2 Notices go to the Buyer.\n", ""));
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    assert!(plan(&p, |t| t.to_string()).unwrap().ops.is_empty());
}

#[test]
fn problems_of_one_kind_are_reported_together_with_the_code_of_the_first() {
    let e = refused(&doc(), |t| {
        format!("{t}p41 a\np42 b\np43 c\np44 d\np45 e\np46 f\np47 g\n")
    });
    assert_eq!(e.code(), ErrorCode::NotFound);
    let lines = e.message().lines().count();
    assert_eq!(
        lines,
        6,
        "five problems and the count of the rest:\n{}",
        e.message()
    );
    assert!(e.message().contains("(and 2 more)"));
}

#[test]
fn files_the_document_does_not_have_and_files_of_another_version_are_refused() {
    let p = doc();
    let format = StetFormat;
    let baseline = baseline_of(&p, &format);
    let mut draft = format
        .parse(
            &format.render(&p).files,
            &crate::api::limits::Limits::default(),
        )
        .unwrap();
    draft.files[0].name = "notes.md".to_string();
    let e = super::align(&baseline, &draft, &styles()).unwrap_err();
    assert_eq!(e.code(), ErrorCode::Unsupported);
    let mut draft = format
        .parse(
            &format.render(&p).files,
            &crate::api::limits::Limits::default(),
        )
        .unwrap();
    draft.files[0].base = Some("0000000000000000:9:0000000000000000".to_string());
    let e = super::align(&baseline, &draft, &styles()).unwrap_err();
    assert_eq!(e.code(), ErrorCode::StaleFiles);
}

#[test]
fn comments_md_is_read_only_whole() {
    use crate::wordml::StoryKind;
    let mut p = doc();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    stories.push(Story {
        kind: StoryKind::Comments,
        part: "word/comments.xml".into(),
        uses: Vec::new(),
        units: vec![Unit {
            kind: UnitKind::Comment,
            id: Some(3),
            ordinal: 0,
            mark: None,
            meta: UnitMeta {
                author: Some(0),
                ..UnitMeta::default()
            },
            blocks: vec![Block::Paragraph(Paragraph {
                r#ref: None,
                ..para(1, vec![text("Capitalise?")])
            })],
        }],
    });
    let run = |edit: &dyn Fn(&str) -> String| {
        plan_with(&p, &StetFormat, &styles(), |name, t| {
            if name == "comments.md" {
                edit(t)
            } else {
                t.to_string()
            }
        })
    };
    assert!(
        run(&|t| format!("{t}\n\n")).unwrap().ops.is_empty(),
        "blank lines are not an edit"
    );
    let e = run(&|t| swap(t, "Capitalise?", "Capitalize?")).unwrap_err();
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    assert!(e.message().contains("comment"), "{}", e.message());
    let e = run(&|t| format!("{t}A new line\n")).unwrap_err();
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    let e = run(&|t| swap(t, "Capitalise?\n", "")).unwrap_err();
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
}

// A ref taken off a line that keeps its text.

#[test]
fn a_ref_taken_off_a_line_that_keeps_its_text_is_refused_and_not_a_delete_and_an_insert() {
    let p = doc();
    let e = plan(&p, |t| {
        swap(t, "p2 Notices go to the Buyer.", "Notices go to the Buyer.")
    })
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    assert!(
        e.message().contains("p2") && e.message().contains("same text"),
        "{}",
        e.message()
    );
    assert!(
        e.next().contains("`p2` back") && e.next().contains("remove the whole line"),
        "{}",
        e.next()
    );
}

#[test]
fn a_line_that_lost_its_ref_and_was_changed_is_a_deletion_and_an_insertion_as_written() {
    let p = doc();
    let plan = plan(&p, |t| {
        swap(
            t,
            "p2 Notices go to the Buyer.",
            "Notices go to the Buyer promptly.",
        )
    })
    .unwrap();
    let ops = show(&plan);
    assert!(ops.iter().any(|o| o.starts_with("delete p2")), "{ops:?}");
    assert!(ops.iter().any(|o| o.starts_with("insert")), "{ops:?}");
}

#[test]
fn the_same_text_written_elsewhere_is_a_deletion_and_an_insertion_with_a_warning() {
    let p = doc();
    let plan = plan(&p, |t| {
        let t = swap(t, "p2 Notices go to the Buyer.\n", "");
        format!("{t}Notices go to the Buyer.\n")
    })
    .unwrap();
    assert_eq!(plan.notes.len(), 1, "{:?}", plan.notes);
}

// The origin of a deletion is a line of the edited file, the one it is next to.

fn line_of(text: &str, start: &str) -> u32 {
    let at = text.lines().position(|l| l.starts_with(start)).unwrap();
    u32::try_from(at + 1).unwrap()
}

#[test]
fn a_deletion_names_the_line_it_is_next_to_and_not_line_zero() {
    let p = doc();
    let text = StetFormat.render(&p).files.remove(0).1;
    let plan = plan(&p, |t| swap(t, "p2 Notices go to the Buyer.\n", "")).unwrap();
    assert_eq!(show(&plan), vec!["delete p2"]);
    assert_eq!(
        plan.origins[0].line,
        line_of(&text, "p1 "),
        "the line before it"
    );
    let first = super::testing::plan(&p, |t| {
        swap(
            t,
            "p1 In this Agreement, the Seller shall pay the Price.\n",
            "",
        )
    })
    .unwrap();
    assert_eq!(
        first.origins[0].line,
        line_of(&text, "p2 ") - 1,
        "at the start, the line that is now first"
    );
    let last = super::testing::plan(&p, |t| swap(t, "p3 Nothing else changes.\n", "")).unwrap();
    assert_eq!(last.origins[0].line, line_of(&text, "p2 "));
    // a table removed whole, and a row of it
    let sample = sample();
    let text = StetFormat.render(&sample).files.remove(0).1;
    let table_lines: Vec<&str> = text.lines().collect();
    let first_t = table_lines
        .iter()
        .position(|l| l.starts_with("t1"))
        .unwrap();
    let last_t = table_lines.iter().position(|l| *l == "{/table}").unwrap();
    let removed = super::testing::plan(&sample, |t| {
        let lines: Vec<&str> = t.lines().collect();
        let mut out: Vec<&str> = lines[..first_t].to_vec();
        out.extend(&lines[last_t + 1..]);
        format!("{}\n", out.join("\n"))
    })
    .unwrap();
    let delete = removed
        .origins
        .iter()
        .zip(&removed.ops)
        .find(|(_, op)| matches!(op, PlannedOp::Delete { block } if block == "t1"))
        .unwrap();
    assert!(delete.0.line > 0, "{:?}", delete.0);
}
