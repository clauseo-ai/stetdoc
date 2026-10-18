//! Reading the ZIP structure: end record, central directory, local headers.
//!
//! Nothing here decompresses anything. The result, `Layout`, is the partition of the input
//! into regions described in `docs/design/package-and-xml.md` section 1.1.

use super::endrec::{last_end_record, read_end, zip64_extra};
use super::errors::{limit, malformed, unsupported};
use super::le::{slice, slice_len, u16_at, u32_at, u64_at};
use super::names::{name_to_string, validate_entry_name};
use crate::api::error::{ErrorCode, StetError};
use crate::api::limits::Limits;
use std::collections::HashSet;
use std::ops::Range;

pub(crate) const SIG_LFH: u32 = 0x0403_4b50;
pub(crate) const SIG_CD: u32 = 0x0201_4b50;
pub(crate) const SIG_Z64_EOCD: u32 = 0x0606_4b50;
pub(crate) const SIG_Z64_LOC: u32 = 0x0706_4b50;
pub(crate) const SIG_DD: u32 = 0x0807_4b50;

pub(crate) const FLAG_DESCRIPTOR: u16 = 0x0008;
/// General-purpose flag bits that mean the entry's data is encrypted: traditional encryption (bit
/// 0), strong encryption (bit 6) and a masked local header, which goes with an encrypted central
/// directory (bit 13).
pub(crate) const ENCRYPTION_FLAGS: u16 = 0x0001 | 0x0040 | 0x2000;

/// The shape of a data descriptor, kept so a rewritten entry gets the same one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DataDescriptor {
    pub signature: bool,
    /// 8-byte sizes instead of 4-byte.
    pub wide: bool,
}

impl DataDescriptor {
    pub(crate) fn len(self) -> usize {
        (if self.signature { 4 } else { 0 }) + 4 + if self.wide { 16 } else { 8 }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub name: String,
    /// The raw central record: 46 fixed bytes, name, extra field, comment.
    pub cd: Range<usize>,
    /// Actual offset of the local header in the input.
    pub lfh: usize,
    /// Local header length: 30 + name + extra.
    pub lfh_len: usize,
    pub data_start: usize,
    pub csize: u64,
    pub usize_: u64,
    /// End of the entry's own bytes: data plus descriptor.
    pub entry_end: usize,
    /// General-purpose flags from the central record.
    pub flags: u16,
    pub lfh_flags: u16,
    pub method: u16,
    pub crc: u32,
    pub dd: Option<DataDescriptor>,
    pub lfh_zip64: bool,
    /// The local header carries a real CRC and sizes (not zeros).
    pub lfh_has_values: bool,
}

/// Where the ZIP64 end records are, when the archive has them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Zip64 {
    pub record_pos: usize,
    pub locator_pos: usize,
    /// Where the locator says the record is, relative to the start of the archive.
    pub stored_off: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct Layout {
    /// Entries in central directory order.
    pub entries: Vec<Entry>,
    /// Indexes into `entries`, ordered by local header offset.
    pub local_order: Vec<usize>,
    pub prefix_end: usize,
    pub cd_end: usize,
    pub eocd_pos: usize,
    /// Actual offset minus stored offset (nonzero when bytes precede the archive proper).
    pub base: i64,
    pub zip64: Option<Zip64>,
}

fn bad(what: impl Into<String>) -> StetError {
    malformed(what)
}

fn short(what: &str) -> StetError {
    bad(format!("truncated or inconsistent ZIP structure: {what}"))
}

/// Parses the container. Returns `malformed_input` or `limit_exceeded` on hostile input.
pub(crate) fn parse(src: &[u8], limits: &Limits) -> Result<Layout, StetError> {
    if src.len() as u64 > limits.max_archive_bytes {
        return Err(limit(format!(
            "package is {} bytes, over the limit of {}",
            src.len(),
            limits.max_archive_bytes
        )));
    }
    if src.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        // Do not read the OLE directory to tell the two apart: stetdoc reads neither.
        return Err(unsupported(
            "this is an OLE compound file: either a password-protected .docx or a legacy .doc, and stetdoc reads only unencrypted .docx",
        )
        .with_next("remove the password, or save the file as .docx in Word, then open it again"));
    }
    parse_at(src, last_end_record(src)?, limits)
}

