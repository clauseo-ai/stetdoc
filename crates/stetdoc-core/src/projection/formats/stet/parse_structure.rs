//! The tags of a paragraph line, the table and row lines, and the structure lines.
//!
//! A ref line is a ref, then tags that are each a marker with a space in front, then one space and the
//! text. A line that starts with `{` is structure when its first word is one of the structure words, and
//! a line with no ref that is not structure is a new paragraph. A tag word and an inline marker never
//! start the same way, so the first thing that is not a tag starts the text.

use super::parse_inline::{ParseError, rev_tokens};
use super::tokens::{Tok, marker_end, tokenize};
use super::words::{cell_from, change_from, row_from};
use crate::projection::keys::{Key, LabelKey, PropKey, RevRef, RowKey, TableKey};
use crate::projection::model::MarkKind;
use crate::refs::BlockRef;

/// The first words of the tags of a paragraph line and of a table line.
pub(super) const TAG_WORDS: &[&str] = &[
    "style",
    "§",
    "•",
    "¶+",
    "¶-",
    "rev",
    "hidden-para",
    "joins-next",
    "+",
    "span",
    "vmerge",
    "cell",
];

/// The first words of the structure lines.
pub(super) const STRUCTURE_WORDS: &[&str] = &[
    "/table", "table", "row", "section", "opaque", "footnote", "endnote", "header", "footer",
    "comment", "reply", "anchored",
];

/// The first words of the lines of a head block.
const HEAD_PREFIXES: &[&str] = &[
    "{rules:",
    "{authors:",
    "{summary:",
    "{warning ",
    "{position:",
];

/// True if the line is one of the head block's.
pub(super) fn is_head_line(line: &str) -> bool {
    HEAD_PREFIXES.iter().any(|prefix| line.starts_with(prefix))
}

/// The first word of the marker that starts at the start of `s`, where `s` starts with `{`.
pub(super) fn first_word(s: &str) -> &str {
    let rest = s.get(1..).unwrap_or("");
    let end = rest.find([' ', '}', '"']).unwrap_or(rest.len());
    rest.get(..end).unwrap_or("")
}

/// True if a line with no ref is a structure line.
pub(super) fn is_structure_line(line: &str) -> bool {
    line.starts_with('{') && STRUCTURE_WORDS.contains(&first_word(line))
}

pub(super) fn marker_tokens(s: &str, at: usize) -> Result<(Vec<Tok>, usize), String> {
    let end = marker_end(s, at)?;
    let body = s.get(at + 1..end).unwrap_or("");
    Ok((tokenize(body)?, end + 1))
}

pub(super) fn rev_from(toks: &[Tok]) -> Result<RevRef, String> {
    let (rev, direction) = rev_tokens(toks)?;
    if direction.is_some() {
        return Err(
            "only a text change or a paragraph mark names the other end of a move".to_string(),
        );
    }
    Ok(rev)
}

