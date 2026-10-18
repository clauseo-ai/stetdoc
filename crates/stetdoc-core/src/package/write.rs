//! Rebuilding the archive: untouched regions copied raw, changed entries rewritten, central
//! directory patched. See `docs/design/package-and-xml.md` sections 1.2 and 1.3.

use super::codec::{crc32, deflate};
use super::endrec::zip64_extra;
use super::errors::{limit, malformed, unsupported};
use super::layout::{
    DataDescriptor, ENCRYPTION_FLAGS, Entry, FLAG_DESCRIPTOR, Layout, SIG_CD, SIG_DD, SIG_LFH,
};
use super::le::{push_u16, push_u32, push_u64, put_u16, put_u32, put_u64, slice, u16_at, u32_at};
use super::{Package, Slot, opc};
use crate::api::error::StetError;

/// Compression-level flag bits (1 and 2) are cleared on recompressed entries.
const LEVEL_BITS: u16 = 0x0006;

/// Values copied from the main document part's entry into new entries.
struct Template {
    version_made: u16,
    version_needed: u16,
    flags: u16,
    lfh_time: u16,
    lfh_date: u16,
    cd_time: u16,
    cd_date: u16,
    internal_attr: u16,
    external_attr: u32,
}

impl Template {
    fn find(pkg: &Package) -> Template {
        let lay = &pkg.layout;
        let main = main_part_index(pkg);
        let mut t = Template {
            version_made: 20,
            version_needed: 20,
            flags: 0,
            lfh_time: 0,
            lfh_date: 0x0021,
            cd_time: 0,
            cd_date: 0x0021,
            internal_attr: 0,
            external_attr: 0,
        };
        if let Some(e) = main.and_then(|i| lay.entries.get(i)) {
            let s: &[u8] = &pkg.src;
            t.version_made = u16_at(s, e.cd.start + 4).unwrap_or(t.version_made);
            t.version_needed = u16_at(s, e.lfh + 4).unwrap_or(t.version_needed);
            t.flags = e.lfh_flags & !(ENCRYPTION_FLAGS | FLAG_DESCRIPTOR | LEVEL_BITS | 0x0800);
            t.lfh_time = u16_at(s, e.lfh + 10).unwrap_or(t.lfh_time);
            t.lfh_date = u16_at(s, e.lfh + 12).unwrap_or(t.lfh_date);
            t.cd_time = u16_at(s, e.cd.start + 12).unwrap_or(t.cd_time);
            t.cd_date = u16_at(s, e.cd.start + 14).unwrap_or(t.cd_date);
            t.internal_attr = u16_at(s, e.cd.start + 36).unwrap_or(0);
            t.external_attr = u32_at(s, e.cd.start + 38).unwrap_or(0);
        }
        t
    }
}

/// The index of the main document part: the target of the `officeDocument` relationship,
/// else `word/document.xml`.
pub(super) fn main_part_index(pkg: &Package) -> Option<usize> {
    if let Ok(Some(rels)) = pkg.part("_rels/.rels")
        && let Some(target) = opc::office_document_target(&rels, &pkg.limits)
        && let Some(i) = pkg.part_index(&target)
    {
        return Some(i);
    }
    pkg.part_index("word/document.xml")
}

struct Encoded {
    crc: u32,
    method: u16,
    data: Vec<u8>,
    usize_: u64,
}

fn encode(method: u16, bytes: &[u8]) -> Result<Encoded, StetError> {
    let data = match method {
        0 => bytes.to_vec(),
        8 => deflate(bytes),
        m => {
            return Err(unsupported(format!(
                "cannot rewrite an entry stored with method {m}"
            )));
        }
    };
    Ok(Encoded {
        crc: crc32(bytes),
        method,
        data,
        usize_: bytes.len() as u64,
    })
}

fn too_big(what: &str) -> StetError {
    limit(format!(
        "{what} does not fit without ZIP64, which this package did not use"
    ))
}

fn to_u32(v: u64, what: &str) -> Result<u32, StetError> {
    u32::try_from(v).map_err(|_| too_big(what))
}

/// The value for a 16-bit field of the end record. A sentinel stays a sentinel, because the ZIP64
/// record holds the value. Otherwise a value that fits is written, and one that does not fit
/// (65,535 is the sentinel itself) becomes a sentinel if there is a ZIP64 record to hold it.
fn end_u16(old: u16, new: u64, zip64: bool, what: &str) -> Result<u16, StetError> {
    if old == u16::MAX {
        return Ok(old);
    }
    match u16::try_from(new) {
        Ok(v) if v != u16::MAX => Ok(v),
        _ if zip64 => Ok(u16::MAX),
        _ => Err(too_big(what)),
    }
}

