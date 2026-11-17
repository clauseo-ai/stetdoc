//! The differences compare once left out without a word (decision 0022): a hyperlink's address, a comment's text, and the
//! properties of tables, rows and cells. Each fixture is held to the compare laws (`check_pair`), whose dump now reads the
//! addresses, the comments and the table properties, and to the shape of the XML.

use super::tests_laws::{Verdict, check_pair};
use super::tests_pkg::Pk;

fn xml_of(v: &Verdict, part: &str) -> String {
    let view = v.redline.parts();
    let id = view.id(part).expect("the part");
    String::from_utf8_lossy(&view.tree(id).expect("a tree").serialize()).into_owned()
}

fn run(a: &Pk, b: &Pk) -> (Verdict, String) {
    let v = check_pair(a.bytes(), b.bytes()).expect("compares");
    assert!(v.failures.is_empty(), "{}", v.failures.join("\n"));
    assert_eq!(v.receipt.broken, 0);
    assert!(
        v.receipt.exclusions.is_empty(),
        "{:?}",
        v.receipt.exclusions
    );
    let xml = xml_of(&v, "word/document.xml");
    (v, xml)
}

fn linked(attrs: &str, text: &str) -> String {
    format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">Pay at </w:t></w:r><w:hyperlink {attrs}><w:r><w:t>{text}</w:t></w:r></w:hyperlink><w:r><w:t>.</w:t></w:r></w:p>"
    )
}

fn with_url(body: &str, url: &str) -> Pk {
    Pk {
        external: vec![("rId20".into(), "hyperlink".into(), url.into())],
        ..Pk::new(body)
    }
}

