//! Tests of `place_markers` and `remove_markers`.

use super::compiler::Compiler;
use super::stamp::Track;
use super::tests_support::*;
use super::types::MarkerSpec;
use crate::api::error::ErrorCode;
use crate::wordml::Mode;

fn comment(id: &str) -> MarkerSpec {
    MarkerSpec {
        start: pel("commentRangeStart", &[("id", id)], vec![]),
        end: pel("commentRangeEnd", &[("id", id)], vec![]),
        reference: Some(pel(
            "r",
            &[],
            vec![
                pel(
                    "rPr",
                    &[],
                    vec![pel("rStyle", &[("val", "CommentReference")], vec![])],
                ),
                pel("commentReference", &[("id", id)], vec![]),
            ],
        )),
    }
}

fn bookmark(id: &str) -> MarkerSpec {
    MarkerSpec {
        start: pel("bookmarkStart", &[("id", id), ("name", "bm")], vec![]),
        end: pel("bookmarkEnd", &[("id", id)], vec![]),
        reference: None,
    }
}

const REF7: &str = "<w:r><w:rPr><w:rStyle w:val=\"CommentReference\"/></w:rPr><w:commentReference w:id=\"7\"/></w:r>";

/// Places markers with a compiler of the given kind and returns the body XML and the texts.
fn place(
    body: &str,
    track: Track,
    para: usize,
    range: std::ops::Range<usize>,
    spec: &MarkerSpec,
) -> (
    Result<(), crate::api::error::StetError>,
    String,
    Vec<String>,
    super::outcome::Outcome,
) {
    let s = state(body);
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    let mut c = Compiler::new(&mut tx, track).unwrap();
    let res = c.place_markers(&ks[para], range, spec).map(|_| ());
    let out = c.finish().unwrap();
    (res, body_xml(&tx), texts(&tx, Mode::Final), out)
}

#[test]
fn markers_go_around_a_range_inside_a_run_after_cutting_it() {
    let (res, xml, fin, out) = place(
        &p(&r("Hello big world")),
        Track::Tracked(stamp()),
        0,
        6..9,
        &comment("7"),
    );
    res.unwrap();
    assert_eq!(
        xml,
        p(&format!(
            "{}<w:commentRangeStart w:id=\"7\"/>{}<w:commentRangeEnd w:id=\"7\"/>{REF7}{}",
            r("Hello "),
            r("big"),
            "<w:r><w:t xml:space=\"preserve\"> world</w:t></w:r>"
        ))
    );
    assert_eq!(fin, ["Hello big world"]);
    assert_eq!(out.minted, 0..0, "markers write no revision and take no id");
}

#[test]
fn markers_write_no_revision_when_the_edit_is_untracked_either() {
    let (res, xml, _, out) = place(&p(&r("Hello")), Track::Untracked, 0, 0..5, &bookmark("3"));
    res.unwrap();
    assert!(
        xml.starts_with("<w:p><w:bookmarkStart w:id=\"3\" w:name=\"bm\"/>"),
        "{xml}"
    );
    assert!(xml.ends_with("<w:bookmarkEnd w:id=\"3\"/></w:p>"), "{xml}");
    assert!(out.created.is_empty());
}

#[test]
fn an_empty_range_puts_both_markers_at_one_place() {
    let (res, xml, _, _) = place(
        &p(&r("abcd")),
        Track::Tracked(stamp()),
        0,
        2..2,
        &bookmark("3"),
    );
    res.unwrap();
    assert!(
        xml.contains("<w:bookmarkStart w:id=\"3\" w:name=\"bm\"/><w:bookmarkEnd w:id=\"3\"/>"),
        "{xml}"
    );
    let (res, xml, _, _) = place(
        &p(&r("abcd")),
        Track::Tracked(stamp()),
        0,
        0..0,
        &bookmark("3"),
    );
    res.unwrap();
    assert!(xml.starts_with("<w:p><w:bookmarkStart"), "{xml}");
    let (res, xml, _, _) = place(
        &p(&r("abcd")),
        Track::Tracked(stamp()),
        0,
        4..4,
        &bookmark("3"),
    );
    res.unwrap();
    assert!(xml.ends_with("<w:bookmarkEnd w:id=\"3\"/></w:p>"), "{xml}");
}

