//! The reading half of the corpus harness (`tests_corpus.rs`): the container and the relationships of the
//! main part, as `xml/corpus_laws.rs` reads the container. It does no file I/O. `wordml` may not depend on
//! `package`, so the parts of a document are read with a small reader of its own.

use super::*;
use crate::api::limits::Limits;
use crate::xml::{Bytes, SpanTree};
use miniz_oxide::inflate::decompress_to_vec_with_limit;

// ---- the container, as `xml/corpus_laws.rs` reads it ------------------------------------------------

pub(super) fn le(b: &[u8], o: usize, n: usize) -> Option<usize> {
    let s = b.get(o..o.checked_add(n)?)?;
    usize::try_from(s.iter().rev().fold(0u64, |v, &x| (v << 8) | u64::from(x))).ok()
}

pub(super) struct RawPart {
    pub(super) name: String,
    pub(super) bytes: Result<Vec<u8>, String>,
}

/// The XML parts of a ZIP file, by the rules Python's `zipfile` follows.
pub(super) fn xml_parts(zip: &[u8]) -> Result<Vec<RawPart>, String> {
    let eocd = (0..zip.len().saturating_sub(21))
        .rev()
        .find(|&i| zip.get(i..i + 4) == Some(b"PK\x05\x06"))
        .ok_or("no end of central directory record")?;
    let (mut total, mut size, mut offset) = (
        le(zip, eocd + 10, 2),
        le(zip, eocd + 12, 4),
        le(zip, eocd + 16, 4),
    );
    let mut zip64 = false;
    if eocd >= 20 && zip.get(eocd - 20..eocd - 16) == Some(b"PK\x06\x07") {
        let record = eocd.checked_sub(76).ok_or("no room for the ZIP64 record")?;
        if zip.get(record..record + 4) == Some(b"PK\x06\x06") {
            (total, size, offset) = (
                le(zip, record + 32, 8),
                le(zip, record + 40, 8),
                le(zip, record + 48, 8),
            );
            zip64 = true;
        }
    }
    let (Some(total), Some(size), Some(offset)) = (total, size, offset) else {
        return Err("the end record is out of range".into());
    };
    let concat = eocd
        .checked_sub(size + offset + if zip64 { 76 } else { 0 })
        .ok_or("the central directory is not where the end record says")?;
    let mut pos = offset + concat;
    let mut out = Vec::new();
    for _ in 0..total {
        if zip.get(pos..pos + 4) != Some(b"PK\x01\x02") {
            return Err("a central directory record is missing".into());
        }
        let field = |o: usize, n: usize| le(zip, pos + o, n).ok_or("a central record is cut short");
        let (method, nl, el, cl) = (field(10, 2)?, field(28, 2)?, field(30, 2)?, field(32, 2)?);
        let (mut csize, mut usize_, mut header) = (field(20, 4)?, field(24, 4)?, field(42, 4)?);
        let name = String::from_utf8_lossy(
            zip.get(pos + 46..pos + 46 + nl)
                .ok_or("a name is cut short")?,
        )
        .into_owned();
        let extra = zip
            .get(pos + 46 + nl..pos + 46 + nl + el)
            .ok_or("an extra field is cut short")?;
        pos += 46 + nl + el + cl;
        let mut x = 0;
        while x + 4 <= extra.len() {
            let len = le(extra, x + 2, 2).ok_or("bad extra field")?;
            if le(extra, x, 2) == Some(1) {
                let mut o = x + 4;
                for value in [&mut usize_, &mut csize, &mut header] {
                    if *value == 0xFFFF_FFFF {
                        *value = le(extra, o, 8).ok_or("a ZIP64 field is cut short")?;
                        o += 8;
                    }
                }
            }
            x += 4 + len;
        }
        let lower = name.to_ascii_lowercase();
        if !(lower.ends_with(".xml") || lower.ends_with(".rels")) {
            continue;
        }
        let bytes = (|| {
            let lfh = header + concat;
            let (lnl, lel) = (
                le(zip, lfh + 26, 2).ok_or("local header")?,
                le(zip, lfh + 28, 2).ok_or("local header")?,
            );
            let start = lfh + 30 + lnl + lel;
            let data = zip.get(start..start + csize).ok_or("data out of range")?;
            match method {
                0 => Ok(data.to_vec()),
                8 if usize_ <= 64 << 20 => decompress_to_vec_with_limit(data, usize_)
                    .map_err(|e| format!("inflate: {e:?}")),
                8 => Err("too large to check".to_owned()),
                m => Err(format!("compression method {m}")),
            }
        })();
        out.push(RawPart { name, bytes });
    }
    Ok(out)
}

