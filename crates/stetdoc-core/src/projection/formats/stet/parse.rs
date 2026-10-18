//! Reading edited files into a draft: the header, the head block, and the lines.
//!
//! Each physical line is read, continuations are joined before anything else is looked at (5.3), blank
//! lines without a ref are dropped (5.4), and each logical line becomes a draft line: a paragraph with
//! its tags, slots and seams, or a structure line. A syntax error is `invalid_payload` and names the
//! file, the line and the way out. The five tolerated changes of 5.4 are the only things that are
//! ignored: trailing spaces, blank lines and the final newline, CRLF, a byte order mark, and the lines
//! of the head block.

use super::parse_inline::{ParseError, parse_text};
use super::parse_lines::parse_structure;
use super::parse_structure::{is_head_line, is_structure_line, parse_table_or_row, parse_tags};
use super::refs_syntax::{read_ref, split_ref};
use super::tokens::{Tok, tokenize};
use crate::api::error::{ErrorCode, StetError};
use crate::api::limits::Limits;
use crate::projection::draft::{Draft, DraftBlock, DraftFile, DraftLine, Facts, Structure};
use crate::refs::RefKind;

fn invalid(name: &str, line: u32, message: impl AsRef<str>) -> StetError {
    StetError::new(
        ErrorCode::InvalidPayload,
        format!("{name} line {line}: {}", message.as_ref()),
    )
    .with_next("Fix the line, or export the files again to start from the document's own text.")
    .in_file(name, line)
}

fn refused(name: &str, line: u32, error: ParseError) -> StetError {
    match error {
        ParseError::Invalid(message) => invalid(name, line, message),
        ParseError::Limit(message) => StetError::new(
            ErrorCode::LimitExceeded,
            format!("{name} line {line}: {message}"),
        )
        .in_file(name, line),
    }
}

/// Reads every file of the format. A file with any other name is not the format's and is ignored.
pub(super) fn parse_files(
    names: &[&str],
    files: &[(String, String)],
    limits: &Limits,
) -> Result<Draft, StetError> {
    let mut draft = Draft::default();
    for (name, text) in files {
        if names.contains(&name.as_str()) {
            draft.files.push(parse_file(name, text, limits)?);
        }
    }
    Ok(draft)
}

/// How many backslashes end a line.
fn trailing_backslashes(line: &str) -> usize {
    line.bytes().rev().take_while(|b| *b == b'\\').count()
}

/// The logical lines of a file, each with the number of the physical line it starts on.
fn logical_lines(name: &str, text: &str, limits: &Limits) -> Result<Vec<(u32, String)>, StetError> {
    let mut physical: Vec<&str> = text.split('\n').collect();
    if text.ends_with('\n') {
        physical.pop();
    }
    let max_lines = usize::try_from(limits.max_xml_nodes).unwrap_or(usize::MAX);
    if physical.len() > max_lines {
        return Err(StetError::new(
            ErrorCode::LimitExceeded,
            format!("{name} has more than {max_lines} lines, which is the limit."),
        )
        .in_file(name, 0));
    }
    let mut lines = Vec::new();
    let mut current: Option<(u32, String)> = None;
    for (index, raw) in physical.iter().enumerate() {
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.contains('\r') {
            return Err(invalid(
                name,
                number,
                "a carriage return that is not part of a line ending. Use line feeds, or CRLF throughout",
            ));
        }
        let (start, mut text) = current.take().unwrap_or((number, String::new()));
        if trailing_backslashes(line) % 2 == 1 {
            text.push_str(line.get(..line.len() - 1).unwrap_or(""));
            current = Some((start, text));
        } else {
            text.push_str(line);
            lines.push((start, text));
        }
    }
    if let Some((start, _)) = current {
        return Err(invalid(
            name,
            start,
            "the file ends in a continuation: a line that ends in a backslash continues on the next line",
        ));
    }
    Ok(lines)
}

struct Header {
    base: String,
    common_style: Option<Box<str>>,
}

