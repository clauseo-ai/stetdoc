//! The end of the archive: the end record, and the ZIP64 record and locator that may precede it.

use super::errors::malformed;
use super::layout::{SIG_Z64_EOCD, SIG_Z64_LOC, Zip64};
use super::le::{u16_at, u32_at, u64_at};
use crate::api::error::StetError;
use std::ops::Range;

/// The end of central directory signature, `PK\5\6`.
const SIG_EOCD: u32 = 0x0605_4b50;

fn bad(what: impl Into<String>) -> StetError {
    malformed(what)
}

fn short(what: &str) -> StetError {
    bad(format!("truncated or inconsistent ZIP structure: {what}"))
}

/// The position of the end record, found as Python's `zipfile`, `unzip` and `bsdtar` find it: the
/// last `PK\5\6` in the final 64 KiB and 22 bytes of the file, whatever follows it. A signature
/// inside the archive comment can be that one, and then it is the end record, as it is for them.
///
/// It is the only one tried. If it is cut off, or its comment runs past the end of the file, or it
/// does not describe an archive that reads, the file is refused, and an earlier end record is not
/// used instead: a file that hides one archive behind another, or that ends in a record only some
/// readers skip, would be read as one thing here and as another by the tools that read it next.
pub(super) fn last_end_record(src: &[u8]) -> Result<usize, StetError> {
    let window = src.len().saturating_sub(22 + 65_535);
    let found = src
        .get(window..)
        .and_then(|w| memchr::memmem::rfind(w, &SIG_EOCD.to_le_bytes()))
        .map(|i| window + i);
    let Some(pos) = found else {
        return Err(bad("not a ZIP package: no end of central directory record"));
    };
    let comment = u16_at(src, pos + 20)
        .ok_or_else(|| bad("the last end of central directory record is cut off"))?;
    if pos + 22 + usize::from(comment) > src.len() {
        return Err(bad(
            "the comment of the last end of central directory record runs past the end of the file",
        ));
    }
    Ok(pos)
}

/// Zip64 extended information extra field (id 1), if present.
pub(crate) fn zip64_extra(extra: &[u8]) -> Option<Range<usize>> {
    let mut o = 0usize;
    while o + 4 <= extra.len() {
        let id = u16_at(extra, o)?;
        let len = usize::from(u16_at(extra, o + 2)?);
        let start = o + 4;
        let end = start.checked_add(len)?;
        if end > extra.len() {
            return None;
        }
        if id == 1 {
            return Some(start..end);
        }
        o = end;
    }
    None
}

/// What the end record, with any ZIP64 record, says about the central directory.
pub(super) struct EndInfo {
    pub(super) total: u64,
    pub(super) cd_size: u64,
    pub(super) cd_off: u64,
    pub(super) zip64: Option<Zip64>,
}

pub(super) fn read_end(src: &[u8], pos: usize) -> Result<EndInfo, StetError> {
    let disk = u16_at(src, pos + 4).ok_or_else(|| short("end record"))?;
    let cd_disk = u16_at(src, pos + 6).ok_or_else(|| short("end record"))?;
    let n_disk = u16_at(src, pos + 8).ok_or_else(|| short("end record"))?;
    let n_total = u16_at(src, pos + 10).ok_or_else(|| short("end record"))?;
    let size = u32_at(src, pos + 12).ok_or_else(|| short("end record"))?;
    let off = u32_at(src, pos + 16).ok_or_else(|| short("end record"))?;
    if disk != 0 && disk != 0xFFFF
        || cd_disk != 0 && cd_disk != 0xFFFF
        || n_disk != n_total && n_disk != 0xFFFF
    {
        return Err(bad("multi-disk ZIP archives are not supported"));
    }
    let mut info = EndInfo {
        total: u64::from(n_total),
        cd_size: u64::from(size),
        cd_off: u64::from(off),
        zip64: None,
    };
    let loc_pos = pos.checked_sub(20);
    let has_loc = loc_pos.is_some_and(|p| u32_at(src, p) == Some(SIG_Z64_LOC));
    let needs = n_total == 0xFFFF || size == u32::MAX || off == u32::MAX;
    if has_loc {
        // A locator right before the end record makes the ZIP64 record part of the archive for
        // the readers that look for it, whether or not the end record's own fields are all ones,
        // and they take their values from it. The ones that do not look use the end record. So the
        // record has to be where every one of them would look (the 56 bytes before the locator,
        // which is where Python's `zipfile` looks, and where the locator says it is, which is
        // where `unzip` and `bsdtar` look), and it has to agree with the end record in each field
        // the end record fills in. Anything else is a file that shows one archive to some readers
        // and another to the rest.
        let lp = loc_pos.unwrap_or(0);
        let rec = lp
            .checked_sub(56)
            .filter(|&r| u32_at(src, r) == Some(SIG_Z64_EOCD))
            .ok_or_else(|| {
                bad("the ZIP64 locator has no ZIP64 end record in the 56 bytes before it (a record with extensible data is not supported)")
            })?;
        let record_size = u64_at(src, rec + 4).ok_or_else(|| short("ZIP64 end record"))?;
        if record_size != 44 {
            return Err(bad(
                "the ZIP64 end record gives a size other than 44, so it has extensible data, which is not supported",
            ));
        }
        let stored = u64_at(src, lp + 8).ok_or_else(|| short("ZIP64 locator"))?;
        let field = |o: usize| u64_at(src, rec + o).ok_or_else(|| short("ZIP64 end record"));
        let (t_disk, t, s, o) = (field(24)?, field(32)?, field(40)?, field(48)?);
        let disks = u32_at(src, rec + 16).zip(u32_at(src, rec + 20));
        if disks != Some((0, 0))
            || t_disk != t
            || u32_at(src, lp + 4) != Some(0)
            || u32_at(src, lp + 16) != Some(1)
        {
            return Err(bad("multi-disk ZIP archives are not supported"));
        }
        let agrees = |filled: bool, in_end: u64, in_record: u64| !filled || in_end == in_record;
        if !agrees(n_total != 0xFFFF, u64::from(n_total), t)
            || !agrees(size != u32::MAX, u64::from(size), s)
            || !agrees(off != u32::MAX, u64::from(off), o)
        {
            return Err(bad(
                "the ZIP64 end record disagrees with the end record about the central directory",
            ));
        }
        if n_total == 0xFFFF {
            info.total = t;
        }
        if size == u32::MAX {
            info.cd_size = s;
        }
        if off == u32::MAX {
            info.cd_off = o;
        }
        info.zip64 = Some(Zip64 {
            record_pos: rec,
            locator_pos: lp,
            stored_off: stored,
        });
    } else if needs {
        return Err(bad("end record says ZIP64 but there is no ZIP64 locator"));
    }
    Ok(info)
}
