//! The pieces of the hand-check pack: the fixture package, the steps that edit it, and the text after accept or reject all.

use std::fmt::Write as _;
use std::path::PathBuf;
use stetdoc_core::{Batch, Document, Edit, Limits, Mode, OpenOptions, Stamp, ViewSpec};
use stetdoc_testkit::fixtures::parts::{self, DECL, NS_W};
use stetdoc_testkit::fixtures::zip::{Entry, Zip};

pub const AUTHOR: &str = "Stet Reviewer";
pub const OTHER: &str = "Other Author";
pub const DATE: &str = "2026-10-06T14:03:27";

const STYLES: &str = concat!(
    "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style>",
    "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/>",
    "<w:pPr><w:keepNext/><w:spacing w:before=\"240\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"32\"/></w:rPr></w:style>",
    "<w:style w:type=\"paragraph\" w:styleId=\"ListParagraph\"><w:name w:val=\"List Paragraph\"/><w:basedOn w:val=\"Normal\"/>",
    "<w:pPr><w:ind w:left=\"720\"/></w:pPr></w:style>",
    "<w:style w:type=\"character\" w:styleId=\"Strong\"><w:name w:val=\"Strong\"/><w:rPr><w:b/></w:rPr></w:style>",
    "<w:style w:type=\"character\" w:styleId=\"FootnoteReference\"><w:name w:val=\"footnote reference\"/>",
    "<w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr></w:style>",
);

const NUMBERING: &str = concat!(
    "<w:abstractNum w:abstractNumId=\"0\"><w:multiLevelType w:val=\"hybridMultilevel\"/>",
    "<w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"decimal\"/><w:lvlText w:val=\"%1.\"/><w:lvlJc w:val=\"left\"/>",
    "<w:pPr><w:ind w:left=\"720\" w:hanging=\"360\"/></w:pPr></w:lvl>",
    "<w:lvl w:ilvl=\"1\"><w:start w:val=\"1\"/><w:numFmt w:val=\"lowerLetter\"/><w:lvlText w:val=\"(%2)\"/><w:lvlJc w:val=\"left\"/>",
    "<w:pPr><w:ind w:left=\"1440\" w:hanging=\"360\"/></w:pPr></w:lvl></w:abstractNum>",
    "<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>",
);

const FOOTNOTES: &str = concat!(
    "<w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>",
    "<w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>",
    "<w:footnote w:id=\"1\"><w:p><w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteRef/></w:r>",
    "<w:r><w:t xml:space=\"preserve\"> Scots law is excluded.</w:t></w:r></w:p></w:footnote>",
);

pub const FN_REF: &str = "<w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteReference w:id=\"1\"/></w:r>";

const CT: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub fn p(text: &str) -> String {
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}

pub fn styled(style: &str, text: &str) -> String {
    format!(
        "<w:p><w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>"
    )
}

pub fn item(level: u8, text: &str) -> String {
    format!(
        "<w:p><w:pPr><w:pStyle w:val=\"ListParagraph\"/><w:numPr><w:ilvl w:val=\"{level}\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>"
    )
}

pub fn table(rows: &[&[&str]]) -> String {
    let cols = rows.first().map_or(1, |r| r.len());
    let mut s = String::from(
        "<w:tbl><w:tblPr><w:tblStyle w:val=\"TableGrid\"/><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblBorders><w:top w:val=\"single\" w:sz=\"4\"/><w:left w:val=\"single\" w:sz=\"4\"/><w:bottom w:val=\"single\" w:sz=\"4\"/><w:right w:val=\"single\" w:sz=\"4\"/><w:insideH w:val=\"single\" w:sz=\"4\"/><w:insideV w:val=\"single\" w:sz=\"4\"/></w:tblBorders></w:tblPr><w:tblGrid>",
    );
    for _ in 0..cols {
        s.push_str("<w:gridCol w:w=\"2400\"/>");
    }
    s.push_str("</w:tblGrid>");
    for row in rows {
        s.push_str("<w:tr>");
        for cell in *row {
            let _ = write!(
                s,
                "<w:tc><w:tcPr><w:tcW w:w=\"2400\" w:type=\"dxa\"/></w:tcPr>{}</w:tc>",
                p(cell)
            );
        }
        s.push_str("</w:tr>");
    }
    s.push_str("</w:tbl>");
    s
}

