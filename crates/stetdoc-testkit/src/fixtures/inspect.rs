//! A small, independent ZIP reader, for the tests of the builder, so that a fixture is checked by
//! code that shares nothing with the writer or with stetdoc's package layer, and for the
//! benchmarks, which unpack a corpus document to scale it up.

use flate2::read::DeflateDecoder;
use std::io::Read;

/// One central directory record.
#[derive(Clone, Debug)]
pub struct Rec {
    pub name: Vec<u8>,
    pub method: u16,
    pub flags: u16,
    pub crc: u32,
    pub csize: u64,
    pub usize: u64,
    pub offset: u64,
    pub extra: Vec<u8>,
    pub comment: Vec<u8>,
    pub version_made: u16,
    pub time: u16,
    pub date: u16,
}

/// The parsed archive.
#[derive(Debug)]
pub struct Archive {
    pub records: Vec<Rec>,
    pub comment: Vec<u8>,
    pub zip64: bool,
}

fn u16_at(b: &[u8], at: usize) -> Result<u16, String> {
    b.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| format!("truncated at {at}"))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, String> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| format!("truncated at {at}"))
}

fn u64_at(b: &[u8], at: usize) -> Result<u64, String> {
    b.get(at..at + 8)
        .map(|s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
        .ok_or_else(|| format!("truncated at {at}"))
}

/// Finds the end record whose comment length fits the file, nearest the end first.
fn find_eocd(b: &[u8]) -> Result<usize, String> {
    if b.len() < 22 {
        return Err("shorter than an end record".into());
    }
    for at in (0..=b.len() - 22).rev() {
        if &b[at..at + 4] == b"PK\x05\x06" {
            let clen = usize::from(u16_at(b, at + 20)?);
            if at + 22 + clen <= b.len() {
                return Ok(at);
            }
        }
    }
    Err("no end of central directory record".into())
}

/// Reads the central directory.
pub fn read(b: &[u8]) -> Result<Archive, String> {
    let eocd = find_eocd(b)?;
    let clen = usize::from(u16_at(b, eocd + 20)?);
    let mut count = u64::from(u16_at(b, eocd + 10)?);
    let mut cd_size = u64::from(u32_at(b, eocd + 12)?);
    let mut cd_off = u64::from(u32_at(b, eocd + 16)?);
    let mut zip64 = false;
    if (count == 0xFFFF || cd_off == 0xFFFF_FFFF)
        && eocd >= 20
        && &b[eocd - 20..eocd - 16] == b"PK\x06\x07"
    {
        let e64 = u64_at(b, eocd - 12)? as usize;
        if b.get(e64..e64 + 4) != Some(b"PK\x06\x06") {
            return Err("zip64 locator points at no record".into());
        }
        count = u64_at(b, e64 + 32)?;
        cd_size = u64_at(b, e64 + 40)?;
        cd_off = u64_at(b, e64 + 48)?;
        zip64 = true;
    }
    let mut records = Vec::new();
    let mut p = usize::try_from(cd_off).map_err(|_| "offset too large")?;
    // Bytes before the archive shift every offset when they are counted from the first local
    // header, so the central directory is not where the end record says. Look for it after the
    // bytes that the sizes leave unexplained, as zip readers do.
    let mut shift = 0usize;
    if b.get(p..p + 4) != Some(b"PK\x01\x02") && !zip64 {
        let end = usize::try_from(cd_size).map_err(|_| "size too large")?;
        if let Some(s) = eocd.checked_sub(end).and_then(|start| start.checked_sub(p))
            && b.get(p + s..p + s + 4) == Some(b"PK\x01\x02")
        {
            shift = s;
            p += s;
        }
    }
    for _ in 0..count {
        if b.get(p..p + 4) != Some(b"PK\x01\x02") {
            return Err(format!("central directory record signature missing at {p}"));
        }
        let nl = usize::from(u16_at(b, p + 28)?);
        let el = usize::from(u16_at(b, p + 30)?);
        let cl = usize::from(u16_at(b, p + 32)?);
        let mut rec = Rec {
            name: b.get(p + 46..p + 46 + nl).ok_or("truncated name")?.to_vec(),
            method: u16_at(b, p + 10)?,
            flags: u16_at(b, p + 8)?,
            crc: u32_at(b, p + 16)?,
            csize: u64::from(u32_at(b, p + 20)?),
            usize: u64::from(u32_at(b, p + 24)?),
            offset: u64::from(u32_at(b, p + 42)?),
            extra: b
                .get(p + 46 + nl..p + 46 + nl + el)
                .ok_or("truncated extra")?
                .to_vec(),
            comment: b
                .get(p + 46 + nl + el..p + 46 + nl + el + cl)
                .ok_or("truncated comment")?
                .to_vec(),
            version_made: u16_at(b, p + 4)?,
            time: u16_at(b, p + 12)?,
            date: u16_at(b, p + 14)?,
        };
        apply_zip64(&mut rec);
        rec.offset += shift as u64;
        records.push(rec);
        p += 46 + nl + el + cl;
    }
    let _ = cd_size;
    Ok(Archive {
        records,
        comment: b[eocd + 22..eocd + 22 + clen].to_vec(),
        zip64,
    })
}

/// The extra field ids of an extra block, in order.
pub fn extra_ids(extra: &[u8]) -> Vec<u16> {
    let mut ids = Vec::new();
    let mut p = 0;
    while p + 4 <= extra.len() {
        ids.push(u16::from_le_bytes([extra[p], extra[p + 1]]));
        p += 4 + usize::from(u16::from_le_bytes([extra[p + 2], extra[p + 3]]));
    }
    ids
}

fn apply_zip64(rec: &mut Rec) {
    let mut p = 0;
    while p + 4 <= rec.extra.len() {
        let id = u16::from_le_bytes([rec.extra[p], rec.extra[p + 1]]);
        let len = usize::from(u16::from_le_bytes([rec.extra[p + 2], rec.extra[p + 3]]));
        if id == 1 {
            let d = &rec.extra[p + 4..(p + 4 + len).min(rec.extra.len())];
            let mut q = 0;
            for field in [&mut rec.usize, &mut rec.csize, &mut rec.offset] {
                if *field == 0xFFFF_FFFF && d.len() >= q + 8 {
                    *field = u64::from_le_bytes([
                        d[q],
                        d[q + 1],
                        d[q + 2],
                        d[q + 3],
                        d[q + 4],
                        d[q + 5],
                        d[q + 6],
                        d[q + 7],
                    ]);
                    q += 8;
                }
            }
        }
        p += 4 + len;
    }
}

/// The compressed bytes of an entry, found through its local header.
pub fn compressed<'a>(b: &'a [u8], rec: &Rec) -> Result<&'a [u8], String> {
    let at = rec.offset as usize;
    if b.get(at..at + 4) != Some(b"PK\x03\x04") {
        return Err(format!("no local header at {at}"));
    }
    let nl = usize::from(u16_at(b, at + 26)?);
    let el = usize::from(u16_at(b, at + 28)?);
    let start = at + 30 + nl + el;
    b.get(start..start + rec.csize as usize)
        .ok_or_else(|| "data runs past the end".into())
}

/// The decompressed bytes of an entry, with its CRC checked.
pub fn extract(b: &[u8], rec: &Rec) -> Result<Vec<u8>, String> {
    let raw = compressed(b, rec)?;
    let out = match rec.method {
        0 => raw.to_vec(),
        8 => {
            let mut v = Vec::new();
            DeflateDecoder::new(raw)
                .read_to_end(&mut v)
                .map_err(|e| e.to_string())?;
            v
        }
        m => return Err(format!("method {m}")),
    };
    let mut crc = flate2::Crc::new();
    crc.update(&out);
    if crc.sum() != rec.crc {
        return Err("crc mismatch".into());
    }
    Ok(out)
}
