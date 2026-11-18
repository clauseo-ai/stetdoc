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

/// A paragraph with a link to `url` (none when empty) on `text`, between `before` and `after`.
fn link_case(before: &str, text: &str, url: &str, after: &str) -> Pk {
    let link = if url.is_empty() {
        format!("<w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r>")
    } else {
        format!(
            "<w:hyperlink r:id=\"rId20\"><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:hyperlink>"
        )
    };
    let body = format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">{before}</w:t></w:r>{link}<w:r><w:t xml:space=\"preserve\">{after}</w:t></w:r></w:p>"
    );
    if url.is_empty() {
        Pk::new(&body)
    } else {
        with_url(&body, url)
    }
}

#[test]
fn a_link_s_address_and_text_changed_together_hold_the_laws_in_every_combination() {
    // the text growing, shrinking, staying, changing at either end or all through, with the address kept or changed, the
    // text around it kept or edited, and the link added or taken away
    let texts = [
        "the portal",
        "the web portal",
        "portal",
        "the portal page",
        "our portal",
        "a new site",
    ];
    let urls = ["https://x.example/1", "https://x.example/2", ""];
    let arounds = [
        ("Pay at ", " after delivery."),
        ("Pay now at ", " after the delivery."),
    ];
    let original = link_case(
        "Pay at ",
        "the portal",
        "https://x.example/1",
        " after delivery.",
    );
    let mut n = 0;
    for text in texts {
        for url in urls {
            for (before, after) in arounds {
                let revised = link_case(before, text, url, after);
                for (a, b) in [(&original, &revised), (&revised, &original)] {
                    let v = check_pair(a.bytes(), b.bytes()).expect("compares");
                    assert!(
                        v.failures.is_empty()
                            && v.receipt.exclusions.is_empty()
                            && v.receipt.broken == 0,
                        "{text:?} {url:?} {before:?}: {}\n{:?}",
                        v.failures.join("\n"),
                        v.receipt.exclusions
                    );
                    n += 1;
                }
            }
        }
    }
    assert_eq!(n, 72);
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

/// A package whose comments are `(id, author, text)` and whose one paragraph holds, on "The second clause", the references in
/// the order of `refs`, then `tail` with the references of `later` after it.
fn comments_on(comments: &[(i64, &str, &str)], refs: &[i64], later: &[i64]) -> Pk {
    let defs: String = comments
        .iter()
        .map(|(id, author, text)| {
            format!(
                "<w:comment w:id=\"{id}\" w:author=\"{author}\" w:initials=\"X\" w:date=\"2026-01-01T00:00:00Z\"><w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p></w:comment>"
            )
        })
        .collect();
    let r = |ids: &[i64]| -> String {
        ids.iter()
            .map(|id| format!("<w:r><w:commentReference w:id=\"{id}\"/></w:r>"))
            .collect()
    };
    Pk {
        comments: Some(defs),
        ..Pk::new(&format!(
            "<w:p><w:r><w:t xml:space=\"preserve\">The second clause</w:t></w:r>{}<w:r><w:t xml:space=\"preserve\"> has comments, and so does the last part of it.</w:t></w:r>{}</w:p>",
            r(refs),
            r(later)
        ))
    }
}

#[test]
fn two_comments_on_the_same_text_pair_by_id_when_their_references_swap() {
    let defs = [(0, "Ann", "First note."), (1, "Bea", "Second note.")];
    let a = comments_on(&defs, &[0, 1], &[]);
    let b = comments_on(&defs, &[1, 0], &[]);
    let (v, _) = run(&a, &b);
    assert!(v.receipt.notices.is_empty(), "{:?}", v.receipt.notices);
    let comments = xml_of(&v, "word/comments.xml");
    assert!(
        !comments.contains("<w:ins ") && !comments.contains("<w:del "),
        "{comments}"
    );
}

#[test]
fn a_new_comment_at_an_existing_comment_s_place_does_not_take_its_pair() {
    let a = comments_on(&[(0, "Ann", "First note.")], &[0], &[]);
    let b = comments_on(
        &[(0, "Ann", "First note."), (1, "Bea", "A new note.")],
        &[1, 0],
        &[],
    );
    let (v, _) = run(&a, &b);
    assert!(v.receipt.notices.is_empty(), "{:?}", v.receipt.notices);
    let comments = xml_of(&v, "word/comments.xml");
    assert!(
        !comments.contains("<w:ins ") && !comments.contains("<w:del "),
        "{comments}"
    );
    // renumbered, the same author and text still find each other
    let b = comments_on(
        &[(5, "Bea", "A new note."), (6, "Ann", "First note.")],
        &[5, 6],
        &[],
    );
    let (v, _) = run(&a, &b);
    assert!(v.receipt.notices.is_empty(), "{:?}", v.receipt.notices);
}

#[test]
fn a_parent_and_its_reply_pair_by_content_when_word_renumbered_their_ids() {
    // an earlier comment is deleted, and Word renumbers the two that stay, at one place, from 1 and 2 to 0 and 1
    let a = comments_on(
        &[
            (0, "Ann", "An earlier note."),
            (1, "Ann", "Parent note."),
            (2, "Ann", "Reply note."),
        ],
        &[1, 2],
        &[0],
    );
    let b = comments_on(
        &[(0, "Ann", "Parent note."), (1, "Ann", "Reply note.")],
        &[0, 1],
        &[],
    );
    let (v, _) = run(&a, &b);
    assert!(v.receipt.notices.is_empty(), "{:?}", v.receipt.notices);
    let comments = xml_of(&v, "word/comments.xml");
    assert!(
        !comments.contains("<w:ins ") && !comments.contains("<w:del "),
        "{comments}"
    );
    // with other authors, and the reply's text edited, the pair of the same author still finds each other
    let a = comments_on(
        &[
            (0, "Ann", "An earlier note."),
            (1, "Ann", "Parent note."),
            (2, "Bea", "Reply note."),
        ],
        &[1, 2],
        &[0],
    );
    let b = comments_on(
        &[(0, "Ann", "Parent note."), (1, "Bea", "Reply, edited.")],
        &[0, 1],
        &[],
    );
    let (v, _) = run(&a, &b);
    assert!(v.receipt.notices.is_empty(), "{:?}", v.receipt.notices);
    let comments = xml_of(&v, "word/comments.xml");
    assert!(comments.contains("> note</w:delText>"), "{comments}");
}

#[test]
fn a_left_out_comment_does_not_keep_the_laws_from_reading_the_others() {
    // two comments by one author at one place, both edited, cannot be told apart; a third elsewhere is still compared
    let a = comments_on(
        &[
            (0, "Ann", "First note."),
            (1, "Ann", "Second note."),
            (2, "Cy", "Last note."),
        ],
        &[0, 1],
        &[2],
    );
    let b = comments_on(
        &[
            (0, "Ann", "First, edited."),
            (1, "Ann", "Second, edited."),
            (2, "Cy", "Last note, edited."),
        ],
        &[0, 1],
        &[2],
    );
    let (v, _) = run(&a, &b);
    assert_eq!(v.receipt.notices.len(), 2, "{:?}", v.receipt.notices);
    let comments = xml_of(&v, "word/comments.xml");
    assert_eq!(comments.matches("<w:ins ").count(), 1, "{comments}");
}

#[test]
fn comments_at_one_place_that_cannot_be_told_apart_are_not_compared_and_say_so() {
    let a = comments_on(
        &[(0, "Ann", "First note."), (1, "Ann", "Second note.")],
        &[0, 1],
        &[],
    );
    let b = comments_on(
        &[
            (7, "Ann", "First note, edited."),
            (8, "Ann", "Second note, edited."),
        ],
        &[7, 8],
        &[],
    );
    let (v, _) = run(&a, &b);
    assert_eq!(v.receipt.notices.len(), 2, "{:?}", v.receipt.notices);
    assert!(
        v.receipt
            .notices
            .iter()
            .all(|n| n.code == "compare_not_tracked" && n.message.contains("shares its place"))
    );
    let comments = xml_of(&v, "word/comments.xml");
    assert!(!comments.contains("<w:ins "), "{comments}");
}

#[test]
fn a_comment_whose_reference_moved_and_whose_text_changed_is_named() {
    let a = comments_on(&[(0, "Ann", "First note.")], &[0], &[]);
    let b = comments_on(&[(0, "Ann", "First note, edited.")], &[], &[0]);
    let (v, _) = run(&a, &b);
    let [n] = &v.receipt.notices[..] else {
        panic!("{:?}", v.receipt.notices)
    };
    assert!(n.message.contains("reference moved"), "{}", n.message);
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
    for b in [
        table("", "", "<w:cnfStyle w:val=\"100000000000\"/>"),
        table(
            "<w:tblLook w:val=\"04A0\" w:firstRow=\"1\" w:lastRow=\"0\" w:firstColumn=\"1\" w:lastColumn=\"0\" w:noHBand=\"0\" w:noVBand=\"1\"/>",
            "",
            "",
        ),
    ] {
        let (v, xml) = run(&a, &b);
        assert_eq!(v.receipt.counts.props, 0);
        assert!(
            !xml.contains("PrChange") && v.receipt.notices.is_empty(),
            "{xml}"
        );
    }
}
