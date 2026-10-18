//! The import, one fixture for each row of design section 7: styles that are equivalent, missing and collided, lists that map
//! and lists that are new, hyperlinks, pictures, notes, bookmarks, comments, simple fields, and a fragment of a paragraph.

use super::import::{Cx, Importer};
use super::tests_pkg::Pk;
use crate::revisions::compiler::Compiler;
use crate::revisions::stamp::Track;
use crate::state::Tx;
use crate::wordml::NodeId;
use std::sync::Arc;

const BODY: &str = "word/document.xml";

/// The part's XML, as the transaction sees it.
fn xml_of(tx: &Tx<'_>, part: &str) -> String {
    let v = tx.parts();
    let id = v.id(part).unwrap_or_else(|| panic!("no part {part}"));
    String::from_utf8(v.tree(id).unwrap().serialize()).unwrap()
}

fn body_blocks(state: &crate::state::State) -> Vec<NodeId> {
    let v = state.parts();
    let id = v.id(BODY).unwrap();
    let tree = v.tree(id).unwrap();
    tree.root()
        .child_elements()
        .find(|c| c.local_name() == "body")
        .unwrap()
        .child_elements()
        .filter(|c| matches!(c.local_name(), "p" | "tbl"))
        .map(|c| c.id())
        .collect()
}

struct Out {
    body: String,
    stats: super::import::Stats,
    tx_xml: Vec<(String, String)>,
}

/// Copies the first block of the original's body into the revised body (as its last block) and reports what came out.
fn copy_first(orig: &Pk, rev: &Pk, parts: &[&str]) -> Out {
    let (so, sr) = (orig.state(), rev.state());
    let mut tx = sr.begin();
    let mut imp = Importer::new(&so);
    let body: Arc<str> = Arc::from(BODY);
    let cx = Cx {
        src: &body,
        dst: &body,
    };
    let first = body_blocks(&so)[0];
    let copied = {
        let mut c = Compiler::new(&mut tx, Track::Untracked).unwrap();
        imp.block(&mut c.import_writer(), cx, first).unwrap()
    };
    {
        let tree = tx.part_mut(BODY).unwrap();
        let b = tree
            .root()
            .child_elements()
            .find(|c| c.local_name() == "body")
            .unwrap()
            .id();
        let mut ed = tree.edit();
        ed.append_child(b, copied.node).unwrap();
    }
    Out {
        body: xml_of(&tx, BODY),
        stats: imp.stats.clone(),
        tx_xml: parts
            .iter()
            .map(|p| (p.to_string(), xml_of(&tx, p)))
            .collect(),
    }
}

fn style(id: &str, name: &str, extra: &str, based: Option<&str>) -> String {
    let b = based.map_or(String::new(), |b| format!("<w:basedOn w:val=\"{b}\"/>"));
    format!(
        "<w:style w:type=\"paragraph\" w:styleId=\"{id}\"><w:name w:val=\"{name}\"/>{b}{extra}</w:style>"
    )
}

fn styled(id: &str, text: &str) -> String {
    format!("<w:p><w:pPr><w:pStyle w:val=\"{id}\"/></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>")
}

#[test]
fn a_style_that_is_the_same_in_both_documents_is_not_imported() {
    let def = style("Quote", "Quote", "<w:rPr><w:i/></w:rPr>", None);
    let o = Pk {
        styles: Some(def.clone()),
        ..Pk::new(&styled("Quote", "x"))
    };
    let r = Pk {
        styles: Some(def),
        ..Pk::new("<w:p/>")
    };
    let out = copy_first(&o, &r, &["word/styles.xml"]);
    assert_eq!(out.stats.styles_imported, 0, "{:?}", out.tx_xml);
    assert!(
        out.body.contains("w:pStyle w:val=\"Quote\""),
        "{}",
        out.body
    );
}

