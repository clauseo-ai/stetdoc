//! L2 Locality by package entry, for the random batches of `laws::random`: an edit loses no entry, and every entry outside the
//! story parts that the view names and the parts an edit may change as a side effect keeps its raw bytes, as stored in the ZIP.

use std::collections::BTreeMap;

/// L2 by package entry.
pub fn locality(before: &[u8], after: &[u8], stories: &[String]) -> Result<(), String> {
    let (Some(a), Some(b)) = (entries(before), entries(after)) else {
        return Ok(()); // a ZIP64 or unusual container, which this reader does not walk
    };
    for (name, raw) in &a {
        match b.get(name) {
            None => return Err(format!("L2: the edit lost the entry {name}")),
            Some(new) if new != raw && !stories.contains(name) && !may_change(name) => {
                return Err(format!(
                    "L2: the edit changed the entry {name}, which no edit touches"
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

/// The parts besides the stories of the view that an edit may change: the comment and note parts, and the parts an edit declares
/// as side effects.
fn may_change(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let file = lower.rsplit('/').next().unwrap_or_default();
    lower == "[content_types].xml"
        || lower.ends_with(".rels")
        || (lower.starts_with("word/")
            && [
                "document",
                "footnotes",
                "endnotes",
                "header",
                "footer",
                "comments",
                "people",
                "numbering",
                "styles",
                "settings",
                "glossary",
            ]
            .iter()
            .any(|p| file.starts_with(p)))
}

/// The method (0 stored, 8 deflated) and the raw bytes of each entry of a ZIP, as stored, by name: `None` when the container is one this reader does not walk.
pub fn entries(zip: &[u8]) -> Option<BTreeMap<String, (usize, Vec<u8>)>> {
    let u16_at = |at: usize| {
        zip.get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
    };
    let u32_at = |at: usize| {
        zip.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    let eocd = (0..zip.len().saturating_sub(21))
        .rev()
        .find(|&i| zip.get(i..i + 4) == Some(&[0x50, 0x4b, 0x05, 0x06]))?;
    let count = u16_at(eocd + 10)?;
    let mut at = u32_at(eocd + 16)?;
    if count == 0xffff || at == 0xffff_ffff {
        return None;
    }
    let mut out = BTreeMap::new();
    for _ in 0..count {
        if zip.get(at..at + 4) != Some(&[0x50, 0x4b, 0x01, 0x02]) {
            return None;
        }
        let method = u16_at(at + 10)?;
        let size = u32_at(at + 20)?;
        let (n, m, k) = (u16_at(at + 28)?, u16_at(at + 30)?, u16_at(at + 32)?);
        let local = u32_at(at + 42)?;
        let name = String::from_utf8_lossy(zip.get(at + 46..at + 46 + n)?).into_owned();
        if size == 0xffff_ffff || local == 0xffff_ffff {
            return None;
        }
        let data = local + 30 + u16_at(local + 26)? + u16_at(local + 28)?;
        out.insert(name, (method, zip.get(data..data + size)?.to_vec()));
        at += 46 + n + m + k;
    }
    Some(out)
}

/// The bytes of one part of a package, inflated: `None` when the container is one [`entries`] does not walk, the part is
/// missing, or its method is neither stored nor deflated.
pub fn part(zip: &[u8], name: &str) -> Option<Vec<u8>> {
    let (method, raw) = entries(zip)?.remove(name.trim_start_matches('/'))?;
    match method {
        0 => Some(raw),
        8 => {
            let mut out = Vec::new();
            std::io::Read::read_to_end(
                &mut flate2::read::DeflateDecoder::new(raw.as_slice()),
                &mut out,
            )
            .ok()?;
            Some(out)
        }
        _ => None,
    }
}
