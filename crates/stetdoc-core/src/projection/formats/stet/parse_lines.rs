//! The structure lines that are one marker: the end of a table, a section, a block that is not shown, and
//! the lines that open a note, a header or footer, or a comment.

use super::parse_inline::ParseError;
use super::parse_structure::{first_word, rev_from, row_key, table_key};
use super::refs_syntax::read_ref;
use super::tokens::{Tok, marker_end, tokenize};
use super::words::{break_from, change_from, mark_from, slot_from};
use crate::projection::keys::{Key, PropKey, SectionKey, UnitKey};
use crate::projection::model::{OpaqueBlock, SectionUse};

/// A structure line, which is one marker.
pub(super) fn parse_structure(line: &str) -> Result<Key, ParseError> {
    let word = first_word(line);
    if word == "table" {
        return table_key(None, Vec::new(), line, 0);
    }
    if word == "row" {
        return row_key(None, line, 0);
    }
    let end = marker_end(line, 0)?;
    if line.get(end + 1..).is_some_and(|r| !r.is_empty()) {
        return Err(ParseError::Invalid(
            "something after the end of a structure line's marker".to_string(),
        ));
    }
    let toks = tokenize(line.get(1..end).unwrap_or(""))?;
    match word {
        "/table" => {
            if toks.len() == 1 {
                Ok(Key::EndTable)
            } else {
                Err(ParseError::Invalid("{/table} takes nothing".to_string()))
            }
        }
        "section" => section(&toks, line, end).map_err(ParseError::Invalid),
        "opaque" => opaque(&toks).map_err(ParseError::Invalid),
        "footnote" | "endnote" => {
            let number = |n: &str| {
                n.parse::<u32>()
                    .map_err(|_| ParseError::Invalid(format!("{n:?} is not a note number")))
            };
            let (ordinal, mark) = match toks.as_slice() {
                [_, Tok::Word(n)] => (number(n)?, None),
                [_, Tok::Word(n), Tok::Named(name, m)] if name == "mark" => match m.as_ref() {
                    Tok::Word(m) => (number(n)?, Some(number(m)?)),
                    _ => {
                        return Err(ParseError::Invalid(
                            "mark= takes the number Word shows".to_string(),
                        ));
                    }
                },
                _ => {
                    return Err(ParseError::Invalid(format!(
                        "{{{word} N}} needs the note's number, and may say mark=M"
                    )));
                }
            };
            Ok(Key::Unit(UnitKey::Note {
                endnote: word == "endnote",
                ordinal,
                mark,
            }))
        }
        "header" | "footer" => header(&toks, word == "footer").map_err(ParseError::Invalid),
        "comment" | "reply" => comment(&toks, word == "reply").map_err(ParseError::Invalid),
        "anchored" => match toks.as_slice() {
            [_, Tok::Quoted(text)] => Ok(Key::Anchored(text.as_str().into())),
            _ => Err(ParseError::Invalid(
                "{anchored} needs the text in quotes".to_string(),
            )),
        },
        other => Err(ParseError::Invalid(format!(
            "{{{other} ..}} is not a structure line"
        ))),
    }
}

fn section(toks: &[Tok], _line: &str, _end: usize) -> Result<Key, String> {
    let index = match toks.get(1) {
        Some(Tok::Word(n)) => n
            .parse::<u32>()
            .map_err(|_| format!("{n:?} is not a section number"))?,
        _ => return Err("{section N ends: ..} needs the section's number".to_string()),
    };
    if !matches!(toks.get(2), Some(Tok::Word(w)) if w == "ends:") {
        return Err("{section N ends: ..} needs `ends:`".to_string());
    }
    let Some(Tok::Word(kind)) = toks.get(3) else {
        return Err("{section N ends: ..} needs how the next section starts".to_string());
    };
    let break_type =
        break_from(kind).ok_or_else(|| format!("{kind:?} is not a kind of section break"))?;
    let mut title_pg = false;
    let mut mark = None;
    let mut revs = Vec::new();
    let mut i = 4;
    while let Some(tok) = toks.get(i) {
        match tok {
            Tok::Word(w) if w == "title-page" => {
                title_pg = true;
                i += 1;
            }
            Tok::Word(w) if w == "mark" => {
                let Some(Tok::Word(kind)) = toks.get(i + 1) else {
                    return Err("`mark` needs how the paragraph mark changed".to_string());
                };
                let kind = mark_from(kind)
                    .ok_or_else(|| format!("{kind:?} is not a kind of mark change"))?;
                let mut rev = rev_from(toks.get(i + 2..i + 4).unwrap_or(&[]))?;
                if !matches!(toks.get(i + 4), Some(Tok::Word(w)) if w == "peer") {
                    return Err("a mark change needs `peer`".to_string());
                }
                rev.peer = match toks.get(i + 5) {
                    Some(Tok::Word(p)) if p == "-" => None,
                    Some(Tok::Word(p)) => Some(
                        p.strip_prefix('#')
                            .and_then(|n| n.parse::<i64>().ok())
                            .ok_or_else(|| format!("{p:?} is not an id such as #13"))?,
                    ),
                    _ => return Err("`peer` needs an id or -".to_string()),
                };
                mark = Some((kind, rev));
                i += 6;
            }
            Tok::Word(w) if w == "rev" => {
                let Some(Tok::Word(kind)) = toks.get(i + 1) else {
                    return Err("`rev` needs the kind of change".to_string());
                };
                let kind =
                    change_from(kind).ok_or_else(|| format!("{kind:?} is not a kind of change"))?;
                let rev = rev_from(toks.get(i + 2..i + 4).unwrap_or(&[]))?;
                revs.push(PropKey { kind, rev });
                i += 4;
            }
            _ => return Err("something unexpected in a section line".to_string()),
        }
    }
    Ok(Key::Section(SectionKey {
        index,
        break_type,
        title_pg,
        mark,
        revs,
    }))
}