#[test]
fn a_style_the_revised_document_lacks_is_imported_with_the_style_it_is_based_on() {
    let defs = format!(
        "{}{}",
        style("Base", "Base", "<w:rPr><w:b/></w:rPr>", None),
        style(
            "Child",
            "Child",
            "<w:next w:val=\"Base\"/><w:rPr><w:i/></w:rPr>",
            Some("Base")
        )
    );
    let o = Pk {
        styles: Some(defs),
        ..Pk::new(&styled("Child", "x"))
    };
    let r = Pk {
        styles: Some(style("Other", "Other", "", None)),
        ..Pk::new("<w:p/>")
    };
    let out = copy_first(&o, &r, &["word/styles.xml"]);
    let styles = &out.tx_xml[0].1;
    assert!(
        styles.contains("w:styleId=\"Child\"") && styles.contains("w:styleId=\"Base\""),
        "{styles}"
    );
    assert!(!styles.contains("w:next"), "next is dropped\n{styles}");
    assert_eq!(out.stats.styles_imported, 2);
}

#[test]
fn a_style_with_another_definition_is_imported_under_a_new_id_and_a_marked_name() {
    let o = Pk {
        styles: Some(style("Quote", "Quote", "<w:rPr><w:i/></w:rPr>", None)),
        ..Pk::new(&styled("Quote", "x"))
    };
    let r = Pk {
        styles: Some(style("Quote", "Quote", "<w:rPr><w:b/></w:rPr>", None)),
        ..Pk::new("<w:p/>")
    };
    let out = copy_first(&o, &r, &["word/styles.xml"]);
    assert!(
        out.body.contains("w:pStyle w:val=\"QuoteOriginal\""),
        "{}",
        out.body
    );
    let styles = &out.tx_xml[0].1;
    assert!(
        styles.contains("w:styleId=\"QuoteOriginal\"") && styles.contains("Quote (original)"),
        "{styles}"
    );
    assert_eq!(out.stats.styles_renamed, 1);
}

fn abstract_num(id: u32, fmt: &str) -> String {
    format!(
        "<w:abstractNum w:abstractNumId=\"{id}\"><w:nsid w:val=\"0000000{id}\"/><w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{fmt}\"/><w:lvlText w:val=\"%1.\"/></w:lvl></w:abstractNum>"
    )
}

fn num(id: u32, abs: u32) -> String {
    format!("<w:num w:numId=\"{id}\"><w:abstractNumId w:val=\"{abs}\"/></w:num>")
}

fn listed(id: u32, text: &str) -> String {
    format!(
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"{id}\"/></w:numPr></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"
    )
}

#[test]
fn a_list_with_the_same_definition_maps_to_the_revised_document_s_list() {
    let o = Pk {
        numbering: Some(format!("{}{}", abstract_num(0, "decimal"), num(1, 0))),
        ..Pk::new(&listed(1, "x"))
    };
    let r = Pk {
        numbering: Some(format!(
            "{}{}{}",
            abstract_num(0, "lowerLetter"),
            abstract_num(5, "decimal"),
            [num(2, 0), num(7, 5)].concat()
        )),
        ..Pk::new("<w:p/>")
    };
    let out = copy_first(&o, &r, &["word/numbering.xml"]);
    assert!(out.body.contains("w:numId w:val=\"7\""), "{}", out.body);
    assert_eq!(out.stats.lists_imported, 0);
}

#[test]
fn a_list_the_revised_document_does_not_have_is_imported_under_fresh_ids() {
    let o = Pk {
        numbering: Some(format!("{}{}", abstract_num(0, "upperRoman"), num(1, 0))),
        ..Pk::new(&listed(1, "x"))
    };
    let r = Pk {
        numbering: Some(format!("{}{}", abstract_num(0, "decimal"), num(1, 0))),
        ..Pk::new("<w:p/>")
    };
    let out = copy_first(&o, &r, &["word/numbering.xml"]);
    let numbering = &out.tx_xml[0].1;
    assert!(out.body.contains("w:numId w:val=\"2\""), "{}", out.body);
    assert!(
        numbering.contains("w:abstractNumId=\"1\"")
            && numbering.contains("upperRoman")
            && numbering.contains("w:numId=\"2\""),
        "{numbering}"
    );
    assert!(
        numbering.find("w:abstractNumId=\"1\"").unwrap() < numbering.find("<w:num ").unwrap(),
        "abstract numberings come first\n{numbering}"
    );
    assert_eq!(out.stats.lists_imported, 1);
}

