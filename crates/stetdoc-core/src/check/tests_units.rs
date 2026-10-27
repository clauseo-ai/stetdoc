//! Which units `check::edit` looks at: the paragraph, the shell of a table or of the frame, a note,
//! a top-level element, a part that `add_part` made. V8 runs inside every test through `say`: the
//! shortcuts that find the units give the verdict of a check of every unit.

use super::tests::{state, state_with};
use super::tests_edit::{
    MAIN, MISORDERED_RPR, Said, accepted, append, base_node, detach, el, el_with, in_part, key,
    nth, refused, retext, run_of, said, say, set_attr, transaction,
};
use super::{Checked, Declared, edit};
use crate::api::check::FindingCode as C;
use crate::package::NewPart;
use crate::state::State;
use crate::xml::SpanTree;

pub(super) const TABLE: &str = r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="100"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;

pub(super) fn para(text: &str) -> String {
    format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>")
}

#[test]
fn a_text_edit_validates_the_paragraph_and_leaves_the_table_and_the_frame_alone() {
    let s = state(&format!("{}{TABLE}{}", para("one"), para("two")));
    let said = said(&s, |t| retext(t, 1, "edited"));
    let v = accepted(&said);
    assert_eq!(
        v.checked,
        Checked {
            parts: 1,
            dirty: 2,
            equal: 0,
            shells_skipped: 1,
            validated: 1
        },
        "the cell's paragraph is validated, and the table that holds it was only marked"
    );
    assert_eq!(v.carried_total, 0);
    assert!(v.notes.is_empty(), "nothing the edit adds is a note");
    assert_eq!(
        v.changed,
        [key(base_node(&s, "p", 1))],
        "the paragraph the edit changed, and not the table or the body"
    );
}

#[test]
fn nothing_is_read_in_a_part_the_transaction_did_not_change() {
    let s = state(&para("one"));
    let tx = s.begin();
    let said = say(&tx, &Declared::default());
    assert_eq!(accepted(&said).checked, Checked::default());
    // borrowing a part for a change that is not made is no change either
    let tx = transaction(&s, |_| {});
    assert_eq!(
        edit(&tx, &Declared::default()).expect("accepted").checked,
        Checked::default()
    );
}

#[test]
fn a_paragraph_inserted_in_the_frame_validates_the_frame_and_the_order_in_it() {
    let s = state(&para("one"));
    // the section properties are last in the body, so before them is the place
    let right = said(&s, |t| {
        let body = nth(t, "body", 0);
        let sect = nth(t, "sectPr", 0);
        let p = el(t, "p");
        let r = run_of(t, "new");
        append(t, p, r);
        t.edit()
            .insert_before(body, Some(sect), p)
            .expect("inserts");
    });
    let v = accepted(&right);
    assert_eq!(
        (v.checked.validated, v.checked.dirty),
        (2, 1),
        "the paragraph and the frame"
    );
    let wrong = said(&s, |t| {
        let body = nth(t, "body", 0);
        let p = el(t, "p");
        let r = run_of(t, "new");
        append(t, p, r);
        append(t, body, p);
    });
    refused(&wrong);
    assert_eq!(wrong.new, [C::SchemaOutOfOrder]);
    // a body that already has a block after its section properties does not refuse another
    let old = state(&format!("{}<w:sectPr/>{}", para("one"), para("two")));
    let again = said(&old, |t| {
        let body = nth(t, "body", 0);
        let p = el(t, "p");
        let r = run_of(t, "new");
        append(t, p, r);
        append(t, body, p);
    });
    assert!(accepted(&again).carried_total >= 1);
}

#[test]
fn a_row_added_to_a_table_validates_the_table_and_the_cells_paragraphs() {
    let s = state(TABLE);
    let said = said(&s, |t| {
        let tbl = nth(t, "tbl", 0);
        let row = nth(t, "tr", 0);
        let copy = t.edit().clone_subtree(row).expect("clones");
        append(t, tbl, copy);
    });
    // the paragraph of the new row has no origin that was declared, and it is clean
    let v = accepted(&said);
    assert_eq!(
        v.checked.validated, 2,
        "the table, and the paragraph of the new row"
    );
    assert_eq!(v.changed.len(), 2, "the table, and the new paragraph");
    assert!(v.changed.contains(&key(base_node(&s, "tbl", 0))));
    // a table with no row at all is the table's own problem
    let said = said_after(&s, |t| {
        let row = nth(t, "tr", 0);
        detach(t, row);
    });
    refused(&said);
    assert_eq!(said.new, [C::TableWithoutRow]);
    // a cell with no paragraph is the table's
    let said = said_after(&s, |t| {
        let p = nth(t, "p", 0);
        detach(t, p);
    });
    refused(&said);
    assert_eq!(said.new, [C::CellWithoutFinalParagraph]);
}