/// One tag, from its tokens.
fn tag_key(toks: &[Tok]) -> Result<Key, String> {
    let Some(Tok::Word(first)) = toks.first() else {
        return Err("an empty tag".to_string());
    };
    let rest = toks.get(1..).unwrap_or(&[]);
    match first.as_str() {
        "style" => match rest {
            [Tok::Quoted(name)] => Ok(Key::Style(name.as_str().into())),
            _ => Err("{style} needs the style's name in quotes".to_string()),
        },
        "§" => {
            let Some(Tok::Quoted(text)) = rest.first() else {
                return Err("{§} needs the number in quotes".to_string());
            };
            let (ghost, hidden) = label_flags(rest.get(1..).unwrap_or(&[]))?;
            Ok(Key::Label(LabelKey {
                text: text.as_str().into(),
                bullet: false,
                ghost,
                hidden,
            }))
        }
        "•" => {
            let (glyph, flags) = match rest.first() {
                Some(Tok::Quoted(glyph)) => (glyph.as_str(), rest.get(1..).unwrap_or(&[])),
                _ => ("•", rest),
            };
            let (ghost, hidden) = label_flags(flags)?;
            Ok(Key::Label(LabelKey {
                text: glyph.into(),
                bullet: true,
                ghost,
                hidden,
            }))
        }
        sign @ ("¶+" | "¶-") => {
            let (rev, direction) = rev_tokens(rest)?;
            let plus = sign == "¶+";
            let kind = match (plus, direction) {
                (true, None) => MarkKind::Inserted,
                (false, None) => MarkKind::Deleted,
                (true, Some(super::parse_inline::Direction::From)) => MarkKind::MovedTo,
                (false, Some(super::parse_inline::Direction::To)) => MarkKind::MovedFrom,
                _ => return Err("a paragraph mark is inserted or moved in with `¶+`, deleted or moved away with `¶-`".to_string()),
            };
            Ok(Key::Mark { kind, rev })
        }
        "rev" => {
            let Some(Tok::Word(kind)) = rest.first() else {
                return Err("{rev} needs the kind of change".to_string());
            };
            let kind =
                change_from(kind).ok_or_else(|| format!("{kind:?} is not a kind of change"))?;
            let rev = rev_from(rest.get(1..).unwrap_or(&[]))?;
            Ok(Key::Rev(PropKey { kind, rev }))
        }
        "hidden-para" if rest.is_empty() => Ok(Key::HiddenPara),
        "joins-next" if rest.is_empty() => Ok(Key::JoinsNext),
        "+" if rest.is_empty() => Ok(Key::CellPlus),
        "span" => match rest {
            [Tok::Word(n)] => n
                .parse::<u32>()
                .map(Key::CellSpan)
                .map_err(|_| format!("{n:?} is not a number of columns")),
            _ => Err("{span} needs a number of columns".to_string()),
        },
        "vmerge" => match rest {
            [] => Ok(Key::CellVMerge { restart: false }),
            [Tok::Word(w)] if w == "restart" => Ok(Key::CellVMerge { restart: true }),
            _ => Err("{vmerge} is `{vmerge}` or `{vmerge restart}`".to_string()),
        },
        "cell" => {
            let Some(Tok::Word(kind)) = rest.first() else {
                return Err("{cell} needs `inserted`, `deleted` or `merged`".to_string());
            };
            let kind = cell_from(kind)
                .ok_or_else(|| format!("{kind:?} is not `inserted`, `deleted` or `merged`"))?;
            let rev = rev_from(rest.get(1..).unwrap_or(&[]))?;
            Ok(Key::CellChange { kind, rev })
        }
        other => Err(format!("{{{other} ..}} is not a tag")),
    }
}

fn label_flags(toks: &[Tok]) -> Result<(bool, bool), String> {
    let (mut ghost, mut hidden) = (false, false);
    for tok in toks {
        match tok {
            Tok::Word(w) if w == "ghost" => ghost = true,
            Tok::Word(w) if w == "hidden" => hidden = true,
            _ => return Err("a number's tag takes only `ghost` and `hidden`".to_string()),
        }
    }
    Ok((ghost, hidden))
}

/// The tags at the start of `rest`, which is what follows a ref: each is a space and a marker. Returns
/// the tags and what is left, which is empty or starts with the one space that separates the text.
pub(super) fn parse_tags(rest: &str, only: &[&str]) -> Result<(Vec<Key>, usize), ParseError> {
    let mut keys = Vec::new();
    let mut at = 0;
    loop {
        let after = rest.get(at..).unwrap_or("");
        let Some(marker) = after.strip_prefix(' ') else {
            return Ok((keys, at));
        };
        if !marker.starts_with('{') {
            return Ok((keys, at));
        }
        let word = first_word(marker);
        if !TAG_WORDS.contains(&word) {
            return Ok((keys, at));
        }
        if !only.is_empty() && !only.contains(&word) {
            return Err(ParseError::Invalid(format!(
                "{{{word} ..}} is not a tag of this line"
            )));
        }
        let (toks, next) = marker_tokens(rest, at + 1)?;
        keys.push(tag_key(&toks)?);
        at = next;
    }
}

/// The cell tags that may precede a table marker.
pub(super) const CELL_TAG_WORDS: &[&str] = &["+", "span", "vmerge", "cell"];

fn revs_after(rest: &str, mut at: usize) -> Result<Vec<PropKey>, ParseError> {
    let mut revs = Vec::new();
    loop {
        let after = rest.get(at..).unwrap_or("");
        if after.is_empty() {
            return Ok(revs);
        }
        let Some(marker) = after.strip_prefix(' ') else {
            return Err(ParseError::Invalid(
                "something after the end of the line's marker".to_string(),
            ));
        };
        if !marker.starts_with('{') || first_word(marker) != "rev" {
            return Err(ParseError::Invalid(
                "a table, row or section line holds only its marker and its changes".to_string(),
            ));
        }
        let (toks, next) = marker_tokens(rest, at + 1)?;
        match tag_key(&toks)? {
            Key::Rev(prop) => revs.push(prop),
            _ => {
                return Err(ParseError::Invalid(
                    "a {rev ..} tag was expected".to_string(),
                ));
            }
        }
        at = next;
    }
}

