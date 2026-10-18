//! How stetdoc reads and edits what Word shows differently from what the file holds (the LibreOffice oracle's suspects,
//! settled): a note's own number, a note whose reference is deleted, white space at the ends of a `w:t` without
//! `xml:space="preserve"`, an emoji written as `w16se:symEx`, a table placed directly in a paragraph, and the text of a
//! vertically merged continuation cell. Each is held at the interface, through `Document`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use stetdoc_core::{
    Batch, Budget, Document, Edit, FormatId, Limits, Mode, OpenOptions, Scope, Stamp, StoryName,
    ViewSpec,
};
use stetdoc_testkit::fixtures::{
    contract, parts,
    zip::{Entry, Zip},
};
use stetdoc_testkit::laws::locality::part;

fn open(bytes: &[u8]) -> Document {
    Document::open(bytes.to_vec(), &OpenOptions::default()).unwrap()
}

fn edit(doc: &mut Document, ops: &str, tracked: bool) {
    let b = Batch::parse(
        &format!(r#"{{"base":"{}","ops":[{ops}]}}"#, doc.base()),
        &Limits::default(),
    )
    .unwrap();
    let e = Edit::ops(b, Stamp::new("Clauseo", "2026-10-09T10:00:00"));
    doc.edit(if tracked { e } else { e.untracked() }).unwrap();
}

fn saved(doc: &mut Document) -> Vec<u8> {
    doc.save().unwrap().bytes
}

fn view(doc: &Document, story: StoryName, mode: Mode) -> String {
    doc.view(&ViewSpec {
        scope: Scope::Story(story),
        mode,
        budget: Budget(u32::MAX),
        ..ViewSpec::default()
    })
    .map(|p| p.text)
    .unwrap_or_default()
}

fn json(doc: &Document, mode: Mode) -> String {
    doc.view(&ViewSpec {
        scope: Scope::Story(StoryName::Body),
        mode,
        budget: Budget(u32::MAX),
        format: FormatId::new(FormatId::JSON),
        ..ViewSpec::default()
    })
    .unwrap()
    .text
}

fn xml(bytes: &[u8], name: &str) -> String {
    String::from_utf8(part(bytes, name).unwrap()).unwrap()
}

fn package(body: &str, root_attrs: &str) -> Vec<u8> {
    let doc = parts::Doc::new().body(body).root_attrs(root_attrs).bytes();
    Zip::new(parts::entries_doc(&doc)).build()
}

fn review(doc: &mut Document, action: &str) {
    edit(doc, &format!(r#"{{"op":"{action}","all":true}}"#), true);
}

#[test]
fn deleting_a_notes_whole_text_keeps_the_notes_own_number() {
    // p11 is the only paragraph of footnote 1: its text is " Scots law is excluded." after the number.
    for ops in [
        r#"{"op":"delete","ref":"p11"}"#,
        r#"{"op":"replace","ref":"p11","with":"New note."}"#,
        r#"{"op":"delete","ref":"p11","start":0,"end":5}"#,
    ] {
        for tracked in [true, false] {
            let mut doc = open(&contract::package_rich());
            edit(&mut doc, ops, tracked);
            let bytes = saved(&mut doc);
            let notes = xml(&bytes, "word/footnotes.xml");
            let at = notes.find("<w:footnoteRef/>").expect("the number is there");
            let before = &notes[..at];
            assert!(
                before
                    .rfind("<w:del ")
                    .is_none_or(|d| before[d..].contains("</w:del>")),
                "{ops} tracked={tracked}: the number is inside a deletion: {notes}"
            );
            let mut reopened = open(&bytes);
            review(&mut reopened, "accept");
            assert!(
                xml(&saved(&mut reopened), "word/footnotes.xml").contains("<w:footnoteRef/>"),
                "{ops}: accepting keeps the number"
            );
            assert!(view(&reopened, StoryName::Notes, Mode::Final).contains("{fn.}"));
        }
    }
}

#[test]
fn a_note_whose_reference_is_a_pending_deletion_is_not_in_the_final_view_of_the_notes() {
    // p5 holds the reference to footnote 1.
    let mut doc = open(&contract::package_rich());
    assert!(view(&doc, StoryName::Notes, Mode::Final).contains("Scots law"));
    edit(&mut doc, r#"{"op":"delete","ref":"p5"}"#, true);
    let doc = open(&saved(&mut doc));
    assert!(
        !view(&doc, StoryName::Notes, Mode::Final).contains("Scots law"),
        "{}",
        view(&doc, StoryName::Notes, Mode::Final)
    );
    assert!(view(&doc, StoryName::Notes, Mode::Original).contains("Scots law"));
    assert!(view(&doc, StoryName::Notes, Mode::Redline).contains("Scots law"));
}

/// A paragraph whose text nodes carry white space at their ends without `xml:space="preserve"`: Word shows " one two " with
/// its spaces, and in the second the space between the symbol and "end" (hand check 37, 2026-10-10). LibreOffice drops them.
const BARE: &str = "<w:p><w:r><w:t> one two </w:t></w:r></w:p><w:p><w:r><w:t>Symbol </w:t><w:sym w:font=\"Wingdings\" w:char=\"F03A\"/></w:r><w:r><w:t> </w:t></w:r><w:r><w:t>end</w:t></w:r></w:p>";

#[test]
fn white_space_at_the_ends_of_a_w_t_without_preserve_is_text_as_word_shows_it() {
    // `xml:space="preserve"` on an ancestor changes nothing Word shows.
    let inherited = view(
        &open(&package(BARE, r#" xml:space="preserve""#)),
        StoryName::Body,
        Mode::Final,
    );
    assert!(inherited.contains("Symbol \\u{f03a} end"), "{inherited}");
    let bytes = package(BARE, "");
    let doc = open(&bytes);
    let body = view(&doc, StoryName::Body, Mode::Final);
    assert!(body.contains("p1  one two\n"), "{body}");
    // The view writes a private-use character as an escape.
    assert!(body.contains("p2 Symbol \\u{f03a} end\n"), "{body}");
    // Offsets count every character: 1..4 is "one", 5..8 is "two".
    for (ops, final_text) in [
        (r#"{"op":"delete","ref":"p1","start":1,"end":4}"#, "  two"),
        (r#"{"op":"delete","ref":"p1","start":4,"end":5}"#, " onetwo"),
        (
            r#"{"op":"replace","ref":"p1","start":5,"end":8,"with":"three"}"#,
            " one three",
        ),
        (
            r#"{"op":"replace","ref":"p1","start":9,"end":9,"with":" more"}"#,
            " one two  more",
        ),
        (
            r#"{"op":"replace","ref":"p1","with":"Whole new text."}"#,
            "Whole new text.",
        ),
        (
            r#"{"op":"format","ref":"p1","start":5,"end":8,"bold":true}"#,
            " one two",
        ),
        (r#"{"op":"split","ref":"p1","at":5}"#, " one"),
    ] {
        for tracked in [true, false] {
            let mut doc = open(&bytes);
            edit(&mut doc, ops, tracked);
            let out = saved(&mut doc);
            let mut again = open(&out);
            let shown = view(&again, StoryName::Body, Mode::Final);
            let first = shown
                .lines()
                .find(|l| l.starts_with("p1"))
                .unwrap_or_default();
            assert!(
                first.trim_end().ends_with(final_text.trim_end()),
                "{ops} tracked={tracked}: {first:?}\n{}",
                xml(&out, "word/document.xml")
            );
            assert!(
                view(&again, StoryName::Body, Mode::Original).contains("p1  one two\n")
                    || !tracked
                    || ops.contains("split"),
                "{ops}: the Original view keeps the text as Word showed it"
            );
            if tracked {
                review(&mut again, "reject");
                let back = view(&again, StoryName::Body, Mode::Final);
                assert!(
                    back.contains("p1  one two\n"),
                    "{ops}: reject restores: {back}"
                );
            }
        }
    }
}

const SYMEX_NS: &str = r#" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:w16se="http://schemas.microsoft.com/office/word/2015/wordml/symex" mc:Ignorable="w16se""#;

fn symex(char_attr: &str) -> String {
    format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">Emoji: </w:t></w:r><w:r><mc:AlternateContent><mc:Choice Requires=\"w16se\"><w16se:symEx w16se:font=\"Segoe UI Emoji\" w16se:char=\"{char_attr}\"/></mc:Choice><mc:Fallback><w:t>\u{1f60a}</w:t></mc:Fallback></mc:AlternateContent></w:r><w:r><w:t>\u{2705}</w:t></w:r></w:p>"
    )
}

#[test]
fn an_emoji_written_as_symex_shows_its_character_or_else_the_fallback_text() {
    for attr in ["1F60A", "not hex", "D800"] {
        let doc = open(&package(&symex(attr), SYMEX_NS));
        let body = view(&doc, StoryName::Body, Mode::Final);
        assert!(body.contains("Emoji: \u{1f60a}\u{2705}"), "{attr}: {body}");
    }
}

#[test]
fn a_table_placed_directly_in_a_paragraph_is_left_out_of_the_views_and_kept_by_edits_around_it() {
    let body = "<w:p><w:r><w:t>[outer]</w:t><w:br/></w:r><w:tbl><w:tr><w:tc><w:p><w:r><w:t>[inner]</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:p><w:p><w:r><w:t>After.</w:t></w:r></w:p>";
    let bytes = package(body, "");
    let doc = open(&bytes);
    assert!(!view(&doc, StoryName::Body, Mode::Final).contains("[inner]"));
    for ops in [
        r#"{"op":"replace","ref":"p1","find":"outer","with":"changed"}"#,
        r#"{"op":"insert","ref":"p1","place":"after","paragraphs":["A new paragraph."]}"#,
        r#"{"op":"replace","ref":"p2","with":"Rewritten."}"#,
        r#"{"op":"comment","ref":"p1","text":"A note."}"#,
    ] {
        for tracked in [true, false] {
            let mut doc = open(&bytes);
            edit(&mut doc, ops, tracked);
            let out = xml(&saved(&mut doc), "word/document.xml");
            assert!(
                out.contains("<w:tbl><w:tr><w:tc><w:p><w:r><w:t>[inner]</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"),
                "{ops} tracked={tracked}: {out}"
            );
        }
    }
}

#[test]
fn the_text_of_a_vertically_merged_continuation_cell_is_marked_hidden() {
    let body = "<w:tbl><w:tblGrid><w:gridCol w:w=\"2000\"/><w:gridCol w:w=\"2000\"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:vMerge w:val=\"restart\"/></w:tcPr><w:p><w:r><w:t>Merged</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Right</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p><w:r><w:t>Continuation text</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Lower right</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>";
    let doc = open(&package(body, ""));
    let v: serde_json::Value = serde_json::from_str(&json(&doc, Mode::Final)).unwrap();
    let rows = v
        .pointer("/body/stories/0/units/0/blocks/0/table/rows")
        .and_then(|r| r.as_array())
        .unwrap();
    let hidden = |row: usize, cell: usize| {
        rows[row]
            .pointer(&format!("/cells/{cell}/blocks/0/paragraph/flags/hidden"))
            .and_then(serde_json::Value::as_bool)
            .unwrap()
    };
    assert!(!hidden(0, 0) && !hidden(0, 1) && !hidden(1, 1));
    assert!(hidden(1, 0), "the continuation cell's text is not shown");
}

/// A document whose body refers to three footnotes: the first reference in a pending deletion, the second plain, the third in
/// a pending insertion, each by another author than the one who edits.
fn three_notes() -> Vec<u8> {
    let at = r#"w:author="Other" w:date="2026-10-01T09:00:00Z""#;
    let reference = |id: u32| {
        format!(
            "<w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteReference w:id=\"{id}\"/></w:r>"
        )
    };
    let body = format!(
        "<w:p><w:r><w:t>Deleted</w:t></w:r><w:del w:id=\"91\" {at}>{}</w:del></w:p>\
         <w:p><w:r><w:t>Plain</w:t></w:r>{}</w:p>\
         <w:p><w:r><w:t>Inserted</w:t></w:r><w:ins w:id=\"92\" {at}>{}</w:ins></w:p>",
        reference(1),
        reference(2),
        reference(3)
    );
    let note = |id: u32, text: &str| {
        format!(
            "<w:footnote w:id=\"{id}\"><w:p><w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteRef/></w:r><w:r><w:t xml:space=\"preserve\"> {text}</w:t></w:r></w:p></w:footnote>"
        )
    };
    let notes = format!(
        "{}<w:footnotes xmlns:w=\"{}\"><w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>{}{}{}</w:footnotes>",
        parts::DECL,
        parts::NS_W,
        note(1, "Note of the deleted reference."),
        note(2, "Note of the plain reference."),
        note(3, "Note of the inserted reference.")
    );
    let doc = parts::Doc::new().body(&body).bytes();
    let types = parts::content_types().replace(
        "</Types>",
        "<Override PartName=\"/word/footnotes.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml\"/></Types>",
    );
    let mut entries = parts::entries_with(&doc, &parts::root_rels(), &types);
    entries.push(Entry::new(
        "word/_rels/document.xml.rels",
        format!(
            "{}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes\" Target=\"footnotes.xml\"/></Relationships>",
            parts::DECL
        ),
    ));
    entries.push(Entry::new("word/footnotes.xml", notes));
    Zip::new(entries).build()
}

/// A note as a view shows it: its id, its mark when the view gives one, and its text.
type Shown = (String, Option<String>, String);

/// The notes a view shows, as `(id, mark, text)` (the mark when the view gives one other than the id), and the ids its
/// body's paragraphs show after `fn`, in order. (The Redline view shows a deleted reference as a deletion, without its id.)
fn notes_of(doc: &Document, mode: Mode) -> (Vec<Shown>, Vec<String>) {
    let notes = view(doc, StoryName::Notes, mode);
    let mut shown = Vec::new();
    let mut head = (String::new(), None);
    for line in notes.lines() {
        if let Some(rest) = line
            .strip_prefix("{footnote ")
            .and_then(|l| l.strip_suffix('}'))
        {
            head = match rest.split_once(" mark=") {
                Some((id, mark)) => (id.to_string(), Some(mark.to_string())),
                None => (rest.to_string(), None),
            };
        } else if let Some(at) = line.find("Note of") {
            shown.push((head.0.clone(), head.1.clone(), line[at..].to_string()));
        }
    }
    let full = view(doc, StoryName::Body, mode);
    let body = full.split("{appendix}").next().unwrap_or_default();
    let refs = body
        .match_indices("{fn")
        .filter_map(|(i, _)| {
            let rest = &body[i + 3..];
            let end = rest.find('}')?;
            rest[..end].parse::<u32>().ok().map(|n| n.to_string())
        })
        .collect();
    (shown, refs)
}

#[test]
fn the_original_view_of_the_notes_leaves_out_a_note_whose_reference_is_a_pending_insertion() {
    let doc = open(&three_notes());
    let has = |mode, what: &str| {
        notes_of(&doc, mode)
            .0
            .iter()
            .any(|(_, _, t)| t.contains(what))
    };
    assert!(!has(Mode::Original, "inserted") && has(Mode::Original, "deleted"));
    assert!(!has(Mode::Final, "deleted") && has(Mode::Final, "inserted"));
    assert_eq!(notes_of(&doc, Mode::Redline).0.len(), 3);
}

#[test]
fn a_notes_id_is_the_same_in_every_view_and_its_mark_follows_the_view() {
    let doc = open(&three_notes());
    let note = |shown: &[Shown], what: &str| {
        shown
            .iter()
            .find(|(_, _, t)| t.contains(what))
            .map(|(id, mark, _)| (id.clone(), mark.clone()))
    };
    // The ids count every reference, in every view; the mark is what Word shows once the view's changes are resolved.
    let (fin, body) = notes_of(&doc, Mode::Final);
    assert_eq!(body, ["2", "3"], "the Final body shows the ids");
    assert_eq!(
        note(&fin, "plain"),
        Some(("2".into(), Some("1".into()))),
        "{fin:?}"
    );
    assert_eq!(
        note(&fin, "inserted"),
        Some(("3".into(), Some("2".into()))),
        "{fin:?}"
    );
    let (original, body) = notes_of(&doc, Mode::Original);
    assert_eq!(body, ["1", "2"]);
    assert_eq!(
        note(&original, "plain"),
        Some(("2".into(), None)),
        "{original:?}"
    );
    let (redline, _) = notes_of(&doc, Mode::Redline);
    assert!(
        redline.iter().all(|(_, mark, _)| mark.is_none()),
        "{redline:?}"
    );
    let v: serde_json::Value = serde_json::from_str(
        &doc.view(&ViewSpec {
            scope: Scope::Story(StoryName::Notes),
            mode: Mode::Final,
            budget: Budget(u32::MAX),
            format: FormatId::new(FormatId::JSON),
            ..ViewSpec::default()
        })
        .unwrap()
        .text,
    )
    .unwrap();
    let first = v.pointer("/body/stories/0/units/0").unwrap();
    assert_eq!(
        (first["ordinal"].as_u64(), first["mark"].as_u64()),
        (Some(2), Some(1))
    );
    // `delete-note` names the note by its id: fn2, shown with mark 1 in the Final view, is the note of the plain reference.
    let mut edited = open(&three_notes());
    edit(&mut edited, r#"{"op":"delete-note","note":"fn2"}"#, true);
    let after = open(&saved(&mut edited));
    let (fin, _) = notes_of(&after, Mode::Final);
    assert!(note(&fin, "plain").is_none(), "{fin:?}");
    assert_eq!(
        note(&fin, "inserted"),
        Some(("3".into(), Some("1".into()))),
        "{fin:?}"
    );
    assert!(note(&notes_of(&after, Mode::Original).0, "plain").is_some());
}

#[test]
fn a_paragraph_of_a_field_alone_that_joins_the_next_stays_in_the_view() {
    // A drop-down form field with no stored result is the whole of the body's last paragraph. A paragraph inserted after it
    // gives its mark the insertion (Word's Enter shape, revisions.md 3.2.1), so in Original it joins the new one: the view keeps
    // the field, which an empty paragraph that joins the next would lose.
    let body = r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> FORMDROPDOWN </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let mut doc = open(&package(body, ""));
    edit(
        &mut doc,
        r#"{"op":"insert","ref":"p1","place":"after","paragraphs":["New"]}"#,
        true,
    );
    let doc = open(&saved(&mut doc));
    let original = view(&doc, StoryName::Body, Mode::Original);
    assert!(
        original.contains("p1 {joins-next} {field FORMDROPDOWN"),
        "{original}"
    );
    let final_view = view(&doc, StoryName::Body, Mode::Final);
    assert!(final_view.contains("p2 New"), "{final_view}");
}

#[test]
fn deleting_a_whole_note_deletes_its_own_number_with_its_text_as_word_writes_it() {
    // RP050, Word's own file: the reference in a `w:del`, and the note's paragraphs deleted with their own number inside the
    // deletion. Word for the web kept an empty note "1" after Accept All when the number was left live (hand check 26).
    let mut doc = open(&contract::package_rich());
    edit(&mut doc, r#"{"op":"delete-note","note":"fn1"}"#, true);
    let out = saved(&mut doc);
    let notes = xml(&out, "word/footnotes.xml");
    let note = &notes[notes.find("<w:footnote w:id=\"1\"").expect("footnote 1")..];
    let note = &note[..note.find("</w:footnote>").expect("its end")];
    let del = note.find("<w:del ").expect("a deletion");
    let number = note.find("<w:footnoteRef/>").expect("the note's number");
    assert!(
        del < number && !note[del..number].contains("</w:del>"),
        "{note}"
    );
    // Accepting removes the note, and rejecting gives it back whole.
    let mut accepted = open(&out);
    review(&mut accepted, "accept");
    assert!(!view(&accepted, StoryName::Notes, Mode::Final).contains("Scots law"));
    let mut rejected = open(&out);
    review(&mut rejected, "reject");
    assert!(view(&rejected, StoryName::Notes, Mode::Final).contains("Scots law"));
}