fn said_after(s: &State, change: impl FnOnce(&mut SpanTree)) -> Said {
    said(s, change)
}

#[test]
fn a_moved_paragraph_is_checked_again_because_its_parent_is_not_the_bases() {
    let s0 = state(&format!(
        "<w:p><w:r>{MISORDERED_RPR}<w:t>moved</w:t></w:r></w:p>{TABLE}"
    ));
    let said = said(&s0, |t| {
        let p = nth(t, "p", 0);
        let tc = nth(t, "tc", 0);
        detach(t, p);
        append(t, tc, p);
    });
    let v = accepted(&said);
    // the moved paragraph, the frame it left and the table it entered; it kept its problem,
    // which is the one it had
    assert_eq!(v.checked.validated, 3, "{:?}", v.checked);
    assert!(v.carried_total >= 1);
    // the blocks the edit changed, and the frame it left, keyed by the root of the part
    assert_eq!(v.changed.len(), 3, "{:?}", v.changed);
    assert!(v.changed.contains(&key(base_node(&s0, "document", 0))));
    assert!(v.changed.contains(&key(base_node(&s0, "tbl", 0))));
    assert!(v.changed.contains(&key(base_node(&s0, "p", 0))));
}

#[test]
fn each_note_is_a_unit_and_the_frame_of_the_notes_part_is_its_root() {
    let notes = format!(
        r#"<w:footnotes xmlns:w="{}"><w:footnote w:id="1"><w:p><w:r><w:t>one</w:t></w:r></w:p></w:footnote><w:footnote w:id="2"><w:p><w:r><w:t>two</w:t></w:r></w:p></w:footnote></w:footnotes>"#,
        super::tests::W
    );
    let s = state_with(
        &para("body"),
        &[("word/footnotes.xml", "footnotes", "footnotes", &notes)],
    );
    // a text edit in a note: the note is marked and is the base's, the paragraph is checked
    let tx = in_part(&s, "word/footnotes.xml", |t| retext(t, 0, "edited"));
    let v = accepted(&say(&tx, &Declared::default())).checked;
    assert_eq!((v.dirty, v.equal, v.validated), (2, 1, 1), "{v:?}");
    // a note with no paragraph is refused, one with a paragraph is not
    let tx = in_part(&s, "word/footnotes.xml", |t| {
        let root = t.root().id();
        let note = el_with(t, "footnote", &[("id", "3")]);
        append(t, root, note);
    });
    let said = say(&tx, &Declared::default());
    refused(&said);
    assert_eq!(said.new, [C::ContainerWithoutBlock]);
    let tx = in_part(&s, "word/footnotes.xml", |t| {
        let root = t.root().id();
        let note = el_with(t, "footnote", &[("id", "3")]);
        let p = el(t, "p");
        let r = run_of(t, "three");
        append(t, p, r);
        append(t, note, p);
        append(t, root, note);
    });
    let v = accepted(&say(&tx, &Declared::default())).checked;
    assert_eq!(
        v.validated, 3,
        "the note, its paragraph and the frame: {v:?}"
    );
    // taking a note out changes the frame and nothing else
    let tx = in_part(&s, "word/footnotes.xml", |t| {
        let note = nth(t, "footnote", 1);
        detach(t, note);
    });
    let v = accepted(&say(&tx, &Declared::default())).checked;
    assert_eq!(v.validated, 1, "{v:?}");
}

#[test]
fn a_top_level_element_of_a_part_that_is_not_a_story_is_a_unit() {
    let styles = format!(
        r#"<w:styles xmlns:w="{}"><w:style w:type="paragraph" w:styleId="A"><w:name w:val="A"/></w:style><w:style w:type="paragraph" w:styleId="B" w:bogus="1"><w:name w:val="B"/></w:style></w:styles>"#,
        super::tests::W
    );
    let s = state_with(
        &para("body"),
        &[("word/styles.xml", "styles", "styles", &styles)],
    );
    // a change to style A is judged on style A: style B's problem is not its
    let tx = in_part(&s, "word/styles.xml", |t| {
        let a = nth(t, "style", 0);
        set_attr(t, a, "customStyle", "1");
    });
    let v = accepted(&say(&tx, &Declared::default())).checked;
    assert_eq!(v.validated, 1, "{v:?}");
    // a new style with the same kind of problem has no origin
    let tx = in_part(&s, "word/styles.xml", |t| {
        let root = t.root().id();
        let style = el_with(
            t,
            "style",
            &[("type", "paragraph"), ("styleId", "C"), ("bogus", "1")],
        );
        let name = el_with(t, "name", &[("val", "C")]);
        append(t, style, name);
        append(t, root, style);
    });
    let said = say(&tx, &Declared::default());
    refused(&said);
    assert_eq!(said.new, [C::SchemaUnknownAttribute]);
    // a change to an attribute of the root is the frame's
    let tx = in_part(&s, "word/styles.xml", |t| {
        let root = t.root().id();
        set_attr(t, root, "bogus", "1");
    });
    let said = say(&tx, &Declared::default());
    refused(&said);
    assert_eq!(said.new, [C::SchemaUnknownAttribute]);
}