fn link(rid: &str, text: &str) -> String {
    format!("<w:p><w:hyperlink r:id=\"{rid}\"><w:r><w:t>{text}</w:t></w:r></w:hyperlink></w:p>")
}

#[test]
fn a_hyperlink_keeps_its_address_when_the_revised_part_has_one_and_loses_the_link_when_it_does_not()
{
    let ext = |id: &str, url: &str| (id.to_owned(), "hyperlink".to_owned(), url.to_owned());
    let o = Pk {
        external: vec![ext("rId20", "https://example.com/a")],
        ..Pk::new(&link("rId20", "click"))
    };
    let same = Pk {
        external: vec![ext("rId31", "https://example.com/a")],
        ..Pk::new("<w:p/>")
    };
    let out = copy_first(&o, &same, &[]);
    assert!(
        out.body.contains("r:id=\"rId31\"") && out.body.contains("click"),
        "{}",
        out.body
    );
    let other = Pk {
        external: vec![ext("rId31", "https://example.com/b")],
        ..Pk::new("<w:p/>")
    };
    let out = copy_first(&o, &other, &[]);
    assert!(
        !out.body.contains("hyperlink") && out.body.contains("click"),
        "{}",
        out.body
    );
    assert_eq!(out.stats.hyperlinks_unlinked, 1);
}

#[test]
fn a_picture_is_copied_with_its_part_once_for_the_same_bytes() {
    let drawing = |id: u32, rid: &str| {
        format!(
            "<w:r><w:drawing><wp:inline><wp:docPr id=\"{id}\" name=\"p\"/><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed=\"{rid}\"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
        )
    };
    let body = format!("<w:p>{}{}</w:p>", drawing(1, "rId40"), drawing(2, "rId40"));
    let o = Pk {
        media: vec![(
            "media/image1.png".into(),
            String::new(),
            vec![1, 2, 3, 4],
            "rId40".into(),
        )],
        ..Pk::new(&body)
    };
    let r = Pk {
        media: vec![(
            "media/image1.png".into(),
            String::new(),
            vec![9, 9],
            "rId41".into(),
        )],
        ..Pk::new(
            "<w:p><w:r><w:drawing><wp:inline><wp:docPr id=\"7\" name=\"q\"/></wp:inline></w:drawing></w:r></w:p>",
        )
    };
    let out = copy_first(&o, &r, &[]);
    assert_eq!(out.stats.parts_copied, 1, "{}", out.body);
    assert_eq!(out.body.matches("r:embed=\"").count(), 2, "{}", out.body);
    // drawing ids are unique across the revised document: 7 is taken
    assert!(
        !out.body.contains("id=\"1\" name=\"p\"") && !out.body.contains("id=\"2\" name=\"p\""),
        "{}",
        out.body
    );
}

