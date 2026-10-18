//! The independent block count of the corpus harness: a streaming read of the bytes of a part that
//! counts the `w:p`, `w:tbl` and `w:tr` start tags that are blocks under the rules of
//! `docs/design/wordml.md` 3.2, without a tree and without the code of `story_scan.rs`.
//!
//! What counts: a paragraph or a table where blocks stand (the body, a header or a footer, a note or a
//! comment, a cell), and a row in a table. Blocks are not counted inside a paragraph (a text box is
//! inside one), inside an element the scan does not walk through (a control's properties, an unknown
//! element), in the branch of an `mc:AlternateContent` that Word does not take, or in a separator
//! note. A block-level `w:sdt`, `w:customXml`, `w:smartTag`, `w:ins`, `w:del`, `w:moveFrom`,
//! `w:moveTo` and the taken branch are walked through.

use super::*;

/// How many blocks of each kind a part has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Count {
    pub paragraphs: usize,
    pub tables: usize,
    pub rows: usize,
}

/// What may stand directly in the element being read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Level {
    /// Nothing is counted below here.
    Dead,
    /// Paragraphs and tables.
    Block,
    /// Rows.
    Table,
    /// Cells, which put the reader back at the block level.
    Row,
    /// Notes or comments, and nothing else.
    Units,
}

struct Open {
    /// The level inside this element.
    level: Level,
    /// For an `mc:AlternateContent`: whether a branch was taken yet.
    taken: bool,
    /// How many namespace declarations this element made.
    declared: usize,
    is_alternate: bool,
}

const W: [&str; 2] = [
    "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    "http://purl.oclc.org/ooxml/wordprocessingml/main",
];
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";

/// Whether Word understands a namespace named in a `Requires`: the namespace table of `schema` has a
/// row for it, which is the table `schema::taken_branch` reads. The counter reads the text of the part
/// and never calls `taken_branch`, so it stays an independent check of the block scan. It does not
/// model `mc:ProcessContent`, which none of the curated documents uses (`tests_process_content.rs`
/// builds those shapes by hand).
fn understood(uri: &str) -> bool {
    crate::schema::preferred_prefix(uri).is_some()
}

/// The text of a part, decoded as its bytes declare: UTF-16 with a byte order mark, or with the `<?`
/// of an XML declaration in that encoding, else UTF-8 with or without a mark. A part in UTF-16 read as
/// UTF-8 has a NUL between every two characters and no tag a streaming read can find.
fn decode(xml: &[u8]) -> std::borrow::Cow<'_, str> {
    let utf16 =
        |little: bool, from: usize| -> String {
            let units = xml.get(from..).unwrap_or(&[]).chunks_exact(2).map(|b| {
                match (b.first(), b.get(1)) {
                    (Some(&x), Some(&y)) if little => u16::from_le_bytes([x, y]),
                    (Some(&x), Some(&y)) => u16::from_be_bytes([x, y]),
                    _ => 0,
                }
            });
            char::decode_utf16(units)
                .map(|r| r.unwrap_or('\u{fffd}'))
                .collect()
        };
    match xml {
        [0xFF, 0xFE, ..] => utf16(true, 2).into(),
        [0xFE, 0xFF, ..] => utf16(false, 2).into(),
        [0x3C, 0x00, 0x3F, 0x00, ..] => utf16(true, 0).into(),
        [0x00, 0x3C, 0x00, 0x3F, ..] => utf16(false, 0).into(),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest),
        _ => String::from_utf8_lossy(xml),
    }
}

/// The attributes of a start tag, as (name, value) pairs, and whether the tag closes itself.
fn attributes(tag: &str) -> (Vec<(&str, &str)>, bool) {
    let selfclosing = tag.ends_with('/');
    let body = tag.trim_end_matches('/');
    let mut rest = body.split_once(char::is_whitespace).map_or("", |(_, r)| r);
    let mut out = Vec::new();
    while let Some((name, after)) = rest.split_once('=') {
        let after = after.trim_start();
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            break;
        };
        let Some((value, tail)) = after[1..].split_once(quote) else {
            break;
        };
        out.push((name.trim(), value));
        rest = tail;
    }
    (out, selfclosing)
}