#[test]
fn markers_inside_another_authors_insertion_are_siblings_of_its_runs() {
    let body = p(&format!(
        "{}<w:ins {}>{}</w:ins>{}",
        r("ab "),
        eric(900),
        r("inserted text"),
        r(" cd")
    ));
    let (res, xml, _, _) = place(&body, Track::Tracked(stamp()), 0, 6..9, &comment("7"));
    res.unwrap();
    assert_eq!(
        xml.matches("<w:ins ").count(),
        1,
        "the insertion is not split: {xml}"
    );
    let a = xml.find("<w:ins ").unwrap();
    let b = xml.find("</w:ins>").unwrap();
    assert!(
        xml[a..b].contains("commentRangeStart") && xml[a..b].contains("commentRangeEnd"),
        "{xml}"
    );
}

#[test]
fn a_range_over_a_hyperlink_edge_puts_each_marker_in_its_own_container() {
    let body = p(&format!(
        "{}<w:hyperlink w:anchor=\"x\">{}</w:hyperlink>{}",
        r("ab "),
        r("link"),
        r(" cd")
    ));
    let (res, xml, _, _) = place(&body, Track::Tracked(stamp()), 0, 1..5, &bookmark("3"));
    res.unwrap();
    let h = xml.find("<w:hyperlink").unwrap();
    assert!(xml.find("bookmarkStart").unwrap() < h, "{xml}");
    assert!(xml.find("bookmarkEnd").unwrap() > h, "{xml}");
}

#[test]
fn a_field_is_covered_whole_or_refused() {
    let fld = format!(
        "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
        r("123")
    );
    let body = p(&format!("{}{}{}", r("a "), fld, r(" b")));
    let (res, xml, _, _) = place(&body, Track::Tracked(stamp()), 0, 2..5, &bookmark("3"));
    res.unwrap();
    assert!(
        xml.find("bookmarkStart").unwrap() < xml.find("fldCharType=\"begin\"").unwrap(),
        "{xml}"
    );
    assert!(
        xml.find("bookmarkEnd").unwrap() > xml.rfind("fldCharType=\"end\"").unwrap(),
        "{xml}"
    );
    let (res, ..) = place(&body, Track::Tracked(stamp()), 0, 3..4, &bookmark("3"));
    assert_eq!(res.unwrap_err().code(), ErrorCode::ReadonlyRegion);
    let (res, ..) = place(&body, Track::Tracked(stamp()), 0, 3..3, &bookmark("3"));
    assert_eq!(res.unwrap_err().code(), ErrorCode::ReadonlyRegion);
}

#[test]
fn a_range_outside_the_text_is_invalid() {
    let (res, ..) = place(
        &p(&r("abc")),
        Track::Tracked(stamp()),
        0,
        1..9,
        &bookmark("3"),
    );
    assert_eq!(res.unwrap_err().code(), ErrorCode::InvalidPayload);
}

#[test]
fn removing_markers_takes_the_ends_and_the_reference_run_and_leaves_the_rest() {
    let s = state(&p(&r("Hello big world")));
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    let mut c = tracked(&mut tx);
    c.place_markers(&ks[0], 6..9, &comment("7")).unwrap();
    c.place_markers(&ks[0], 0..5, &bookmark("8")).unwrap();
    c.remove_markers(&ks[0], &[7]).unwrap();
    c.finish().unwrap();
    let xml = body_xml(&tx);
    assert!(!xml.contains("comment"), "{xml}");
    assert!(
        xml.contains("bookmarkStart w:id=\"8\"") && xml.contains("bookmarkEnd w:id=\"8\""),
        "{xml}"
    );
    assert_eq!(texts(&tx, Mode::Final), ["Hello big world"]);
}

