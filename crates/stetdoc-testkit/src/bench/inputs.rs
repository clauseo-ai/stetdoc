//! The documents the benchmarks run on. None is committed: one comes from the corpus and is
//! scaled up, and the rest are built here, the same bytes every time.
//!
//! - `contract-300p`: the largest contract of the curated Harvey LAB documents (a first-turn
//!   markup of a credit agreement, about 76 pages by its text), with its body repeated and cut at
//!   a block until it holds the text of 300 pages at 3,000 characters a page. The other parts are
//!   the document's own. It is the input the budgets are written for.
//! - `dense-contract-300p`: the same for the largest Word-authored legal text of the curated corpus
//!   (a Council draft of the AI Act, about 138 pages by its text). Word writes far more nodes for
//!   the same text than the LAB tool does, so this is the densest real document there is: the tree
//!   is four times its XML, five with the part it was parsed from, and the budget has eight.
//! - `worst-blocks-12mb`: the worst body that the design of `state` measured, 12 MB in 18,273
//!   blocks: 15,942 paragraphs, 21 tables and their 2,310 rows.
//! - `node-dense-18mb`: 18 MB of paragraphs with eleven short runs each, about 1.2 million nodes,
//!   the body that `state` was reviewed against, which is what the span tree pays for by the node.
//!   It is denser than any document in the corpus (the largest Harvey LAB body is 4.5 MB), and the
//!   design notes of `state` say it exceeded the memory budget until the tree stored its nodes in
//!   chunks.
//! - `fields-13mb`: 13 MB of paragraphs, one in eight of them with a field (a complex field in every
//!   sixteenth, a simple one in every sixteenth between those: 2,963 in all, the density of the body
//!   whose field pass `wordml` measured), which is what the field pass and the refresh of `wordml`
//!   pay for.
//! - `numbered-20k`, which only the benchmarks that name it run on (`Inputs::Only`): 20,000
//!   paragraphs with the styles and the numbering of the gates of `numbering` (`numbered.rs`).

use super::numbered::numbered;
use super::spec::Input;
use crate::corpus::Corpus;
use crate::fixtures::inspect;
use crate::fixtures::parts::{Doc, entries_doc};
use crate::fixtures::zip::{Entry, Zip};
use std::ops::Range;

/// The corpus document that the contract is made from.
pub(crate) const CONTRACT_SOURCE: &str = "harvey-labs:tasks/contracts/financing/credit-lending-first-turn-redline/scenario-02/documents/borrower-first-markup-redline.docx";
/// The corpus document that the dense contract is made from.
pub(crate) const DENSE_CONTRACT_SOURCE: &str =
    "eu-ai-act:orig-docx/2022-11-03-AIA-CZ-Draft-for-Coreper-3-Nov-22.docx";
/// How many characters of text a page holds.
pub(crate) const CHARS_PER_PAGE: usize = 3_000;
/// How many pages the contract has.
pub(crate) const CONTRACT_PAGES: usize = 300;

/// The names of the document inputs, in the order they run.
pub const NAMES: [&str; 5] = [
    "contract-300p",
    "dense-contract-300p",
    "worst-blocks-12mb",
    "node-dense-18mb",
    "fields-13mb",
];

/// The name of the input that only the benchmarks which name it run on: a body of 20,000
/// paragraphs with the styles and the numbering that the gates of `numbering` are written for.
pub const NUMBERED: &str = "numbered-20k";