/// The same for a 32-bit field.
fn end_u32(old: u32, new: u64, zip64: bool, what: &str) -> Result<u32, StetError> {
    if old == u32::MAX {
        return Ok(old);
    }
    match u32::try_from(new) {
        Ok(v) if v != u32::MAX => Ok(v),
        _ if zip64 => Ok(u32::MAX),
        _ => Err(too_big(what)),
    }
}

fn fail(what: &str) -> StetError {
    malformed(format!("internal ZIP record patch failed: {what}"))
}

/// Writes a replaced entry: new local header, new data, new descriptor.
fn write_replaced(
    out: &mut Vec<u8>,
    src: &[u8],
    e: &Entry,
    enc: &Encoded,
) -> Result<(), StetError> {
    let hdr = slice(src, e.lfh, e.lfh + e.lfh_len).ok_or_else(|| fail("local header range"))?;
    let mut h = hdr.to_vec();
    let flags = e.lfh_flags & !LEVEL_BITS;
    put_u16(&mut h, 6, flags).ok_or_else(|| fail("flags"))?;
    let csize = enc.data.len() as u64;
    let descriptor = e.dd.is_some() || e.lfh_flags & FLAG_DESCRIPTOR != 0;
    if !descriptor || e.lfh_has_values {
        put_u32(&mut h, 14, enc.crc).ok_or_else(|| fail("crc"))?;
        if e.lfh_zip64 {
            put_u32(&mut h, 18, u32::MAX).ok_or_else(|| fail("size"))?;
            put_u32(&mut h, 22, u32::MAX).ok_or_else(|| fail("size"))?;
            let name_len = usize::from(u16_at(&h, 26).ok_or_else(|| fail("name length"))?);
            let extra = h.get(30 + name_len..).ok_or_else(|| fail("extra"))?;
            let r = zip64_extra(extra).ok_or_else(|| fail("zip64 extra"))?;
            let at = 30 + name_len + r.start;
            put_u64(&mut h, at, enc.usize_).ok_or_else(|| fail("zip64 size"))?;
            put_u64(&mut h, at + 8, csize).ok_or_else(|| fail("zip64 size"))?;
        } else {
            put_u32(&mut h, 18, to_u32(csize, "compressed size")?).ok_or_else(|| fail("size"))?;
            put_u32(&mut h, 22, to_u32(enc.usize_, "uncompressed size")?)
                .ok_or_else(|| fail("size"))?;
        }
    }
    out.extend_from_slice(&h);
    out.extend_from_slice(&enc.data);
    if descriptor {
        let dd = e.dd.unwrap_or(DataDescriptor {
            signature: true,
            wide: e.lfh_zip64,
        });
        if dd.signature {
            push_u32(out, SIG_DD);
        }
        push_u32(out, enc.crc);
        if dd.wide {
            push_u64(out, csize);
            push_u64(out, enc.usize_);
        } else {
            push_u32(out, to_u32(csize, "compressed size")?);
            push_u32(out, to_u32(enc.usize_, "uncompressed size")?);
        }
    }
    Ok(())
}

/// New values for a central record.
struct CdUpdate {
    /// CRC and sizes, for replaced entries.
    content: Option<(u32, u64, u64)>,
    offset: u64,
}