/// A two-row table whose first column is merged vertically, with text in the continuation cell, and a paragraph after it.
pub fn merged_table() -> String {
    let cell = |props: &str, text: &str| {
        format!(
            "<w:tc><w:tcPr><w:tcW w:w=\"2400\" w:type=\"dxa\"/>{props}</w:tcPr>{}</w:tc>",
            p(text)
        )
    };
    let table = table(&[&["", ""]]);
    let head = table.split("<w:tr>").next().unwrap_or_default().to_string();
    format!(
        "{head}<w:tr>{}{}</w:tr><w:tr>{}{}</w:tr></w:tbl>{}",
        cell("<w:vMerge w:val=\"restart\"/>", "Merged"),
        cell("", "Right"),
        cell("<w:vMerge/>", "Continuation text"),
        cell("", "Lower right"),
        p("After the table.")
    )
}

/// A package with styles and numbering, footnotes when the body cites one, and any extra parts.
pub fn package(body: &str, extra: &[(&str, &str, &str, String)]) -> Vec<u8> {
    let doc = parts::Doc::new().body(body).bytes();
    let mut list: Vec<(&str, &str, &str, String)> = vec![
        (
            "styles.xml",
            "styles",
            "styles",
            format!("{DECL}<w:styles xmlns:w=\"{NS_W}\">{STYLES}</w:styles>"),
        ),
        (
            "numbering.xml",
            "numbering",
            "numbering",
            format!("{DECL}<w:numbering xmlns:w=\"{NS_W}\">{NUMBERING}</w:numbering>"),
        ),
    ];
    if body.contains("footnoteReference") {
        list.push((
            "footnotes.xml",
            "footnotes",
            "footnotes",
            format!("{DECL}<w:footnotes xmlns:w=\"{NS_W}\">{FOOTNOTES}</w:footnotes>"),
        ));
    }
    list.extend(extra.iter().cloned());
    let mut overrides = String::new();
    let mut rels = String::new();
    for (i, (name, ct, rel, _)) in list.iter().enumerate() {
        let _ = write!(
            overrides,
            "<Override PartName=\"/word/{name}\" ContentType=\"{CT}.{ct}+xml\"/>"
        );
        let _ = write!(
            rels,
            "<Relationship Id=\"rId{}\" Type=\"{REL}/{rel}\" Target=\"{name}\"/>",
            i + 1
        );
    }
    let types = parts::content_types().replace("</Types>", &format!("{overrides}</Types>"));
    let mut entries = parts::entries_with(&doc, &parts::root_rels(), &types);
    entries.push(Entry::new(
        "word/_rels/document.xml.rels",
        format!("{DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rels}</Relationships>"),
    ));
    for (name, _, _, xml) in list {
        entries.push(Entry::new(format!("word/{name}"), xml));
    }
    Zip::new(entries).build()
}

pub fn open(bytes: Vec<u8>) -> Document {
    Document::open(bytes, &OpenOptions::default()).expect("the fixture opens")
}