/// Builds the inputs that `wanted` accepts, the document inputs and `NUMBERED`. The contracts need
/// the corpus, so it fails with a message that says how to get it when `STETDOC_CORPUS_DIR` is not
/// set.
pub(crate) fn build(wanted: impl Fn(&str) -> bool) -> Result<Vec<Input>, String> {
    let mut inputs = Vec::new();
    for name in NAMES.into_iter().chain([NUMBERED]).filter(|n| wanted(n)) {
        inputs.push(match name {
            "contract-300p" => contract(name, CONTRACT_SOURCE)?,
            "dense-contract-300p" => contract(name, DENSE_CONTRACT_SOURCE)?,
            "worst-blocks-12mb" => from_body(name, &worst_blocks_body(13_631, 21, 110)),
            "node-dense-18mb" => from_body(name, &node_dense_body(27_300, 11)),
            "numbered-20k" => numbered(name),
            _ => from_body(name, &fields_body(23_700)),
        });
    }
    Ok(inputs)
}

/// A document around a body, with the parts of the smallest valid package.
pub fn from_body(name: &'static str, body: &str) -> Input {
    let main_part = Doc::new().body(body).bytes();
    let docx = Zip::new(entries_doc(&main_part)).build();
    let xml_bytes = xml_size(&docx);
    Input {
        name,
        docx,
        main_part,
        xml_bytes,
    }
}

/// The paragraphs, tables and table rows of a piece of XML at any depth: what the design of `state`
/// calls the blocks of a body.
pub(crate) fn blocks(xml: &str) -> usize {
    ["<w:p>", "<w:p ", "<w:tbl>", "<w:tbl ", "<w:tr>", "<w:tr "]
        .iter()
        .map(|tag| xml.matches(tag).count())
        .sum()
}

/// One line that says what an input is: the sizes the memory budget is written against, and what
/// the body holds. A run prints it above the table, so that the limits in the table can be read.
pub(crate) fn describe(input: &Input) -> String {
    let mib = |n: usize| n as f64 / (1u64 << 20) as f64;
    let main = String::from_utf8_lossy(&input.main_part);
    format!(
        "{:<20} file {:>5.2} MiB, xml {:>5.2} MiB of which the main part {:>5.2} MiB, {} blocks, {:.0} pages of text",
        input.name,
        mib(input.docx.len()),
        input.xml_bytes as f64 / (1u64 << 20) as f64,
        mib(input.main_part.len()),
        blocks(&main),
        text_len(&main) as f64 / CHARS_PER_PAGE as f64,
    )
}

/// The sum of the inflated sizes of the XML parts of an archive.
pub(super) fn xml_size(docx: &[u8]) -> u64 {
    inspect::read(docx).map_or(0, |archive| {
        archive
            .records
            .iter()
            .filter(|r| {
                let name = String::from_utf8_lossy(&r.name).to_ascii_lowercase();
                name.ends_with(".xml") || name.ends_with(".rels")
            })
            .map(|r| r.usize)
            .sum()
    })
}

/// A contract: the corpus document `source`, scaled to 300 pages.
fn contract(name: &'static str, source: &str) -> Result<Input, String> {
    let corpus = Corpus::from_env()
        .ok_or("the contracts come from the corpus: run `cargo xtask bench`, which fetches it, or set STETDOC_CORPUS_DIR")??;
    let entry = corpus
        .entries
        .iter()
        .find(|e| e.id == source)
        .ok_or_else(|| format!("the corpus has no {source}"))?;
    let bytes = entry.bytes();
    let archive = inspect::read(&bytes)?;
    let mut entries = Vec::new();
    let mut main_part = Vec::new();
    for rec in &archive.records {
        let data = inspect::extract(&bytes, rec)?;
        if rec.name == b"word/document.xml" {
            let text = String::from_utf8(data).map_err(|e| format!("the main part: {e}"))?;
            main_part = scale_body(&text, CONTRACT_PAGES * CHARS_PER_PAGE)?.into_bytes();
            entries.push(Entry::new(&rec.name, &main_part));
        } else if !rec.name.ends_with(b"/") {
            entries.push(Entry::new(&rec.name, data));
        }
    }
    if main_part.is_empty() {
        return Err(format!("{source} has no word/document.xml"));
    }
    let docx = Zip::new(entries).build();
    let xml_bytes = xml_size(&docx);
    Ok(Input {
        name,
        docx,
        main_part,
        xml_bytes,
    })
}