#[test]
fn a_deletion_of_the_commented_text_keeps_the_markers_paired_inside_the_del() {
    let s = state(&p(&r("Hello big world")));
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    let mut c = tracked(&mut tx);
    c.place_markers(&ks[0], 6..9, &comment("7")).unwrap();
    c.replace_text(&ks[0], 5..10, "").unwrap();
    c.finish().unwrap();
    let xml = body_xml(&tx);
    let a = xml.find("<w:del ").unwrap();
    let b = xml.find("</w:del>").unwrap();
    assert!(
        xml[a..b].contains("commentRangeStart") && xml[a..b].contains("commentRangeEnd"),
        "{xml}"
    );
    assert!(xml[a..b].contains("commentReference"), "{xml}");
    assert_eq!(texts(&tx, Mode::Final), ["Helloworld"]);
}

#[test]
fn new_text_at_the_end_of_a_commented_range_goes_after_the_end_marker_and_the_reference() {
    let s = state(&p(&r("Hello big world")));
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    let mut c = tracked(&mut tx);
    c.place_markers(&ks[0], 6..9, &comment("7")).unwrap();
    c.replace_text(&ks[0], 9..9, "X").unwrap();
    c.finish().unwrap();
    let xml = body_xml(&tx);
    assert!(
        xml.contains(&format!("<w:commentRangeEnd w:id=\"7\"/>{REF7}<w:ins ")),
        "{xml}"
    );
    assert_eq!(texts(&tx, Mode::Final), ["Hello bigX world"]);
}

// --- a document in the Strict namespaces

/// A state whose main part is in the Strict namespace of WordprocessingML.
fn strict_state(body: &str) -> crate::state::State {
    let transitional = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let strict = "http://purl.oclc.org/ooxml/wordprocessingml/main";
    let doc = stetdoc_testkit::fixtures::parts::Doc::new()
        .body(body)
        .root_attrs(EXTRA_NS)
        .bytes();
    let text = String::from_utf8(doc)
        .unwrap()
        .replace(transitional, strict);
    crate::state::State::open(
        stetdoc_testkit::fixtures::zip::Zip::new(stetdoc_testkit::fixtures::parts::entries_doc(
            text.as_bytes(),
        ))
        .build(),
        &crate::api::limits::Limits::default(),
    )
    .unwrap_or_else(|e| panic!("the strict body does not open: {e}"))
}

#[test]
fn markers_and_properties_that_a_caller_names_in_the_transitional_namespace_are_written_in_the_strict_one()
 {
    use super::types::{MarkerSpec, PropEdit, PropElement, PropTarget};
    const T: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let s = strict_state("<w:p><w:r><w:t>one two</w:t></w:r></w:p>");
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    let mut c = tracked(&mut tx);
    let marker = |local: &str| PropElement {
        ns: T.to_owned(),
        local: local.to_owned(),
        attrs: vec![(T.to_owned(), "id".to_owned(), "7".to_owned())],
        children: Vec::new(),
    };
    c.place_markers(
        &ks[0],
        0..3,
        &MarkerSpec {
            start: marker("bookmarkStart"),
            end: marker("bookmarkEnd"),
            reference: None,
        },
    )
    .unwrap();
    c.set_props(
        &PropTarget::Run {
            block: ks[0].clone(),
            range: 4..7,
        },
        &PropEdit {
            set: vec![PropElement {
                ns: T.to_owned(),
                local: "b".to_owned(),
                attrs: Vec::new(),
                children: Vec::new(),
            }],
            clear: Vec::new(),
        },
    )
    .unwrap();
    c.finish().unwrap();
    let xml = body_xml(&tx);
    assert!(!xml.contains("ns:") && !xml.contains(T), "{xml}");
    assert!(
        xml.contains("<w:bookmarkStart w:id=\"7\"/>") && xml.contains("<w:b/>"),
        "{xml}"
    );
}