/// The block count of one story part.
pub(super) fn count_blocks(xml: &[u8], kind: StoryKind) -> Count {
    let text = decode(xml);
    let mut count = Count::default();
    let mut bodies = 0usize;
    let mut stack: Vec<Open> = Vec::new();
    // prefix -> uri, innermost last
    let mut scope: Vec<(String, String)> = Vec::new();
    let mut at = 0usize;
    while let Some(lt) = text[at..].find('<').map(|i| at + i) {
        let rest = &text[lt..];
        let skip = |end: &str, from: usize| {
            rest[from..]
                .find(end)
                .map_or(rest.len(), |i| from + i + end.len())
        };
        if rest.starts_with("<!--") {
            at = lt + skip("-->", 4);
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            at = lt + skip("]]>", 9);
            continue;
        }
        if rest.starts_with("<?") {
            at = lt + skip("?>", 2);
            continue;
        }
        if rest.starts_with("<!") {
            at = lt + skip(">", 2);
            continue;
        }
        // the end of the tag, with quotes respected
        let mut quote = None;
        let mut end = rest.len();
        for (i, c) in rest.char_indices().skip(1) {
            match (quote, c) {
                (None, '"' | '\'') => quote = Some(c),
                (Some(q), c) if c == q => quote = None,
                (None, '>') => {
                    end = i;
                    break;
                }
                _ => {}
            }
        }
        at = lt + end + 1;
        let tag = &rest[1..end];
        if let Some(_closing) = tag.strip_prefix('/') {
            if let Some(open) = stack.pop() {
                scope.truncate(scope.len() - open.declared);
            }
            continue;
        }
        let name = tag
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        let (attrs, selfclosing) = attributes(tag);
        let declared = attrs
            .iter()
            .filter(|(n, _)| *n == "xmlns" || n.starts_with("xmlns:"))
            .map(|(n, v)| {
                scope.push((
                    n.strip_prefix("xmlns:").unwrap_or("").to_string(),
                    (*v).to_string(),
                ));
            })
            .count();
        let (prefix, local) = name.split_once(':').unwrap_or(("", name));
        let uri_of = |p: &str| {
            scope
                .iter()
                .rev()
                .find(|(q, _)| q == p)
                .map(|(_, u)| u.as_str())
        };
        let uri = uri_of(prefix).unwrap_or("");
        let is_w = W.contains(&uri);
        let attr = |n: &str| {
            attrs
                .iter()
                .find(|(a, _)| *a == n || a.ends_with(&format!(":{n}")))
                .map(|(_, v)| *v)
        };
        let parent = stack.last().map(|o| o.level);
        let mut is_alternate = false;
        let level = if stack.is_empty() {
            match (kind, is_w, local) {
                (StoryKind::Body, true, "document") => Level::Dead,
                (StoryKind::Header, true, "hdr") | (StoryKind::Footer, true, "ftr") => Level::Block,
                (StoryKind::Footnotes, true, "footnotes")
                | (StoryKind::Endnotes, true, "endnotes")
                | (StoryKind::Comments, true, "comments") => Level::Units,
                _ => Level::Dead,
            }
        } else if uri == MC && local == "AlternateContent" {
            is_alternate = true;
            parent.unwrap_or(Level::Dead)
        } else if uri == MC && (local == "Choice" || local == "Fallback") {
            let ac = stack.last_mut().filter(|o| o.is_alternate);
            match ac {
                Some(ac) if !ac.taken => {
                    let ok = local == "Fallback"
                        || attr("Requires").is_some_and(|r| {
                            let mut any = false;
                            let all = r.split_whitespace().all(|p| {
                                any = true;
                                uri_of(p).is_some_and(understood)
                            });
                            any && all
                        });
                    ac.taken = ok;
                    if ok { ac.level } else { Level::Dead }
                }
                _ => Level::Dead,
            }
        } else if !is_w {
            Level::Dead
        } else {
            match (parent, local) {
                // Word reads the first body, as the scan does: a second one is content nobody sees
                (Some(_), "body") if stack.len() == 1 && kind == StoryKind::Body => {
                    bodies += 1;
                    if bodies == 1 {
                        Level::Block
                    } else {
                        Level::Dead
                    }
                }
                (Some(Level::Dead), _) | (None, _) => Level::Dead,
                (Some(Level::Units), "footnote" | "endnote") if kind != StoryKind::Comments => {
                    let separator = attr("type").is_some_and(|t| {
                        matches!(
                            t,
                            "separator" | "continuationSeparator" | "continuationNotice"
                        )
                    });
                    if separator { Level::Dead } else { Level::Block }
                }
                (Some(Level::Units), "comment") => Level::Block,
                (Some(Level::Units), _) => Level::Dead,
                (Some(Level::Block), "p") => {
                    count.paragraphs += 1;
                    Level::Dead
                }
                (Some(Level::Block), "tbl") => {
                    count.tables += 1;
                    Level::Table
                }
                (Some(Level::Table), "tr") => {
                    count.rows += 1;
                    Level::Row
                }
                (Some(Level::Row), "tc") => Level::Block,
                (
                    Some(l),
                    "sdtContent" | "customXml" | "smartTag" | "ins" | "del" | "moveFrom" | "moveTo",
                ) => l,
                (Some(l), "sdt") => l,
                _ => Level::Dead,
            }
        };
        // a control's properties and end properties are not content, and nothing below them counts
        let level = if local == "sdtPr" || local == "sdtEndPr" {
            Level::Dead
        } else {
            level
        };
        if selfclosing {
            scope.truncate(scope.len() - declared);
        } else {
            stack.push(Open {
                level,
                taken: false,
                declared,
                is_alternate,
            });
        }
    }
    count
}

