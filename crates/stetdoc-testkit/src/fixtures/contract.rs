//! The document that the edit tests of `document` and the conformance suite of the testkit share: a small contract with a
//! little of everything an operation meets, and refs that are the same in every run.

use super::parts::{self, DECL, NS_W};
use super::zip::{Entry, Zip};

/// The body: ten paragraphs and a table. The refs are `p1` to `p5`, the table `t1`, its rows `r1` and `r2`, the paragraphs of
/// its cells `p6` to `p9`, and `p10` after it.
///
/// - `p1` has a word that repeats (`shall`),
/// - `p2` has curly quotes,
/// - `p3` has an emoji, which is two UTF-16 units, and a tab, which is an atom,
/// - `p4` and `p5` are plain.
pub const BODY: &str = concat!(
    "<w:p><w:r><w:t>The Supplier shall indemnify the Customer, and shall pay on demand.</w:t></w:r></w:p>",
    "<w:p><w:r><w:t>\u{201C}Losses\u{201D} means any loss, and the Supplier\u{2019}s liability.</w:t></w:r></w:p>",
    "<w:p><w:r><w:t xml:space=\"preserve\">Emoji \u{1F600} then a tab</w:t></w:r><w:r><w:tab/></w:r><w:r><w:t>and the end.</w:t></w:r></w:p>",
    "<w:p><w:r><w:t>Payment is due within thirty days.</w:t></w:r></w:p>",
    "<w:p><w:r><w:t>This Agreement is governed by English law.</w:t></w:r></w:p>",
    "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"2000\"/><w:gridCol w:w=\"2000\"/></w:tblGrid>",
    "<w:tr><w:tc><w:p><w:r><w:t>a1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>b1</w:t></w:r></w:p></w:tc></w:tr>",
    "<w:tr><w:tc><w:p><w:r><w:t>a2</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>b2</w:t></w:r></w:p></w:tc></w:tr>",
    "</w:tbl>",
    "<w:p><w:r><w:t>After the table.</w:t></w:r></w:p>",
);

/// The bytes of the package. Building is deterministic.
pub fn package() -> Vec<u8> {
    let doc = parts::Doc::new().body(BODY).bytes();
    Zip::new(parts::entries_doc(&doc)).build()
}

/// The contract with one footnote, `fn1`, whose reference ends `p5`, and a styles part with `Heading 1` and `Strong`: the
/// document of `delete-note` and `style`. The refs are the contract's.
pub fn package_rich() -> Vec<u8> {
    let reference = "<w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteReference w:id=\"1\"/></w:r>";
    let body = BODY.replacen(
        "governed by English law.</w:t></w:r>",
        &format!("governed by English law.</w:t></w:r>{reference}"),
        1,
    );
    let doc = parts::Doc::new().body(&body).bytes();
    let notes = format!(
        "{DECL}<w:footnotes xmlns:w=\"{NS_W}\"><w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote><w:footnote w:id=\"1\"><w:p><w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteRef/></w:r><w:r><w:t xml:space=\"preserve\"> Scots law is excluded.</w:t></w:r></w:p></w:footnote></w:footnotes>"
    );
    let types = parts::content_types().replace(
        "</Types>",
        "<Override PartName=\"/word/footnotes.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/></Types>",
    );
    let mut entries = parts::entries_with(&doc, &parts::root_rels(), &types);
    entries.push(Entry::new(
        "word/_rels/document.xml.rels",
        format!(
            "{DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes\" Target=\"footnotes.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>"
        ),
    ));
    entries.push(Entry::new("word/footnotes.xml", notes));
    entries.push(Entry::new(
        "word/styles.xml",
        format!(
            "{DECL}<w:styles xmlns:w=\"{NS_W}\"><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style><w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/></w:style><w:style w:type=\"character\" w:styleId=\"Strong\"><w:name w:val=\"Strong\"/></w:style></w:styles>"
        ),
    ));
    Zip::new(entries).build()
}