#[test]
fn a_note_reference_brings_its_note_with_a_new_id_and_the_note_is_reported_to_the_build() {
    let notes = "<w:footnote w:id=\"1\"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t>the note</w:t></w:r></w:p></w:footnote>";
    let body = "<w:p><w:r><w:t>text</w:t></w:r><w:r><w:footnoteReference w:id=\"1\"/></w:r></w:p>";
    let o = Pk {
        footnotes: Some(notes.to_owned()),
        ..Pk::new(body)
    };
    let r = Pk {
        footnotes: Some(
            "<w:footnote w:id=\"1\"><w:p><w:r><w:t>other</w:t></w:r></w:p></w:footnote>".into(),
        ),
        ..Pk::new("<w:p/>")
    };
    let (so, sr) = (o.state(), r.state());
    let mut tx = sr.begin();
    let mut imp = Importer::new(&so);
    let body_part: Arc<str> = Arc::from(BODY);
    let mut comp = Compiler::new(&mut tx, Track::Tracked(super::tests_laws::stamp())).unwrap();
    let c = imp
        .block(
            &mut comp.import_writer(),
            Cx {
                src: &body_part,
                dst: &body_part,
            },
            body_blocks(&so)[0],
        )
        .unwrap();
    // the note is added with its paragraphs deleted once the copy that refers to it is in the story
    let fresh = imp.take_new_notes();
    assert_eq!(fresh.len(), 1);
    assert_eq!(fresh[0].paragraphs.len(), 1);
    assert_eq!(fresh[0].holders, vec![c.node]);
    let blocks: Vec<crate::revisions::redline_types::Imported> = fresh[0]
        .paragraphs
        .iter()
        .map(|p| crate::revisions::redline_types::Imported {
            node: p.node,
            source: p.source.clone(),
        })
        .collect();
    comp.add_deleted_note(&fresh[0].part, false, fresh[0].id, &blocks)
        .unwrap();
    drop(comp);
    {
        let tree = tx.part_mut(BODY).unwrap();
        let b = tree
            .root()
            .child_elements()
            .find(|c| c.local_name() == "body")
            .unwrap()
            .id();
        tree.edit().append_child(b, c.node).unwrap();
    }
    assert!(
        xml_of(&tx, BODY).contains("w:footnoteReference w:id=\"2\""),
        "{}",
        xml_of(&tx, BODY)
    );
    let notes_xml = xml_of(&tx, "word/footnotes.xml");
    assert!(
        notes_xml.contains("w:id=\"2\"")
            && notes_xml.contains("the note")
            && notes_xml.contains("other"),
        "{notes_xml}"
    );
    assert!(notes_xml.contains("w:delText"), "{notes_xml}");
}

#[test]
fn bookmarks_get_new_ids_and_a_name_the_revised_document_has_is_dropped_with_its_end() {
    let bm = |id: u32, name: &str| {
        format!(
            "<w:bookmarkStart w:id=\"{id}\" w:name=\"{name}\"/><w:r><w:t>t</w:t></w:r><w:bookmarkEnd w:id=\"{id}\"/>"
        )
    };
    let body = format!(
        "<w:p>{}{}{}</w:p>",
        bm(0, "keep"),
        bm(1, "taken"),
        bm(2, "_GoBack")
    );
    let o = Pk::new(&body);
    let r = Pk::new(
        "<w:p><w:bookmarkStart w:id=\"5\" w:name=\"taken\"/><w:bookmarkEnd w:id=\"5\"/></w:p>",
    );
    let out = copy_first(&o, &r, &[]);
    assert_eq!(
        out.body.matches("bookmarkStart").count(),
        2,
        "taken stays once, keep arrives\n{}",
        out.body
    );
    assert!(
        out.body.contains("w:name=\"keep\"") && !out.body.contains("_GoBack"),
        "{}",
        out.body
    );
    assert_eq!(out.stats.bookmarks_dropped, 1);
    // the new id is not 0 (taken by the revised ids' counter): it is one past the highest
    assert!(
        out.body.contains("w:name=\"keep\"") && !out.body.contains("w:id=\"0\" w:name=\"keep\""),
        "{}",
        out.body
    );
}

#[test]
fn a_vml_shape_gets_an_id_no_shape_of_the_revised_document_has_and_its_spid_follows() {
    let pict = |id: u32| {
        format!(
            "<w:r><w:pict xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\"><v:shapetype id=\"_x0000_t202\"/><v:shape id=\"_x0000_s{id}\" o:spid=\"_x0000_s{id}\" type=\"#_x0000_t202\"/></w:pict></w:r>"
        )
    };
    let o = Pk::new(&format!("<w:p>{}</w:p>", pict(1026)));
    let r = Pk::new(&format!("<w:p>{}</w:p>", pict(1030)));
    let out = copy_first(&o, &r, &[]);
    assert_eq!(out.body.matches("_x0000_s1031").count(), 2, "{}", out.body);
    assert!(!out.body.contains("_x0000_s1026"), "{}", out.body);
    assert_eq!(out.body.matches("_x0000_t202").count(), 4, "{}", out.body);
}