fn parse_at(src: &[u8], eocd_pos: usize, limits: &Limits) -> Result<Layout, StetError> {
    let end = read_end(src, eocd_pos)?;
    if end.total > u64::from(limits.max_entries) {
        return Err(limit(format!(
            "package has {} entries, over the limit of {}",
            end.total, limits.max_entries
        )));
    }
    let total = usize::try_from(end.total).map_err(|_| short("entry count"))?;
    let size = usize::try_from(end.cd_size).map_err(|_| short("central directory size"))?;
    let stored_off = usize::try_from(end.cd_off).map_err(|_| short("central directory offset"))?;
    // The central directory ends where the end record begins, or where the ZIP64 end record does:
    // that is how Python's `zipfile`, `unzip` and `bsdtar` find it, and they use the stored offset
    // only to learn how far the archive is shifted by bytes in front of it. Taking the stored
    // offset at its word instead can show this reader a different directory than theirs.
    let after_cd = end.zip64.map_or(eocd_pos, |z| z.record_pos);
    let cd_start = after_cd
        .checked_sub(size)
        .ok_or_else(|| short("central directory size"))?;
    let empty = total == 0 && size == 0;
    if !empty && u32_at(src, cd_start) != Some(SIG_CD) {
        return Err(bad(
            "central directory not found: it has to end where the end record (or the ZIP64 end record) begins",
        ));
    }
    let base = i64::try_from(cd_start)
        .ok()
        .zip(i64::try_from(stored_off).ok())
        .and_then(|(a, b)| a.checked_sub(b))
        .ok_or_else(|| short("central directory offset"))?;
    if let Some(z) = end.zip64
        && i64::try_from(z.stored_off)
            .ok()
            .and_then(|o| o.checked_add(base))
            != i64::try_from(z.record_pos).ok()
    {
        return Err(bad(
            "the ZIP64 locator points somewhere else than the ZIP64 end record, once the shift of the archive is allowed for",
        ));
    }

    let mut entries: Vec<Entry> = Vec::with_capacity(total.min(4096));
    let mut seen: HashSet<String> = HashSet::with_capacity(total.min(4096));
    let mut sum: u64 = 0;
    let mut pos = cd_start;
    for _ in 0..total {
        let e = parse_cd_record(src, pos, cd_start, base, limits, &mut sum)?;
        validate_entry_name(slice(src, pos + 46, pos + 46 + e.1).unwrap_or(&[]))
            .map_err(|m| bad(format!("{m}: {:?}", e.0.name)))?;
        if !seen.insert(e.0.name.to_ascii_lowercase()) {
            return Err(bad(format!(
                "duplicate entry name (ignoring ASCII case): {:?}",
                e.0.name
            )));
        }
        pos = e.0.cd.end;
        entries.push(e.0);
    }
    let cd_end = pos;
    if cd_end > after_cd || cd_end - cd_start != size {
        return Err(bad(
            "central directory size does not match the records it holds",
        ));
    }
    let layout = finish(entries, cd_start, cd_end, eocd_pos, base, end.zip64)?;
    // Bytes in front of the first entry are accepted (a self-extractor stub, or an archive that
    // was appended to), but not when they start like a ZIP file. A reader that goes through the
    // file from the front, and does not look for the end record, reads whatever starts at byte 0
    // as the archive, and this reader would be showing something else: the file `two-zips.docx` is
    // one archive followed by another, and the two are read as different documents.
    if layout.prefix_end > 0 && src.starts_with(b"PK") {
        return Err(bad(
            "the bytes in front of the first entry start like a ZIP file, so the file holds more than one archive",
        ));
    }
    Ok(layout)
}