pub fn run(doc: &mut Document, author: &str, date: &str, ops: &str, tracked: bool) {
    let batch = Batch::parse(
        &format!(r#"{{"base":"{}","ops":[{ops}]}}"#, doc.base()),
        &Limits::default(),
    )
    .unwrap_or_else(|e| panic!("the batch {ops} does not parse: {e}"));
    let edit = Edit::ops(batch, Stamp::new(author, date));
    let edit = if tracked { edit } else { edit.untracked() };
    doc.edit(edit)
        .unwrap_or_else(|e| panic!("the edit {ops} fails: {e}"));
}

/// Opens the fixture and applies each step as one batch: (author, ops), tracked.
pub fn edited(bytes: Vec<u8>, steps: &[(&str, &str)]) -> Document {
    let mut doc = open(bytes);
    for (author, ops) in steps {
        run(&mut doc, author, DATE, ops, true);
    }
    doc
}

/// The body text after stetdoc resolves every revision (`accept` or `reject`), one paragraph per `/`.
pub fn resolved(bytes: &[u8], op: &str) -> String {
    let mut doc = open(bytes.to_vec());
    let batch = Batch::parse(
        &format!(
            r#"{{"base":"{}","ops":[{{"op":"{op}","all":true}}]}}"#,
            doc.base()
        ),
        &Limits::default(),
    )
    .expect("the batch parses");
    // A document with no revision refuses the batch, and its text is already the answer.
    let _ = doc.edit(Edit::ops(batch, Stamp::new(AUTHOR, DATE)));
    let view = doc
        .view(&ViewSpec {
            mode: Mode::Final,
            notes: false,
            ..ViewSpec::default()
        })
        .expect("the view");
    let lines: Vec<String> = view
        .text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter_map(clean_line)
        .collect();
    lines.join(" / ")
}

/// A ref token such as `p3`, `t1` or `r2`.
fn is_ref(token: &str) -> bool {
    token.len() > 1
        && matches!(token.as_bytes().first(), Some(b'p' | b't' | b'r'))
        && token[1..].chars().all(|c| c.is_ascii_digit())
}

/// Drops the view's header, summary and position lines and its markup in braces, and keeps list numbers.
fn clean_line(line: &str) -> Option<String> {
    if ["{stet", "{summary", "{position"]
        .iter()
        .any(|h| line.starts_with(h))
    {
        return None;
    }
    let first = line.split_whitespace().next()?;
    let (paragraph, line) = if is_ref(first) {
        (first.starts_with('p'), line[first.len()..].trim_start())
    } else {
        (false, line)
    };
    let mut out = String::new();
    let mut rest = line;
    let mut hidden = false;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let token = &rest[open + 1..open + close];
        hidden |= token == "hidden-para";
        if let Some(label) = token.strip_prefix("\u{a7}\"") {
            out.push_str(label.trim_end_matches('"'));
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    let words = out.split_whitespace().collect::<Vec<_>>().join(" ");
    match (words.is_empty(), paragraph) {
        (false, _) => Some(words),
        (true, true) if hidden => Some("(hidden empty paragraph, not shown)".into()),
        (true, true) => Some("(empty paragraph)".into()),
        (true, false) => None,
    }
}

pub fn edited_files(bytes: Vec<u8>, from: &str, to: &str) -> Document {
    let mut doc = open(bytes);
    let mut files = doc.files().expect("files");
    let mut hit = false;
    for (_, content) in &mut files.files {
        if content.contains(from) {
            *content = content.replacen(from, to, 1);
            hit = true;
        }
    }
    assert!(hit, "no readable file holds {from}");
    doc.edit(Edit::files(files, Stamp::new(AUTHOR, DATE)))
        .expect("the files edit");
    doc
}

pub trait Tap {
    fn tap(self, f: impl FnOnce(&mut Self)) -> Self;
}

impl Tap for Document {
    fn tap(mut self, f: impl FnOnce(&mut Document)) -> Document {
        f(&mut self);
        self
    }
}

pub fn out_dir() -> PathBuf {
    if let Ok(d) = std::env::var("STETDOC_WORD_PACK_DIR") {
        return PathBuf::from(d);
    }
    let home = std::env::var("HOME").expect("HOME is set");
    PathBuf::from(home)
        .join("Desktop")
        .join("stetdoc-word-check")
}

pub fn cell(s: &str) -> String {
    s.replace('|', "\\|")
}