/// The characters of text in a piece of XML: what `w:t` elements hold, counted in bytes.
pub(crate) fn text_len(xml: &str) -> usize {
    let mut total = 0;
    let mut rest = xml;
    while let Some(at) = rest.find("<w:t") {
        let after = &rest[at + 4..];
        let Some(open_end) = after.find('>') else {
            break;
        };
        let is_t = matches!(after.as_bytes().first(), Some(b'>' | b' '));
        let content = &after[open_end + 1..];
        let close = content.find("</w:t>").unwrap_or(content.len());
        if is_t && !after[..open_end].ends_with('/') {
            total += close;
        }
        rest = &content[close.min(content.len())..];
    }
    total
}

/// The top-level elements of a piece of XML, as byte ranges: what is between the tags of the
/// body's children. Comments, processing instructions and CDATA are skipped over, and a `>` in an
/// attribute value does not end a tag.
pub(crate) fn top_level_blocks(xml: &str) -> Vec<Range<usize>> {
    let b = xml.as_bytes();
    let mut blocks = Vec::new();
    let (mut depth, mut start, mut i) = (0usize, 0usize, 0usize);
    let find = |from: usize, pat: &[u8]| {
        b[from..]
            .windows(pat.len())
            .position(|w| w == pat)
            .map(|p| from + p + pat.len())
    };
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
        } else if b[i..].starts_with(b"<!--") {
            i = find(i, b"-->").unwrap_or(b.len());
        } else if b[i..].starts_with(b"<![CDATA[") {
            i = find(i, b"]]>").unwrap_or(b.len());
        } else if b[i..].starts_with(b"<?") {
            i = find(i, b"?>").unwrap_or(b.len());
        } else if b[i..].starts_with(b"</") {
            i = find(i, b">").unwrap_or(b.len());
            depth = depth.saturating_sub(1);
            if depth == 0 {
                blocks.push(start..i);
            }
        } else {
            let mut j = i + 1;
            let mut quote = 0u8;
            while j < b.len() && (quote != 0 || b[j] != b'>') {
                match (quote, b[j]) {
                    (0, q @ (b'"' | b'\'')) => quote = q,
                    (q, c) if q != 0 && c == q => quote = 0,
                    _ => {}
                }
                j += 1;
            }
            let end = (j + 1).min(b.len());
            if depth == 0 {
                start = i;
            }
            if b[..j].last() == Some(&b'/') {
                if depth == 0 {
                    blocks.push(start..end);
                }
            } else {
                depth += 1;
            }
            i = end;
        }
    }
    blocks
}

/// A main part whose body is repeated, a block at a time and cut at a block, until it holds
/// `target_chars` characters of text. The last block of the body, if it is `w:sectPr`, stays last.
pub(crate) fn scale_body(xml: &str, target_chars: usize) -> Result<String, String> {
    let open = xml.find("<w:body>").ok_or("no <w:body>")? + "<w:body>".len();
    let close = xml.rfind("</w:body>").ok_or("no </w:body>")?;
    let content = &xml[open..close];
    let mut blocks = top_level_blocks(content);
    let tail = match blocks.last() {
        Some(r) if content[r.clone()].starts_with("<w:sectPr") => blocks.pop().map(|r| &content[r]),
        _ => None,
    };
    let lens: Vec<usize> = blocks
        .iter()
        .map(|r| text_len(&content[r.clone()]))
        .collect();
    if lens.iter().sum::<usize>() == 0 {
        return Err("the body has no text to repeat".to_owned());
    }
    let mut out = String::with_capacity(target_chars * 8);
    out.push_str(&xml[..open]);
    let mut chars = 0;
    for (r, len) in blocks.iter().zip(&lens).cycle() {
        if chars >= target_chars {
            break;
        }
        out.push_str(&content[r.clone()]);
        chars += len;
    }
    out.push_str(tail.unwrap_or(""));
    out.push_str(&xml[close..]);
    Ok(out)
}