/// Parses one central record, then its local header. Returns the entry and the name length.
fn parse_cd_record(
    src: &[u8],
    pos: usize,
    cd_start: usize,
    base: i64,
    limits: &Limits,
    sum: &mut u64,
) -> Result<(Entry, usize), StetError> {
    if u32_at(src, pos) != Some(SIG_CD) {
        return Err(short("central directory record signature"));
    }
    let f = |o: usize| u16_at(src, pos + o).ok_or_else(|| short("central record"));
    let g = |o: usize| u32_at(src, pos + o).ok_or_else(|| short("central record"));
    let flags = f(8)?;
    let method = f(10)?;
    let crc = g(16)?;
    let csize32 = g(20)?;
    let usize32 = g(24)?;
    let name_len = usize::from(f(28)?);
    let extra_len = usize::from(f(30)?);
    let comment_len = usize::from(f(32)?);
    let disk = f(34)?;
    let off32 = g(42)?;
    let rec_end = pos
        .checked_add(46 + name_len + extra_len + comment_len)
        .filter(|&e| e <= src.len())
        .ok_or_else(|| short("central record length"))?;
    if rec_end > src.len() {
        return Err(short("central record length"));
    }
    let raw_name = slice_len(src, pos + 46, name_len).ok_or_else(|| short("entry name"))?;
    let extra =
        slice_len(src, pos + 46 + name_len, extra_len).ok_or_else(|| short("extra field"))?;

    let (mut usize_, mut csize, mut stored_off) =
        (u64::from(usize32), u64::from(csize32), u64::from(off32));
    let need_z = usize32 == u32::MAX || csize32 == u32::MAX || off32 == u32::MAX || disk == 0xFFFF;
    let mut z_data: &[u8] = &[];
    if need_z {
        let r = zip64_extra(extra)
            .ok_or_else(|| bad("entry uses ZIP64 values but has no ZIP64 extra field"))?;
        z_data = slice(extra, r.start, r.end).unwrap_or(&[]);
    }
    let mut zo = 0usize;
    let take64 = |o: &mut usize| -> Result<u64, StetError> {
        let v = u64_at(z_data, *o).ok_or_else(|| bad("ZIP64 extra field is too short"))?;
        *o += 8;
        Ok(v)
    };
    if usize32 == u32::MAX {
        usize_ = take64(&mut zo)?;
    }
    if csize32 == u32::MAX {
        csize = take64(&mut zo)?;
    }
    if off32 == u32::MAX {
        stored_off = take64(&mut zo)?;
    }
    if disk == 0xFFFF {
        let d = u32_at(z_data, zo).ok_or_else(|| bad("ZIP64 extra field is too short"))?;
        if d != 0 {
            return Err(bad("multi-disk ZIP archives are not supported"));
        }
    } else if disk != 0 {
        return Err(bad("multi-disk ZIP archives are not supported"));
    }

    if usize_ > limits.max_entry_bytes {
        return Err(limit(format!(
            "entry {:?} declares {} bytes uncompressed, over the limit of {}",
            name_to_string(raw_name),
            usize_,
            limits.max_entry_bytes
        )));
    }
    *sum = sum.saturating_add(usize_);
    if *sum > limits.max_total_bytes {
        return Err(limit(format!(
            "entries declare more than {} bytes uncompressed in total",
            limits.max_total_bytes
        )));
    }

    // The local header.
    let stored = i64::try_from(stored_off).map_err(|_| short("entry offset"))?;
    let lfh = usize::try_from(
        stored
            .checked_add(base)
            .ok_or_else(|| short("entry offset"))?,
    )
    .map_err(|_| bad("entry offset is before the start of the file"))?;
    if lfh.checked_add(30).is_none_or(|e| e > cd_start) || u32_at(src, lfh) != Some(SIG_LFH) {
        return Err(bad(format!(
            "local header of {:?} is missing or out of range",
            name_to_string(raw_name)
        )));
    }
    let l16 = |o: usize| u16_at(src, lfh + o).ok_or_else(|| short("local header"));
    let l32 = |o: usize| u32_at(src, lfh + o).ok_or_else(|| short("local header"));
    let lfh_flags = l16(6)?;
    // A reader that takes the method from the local header and one that takes it from the central
    // directory would inflate this entry differently, so an archive that disagrees is refused.
    let lfh_method = l16(8)?;
    if lfh_method != method {
        return Err(bad(format!(
            "local header method ({lfh_method}) differs from the central directory method ({method}) for {:?}",
            name_to_string(raw_name)
        )));
    }
    let lname_len = usize::from(l16(26)?);
    let lextra_len = usize::from(l16(28)?);
    let lname = slice_len(src, lfh + 30, lname_len).ok_or_else(|| short("local header name"))?;
    if lname != raw_name {
        return Err(bad(format!(
            "local header name differs from the central directory name for {:?}",
            name_to_string(raw_name)
        )));
    }
    let lextra = slice_len(src, lfh + 30 + lname_len, lextra_len)
        .ok_or_else(|| short("local extra field"))?;
    let lfh_len = 30 + lname_len + lextra_len;
    let lfh_has_values = l32(14)? != 0 || l32(18)? != 0 || l32(22)? != 0;
    // Only a ZIP64 field that holds both sizes counts: a rewrite writes 16 bytes into it, and a
    // shorter one would run into the field after it.
    let lfh_zip64 = zip64_extra(lextra).is_some_and(|r| r.len() >= 16);
    let data_start = lfh + lfh_len;
    let data_end = usize::try_from(csize)
        .ok()
        .and_then(|c| data_start.checked_add(c))
        .filter(|&e| e <= cd_start)
        .ok_or_else(|| {
            bad(format!(
                "data of {:?} runs past the central directory",
                name_to_string(raw_name)
            ))
        })?;
    let dd = if (flags | lfh_flags) & FLAG_DESCRIPTOR != 0 {
        detect_descriptor(src, data_end, cd_start, crc, csize, usize_, lfh_zip64)
    } else {
        None
    };
    let entry_end = data_end + dd.map_or(0, DataDescriptor::len);
    Ok((
        Entry {
            name: name_to_string(raw_name),
            cd: pos..rec_end,
            lfh,
            lfh_len,
            data_start,
            csize,
            usize_,
            entry_end,
            flags,
            lfh_flags,
            method,
            crc,
            dd,
            lfh_zip64,
            lfh_has_values,
        },
        name_len,
    ))
}