// --- a comment that spans paragraphs: a reply beside it, and its markers out of a part

/// A comment on two paragraphs: the start in the first, the end and the reference in the second.
fn spanning() -> String {
    format!(
        "<w:p><w:commentRangeStart w:id=\"5\"/>{}</w:p><w:p>{}<w:commentRangeEnd w:id=\"5\"/><w:r><w:commentReference w:id=\"5\"/></w:r></w:p>",
        r("first"),
        r("second")
    )
}

#[test]
fn a_reply_goes_beside_the_markers_of_a_comment_that_spans_paragraphs() {
    let s = state(&spanning());
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    let mut c = Compiler::new(&mut tx, Track::Tracked(stamp())).unwrap();
    let fx = c
        .place_markers_beside(&std::sync::Arc::from(MAIN), 5, &comment("6"))
        .unwrap();
    c.finish().unwrap();
    assert_eq!(fx.touched, ks, "both paragraphs are touched");
    let xml = body_xml(&tx);
    // The start of the reply is right after the start of the comment, in the first paragraph, and its end
    // and reference are right after the comment's reference, in the second.
    assert!(
        xml.contains("<w:commentRangeStart w:id=\"5\"/><w:commentRangeStart w:id=\"6\"/><w:r><w:t xml:space=\"preserve\">first"),
        "{xml}"
    );
    assert!(
        xml.contains("<w:commentReference w:id=\"5\"/></w:r><w:commentRangeEnd w:id=\"6\"/><w:r><w:rPr><w:rStyle w:val=\"CommentReference\"/></w:rPr><w:commentReference w:id=\"6\"/></w:r></w:p>"),
        "{xml}"
    );
    assert_eq!(texts(&tx, Mode::Final), ["first", "second"]);
}

#[test]
fn a_reply_beside_a_comment_that_is_not_there_is_refused() {
    let s = state(&p(&r("text")));
    let mut tx = s.begin();
    let mut c = Compiler::new(&mut tx, Track::Tracked(stamp())).unwrap();
    let err = c
        .place_markers_beside(&std::sync::Arc::from(MAIN), 5, &comment("6"))
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::InvalidPayload);
}

#[test]
fn the_markers_of_a_comment_are_removed_from_a_whole_part_wherever_they_are() {
    // The two ends are in two paragraphs, a bookmark stands between the blocks, and the other comment
    // stays.
    let body = format!(
        "{}<w:bookmarkStart w:id=\"7\" w:name=\"b\"/><w:bookmarkEnd w:id=\"7\"/><w:p>{}<w:commentRangeStart w:id=\"8\"/>{}<w:commentRangeEnd w:id=\"8\"/></w:p>",
        spanning(),
        r("third"),
        r("fourth")
    );
    let s = state(&body);
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    let mut c = Compiler::new(&mut tx, Track::Tracked(stamp())).unwrap();
    let fx = c
        .remove_markers_in_part(&std::sync::Arc::from(MAIN), &[5, 7])
        .unwrap();
    c.finish().unwrap();
    let xml = body_xml(&tx);
    assert!(
        !xml.contains("w:id=\"5\"") && !xml.contains("w:id=\"7\""),
        "{xml}"
    );
    assert!(
        xml.contains("<w:commentRangeStart w:id=\"8\"/>")
            && xml.contains("<w:commentRangeEnd w:id=\"8\"/>"),
        "{xml}"
    );
    assert!(
        !xml.contains("w:p></w:p>") && xml.matches("<w:p>").count() == 3,
        "{xml}"
    );
    assert_eq!(
        fx.touched,
        ks[..2].to_vec(),
        "the two paragraphs that held them"
    );
    assert_eq!(texts(&tx, Mode::Final), ["first", "second", "thirdfourth"]);
}