fn parse_header(name: &str, line: u32, text: &str) -> Result<Header, StetError> {
    let trimmed = text.trim_end_matches([' ', '\t']);
    let bad = |why: &str| {
        invalid(
            name,
            line,
            format!(
                "the first line must be the header, such as {{stet 1 file={name} base=.. mode=redline}}: {why}"
            ),
        )
    };
    let inner = trimmed
        .strip_prefix("{stet ")
        .and_then(|rest| rest.strip_suffix('}'))
        .ok_or_else(|| bad("it is missing or damaged"))?;
    let toks = tokenize(inner).map_err(|e| bad(&e))?;
    match toks.first() {
        Some(Tok::Word(version)) if version == "1" => {}
        Some(Tok::Word(version)) if version.bytes().all(|b| b.is_ascii_digit()) => {
            return Err(StetError::new(
                ErrorCode::UnsupportedVersion,
                format!("{name} is written in stet/{version}, and this release reads stet/1."),
            )
            .with_next("Export the files again with this release.")
            .in_file(name, line));
        }
        _ => return Err(bad("the format version is missing")),
    }
    let (mut file, mut base, mut mode, mut common_style) = (None, None, None, None);
    for tok in toks.iter().skip(1) {
        match tok {
            Tok::Named(key, value) => match (key.as_str(), &**value) {
                ("file", Tok::Word(w)) => file = Some(w.clone()),
                ("base", Tok::Word(w)) => base = Some(w.clone()),
                ("mode", Tok::Word(w)) => mode = Some(w.clone()),
                ("common-style", Tok::Quoted(s)) => common_style = Some(s.as_str().into()),
                ("view", _) => {
                    return Err(bad(
                        "this is a view, and a view is not a file that can be edited",
                    ));
                }
                (other, _) => return Err(bad(&format!("{other}= is not known"))),
            },
            _ => return Err(bad("something unexpected")),
        }
    }
    if file.as_deref() != Some(name) {
        return Err(bad(&format!(
            "its file= is {:?}, and this file is {name}. Were two files swapped?",
            file.unwrap_or_default()
        )));
    }
    if mode.as_deref() != Some("redline") {
        return Err(bad("readable files are always mode=redline"));
    }
    let base = base.ok_or_else(|| bad("base= is missing"))?;
    Ok(Header { base, common_style })
}

fn parse_file(name: &str, text: &str, limits: &Limits) -> Result<DraftFile, StetError> {
    if u64::try_from(text.len()).map_or(true, |len| len > limits.max_entry_bytes) {
        return Err(StetError::new(
            ErrorCode::LimitExceeded,
            format!(
                "{name} is larger than the limit of {} bytes for one file.",
                limits.max_entry_bytes
            ),
        )
        .in_file(name, 0));
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let logical = logical_lines(name, text, limits)?;
    let mut iter = logical.iter().peekable();
    while iter
        .next_if(|(_, line)| line.trim_end_matches([' ', '\t']).is_empty())
        .is_some()
    {}
    let Some((header_line, header_text)) = iter.next() else {
        return Err(invalid(
            name,
            1,
            "the file is empty: it must start with the header",
        ));
    };
    let header = parse_header(name, *header_line, header_text)?;
    while let Some((_, line)) = iter.peek() {
        let line = line.trim_end_matches([' ', '\t']);
        if line.is_empty() || is_head_line(line) {
            iter.next();
        } else {
            break;
        }
    }
    let mut lines = Vec::new();
    for (number, raw) in iter {
        let line = raw.trim_end_matches([' ', '\t']);
        if line.is_empty() {
            continue;
        }
        if is_head_line(line) {
            return Err(invalid(
                name,
                *number,
                "a line of the head block outside the head block. The head block is the header and the lines after it that start with {rules:, {authors:, {summary:, {warning or {position:. To write a literal brace, write \\{",
            ));
        }
        if is_view_line(line) {
            return Err(invalid(
                name,
                *number,
                "this line comes from a view: a margin note, an appendix or an outline. Views are for reading, and a file holds none of them. Delete the line",
            ));
        }
        lines.push(parse_line(name, *number, line)?);
    }
    Ok(DraftFile {
        name: name.to_string(),
        base: Some(header.base),
        common_style: header.common_style,
        lines,
    })
}

/// True for a line that only a page of a view has.
fn is_view_line(line: &str) -> bool {
    line.starts_with("{~") || line.starts_with("{appendix}") || line.starts_with("{outline:")
}

fn parse_line(name: &str, number: u32, line: &str) -> Result<DraftLine, StetError> {
    let err = |e: ParseError| refused(name, number, e);
    if let Some((token, rest)) = split_ref(line) {
        let r = read_ref(token).map_err(|e| invalid(name, number, e.message()))?;
        return match r.kind() {
            RefKind::Paragraph => {
                let (tags, at) = parse_tags(rest, &[]).map_err(err)?;
                let tail = rest.get(at..).unwrap_or("");
                let text = if tail.is_empty() {
                    ""
                } else {
                    tail.strip_prefix(' ').ok_or_else(|| {
                        invalid(
                            name,
                            number,
                            "put exactly one space between the tags and the text",
                        )
                    })?
                };
                let cut = parse_text(text).map_err(|e| refused(name, number, e))?;
                Ok(DraftLine::Block(DraftBlock {
                    r#ref: Some(r),
                    tags,
                    texts: cut.texts,
                    seams: cut.seams,
                    line: number,
                    facts: Facts::default(),
                }))
            }
            RefKind::Table | RefKind::Row => {
                let key = parse_table_or_row(r, rest).map_err(err)?;
                Ok(DraftLine::Structure(Structure { key, line: number }))
            }
        };
    }
    if is_structure_line(line) {
        let key = parse_structure(line).map_err(err)?;
        return Ok(DraftLine::Structure(Structure { key, line: number }));
    }
    let cut = parse_text(line).map_err(err)?;
    Ok(DraftLine::Block(DraftBlock {
        r#ref: None,
        tags: Vec::new(),
        texts: cut.texts,
        seams: cut.seams,
        line: number,
        facts: Facts::default(),
    }))
}