/// The worst body of the `state` design: plain paragraphs, then tables of one-cell rows.
pub(crate) fn worst_blocks_body(paragraphs: usize, tables: usize, rows: usize) -> String {
    let text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor. "
        .repeat(6);
    let para = |i: usize| {
        format!(
            "<w:p><w:pPr><w:pStyle w:val=\"BodyText\"/><w:spacing w:before=\"120\" w:after=\"120\"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\"/><w:sz w:val=\"22\"/></w:rPr><w:t xml:space=\"preserve\">{i} {text}</w:t></w:r></w:p>"
        )
    };
    let mut body = String::new();
    let mut n = 0;
    for _ in 0..paragraphs {
        body.push_str(&para(n));
        n += 1;
    }
    for _ in 0..tables {
        body.push_str("<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"9000\"/></w:tblGrid>");
        for _ in 0..rows {
            body.push_str(&format!("<w:tr><w:tc><w:tcPr/>{}</w:tc></w:tr>", para(n)));
            n += 1;
        }
        body.push_str("</w:tbl>");
    }
    body.push_str("<w:p><w:r><w:t>end</w:t></w:r></w:p>");
    body
}

/// Paragraphs of short runs: a few words in each.
pub(crate) fn node_dense_body(paragraphs: usize, runs: usize) -> String {
    let run = "<w:r><w:t xml:space=\"preserve\">short text 1234 </w:t></w:r>";
    format!("<w:p>{}</w:p>", run.repeat(runs)).repeat(paragraphs)
}