fn named_u32(toks: &[Tok], name: &str) -> Option<u32> {
    toks.iter().find_map(|tok| match tok {
        Tok::Named(n, v) if n == name => match &**v {
            Tok::Word(w) => w.parse().ok(),
            _ => None,
        },
        _ => None,
    })
}

pub(super) fn table_key(
    r#ref: Option<BlockRef>,
    cell: Vec<Key>,
    rest: &str,
    at: usize,
) -> Result<Key, ParseError> {
    let after = rest.get(at..).unwrap_or("");
    let Some(marker) = after
        .strip_prefix(' ')
        .or(if r#ref.is_none() { Some(after) } else { None })
    else {
        return Err(ParseError::Invalid(
            "a table line is `t3 {table cols=2}`".to_string(),
        ));
    };
    if !marker.starts_with("{table") {
        return Err(ParseError::Invalid(
            "a `t` ref belongs to a `{table ..}` line".to_string(),
        ));
    }
    let start = rest.len() - marker.len();
    let (toks, next) = marker_tokens(rest, start)?;
    if !matches!(toks.first(), Some(Tok::Word(w)) if w == "table") {
        return Err(ParseError::Invalid(
            "a `t` ref belongs to a `{table ..}` line".to_string(),
        ));
    }
    let cols = named_u32(&toks, "cols")
        .ok_or_else(|| ParseError::Invalid("{table} needs cols=N".to_string()))?;
    let mut style = None;
    for tok in toks.iter().skip(1) {
        match tok {
            Tok::Named(n, v) if n == "cols" && matches!(&**v, Tok::Word(_)) => {}
            Tok::Named(n, v) if n == "style" => match &**v {
                Tok::Quoted(text) => style = Some(text.as_str().into()),
                Tok::Word(_) => {
                    return Err(ParseError::Invalid(
                        "style= needs the name in quotes".to_string(),
                    ));
                }
                Tok::Named(..) => {}
            },
            _ => {
                return Err(ParseError::Invalid(
                    "{table} takes cols=N and style=\"..\"".to_string(),
                ));
            }
        }
    }
    let revs = revs_after(rest, next)?;
    Ok(Key::Table(TableKey {
        r#ref,
        cols,
        style,
        revs,
        cell,
    }))
}

pub(super) fn row_key(r#ref: Option<BlockRef>, rest: &str, at: usize) -> Result<Key, ParseError> {
    let after = rest.get(at..).unwrap_or("");
    let Some(marker) = after
        .strip_prefix(' ')
        .or(if r#ref.is_none() { Some(after) } else { None })
    else {
        return Err(ParseError::Invalid("a row line is `r1 {row}`".to_string()));
    };
    if !marker.starts_with("{row") {
        return Err(ParseError::Invalid(
            "an `r` ref belongs to a `{row}` line".to_string(),
        ));
    }
    let start = rest.len() - marker.len();
    let (toks, next) = marker_tokens(rest, start)?;
    if !matches!(toks.first(), Some(Tok::Word(w)) if w == "row") {
        return Err(ParseError::Invalid(
            "an `r` ref belongs to a `{row}` line".to_string(),
        ));
    }
    let mut header = false;
    let mut change = None;
    let mut i = 1;
    while let Some(tok) = toks.get(i) {
        match tok {
            Tok::Word(w) if w == "header" => {
                header = true;
                i += 1;
            }
            Tok::Word(w) if row_from(w).is_some() => {
                let kind =
                    row_from(w).ok_or_else(|| ParseError::Invalid("a row change".to_string()))?;
                change = Some((kind, rev_from(toks.get(i + 1..).unwrap_or(&[]))?));
                i = toks.len();
            }
            _ => {
                return Err(ParseError::Invalid(
                    "{row} takes `header`, `inserted` or `deleted`".to_string(),
                ));
            }
        }
    }
    let revs = revs_after(rest, next)?;
    Ok(Key::Row(RowKey {
        r#ref,
        header,
        change,
        revs,
    }))
}

/// A `t` or `r` ref line, after the ref.
pub(super) fn parse_table_or_row(r#ref: BlockRef, rest: &str) -> Result<Key, ParseError> {
    match r#ref.kind() {
        crate::refs::RefKind::Table => {
            let (cell, at) = parse_tags(rest, CELL_TAG_WORDS)?;
            table_key(Some(r#ref), cell, rest, at)
        }
        _ => row_key(Some(r#ref), rest, 0),
    }
}