/// Patches a copied central record in place. ZIP64 extra fields are patched only for the
/// fields whose 32-bit value is a sentinel.
fn patch_cd(rec: &mut [u8], up: &CdUpdate) -> Result<(), StetError> {
    let name_len = usize::from(u16_at(rec, 28).ok_or_else(|| fail("name length"))?);
    let extra_len = usize::from(u16_at(rec, 30).ok_or_else(|| fail("extra length"))?);
    let extra_at = 46 + name_len;
    let z = rec
        .get(extra_at..extra_at + extra_len)
        .and_then(zip64_extra)
        .map(|r| (extra_at + r.start, r.len()));
    let sent = |o: usize| u32_at(rec, o) == Some(u32::MAX);
    // slot order inside the ZIP64 extra: uncompressed size, compressed size, offset
    let (s_us, s_cs, s_off) = (sent(24), sent(20), sent(42));
    let slot = |which: usize| -> Option<usize> {
        let before = [s_us, s_cs, s_off]
            .iter()
            .take(which)
            .filter(|&&b| b)
            .count();
        Some(before)
    };
    let set = |rec: &mut [u8], fixed: usize, sentinel: bool, which: usize, v: u64, what: &str| {
        if sentinel {
            let (zs, zl) = z.ok_or_else(|| fail("zip64 extra missing"))?;
            let s = slot(which).ok_or_else(|| fail("slot"))?;
            if (s + 1) * 8 > zl {
                return Err(fail("zip64 extra too short"));
            }
            put_u64(rec, zs + s * 8, v).ok_or_else(|| fail("zip64 value"))
        } else {
            put_u32(rec, fixed, to_u32(v, what)?).ok_or_else(|| fail("value"))
        }
    };
    if let Some((crc, csize, usize_)) = up.content {
        let flags = u16_at(rec, 8).ok_or_else(|| fail("flags"))? & !LEVEL_BITS;
        put_u16(rec, 8, flags).ok_or_else(|| fail("flags"))?;
        put_u32(rec, 16, crc).ok_or_else(|| fail("crc"))?;
        set(rec, 24, s_us, 0, usize_, "uncompressed size")?;
        set(rec, 20, s_cs, 1, csize, "compressed size")?;
    }
    set(rec, 42, s_off, 2, up.offset, "entry offset")
}

fn stored_offset(actual: usize, base: i64) -> Result<u64, StetError> {
    let v = (actual as i64)
        .checked_sub(base)
        .ok_or_else(|| fail("offset"))?;
    u64::try_from(v).map_err(|_| fail("negative offset"))
}

fn ascii(name: &str) -> bool {
    name.is_ascii()
}