pub(super) fn comments_part(inner: &str) -> NewPart<'static> {
    NewPart {
        name: "word/comments.xml",
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
        bytes: format!(
            r#"<w:comments xmlns:w="{}">{inner}</w:comments>"#,
            super::tests::W
        )
        .into_bytes(),
        related_from: Some((
            MAIN,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments",
        )),
    }
}

#[test]
fn a_part_that_add_part_made_is_validated_whole_and_everything_in_it_is_new() {
    let s = state(&para("one"));
    let clean =
        r#"<w:comment w:id="0" w:author="a"><w:p><w:r><w:t>note</w:t></w:r></w:p></w:comment>"#;
    let mut tx = s.begin();
    tx.add_part(comments_part(clean)).expect("adds");
    let said = say(&tx, &Declared::default());
    let v = accepted(&said).checked;
    assert_eq!(v.parts, 1, "{v:?}");
    let faulty = format!(
        r#"<w:comment w:id="0" w:author="a"><w:p><w:r>{MISORDERED_RPR}<w:t>note</w:t></w:r></w:p></w:comment>"#
    );
    let mut tx = s.begin();
    tx.add_part(comments_part(&faulty)).expect("adds");
    let said = say(&tx, &Declared::default());
    refused(&said);
    assert_eq!(said.new, [C::SchemaNonCanonicalOrder]);
    // an empty comment is a Word-rule violation of the new part
    let mut tx = s.begin();
    tx.add_part(comments_part(r#"<w:comment w:id="0" w:author="a"/>"#))
        .expect("adds");
    let said = say(&tx, &Declared::default());
    refused(&said);
    assert_eq!(said.new, [C::ContainerWithoutBlock]);
}

#[test]
fn v9_after_twenty_committed_batches_a_batch_costs_what_its_own_units_cost() {
    let body: String = (0..40).map(|i| para(&format!("paragraph {i}"))).collect();
    let mut state = state(&body);
    for k in 0..20 {
        let tx = transaction(&state, |t| retext(t, k, "changed"));
        accepted(&say(&tx, &Declared::default()));
        let (next, _) = tx.commit().expect("commits");
        state = next;
    }
    let tx = transaction(&state, |t| retext(t, 30, "changed too"));
    let said = say(&tx, &Declared::default());
    let v = accepted(&said).checked;
    // the twenty paragraphs that earlier commits left marked are met and dropped, each by
    // comparing it with the base; one is validated
    assert_eq!(
        v,
        Checked {
            parts: 1,
            dirty: 21,
            equal: 20,
            shells_skipped: 0,
            validated: 1
        }
    );
}

#[test]
fn the_keys_of_a_frame_that_a_batch_computed_are_the_next_batchs_base_keys() {
    use super::origins::{UnitKeys, variant};
    use super::units::{Unit, UnitKind};
    use crate::api::error::{ErrorCode, StetError};
    use crate::state::Dep;
    let s = state(&para("one"));
    let tx = transaction(&s, |t| {
        let body = nth(t, "body", 0);
        let sect = nth(t, "sectPr", 0);
        let p = el(t, "p");
        let r = run_of(t, "new");
        append(t, p, r);
        t.edit()
            .insert_before(body, Some(sect), p)
            .expect("inserts");
    });
    let said = say(&tx, &Declared::default());
    assert_eq!(accepted(&said).checked.validated, 2);
    let (next, _) = tx.commit().expect("commits");
    let view = next.parts();
    let main = view.id(MAIN).expect("the main part");
    let root = view.tree(main).expect("parses").root().id();
    let frame = Unit {
        root,
        kind: UnitKind::Frame,
    };
    // the entry is there, so the builder that would make it again is never called
    let refuse = |_: Option<&UnitKeys>| -> Result<UnitKeys, StetError> {
        Err(StetError::new(
            ErrorCode::Internal,
            "the keys were built again",
        ))
    };
    let kept = view
        .derived(variant(frame), &[Dep::Part(main)], &refuse)
        .expect("the frame's keys are in the cache of the next state");
    assert!(kept.0.is_empty(), "a clean frame has no keys");
}