// --- a marker whose partner survives, when its region goes

fn region_with_a_start() -> String {
    format!(
        "<w:p><w:commentRangeStart w:id=\"5\"/>{}</w:p><w:p>{}<w:commentRangeEnd w:id=\"5\"/><w:r><w:commentReference w:id=\"5\"/></w:r></w:p>",
        r("gone"),
        r("kept")
    )
}

#[test]
fn an_untracked_deletion_of_a_paragraph_hoists_the_start_of_a_range_whose_end_survives() {
    let s = with_compiler(&region_with_a_start(), Track::Untracked, |c, _, ks| {
        vec![c.delete_block(&ks[0])]
    });
    assert!(s.effects[0].is_ok(), "{:?}", s.effects);
    assert_eq!(s.fin, ["kept"], "{}", s.xml);
    assert_eq!(s.xml.matches("w:commentRangeStart").count(), 1, "{}", s.xml);
    assert_eq!(s.xml.matches("w:commentRangeEnd").count(), 1, "{}", s.xml);
    let (a, b) = (
        s.xml.find("commentRangeStart").unwrap(),
        s.xml.find("commentRangeEnd").unwrap(),
    );
    assert!(a < b, "the start is still before the end: {}", s.xml);
}

#[test]
fn accepting_the_deletion_of_text_hoists_the_start_of_a_range_whose_end_survives() {
    let body = region_with_a_start();
    let tracked = with_compiler(&body, Track::Tracked(stamp()), |c, _, ks| {
        vec![c.delete_block(&ks[0])]
    });
    assert!(tracked.effects[0].is_ok());
    let s = state(&body);
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    {
        let mut c = Compiler::new(&mut tx, Track::Tracked(stamp())).unwrap();
        c.delete_block(&ks[0]).unwrap();
        c.finish().unwrap();
    }
    super::resolve::resolve(
        &mut tx,
        &mut super::index::ChangeIndex::new(),
        &[super::types::Step {
            action: super::types::Action::Accept,
            selector: super::types::Selector::All,
        }],
    )
    .unwrap();
    let xml = body_xml(&tx);
    assert_eq!(xml.matches("w:commentRangeStart").count(), 1, "{xml}");
    assert_eq!(xml.matches("w:commentRangeEnd").count(), 1, "{xml}");
    assert!(
        xml.find("commentRangeStart").unwrap() < xml.find("commentRangeEnd").unwrap(),
        "{xml}"
    );
    assert_eq!(texts(&tx, Mode::Final), ["kept"]);
}

#[test]
fn an_untracked_deletion_of_a_paragraph_hoists_the_end_of_a_range_whose_start_survives() {
    // The other way round: the range starts in a paragraph that stays and ends in the one that goes. The
    // end moves back to the end of the paragraph before, and no marker is left without its partner.
    let body = format!(
        "<w:p><w:commentRangeStart w:id=\"5\"/>{}</w:p><w:p>{}<w:commentRangeEnd w:id=\"5\"/></w:p><w:p>{}</w:p>",
        r("kept"),
        r("gone"),
        r("after")
    );
    let s = with_compiler(&body, Track::Untracked, |c, _, ks| {
        vec![c.delete_block(&ks[1])]
    });
    assert!(s.effects[0].is_ok(), "{:?}", s.effects);
    assert_eq!(s.fin, ["kept", "after"], "{}", s.xml);
    assert_eq!(s.xml.matches("w:commentRangeStart").count(), 1, "{}", s.xml);
    assert_eq!(s.xml.matches("w:commentRangeEnd").count(), 1, "{}", s.xml);
    assert!(
        s.xml.find("commentRangeStart").unwrap() < s.xml.find("commentRangeEnd").unwrap(),
        "{}",
        s.xml
    );
}