#[test]
fn a_changed_hyperlink_address_is_the_old_link_deleted_beside_the_new_one_inserted() {
    let body = linked("r:id=\"rId20\"", "the payment page");
    let a = with_url(&body, "https://pay.example.com/acct-111");
    let b = with_url(&body, "https://pay.example.com/acct-999");
    let (v, xml) = run(&a, &b);
    // the old link, its text deleted, then the revised link, its text inserted: two links side by side, neither in the other
    let old = xml.find("<w:del ").expect("a deletion");
    let new = xml
        .find("<w:hyperlink r:id=\"rId20\"><w:ins ")
        .expect("the revised link");
    assert!(old < new, "{xml}");
    assert_eq!(xml.matches("<w:hyperlink ").count(), 2, "{xml}");
    assert!(
        !xml.contains("</w:del></w:hyperlink></w:hyperlink>"),
        "{xml}"
    );
    // the old address came with a relationship of its own, and no link lost its address
    let rels = {
        let view = v.redline.parts();
        let id = view.id("word/document.xml").expect("a body");
        view.rels(id)
            .expect("rels")
            .iter()
            .map(|r| format!("{:?}", r.target))
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert!(
        rels.contains("acct-111") && rels.contains("acct-999"),
        "{rels}"
    );
    assert_eq!(v.receipt.stats.hyperlinks_unlinked, 0);
}

#[test]
fn a_changed_anchor_and_a_link_added_to_plain_text_are_changes_too() {
    let a = Pk::new(&linked("w:anchor=\"clause_1\"", "clause one"));
    let b = Pk::new(&linked("w:anchor=\"clause_2\"", "clause one"));
    let (_, xml) = run(&a, &b);
    assert!(
        xml.contains("<w:hyperlink w:anchor=\"clause_1\"><w:del "),
        "{xml}"
    );
    let plain =
        Pk::new("<w:p><w:r><w:t xml:space=\"preserve\">Pay at clause one.</w:t></w:r></w:p>");
    let (_, xml) = run(&plain, &b);
    assert!(
        xml.contains("<w:hyperlink w:anchor=\"clause_2\"><w:ins "),
        "{xml}"
    );
    assert!(xml.contains(">clause one</w:delText>"), "{xml}");
}

#[test]
fn a_word_changed_inside_a_link_with_the_same_address_stays_a_word_change() {
    let a = with_url(
        &linked("r:id=\"rId20\"", "the old payment page"),
        "https://pay.example.com/",
    );
    let b = with_url(
        &linked("r:id=\"rId20\"", "the new payment page"),
        "https://pay.example.com/",
    );
    let (_, xml) = run(&a, &b);
    assert_eq!(xml.matches("<w:hyperlink ").count(), 1, "{xml}");
    assert!(
        xml.contains("<w:delText>old</w:delText>") || xml.contains(">old</w:delText>"),
        "{xml}"
    );
}

fn commented(text: &str, author: &str) -> Pk {
    Pk {
        comments: Some(format!(
            "<w:comment w:id=\"0\" w:author=\"{author}\" w:initials=\"A\" w:date=\"2026-01-01T00:00:00Z\"><w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p></w:comment>"
        )),
        ..Pk::new(
            "<w:p><w:commentRangeStart w:id=\"0\"/><w:r><w:t xml:space=\"preserve\">The second clause</w:t></w:r><w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r><w:r><w:t xml:space=\"preserve\"> has a comment.</w:t></w:r></w:p>",
        )
    }
}

#[test]
fn an_edited_comment_has_its_changes_tracked_inside_the_comment() {
    let a = commented("Comment original words.", "Ann");
    let b = commented("Comment revised words.", "Ann");
    let (v, body) = run(&a, &b);
    assert!(
        !body.contains("<w:ins ") && !body.contains("<w:del "),
        "{body}"
    );
    let comments = xml_of(&v, "word/comments.xml");
    assert!(
        comments.contains("<w:delText xml:space=\"preserve\">original</w:delText>")
            && comments.contains("<w:ins ")
            && comments.contains(">revised</w:t>"),
        "{comments}"
    );
    assert!(v.receipt.notices.is_empty(), "{:?}", v.receipt.notices);
}

#[test]
fn a_comment_whose_author_changed_is_named_in_a_notice() {
    let a = commented("Comment words.", "Ann");
    let b = commented("Comment words.", "Bea");
    let (v, _) = run(&a, &b);
    let [n] = &v.receipt.notices[..] else {
        panic!("{:?}", v.receipt.notices)
    };
    assert_eq!(n.code, "compare_not_tracked");
    assert!(
        n.message.contains("\"Ann\"") && n.message.contains("\"Bea\""),
        "{}",
        n.message
    );
    assert_eq!(&**n.block.part(), "word/comments.xml");
}

fn table(tbl: &str, tr: &str, tc: &str) -> Pk {
    let cell = |t: &str, x: &str| {
        format!(
            "<w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/>{x}</w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"
        )
    };
    let trpr = if tr.is_empty() {
        String::new()
    } else {
        format!("<w:trPr>{tr}</w:trPr>")
    };
    Pk::new(&format!(
        "<w:p><w:r><w:t>Schedule.</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/>{tbl}</w:tblPr><w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr>{trpr}{}{}</w:tr><w:tr>{}{}</w:tr></w:tbl><w:p><w:r><w:t>End.</w:t></w:r></w:p>",
        cell("Party", tc),
        cell("Acme Ltd", ""),
        cell("Term", ""),
        cell("Two years", "")
    ))
}

#[test]
fn changed_table_row_and_cell_properties_are_tracked_with_the_original_s() {
    let a = table("", "", "");
    let b = table(
        "<w:jc w:val=\"center\"/>",
        "<w:cantSplit/>",
        "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"FFFF00\"/>",
    );
    let (v, xml) = run(&a, &b);
    assert_eq!(v.receipt.counts.props, 3);
    assert!(
        xml.contains("<w:tblPrChange ")
            && xml
                .contains("<w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr></w:tblPrChange>"),
        "{xml}"
    );
    assert!(
        xml.contains("<w:cantSplit/><w:trPrChange ") && xml.contains("<w:trPr/></w:trPrChange>"),
        "{xml}"
    );
    assert!(xml.contains("w:fill=\"FFFF00\"/><w:tcPrChange "), "{xml}");
    assert_eq!(xml.matches("<w:tcPrChange ").count(), 1, "{xml}");
    // the other way round, the revised element has no properties of its own and gets a container for the change
    let (v, xml) = run(&b, &a);
    assert_eq!(v.receipt.counts.props, 3);
    assert!(xml.contains("<w:trPr><w:trPrChange "), "{xml}");
    assert!(
        xml.contains("<w:jc w:val=\"center\"/></w:tblPr></w:tblPrChange>"),
        "{xml}"
    );
}

#[test]
fn table_properties_that_only_differ_in_what_word_rewrites_are_not_a_change() {
    let a = table("", "", "");
    let b = table("", "", "<w:cnfStyle w:val=\"100000000000\"/>");
    let (v, xml) = run(&a, &b);
    assert_eq!(v.receipt.counts.props, 0);
    assert!(!xml.contains("PrChange"), "{xml}");
}