/// Contract paragraphs, one in eight of them with a field: a complex field (a reference) in every
/// sixteenth and a simple field (the page) in every sixteenth between those, 2,963 in all for
/// 23,700 paragraphs. That is the density of the body whose field pass `wordml` measured (2,750
/// fields in 11,000 blocks), and more than a contract has. The other paragraphs are plain text of
/// the same length.
pub(crate) fn fields_body(paragraphs: usize) -> String {
    const LEAD: &str = "<w:p><w:pPr><w:pStyle w:val=\"BodyText\"/></w:pPr><w:r><w:t xml:space=\"preserve\">As set out in clause </w:t></w:r>";
    const TAIL: &str = "<w:r><w:t xml:space=\"preserve\"> and the schedule, the Borrower shall comply with this Agreement at all times and in full, shall keep every Finance Document in full force and effect in accordance with its terms, and shall promptly notify the Lender of any default that is continuing or that would result from the matter in question, in each case as set out in this clause</w:t></w:r>";
    let mut body = String::with_capacity(paragraphs * 560);
    for n in 0..paragraphs {
        let at = format!("{}.{}", n / 40 + 1, n % 40 + 1);
        body.push_str(LEAD);
        if n % 16 == 0 {
            body.push_str(&format!(
                "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> REF _Ref{n} \\r \\h </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>{at}</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>"
            ));
        } else {
            body.push_str(&format!("<w:r><w:t>{at}</w:t></w:r>"));
        }
        body.push_str(TAIL);
        if n % 16 == 8 {
            body.push_str("<w:fldSimple w:instr=\" PAGE \"><w:r><w:t>1</w:t></w:r></w:fldSimple>");
        }
        body.push_str("</w:p>");
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(haystack: &str, needle: &str) -> usize {
        haystack.matches(needle).count()
    }

    #[test]
    fn an_input_is_described_by_its_sizes_and_what_its_body_holds() {
        let body = "<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p>";
        let line = describe(&from_body("small", body));
        assert!(line.starts_with("small "), "{line}");
        assert!(line.contains(" 2 blocks,"), "the two paragraphs: {line}");
        assert!(line.ends_with("0 pages of text"), "{line}");
    }

    #[test]
    fn the_worst_body_has_the_blocks_the_design_measured() {
        let body = worst_blocks_body(13_631, 21, 110);
        assert_eq!(blocks(&body), 18_273);
        assert!(
            (11_000_000..13_500_000).contains(&body.len()),
            "{}",
            body.len()
        );
    }

    #[test]
    fn the_dense_and_field_bodies_have_the_sizes_the_names_say() {
        let dense = node_dense_body(27_300, 11);
        assert!(
            (17_000_000..19_000_000).contains(&dense.len()),
            "{}",
            dense.len()
        );
        // Elements, text nodes and attributes: each run is a `w:r`, a `w:t`, its text and an attribute.
        let nodes = count(&dense, "<w:") + 2 * count(&dense, "<w:t ");
        assert!(
            (1_000_000..1_500_000).contains(&nodes),
            "{nodes} nodes of at most 5,000,000"
        );
        let fields = fields_body(23_700);
        assert!(
            (12_500_000..13_500_000).contains(&fields.len()),
            "{}",
            fields.len()
        );
        assert_eq!(count(&fields, "fldCharType=\"begin\""), 1_482);
        assert_eq!(count(&fields, "<w:fldSimple "), 1_481);
        assert_eq!(count(&fields, "<w:p>"), 23_700);
    }

    #[test]
    fn a_document_is_built_around_a_body_and_its_xml_is_counted() {
        let input = from_body("small", &worst_blocks_body(3, 1, 2));
        assert_eq!(input.name, "small");
        assert!(input.docx.starts_with(b"PK"));
        let inner = String::from_utf8(input.main_part.clone()).unwrap();
        assert_eq!(count(&inner, "<w:tr>"), 2);
        assert!(
            input.xml_bytes > input.main_part.len() as u64,
            "the other parts count too"
        );
    }

    #[test]
    fn top_level_blocks_are_found_whatever_they_hold() {
        let xml = "<w:p a=\"x>y\"><w:r/></w:p><!-- <w:p> --><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl><w:p><![CDATA[</w:p>]]></w:p><w:sectPr/>";
        let found: Vec<&str> = top_level_blocks(xml).into_iter().map(|r| &xml[r]).collect();
        assert_eq!(
            found,
            [
                "<w:p a=\"x>y\"><w:r/></w:p>",
                "<w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>",
                "<w:p><![CDATA[</w:p>]]></w:p>",
                "<w:sectPr/>"
            ]
        );
    }

    #[test]
    fn text_is_counted_in_w_t_elements_only() {
        let xml = "<w:p><w:r><w:t>abc</w:t></w:r><w:r><w:t xml:space=\"preserve\"> de </w:t></w:r><w:r><w:tab/></w:r><w:tbl/><w:t/></w:p>";
        assert_eq!(text_len(xml), 3 + 4);
    }

    #[test]
    fn a_body_is_repeated_to_the_text_it_is_asked_for_and_keeps_its_section_last() {
        let xml = "<?xml?><w:document><w:body><w:p><w:r><w:t>1234567890</w:t></w:r></w:p><w:p><w:r><w:t>abcde</w:t></w:r></w:p><w:sectPr><w:pgSz/></w:sectPr></w:body></w:document>";
        let scaled = scale_body(xml, 40).unwrap();
        assert!(scaled.starts_with("<?xml?><w:document><w:body><w:p>"));
        assert!(scaled.ends_with("<w:sectPr><w:pgSz/></w:sectPr></w:body></w:document>"));
        assert_eq!(count(&scaled, "<w:sectPr>"), 1);
        let chars = text_len(&scaled);
        assert!((40..40 + 15).contains(&chars), "{chars}");
        assert!(
            scale_body("<w:document/>", 10)
                .unwrap_err()
                .contains("no <w:body>")
        );
        assert!(
            scale_body("<w:body></w:body>", 10)
                .unwrap_err()
                .contains("no text")
        );
    }
}