fn opaque(toks: &[Tok]) -> Result<Key, String> {
    let first = toks.get(1);
    let blocks_and_paragraphs = |at: usize| -> Result<u32, String> {
        match (toks.get(at), toks.get(at + 1), toks.get(at + 2)) {
            (Some(Tok::Word(n)), Some(Tok::Word(p)), None) if p == "paragraphs" => n
                .parse()
                .map_err(|_| format!("{n:?} is not a number of paragraphs")),
            _ => Err("{opaque ..} ends with a number and `paragraphs`".to_string()),
        }
    };
    match first {
        Some(Tok::Word(w)) if w == "altchunk" && toks.len() == 2 => {
            Ok(Key::Opaque(OpaqueBlock::AltChunk))
        }
        Some(Tok::Word(w)) if w == "unknown" && toks.len() == 2 => {
            Ok(Key::Opaque(OpaqueBlock::Unknown))
        }
        Some(Tok::Word(w)) if w == "math" => match toks {
            [_, _, Tok::Quoted(text)] => Ok(Key::Opaque(OpaqueBlock::Math(text.as_str().into()))),
            _ => Err("{opaque math} needs the text in quotes".to_string()),
        },
        Some(Tok::Word(keyword) | Tok::Quoted(keyword)) => {
            let mut at = 2;
            let (mut first_ref, mut last_ref) = (None, None);
            while let Some(Tok::Word(w)) = toks.get(at) {
                if let Some((a, b)) = w.split_once("..") {
                    first_ref = Some(read_ref(a).map_err(|e| e.message().to_string())?);
                    last_ref = Some(read_ref(b).map_err(|e| e.message().to_string())?);
                    at += 1;
                } else if w == "first" || w == "last" {
                    let Some(Tok::Word(r)) = toks.get(at + 1) else {
                        return Err(format!("`{w}` needs a ref"));
                    };
                    let parsed = read_ref(r).map_err(|e| e.message().to_string())?;
                    if w == "first" {
                        first_ref = Some(parsed);
                    } else {
                        last_ref = Some(parsed);
                    }
                    at += 2;
                } else {
                    break;
                }
            }
            let blocks = blocks_and_paragraphs(at)?;
            Ok(Key::Opaque(OpaqueBlock::Collapsed {
                keyword: keyword.as_str().into(),
                first: first_ref,
                last: last_ref,
                blocks,
            }))
        }
        _ => Err("{opaque ..} needs what is not shown".to_string()),
    }
}

fn header(toks: &[Tok], footer: bool) -> Result<Key, String> {
    let mut uses = Vec::new();
    let mut at = 1;
    while let Some(tok) = toks.get(at) {
        let Tok::Word(slot) = tok else {
            return Err("a header line is `{header default sections=1,3}`".to_string());
        };
        let slot = slot_from(slot)
            .ok_or_else(|| format!("{slot:?} is not `default`, `first` or `even`"))?;
        let Some(Tok::Named(name, value)) = toks.get(at + 1) else {
            return Err("a slot needs `sections=1,3`".to_string());
        };
        let (Tok::Word(list), true) = (&**value, name == "sections") else {
            return Err("a slot needs `sections=1,3`".to_string());
        };
        for section in list.split(',') {
            let section = section
                .parse::<u32>()
                .map_err(|_| format!("{section:?} is not a section number"))?;
            uses.push(SectionUse { section, slot });
        }
        at += 2;
    }
    Ok(Key::Unit(UnitKey::Header { footer, uses }))
}

fn comment_ref(tok: Option<&Tok>) -> Result<i64, String> {
    match tok {
        Some(Tok::Word(w)) => w
            .strip_prefix('c')
            .and_then(|n| n.parse::<i64>().ok())
            .ok_or_else(|| format!("{w:?} is not a comment id such as c3")),
        _ => Err("a comment id such as c3 is missing".to_string()),
    }
}

fn comment(toks: &[Tok], reply: bool) -> Result<Key, String> {
    let id = comment_ref(toks.get(1))?;
    let mut at = 2;
    let mut reply_to = None;
    if reply {
        if !matches!(toks.get(at), Some(Tok::Word(w)) if w == "to") {
            return Err("a reply names the comment it answers: {reply c4 to c3}".to_string());
        }
        reply_to = Some(comment_ref(toks.get(at + 1))?);
        at += 2;
    }
    let (mut author, mut date, mut on, mut resolved) = (None, None, None, false);
    while let Some(tok) = toks.get(at) {
        match tok {
            Tok::Word(w) if w == "by" => {
                let Some(Tok::Word(code)) = toks.get(at + 1) else {
                    return Err("`by` needs an author's code".to_string());
                };
                author = Some(code.as_str().into());
                at += 2;
            }
            Tok::Word(w) if w == "at" => {
                let Some(Tok::Quoted(text)) = toks.get(at + 1) else {
                    return Err("`at` needs a date in quotes".to_string());
                };
                date = Some(text.as_str().into());
                at += 2;
            }
            Tok::Word(w) if w == "on" => {
                let Some(Tok::Word(r)) = toks.get(at + 1) else {
                    return Err("`on` needs a ref".to_string());
                };
                on = Some(read_ref(r).map_err(|e| e.message().to_string())?);
                at += 2;
            }
            Tok::Word(w) if w == "resolved" => {
                resolved = true;
                at += 1;
            }
            _ => return Err("something unexpected in a comment line".to_string()),
        }
    }
    Ok(Key::Unit(UnitKey::Comment {
        id,
        author,
        date,
        on,
        resolved,
        reply_to,
    }))
}