pub(super) fn parse(name: &str, bytes: &[u8]) -> Result<SpanTree, crate::api::error::StetError> {
    SpanTree::parse(name, Bytes::from(bytes.to_vec()), &Limits::default())
}

/// The part with this name, as the container spells it.
pub(super) fn find<'a>(parts: &'a [RawPart], name: &str) -> Option<&'a RawPart> {
    parts.iter().find(|p| p.name == name)
}

/// The name of the main part: the office document relationship of the package, else the usual name.
pub(super) fn main_name(parts: &[RawPart]) -> Option<String> {
    let by_name = |n: &str| {
        parts
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(n))
            .map(|p| p.name.clone())
    };
    let target = find(parts, "_rels/.rels")
        .and_then(|p| p.bytes.as_ref().ok())
        .and_then(|b| parse("_rels/.rels", b).ok())
        .and_then(|t| {
            t.root()
                .child_elements()
                .find(|r| {
                    r.attr("", "Type")
                        .is_some_and(|ty| ty.ends_with("/officeDocument"))
                })
                .and_then(|r| {
                    r.attr("", "Target")
                        .map(|v| v.trim_start_matches('/').to_owned())
                })
        })
        .filter(|n| !n.is_empty());
    target
        .as_deref()
        .and_then(by_name)
        .or_else(|| by_name("word/document.xml"))
}

/// The relationships of the main part as `wordml` takes them: a target resolved against the part's
/// folder, `External` for an external one, `Invalid` for one that names no part.
pub(super) fn rels_of(parts: &[RawPart], main: &str) -> Rels {
    let (dir, file) = main.rsplit_once('/').unwrap_or(("", main));
    let rels_name = if dir.is_empty() {
        format!("_rels/{file}.rels")
    } else {
        format!("{dir}/_rels/{file}.rels")
    };
    let Some(tree) = find(parts, &rels_name)
        .and_then(|p| p.bytes.as_ref().ok())
        .and_then(|b| parse(&rels_name, b).ok())
    else {
        return Rels::default();
    };
    let mut out = Vec::new();
    for r in tree
        .root()
        .child_elements()
        .filter(|r| r.local_name() == "Relationship")
    {
        let (Some(id), Some(rel_type), Some(target)) =
            (r.attr("", "Id"), r.attr("", "Type"), r.attr("", "Target"))
        else {
            continue;
        };
        let external = r
            .attr("", "TargetMode")
            .is_some_and(|m| m.eq_ignore_ascii_case("External"));
        let target = if external {
            RelTarget::External(target.into_owned())
        } else {
            let mut segments: Vec<&str> = if target.starts_with('/') {
                Vec::new()
            } else {
                dir.split('/').filter(|s| !s.is_empty()).collect()
            };
            for s in target.trim_start_matches('/').split('/') {
                match s {
                    "." | "" => {}
                    ".." => {
                        segments.pop();
                    }
                    s => segments.push(s),
                }
            }
            let name = segments.join("/");
            match find(parts, &name) {
                Some(p) => RelTarget::Part(p.name.clone()),
                None => RelTarget::Invalid,
            }
        };
        out.push(Rel {
            id: id.into_owned(),
            rel_type: rel_type.into_owned(),
            target,
        });
    }
    Rels::new(out)
}