pub(super) fn rebuild(pkg: &Package) -> Result<Vec<u8>, StetError> {
    let src: &[u8] = &pkg.src;
    let lay: &Layout = &pkg.layout;
    let tmpl = Template::find(pkg);
    let mut out: Vec<u8> = Vec::with_capacity(src.len() + 1024);
    out.extend_from_slice(src.get(..lay.prefix_end).ok_or_else(|| fail("prefix"))?);

    let mut encoded: Vec<Option<Encoded>> = Vec::with_capacity(lay.entries.len());
    for (i, e) in lay.entries.iter().enumerate() {
        match pkg.slots.get(i) {
            Some(Slot::Replaced(bytes)) => encoded.push(Some(encode(e.method, bytes)?)),
            _ => encoded.push(None),
        }
    }

    let mut new_off = vec![0usize; lay.entries.len()];
    for &i in &lay.local_order {
        let e = lay.entries.get(i).ok_or_else(|| fail("entry index"))?;
        if let Some(slot) = new_off.get_mut(i) {
            *slot = out.len();
        }
        match encoded.get(i).and_then(Option::as_ref) {
            Some(enc) => write_replaced(&mut out, src, e, enc)?,
            None => {
                out.extend_from_slice(src.get(e.lfh..e.entry_end).ok_or_else(|| fail("entry"))?)
            }
        }
    }

    // New entries, appended in order.
    let mut added_info: Vec<(usize, Encoded)> = Vec::with_capacity(pkg.added.len());
    for a in &pkg.added {
        let enc = encode(8, &a.content)?;
        let at = out.len();
        let mut flags = tmpl.flags;
        if !ascii(&a.name) {
            flags |= 0x0800;
        }
        let name = a.name.as_bytes();
        push_u32(&mut out, SIG_LFH);
        push_u16(&mut out, tmpl.version_needed);
        push_u16(&mut out, flags);
        push_u16(&mut out, enc.method);
        push_u16(&mut out, tmpl.lfh_time);
        push_u16(&mut out, tmpl.lfh_date);
        push_u32(&mut out, enc.crc);
        push_u32(&mut out, to_u32(enc.data.len() as u64, "compressed size")?);
        push_u32(&mut out, to_u32(enc.usize_, "uncompressed size")?);
        push_u16(
            &mut out,
            u16::try_from(name.len()).map_err(|_| fail("name length"))?,
        );
        push_u16(&mut out, 0);
        out.extend_from_slice(name);
        out.extend_from_slice(&enc.data);
        added_info.push((at, enc));
    }

    // Central directory.
    let cd_start = out.len();
    for (i, e) in lay.entries.iter().enumerate() {
        let mut rec = src
            .get(e.cd.clone())
            .ok_or_else(|| fail("central record"))?
            .to_vec();
        let moved = new_off.get(i).copied().unwrap_or(e.lfh) != e.lfh;
        let content = encoded
            .get(i)
            .and_then(Option::as_ref)
            .map(|enc| (enc.crc, enc.data.len() as u64, enc.usize_));
        if moved || content.is_some() {
            let off = stored_offset(new_off.get(i).copied().unwrap_or(e.lfh), lay.base)?;
            patch_cd(
                &mut rec,
                &CdUpdate {
                    content,
                    offset: off,
                },
            )?;
        }
        out.extend_from_slice(&rec);
    }
    for (a, (at, enc)) in pkg.added.iter().zip(&added_info) {
        let name = a.name.as_bytes();
        let mut flags = tmpl.flags;
        if !ascii(&a.name) {
            flags |= 0x0800;
        }
        push_u32(&mut out, SIG_CD);
        push_u16(&mut out, tmpl.version_made);
        push_u16(&mut out, tmpl.version_needed);
        push_u16(&mut out, flags);
        push_u16(&mut out, enc.method);
        push_u16(&mut out, tmpl.cd_time);
        push_u16(&mut out, tmpl.cd_date);
        push_u32(&mut out, enc.crc);
        push_u32(&mut out, to_u32(enc.data.len() as u64, "compressed size")?);
        push_u32(&mut out, to_u32(enc.usize_, "uncompressed size")?);
        push_u16(
            &mut out,
            u16::try_from(name.len()).map_err(|_| fail("name length"))?,
        );
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);
        push_u16(&mut out, tmpl.internal_attr);
        push_u32(&mut out, tmpl.external_attr);
        let off = stored_offset(*at, lay.base)?;
        push_u32(&mut out, to_u32(off, "entry offset")?);
        out.extend_from_slice(name);
    }
    let cd_end = out.len();

    // Bytes between the central directory and the end record (ZIP64 records, signatures).
    let mut pre = src
        .get(lay.cd_end..lay.eocd_pos)
        .ok_or_else(|| fail("pre-eocd"))?
        .to_vec();
    let count = (lay.entries.len() + pkg.added.len()) as u64;
    let cd_size = (cd_end - cd_start) as u64;
    let cd_off = stored_offset(cd_start, lay.base)?;
    let eocd_end = lay.eocd_pos
        + 22
        + usize::from(u16_at(src, lay.eocd_pos + 20).ok_or_else(|| fail("eocd"))?);
    let mut eocd = src
        .get(lay.eocd_pos..eocd_end)
        .ok_or_else(|| fail("eocd"))?
        .to_vec();
    let has_zip64 = lay.zip64.is_some();
    let old16 = |o: usize| u16_at(&eocd, o).ok_or_else(|| fail("eocd field"));
    let old32 = |o: usize| u32_at(&eocd, o).ok_or_else(|| fail("eocd field"));
    let (this_disk, total) = (
        end_u16(old16(8)?, count, has_zip64, "entry count")?,
        end_u16(old16(10)?, count, has_zip64, "entry count")?,
    );
    let size = end_u32(old32(12)?, cd_size, has_zip64, "central directory size")?;
    let off = end_u32(old32(16)?, cd_off, has_zip64, "central directory offset")?;
    put_u16(&mut eocd, 8, this_disk).ok_or_else(|| fail("count"))?;
    put_u16(&mut eocd, 10, total).ok_or_else(|| fail("count"))?;
    put_u32(&mut eocd, 12, size).ok_or_else(|| fail("cd size"))?;
    put_u32(&mut eocd, 16, off).ok_or_else(|| fail("cd offset"))?;
    if let Some(z) = lay.zip64 {
        // A reader that finds a ZIP64 locator trusts the record over the end record, whether or not
        // the end record's fields are sentinels, so the record always gets the new values.
        let r = z.record_pos - lay.cd_end;
        put_u64(&mut pre, r + 24, count).ok_or_else(|| fail("zip64 count"))?;
        put_u64(&mut pre, r + 32, count).ok_or_else(|| fail("zip64 count"))?;
        put_u64(&mut pre, r + 40, cd_size).ok_or_else(|| fail("zip64 size"))?;
        put_u64(&mut pre, r + 48, cd_off).ok_or_else(|| fail("zip64 offset"))?;
        // The locator points at the ZIP64 record, which moved with the central directory.
        let new_record = cd_end + r;
        let l = z.locator_pos - lay.cd_end;
        put_u64(&mut pre, l + 8, stored_offset(new_record, lay.base)?)
            .ok_or_else(|| fail("locator"))?;
    }
    out.extend_from_slice(&pre);
    out.extend_from_slice(&eocd);
    out.extend_from_slice(src.get(eocd_end..).ok_or_else(|| fail("tail"))?);
    Ok(out)
}
