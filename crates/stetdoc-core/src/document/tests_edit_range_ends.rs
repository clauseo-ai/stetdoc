//! A replacement whose range starts inside a hyperlink, a bookmark, a permission range or an inline content control (or at its
//! start) and ends at its end keeps the new text inside it, as typing over the selection in Word does, tracked and then
//! accepted, and untracked (issue #20). A pure insertion at an element's end stays outside it. A deletion that leaves a
//! hyperlink with no text removes the link, and the relationship of its address once nothing uses it, when the deletion is
//! accepted or at once untracked. Rejecting everything gives the source back each time.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::xml::NodeRef;
use stetdoc_testkit::fixtures::parts::{self, DECL};
use stetdoc_testkit::fixtures::zip::{Entry, Zip};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn wr(text: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

/// A document whose first paragraph is `p1`, followed by a plain paragraph, with the relationship `rIdLink` to an external
/// address.
fn open(p1: &str) -> Document {
    let body = format!("<w:p>{p1}</w:p><w:p>{}</w:p>", wr("Unrelated paragraph."));
    let doc = parts::Doc::new()
        .root_attrs(&format!(r#" xmlns:r="{NS_R}""#))
        .body(&body)
        .bytes();
    let rels = format!(
        "{DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rIdLink\" Type=\"{NS_R}/hyperlink\" Target=\"https://example.com/terms\" TargetMode=\"External\"/></Relationships>"
    );
    let mut entries = parts::entries_with(&doc, &parts::root_rels(), &parts::content_types());
    entries.push(Entry::new("word/_rels/document.xml.rels", rels));
    let doc = Document::open(Zip::new(entries).build(), &OpenOptions::default()).unwrap();
    assert!(doc.check().ok, "the input checks clean: {:?}", doc.check());
    doc
}

fn link(inner: &str) -> String {
    format!(r#"<w:hyperlink r:id="rIdLink">{inner}</w:hyperlink>"#)
}

/// `Read {link}the terms page{/link} for details.`
fn a_link() -> String {
    format!(
        "{}{}{}",
        wr("Read "),
        link(&wr("the terms page")),
        wr(" for details.")
    )
}

/// `The [defined term] applies.`, the brackets a bookmark.
fn a_bookmark() -> String {
    format!(
        r#"{}<w:bookmarkStart w:id="0" w:name="Term"/>{}<w:bookmarkEnd w:id="0"/>{}"#,
        wr("The "),
        wr("defined term"),
        wr(" applies.")
    )
}

/// `before [inside] after`, the brackets a bookmark or a permission range for everyone.
fn a_range(perm: bool, before: &str, inside: &str, after: &str) -> String {
    let (start, end) = if perm {
        (
            r#"<w:permStart w:id="1" w:edGrp="everyone"/>"#,
            r#"<w:permEnd w:id="1"/>"#,
        )
    } else {
        (
            r#"<w:bookmarkStart w:id="1" w:name="Range"/>"#,
            r#"<w:bookmarkEnd w:id="1"/>"#,
        )
    };
    format!("{}{start}{}{end}{}", wr(before), wr(inside), wr(after))
}

/// `before {ctl}inside{/ctl} after`, an inline content control, locked against deletion or not.
fn a_control(locked: bool, before: &str, inside: &str, after: &str) -> String {
    let lock = if locked {
        r#"<w:lock w:val="sdtLocked"/>"#
    } else {
        ""
    };
    format!(
        r#"{}<w:sdt><w:sdtPr>{lock}<w:id w:val="5"/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>{}"#,
        wr(before),
        wr(inside),
        wr(after)
    )
}

/// The text of `p1` with the elements around it: `{link}` and `{/link}`, `[` and `]` for a bookmark or a permission range,
/// `{ctl}` and `{/ctl}` for a control's content. Deleted text is left out.
fn shape(doc: &Document) -> String {
    fn walk(n: NodeRef<'_>, out: &mut String) {
        for c in n.children() {
            if !c.is_element() {
                continue;
            }
            match c.local_name() {
                "t" => out.push_str(
                    &c.children()
                        .map(|t| t.text().into_owned())
                        .collect::<String>(),
                ),
                "delText" | "pPr" | "rPr" | "sdtPr" => {}
                "bookmarkStart" | "permStart" => out.push('['),
                "bookmarkEnd" | "permEnd" => out.push(']'),
                "hyperlink" => {
                    out.push_str("{link}");
                    walk(c, out);
                    out.push_str("{/link}");
                }
                "sdtContent" => {
                    out.push_str("{ctl}");
                    walk(c, out);
                    out.push_str("{/ctl}");
                }
                _ => walk(c, out),
            }
        }
    }
    let view = doc.state.parts();
    let tree = view.tree(view.id("word/document.xml").unwrap()).unwrap();
    let body = tree.root().child_elements().next().unwrap();
    let p1 = body.child_elements().next().unwrap();
    let mut out = String::new();
    walk(p1, &mut out);
    out
}

fn main_xml(doc: &Document) -> String {
    super::tests_edit_comment_tables::main_xml(doc)
}

/// True if the main part keeps the relationship `rIdLink`.
fn has_link_rel(doc: &Document) -> bool {
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    view.rels(id).unwrap().iter().any(|r| r.id == "rIdLink")
}

fn untracked(doc: &mut Document, ops: &str) {
    let b = batch(doc, ops);
    doc.edit(Edit::ops(b, stamp()).untracked()).unwrap();
}

/// Applies `ops` to `p1` tracked and accepts everything, tracked and rejects everything, and untracked. The accepted and the
/// untracked shapes are `want`, the rejected one is the source's, and each document checks clean. Returns the three
/// documents: accepted, rejected, untracked.
fn every_way(p1: &str, ops: &str, want: &str) -> [Document; 3] {
    let source = shape(&open(p1));
    let mut out = Vec::new();
    for review in ["accept", "reject"] {
        let mut doc = open(p1);
        run(&mut doc, ops);
        assert!(doc.check().ok, "{ops} tracked: {:?}", doc.check());
        run(&mut doc, &format!(r#"{{"op":"{review}","all":true}}"#));
        let expected = if review == "accept" { want } else { &source };
        assert_eq!(
            shape(&doc),
            expected,
            "{ops}, tracked, then {review}: {}",
            main_xml(&doc)
        );
        assert!(doc.check().ok, "{ops} then {review}: {:?}", doc.check());
        out.push(doc);
    }
    let mut doc = open(p1);
    untracked(&mut doc, ops);
    assert_eq!(shape(&doc), want, "{ops}, untracked: {}", main_xml(&doc));
    assert!(doc.check().ok, "{ops} untracked: {:?}", doc.check());
    out.push(doc);
    let [a, b, c] = <[Document; 3]>::try_from(out).ok().unwrap();
    [a, b, c]
}

#[test]
fn a_replacement_that_ends_at_a_hyperlinks_end_stays_in_the_link() {
    let p1 = a_link();
    for (ops, want) in [
        (
            r#"{"op":"replace","ref":"p1","find":"the terms page","with":"our rules"}"#,
            "Read {link}our rules{/link} for details.",
        ),
        (
            r#"{"op":"replace","ref":"p1","find":"terms page","with":"rules"}"#,
            "Read {link}the rules{/link} for details.",
        ),
        (
            r#"{"op":"replace","ref":"p1","find":"page","with":"site"}"#,
            "Read {link}the terms site{/link} for details.",
        ),
        // Strictly inside, which already worked.
        (
            r#"{"op":"replace","ref":"p1","find":"terms","with":"rules"}"#,
            "Read {link}the rules page{/link} for details.",
        ),
    ] {
        for doc in every_way(&p1, ops, want) {
            assert!(has_link_rel(&doc), "{ops} keeps the link's relationship");
        }
    }
}

#[test]
fn a_replacement_that_ends_at_a_bookmarks_end_stays_in_the_bookmark() {
    let p1 = a_bookmark();
    for (ops, want) in [
        (
            r#"{"op":"replace","ref":"p1","find":"defined term","with":"new phrase"}"#,
            "The [new phrase] applies.",
        ),
        (
            r#"{"op":"replace","ref":"p1","find":"term","with":"word"}"#,
            "The [defined word] applies.",
        ),
        // At the bookmark's start, which already worked.
        (
            r#"{"op":"replace","ref":"p1","find":"defined","with":"agreed"}"#,
            "The [agreed term] applies.",
        ),
    ] {
        every_way(&p1, ops, want);
    }
}

#[test]
fn the_last_character_and_a_one_character_range_keep_their_replacement() {
    for perm in [false, true] {
        let long = a_range(perm, "Mr ", "Kowalsky", " signs.");
        let one = a_range(perm, "Lead ", "X", " tail.");
        for (p1, ops, want) in [
            (
                &long,
                r#"{"op":"replace","ref":"p1","start":10,"end":11,"with":"i"}"#,
                "Mr [Kowalski] signs.",
            ),
            // The workaround the reporter used still gives the same.
            (
                &long,
                r#"{"op":"replace","ref":"p1","start":10,"end":10,"with":"i"},{"op":"delete","ref":"p1","start":11,"end":12}"#,
                "Mr [Kowalski] signs.",
            ),
            (
                &one,
                r#"{"op":"replace","ref":"p1","start":5,"end":6,"with":"j"}"#,
                "Lead [j] tail.",
            ),
            (
                &one,
                r#"{"op":"replace","ref":"p1","start":5,"end":6,"with":"wX"}"#,
                "Lead [wX] tail.",
            ),
            // A pure insertion at either edge stays outside the range.
            (
                &one,
                r#"{"op":"replace","ref":"p1","start":5,"end":5,"with":"w"}"#,
                "Lead w[X] tail.",
            ),
            (
                &one,
                r#"{"op":"replace","ref":"p1","start":6,"end":6,"with":"w"}"#,
                "Lead [X]w tail.",
            ),
        ] {
            every_way(p1, ops, want);
        }
    }
}

#[test]
fn a_replacement_that_ends_at_a_controls_end_stays_in_the_control() {
    let p1 = a_control(false, "Name: ", "Old value", " applies.");
    for (ops, want) in [
        (
            r#"{"op":"replace","ref":"p1","find":"Old value","with":"New text"}"#,
            "Name: {ctl}New text{/ctl} applies.",
        ),
        (
            r#"{"op":"replace","ref":"p1","find":"value","with":"text"}"#,
            "Name: {ctl}Old text{/ctl} applies.",
        ),
        // At the control's start, which already worked.
        (
            r#"{"op":"replace","ref":"p1","find":"Old","with":"New"}"#,
            "Name: {ctl}New value{/ctl} applies.",
        ),
    ] {
        every_way(&p1, ops, want);
    }
}

#[test]
fn a_control_locked_against_deletion_or_not_keeps_a_replacement_at_its_end() {
    for locked in [false, true] {
        let p1 = a_control(locked, "Lead ", "Value text", " tail");
        for (ops, want) in [
            (
                r#"{"op":"replace","ref":"p1","start":11,"end":15,"with":"words"}"#,
                "Lead {ctl}Value words{/ctl} tail",
            ),
            (
                r#"{"op":"replace","ref":"p1","start":11,"end":11,"with":"words"},{"op":"delete","ref":"p1","start":16,"end":20}"#,
                "Lead {ctl}Value words{/ctl} tail",
            ),
            // A pure insertion at the control's end stays outside it.
            (
                r#"{"op":"replace","ref":"p1","start":15,"end":15,"with":" more"}"#,
                "Lead {ctl}Value text{/ctl} more tail",
            ),
        ] {
            every_way(&p1, ops, want);
        }
    }
}

#[test]
fn a_deletion_that_empties_a_hyperlink_removes_it_and_its_relationship() {
    let p1 = a_link();
    for (ops, want) in [
        (
            r#"{"op":"delete","ref":"p1","start":5,"end":19}"#,
            "Read  for details.",
        ),
        (
            r#"{"op":"delete","ref":"p1","start":3,"end":24}"#,
            "Readetails.",
        ),
        (
            r#"{"op":"replace","ref":"p1","with":"Completely new text"}"#,
            "Completely new text",
        ),
    ] {
        let [accepted, rejected, plain] = every_way(&p1, ops, want);
        for doc in [&accepted, &plain] {
            assert!(
                !main_xml(doc).contains("w:hyperlink"),
                "{ops}: {}",
                main_xml(doc)
            );
            assert!(!has_link_rel(doc), "{ops} removes the relationship");
        }
        assert!(
            has_link_rel(&rejected),
            "{ops}, rejected, keeps the relationship"
        );
        // Tracked, the link holds the deletion until it is accepted.
        let mut doc = open(&p1);
        run(&mut doc, ops);
        assert!(
            main_xml(&doc).contains("w:hyperlink") && has_link_rel(&doc),
            "{ops}"
        );
    }
}

#[test]
fn deleting_the_paragraph_of_the_last_link_removes_its_relationship() {
    let ops = r#"{"op":"delete","ref":"p1"}"#;
    let mut doc = open(&a_link());
    run(&mut doc, ops);
    assert!(
        has_link_rel(&doc),
        "a tracked delete keeps it until accepted"
    );
    let mut rejected = open(&a_link());
    run(&mut rejected, ops);
    run(&mut rejected, r#"{"op":"reject","all":true}"#);
    assert!(has_link_rel(&rejected));
    assert_eq!(
        shape(&rejected),
        "Read {link}the terms page{/link} for details."
    );
    run(&mut doc, r#"{"op":"accept","all":true}"#);
    assert!(!has_link_rel(&doc), "accepting the delete removes it");
    assert!(doc.check().ok, "{:?}", doc.check());
    let mut doc = open(&a_link());
    untracked(&mut doc, ops);
    assert!(!has_link_rel(&doc), "an untracked delete removes it");
    assert!(doc.check().ok, "{:?}", doc.check());
}

#[test]
fn a_relationship_that_another_link_still_uses_stays() {
    let p1 = format!("{}{}", a_link(), link(&wr("again")));
    let mut doc = open(&p1);
    untracked(&mut doc, r#"{"op":"delete","ref":"p1","start":5,"end":19}"#);
    assert_eq!(main_xml(&doc).matches("<w:hyperlink").count(), 1);
    assert!(has_link_rel(&doc));
}

#[test]
fn a_plain_delete_leaves_an_empty_bookmark_permission_range_or_control() {
    every_way(
        &a_bookmark(),
        r#"{"op":"delete","ref":"p1","find":"defined term"}"#,
        "The [] applies.",
    );
    every_way(
        &a_range(true, "Lead ", "X", " tail."),
        r#"{"op":"delete","ref":"p1","start":5,"end":6}"#,
        "Lead [] tail.",
    );
    every_way(
        &a_control(false, "Name: ", "Old value", " applies."),
        r#"{"op":"delete","ref":"p1","find":"Old value"}"#,
        "Name: {ctl}{/ctl} applies.",
    );
}