/// The count a first reading would make: every `w:p`, `w:tbl` and `w:tr` start tag, whatever it is in.
/// The harness reports where it differs from `count_blocks`, and why.
pub(super) fn naive_count(xml: &[u8]) -> Count {
    let text = decode(xml);
    let tags = |name: &str| {
        let open = format!("<w:{name}");
        text.match_indices(&open)
            .filter(|(i, _)| {
                matches!(
                    text.as_bytes().get(i + open.len()),
                    Some(b' ' | b'>' | b'/' | b'\t' | b'\n' | b'\r')
                )
            })
            .count()
    };
    Count {
        paragraphs: tags("p"),
        tables: tags("tbl"),
        rows: tags("tr"),
    }
}

/// Why the naive count and the rules can differ, as far as the bytes show it.
pub(super) fn reasons(xml: &[u8]) -> Vec<&'static str> {
    let text = decode(xml);
    let mut out = Vec::new();
    // a paragraph or a table inside a paragraph, outside a text box (not valid, NestedBlock)
    let (mut paragraphs, mut boxes, mut nested) = (0usize, 0usize, false);
    for tag in text.split('<').skip(1) {
        let (end_tag, tag) = tag.strip_prefix('/').map_or((false, tag), |t| (true, t));
        let name = tag
            .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .next()
            .unwrap_or("");
        let selfclosing = tag.split('>').next().unwrap_or("").ends_with('/');
        match (name, end_tag) {
            ("w:p" | "w:tbl", false) if boxes == 0 && paragraphs > 0 => nested = true,
            ("w:p", false) if !selfclosing => paragraphs += 1,
            ("w:p", true) => paragraphs = paragraphs.saturating_sub(1),
            ("w:txbxContent", false) if !selfclosing => boxes += 1,
            ("w:txbxContent", true) => boxes = boxes.saturating_sub(1),
            _ => {}
        }
    }
    if nested {
        out.push("a paragraph or a table inside a paragraph, which is not a block");
    }
    for (needle, why) in [
        (
            "txbxContent",
            "text boxes hold paragraphs inside a paragraph",
        ),
        (
            "<mc:Fallback",
            "an untaken mc:Fallback holds a copy of a block",
        ),
        ("w:type=\"separator\"", "separator notes are not shown"),
        (
            "w:type=\"continuationSeparator\"",
            "separator notes are not shown",
        ),
    ] {
        if text.contains(needle) && !out.contains(&why) {
            out.push(why);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(xml: &str, kind: StoryKind) -> (usize, usize, usize) {
        let n = count_blocks(xml.as_bytes(), kind);
        (n.paragraphs, n.tables, n.rows)
    }

    const NS: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"";

    fn doc(body: &str) -> String {
        format!("<w:document {NS}><w:body>{body}</w:body></w:document>")
    }

    #[test]
    fn the_count_follows_the_rules_of_the_scan() {
        let body = |b: &str| c(&doc(b), StoryKind::Body);
        assert_eq!(
            body("<w:p/><w:p></w:p><w:p w:x=\"/>\"><w:r/></w:p>"),
            (3, 0, 0)
        );
        // a text box is inside a paragraph, and a table in a paragraph is not a block
        assert_eq!(
            body(
                "<w:p><w:r><w:txbxContent><w:p/><w:tbl><w:tr/></w:tbl></w:txbxContent></w:r></w:p>"
            ),
            (1, 0, 0)
        );
        // nested tables, rows and cells
        assert_eq!(
            body(
                "<w:tbl><w:tr><w:tc><w:p/><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl></w:tc><w:tc><w:p/></w:tc></w:tr><w:tr/></w:tbl>"
            ),
            (3, 2, 3)
        );
        // wrappers are walked through, a control's properties are not
        assert_eq!(
            body(
                "<w:sdt><w:sdtPr><w:p/></w:sdtPr><w:sdtContent><w:p/><w:tbl/></w:sdtContent></w:sdt><w:ins><w:p/></w:ins><w:customXml><w:p/></w:customXml>"
            ),
            (3, 1, 0)
        );
        // only the taken branch
        let ac = |c: &str, f: &str| {
            format!(
                "<mc:AlternateContent><mc:Choice {c}><w:p/><w:p/></mc:Choice><mc:Fallback>{f}</mc:Fallback></mc:AlternateContent>"
            )
        };
        assert_eq!(body(&ac("Requires=\"wps\"", "<w:p/>")), (2, 0, 0));
        assert_eq!(
            body(&ac("Requires=\"zz\" xmlns:zz=\"urn:unknown\"", "<w:p/>")),
            (1, 0, 0)
        );
        assert_eq!(body(&ac("", "<w:p/>")), (1, 0, 0));
        // comments, CDATA and misplaced elements
        assert_eq!(
            body(
                "<!-- <w:p/> --><w:p><![CDATA[<w:p/>]]></w:p><w:tbl><w:p/><w:tr><w:p/></w:tr></w:tbl>"
            ),
            (1, 1, 1)
        );
        // a cell outside a row is nothing, another prefix for the same namespace is the same
        assert_eq!(body("<w:tc><w:p/></w:tc>"), (0, 0, 0));
        let other = "<x:document xmlns:x=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><x:body><x:p/><w:p xmlns:w=\"urn:other\"/></x:body></x:document>";
        assert_eq!(c(other, StoryKind::Body), (1, 0, 0));
        let strict = "<w:document xmlns:w=\"http://purl.oclc.org/ooxml/wordprocessingml/main\"><w:body><w:p/></w:body></w:document>";
        assert_eq!(c(strict, StoryKind::Body), (1, 0, 0));
    }

    #[test]
    fn the_count_of_a_note_part_skips_separators_and_the_count_of_a_header_reads_its_root() {
        let notes = format!(
            "<w:footnotes {NS}><w:footnote w:type=\"separator\" w:id=\"-1\"><w:p/></w:footnote><w:footnote w:id=\"1\"><w:p/><w:p/></w:footnote><w:p/></w:footnotes>"
        );
        assert_eq!(c(&notes, StoryKind::Footnotes), (2, 0, 0));
        let comments = notes
            .replace("footnotes", "comments")
            .replace("footnote", "comment");
        assert_eq!(
            c(&comments, StoryKind::Comments),
            (3, 0, 0),
            "a comment is a unit whatever its type says"
        );
        let header =
            format!("<w:hdr {NS}><w:p/><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl></w:hdr>");
        assert_eq!(c(&header, StoryKind::Header), (2, 1, 1));
        assert_eq!(c(&header, StoryKind::Footer), (0, 0, 0));
    }

    #[test]
    fn only_the_first_body_is_counted() {
        let xml = format!(
            "<w:document xmlns:w=\"{}\"><w:body><w:p/></w:body><w:body><w:p/><w:p/></w:body></w:document>",
            W[0]
        );
        assert_eq!(c(&xml, StoryKind::Body), (1, 0, 0));
    }

    #[test]
    fn a_part_in_utf16_is_counted_as_its_bytes_declare() {
        let xml = doc("<w:p/><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>");
        let want = c(&xml, StoryKind::Body);
        assert_eq!(want, (2, 1, 1));
        let units: Vec<u16> = xml.encode_utf16().collect();
        let mut le = vec![0xFF, 0xFE];
        le.extend(units.iter().flat_map(|u| u.to_le_bytes()));
        let mut be = vec![0xFE, 0xFF];
        be.extend(units.iter().flat_map(|u| u.to_be_bytes()));
        let no_mark: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
        let mut utf8_mark = vec![0xEF, 0xBB, 0xBF];
        utf8_mark.extend(xml.as_bytes());
        for bytes in [le, be, utf8_mark] {
            let got = count_blocks(&bytes, StoryKind::Body);
            assert_eq!((got.paragraphs, got.tables, got.rows), want);
        }
        // without a mark, only a part that starts with `<?` is told apart, as XML says
        let decl = format!("<?xml version=\"1.0\" encoding=\"UTF-16\"?>{xml}");
        let bytes: Vec<u8> = decl.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let got = count_blocks(&bytes, StoryKind::Body);
        assert_eq!((got.paragraphs, got.tables, got.rows), want);
        let _ = no_mark;
    }

    #[test]
    fn the_naive_count_is_what_the_rules_correct() {
        let xml = doc("<w:p><w:r><w:txbxContent><w:p/></w:txbxContent></w:r></w:p><w:pPr/>");
        assert_eq!(
            naive_count(xml.as_bytes()).paragraphs,
            2,
            "a w:pPr is not a w:p, a text box paragraph is counted"
        );
        assert_eq!(
            reasons(xml.as_bytes()),
            ["text boxes hold paragraphs inside a paragraph"]
        );
    }
}