#[test]
fn comments_are_dropped_and_counted() {
    let body = "<w:p><w:commentRangeStart w:id=\"3\"/><w:r><w:t>text</w:t></w:r><w:commentRangeEnd w:id=\"3\"/><w:r><w:commentReference w:id=\"3\"/></w:r></w:p>";
    let out = copy_first(&Pk::new(body), &Pk::new("<w:p/>"), &[]);
    assert!(
        !out.body.contains("comment") && out.body.contains("text"),
        "{}",
        out.body
    );
    assert_eq!(out.stats.comments_dropped, 3);
}

#[test]
fn a_simple_field_becomes_the_runs_of_a_complex_field() {
    let body = "<w:p><w:fldSimple w:instr=\" PAGE \"><w:r><w:t>7</w:t></w:r></w:fldSimple></w:p>";
    let out = copy_first(&Pk::new(body), &Pk::new("<w:p/>"), &[]);
    for want in [
        "fldCharType=\"begin\"",
        "fldCharType=\"separate\"",
        "fldCharType=\"end\"",
        "<w:instrText xml:space=\"preserve\"> PAGE </w:instrText>",
        "<w:t>7</w:t>",
    ] {
        assert!(out.body.contains(want), "{want}\n{}", out.body);
    }
    assert!(!out.body.contains("fldSimple"), "{}", out.body);
}

#[test]
fn a_fragment_keeps_the_formatting_and_the_wrapper_of_the_text_it_covers_and_nothing_else() {
    let body = "<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:r><w:t xml:space=\"preserve\">one two </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">three four</w:t></w:r><w:bookmarkStart w:id=\"1\" w:name=\"b\"/><w:bookmarkEnd w:id=\"1\"/></w:p>";
    let (so, sr) = (Pk::new(body).state(), Pk::new("<w:p/>").state());
    let mut tx = sr.begin();
    let mut imp = Importer::new(&so);
    let part: Arc<str> = Arc::from(BODY);
    let mut comp = Compiler::new(&mut tx, Track::Untracked).unwrap();
    let nodes = imp
        .fragment(
            &mut comp.import_writer(),
            Cx {
                src: &part,
                dst: &part,
            },
            body_blocks(&so)[0],
            4..13,
        )
        .unwrap();
    drop(comp);
    assert_eq!(nodes.len(), 2);
    let v = tx.parts();
    let tree = v.tree(v.id(BODY).unwrap()).unwrap();
    let texts: Vec<String> = nodes
        .iter()
        .map(|&n| tree.node_ref(n).text_content())
        .collect();
    assert_eq!(texts, ["two ", "three"]);
    assert!(
        tree.node_ref(nodes[1])
            .child_elements()
            .any(|c| c.local_name() == "rPr")
    );
}

#[test]
fn the_same_inputs_give_the_same_bytes() {
    let o = Pk {
        styles: Some(style("Quote", "Quote", "", None)),
        ..Pk::new(&styled("Quote", "x"))
    };
    let r = Pk {
        styles: Some(style("Quote", "Quote", "<w:rPr><w:b/></w:rPr>", None)),
        ..Pk::new("<w:p/>")
    };
    let a = copy_first(&o, &r, &["word/styles.xml"]);
    let b = copy_first(&o, &r, &["word/styles.xml"]);
    assert_eq!((a.body, a.tx_xml), (b.body, b.tx_xml));
}

#[test]
fn the_import_writer_refuses_a_story_and_writes_the_style_sheet() {
    let r = Pk {
        styles: Some(style("Body", "Body", "", None)),
        ..Pk::new("<w:p/>")
    };
    let s = r.state();
    let mut tx = s.begin();
    let mut c = Compiler::new(&mut tx, Track::Untracked).unwrap();
    let mut w = c.import_writer();
    assert!(w.part_mut(BODY).is_err());
    assert!(w.part_mut("word/styles.xml").is_ok());
}