/// Finds the data descriptor after an entry's data by matching it against the central values.
///
/// Four shapes exist (with or without the signature, with 4-byte or 8-byte sizes). More than one
/// can match: an entry with no data has a wide descriptor whose first 16 bytes also read as a
/// narrow one. Then the shape that ends where the next local header or the central directory
/// begins wins, and otherwise the one the local header's ZIP64 field suggests.
fn detect_descriptor(
    src: &[u8],
    at: usize,
    limit: usize,
    crc: u32,
    csize: u64,
    usize_: u64,
    prefer_wide: bool,
) -> Option<DataDescriptor> {
    let forms = [(true, false), (false, false), (true, true), (false, true)];
    let order: Vec<(bool, bool)> = if prefer_wide {
        vec![forms[2], forms[3], forms[0], forms[1]]
    } else {
        forms.to_vec()
    };
    let matches = |signature: bool, wide: bool| -> Option<DataDescriptor> {
        let dd = DataDescriptor { signature, wide };
        if at.checked_add(dd.len()).is_none_or(|e| e > limit) {
            return None;
        }
        let mut o = at;
        if signature {
            if u32_at(src, o) != Some(SIG_DD) {
                return None;
            }
            o += 4;
        }
        let c = u32_at(src, o)?;
        o += 4;
        let (cs, us) = if wide {
            (u64_at(src, o)?, u64_at(src, o + 8)?)
        } else {
            (u64::from(u32_at(src, o)?), u64::from(u32_at(src, o + 4)?))
        };
        (c == crc && cs == csize && us == usize_).then_some(dd)
    };
    let found: Vec<DataDescriptor> = order
        .into_iter()
        .filter_map(|(signature, wide)| matches(signature, wide))
        .collect();
    let next_is_here = |dd: &DataDescriptor| {
        let end = at + dd.len();
        end == limit || u32_at(src, end) == Some(SIG_LFH)
    };
    found
        .iter()
        .copied()
        .find(next_is_here)
        .or_else(|| found.first().copied())
}

fn gap(after: &Entry, until: usize) -> StetError {
    bad(format!(
        "{:?} is followed by {} bytes that belong to no entry",
        after.name,
        until.saturating_sub(after.entry_end)
    ))
}

fn finish(
    entries: Vec<Entry>,
    cd_start: usize,
    cd_end: usize,
    eocd_pos: usize,
    base: i64,
    zip64: Option<Zip64>,
) -> Result<Layout, StetError> {
    let mut local_order: Vec<usize> = (0..entries.len()).collect();
    local_order.sort_by_key(|&i| entries.get(i).map_or(usize::MAX, |e| e.lfh));
    // Entries lie end to end, from the first one to the central directory. A reader that goes
    // through the file from the front, as `bsdtar` does from a pipe, and one that goes by the
    // central directory would see different entries in bytes that belong to none (a hidden
    // `junk.xml`, or a second `word/document.xml`), and entries that overlap share bytes. Bytes in
    // front of the first entry are a prefix, such as a self-extractor, and are checked by the
    // caller.
    let mut previous: Option<&Entry> = None;
    for &i in &local_order {
        let Some(e) = entries.get(i) else { continue };
        if let Some(p) = previous {
            if p.entry_end > e.lfh {
                return Err(bad(format!(
                    "entry {:?} overlaps the entry before it",
                    e.name
                )));
            }
            if p.entry_end < e.lfh {
                return Err(gap(p, e.lfh));
            }
        }
        previous = Some(e);
    }
    if let Some(p) = previous
        && p.entry_end != cd_start
    {
        return Err(gap(p, cd_start));
    }
    let prefix_end = local_order
        .first()
        .and_then(|&i| entries.get(i))
        .map_or(cd_start, |e| e.lfh);
    Ok(Layout {
        entries,
        local_order,
        prefix_end,
        cd_end,
        eocd_pos,
        base,
        zip64,
    })
}
