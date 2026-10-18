//! A second reader of ZIP archives, for tests. It is written to the rules Python's `zipfile`
//! follows, which `unzip` and `bsdtar` agree with on everything the tests build, and shares no code
//! with this crate's own reader. So a test that writes an archive and reads it back with this does
//! not check the writer with the reader that was written to match it.
//!
//! The rules that matter, and that a reader of our own once missed:
//! - The end record is the last `PK\5\6` in the final 64 KiB, whatever its comment length says.
//! - If a ZIP64 locator sits right before the end record, the record right before the locator
//!   (56 bytes, no extensible data) supplies the entry count, directory size and directory offset,
//!   and the end record's own fields are ignored, sentinel or not.
//! - Offsets are relative to where the archive starts, which is found from where the end record
//!   is and how large the directory is, so a stale value anywhere puts the directory in the wrong
//!   place.

use miniz_oxide::inflate::decompress_to_vec;

/// An entry as this reader found it.
pub(super) struct Found {
    pub name: String,
    pub data: Vec<u8>,
}

fn le(bytes: &[u8], at: usize, n: usize) -> Result<u64, String> {
    let s = at
        .checked_add(n)
        .and_then(|end| bytes.get(at..end))
        .ok_or_else(|| format!("read of {n} bytes at {at} is out of range"))?;
    Ok(s.iter().rev().fold(0u64, |v, &b| (v << 8) | u64::from(b)))
}

/// The table of CRC-32 values for one byte, built bit by bit so that nothing here is borrowed from
/// the crate's own checksum.
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut crc = n as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[n] = crc;
        n += 1;
    }
    table
};

pub(super) fn crc32(data: &[u8]) -> u32 {
    !data.iter().fold(!0u32, |crc, &b| {
        CRC_TABLE[usize::from((crc as u8) ^ b)] ^ (crc >> 8)
    })
}

/// An entry without its data: what is needed to tell whether two readers saw the same one.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Seen {
    pub name: String,
    pub len: usize,
    pub crc: u32,
}

/// Reads every entry and keeps only its name, size and CRC-32. The error says what a strict reader
/// would refuse.
pub(super) fn summary(bytes: &[u8]) -> Result<Vec<Seen>, String> {
    let mut out = Vec::new();
    walk(bytes, |name, data, crc| {
        out.push(Seen {
            name,
            len: data.len(),
            crc,
        });
    })?;
    Ok(out)
}

/// Reads every entry. The error says what a strict reader would refuse.
pub(super) fn read(bytes: &[u8]) -> Result<Vec<Found>, String> {
    let mut out = Vec::new();
    walk(bytes, |name, data, _| out.push(Found { name, data }))?;
    Ok(out)
}

/// An entry name as text, by the convention this crate states for names: valid UTF-8 is the name,
/// and any other bytes (a CP437 name written without the UTF-8 flag) map one to one to code
/// points, so that distinct names stay distinct. This is not a rule of reading ZIP files: it only
/// makes the names of two readers comparable.
fn name_text(raw: &[u8]) -> String {
    match std::str::from_utf8(raw) {
        Ok(s) => s.to_owned(),
        Err(_) => raw.iter().map(|&b| char::from(b)).collect(),
    }
}

/// Goes through the entries in the order of the central directory, checking each one.
fn walk(bytes: &[u8], mut each: impl FnMut(String, Vec<u8>, u32)) -> Result<(), String> {
    let window = bytes.len().saturating_sub(65_536 + 22);
    let eocd = bytes[window..]
        .windows(4)
        .rposition(|w| w == b"PK\x05\x06")
        .map(|i| window + i)
        .ok_or("no end of central directory record")?;
    let mut count = le(bytes, eocd + 10, 2)?;
    let mut size = le(bytes, eocd + 12, 4)?;
    let mut offset = le(bytes, eocd + 16, 4)?;
    let mut zip64 = false;
    if eocd >= 20 && bytes.get(eocd - 20..eocd - 16) == Some(b"PK\x06\x07") {
        let record = eocd
            .checked_sub(20 + 56)
            .ok_or("no room for a ZIP64 record")?;
        if bytes.get(record..record + 4) == Some(b"PK\x06\x06") {
            count = le(bytes, record + 32, 8)?;
            size = le(bytes, record + 40, 8)?;
            offset = le(bytes, record + 48, 8)?;
            zip64 = true;
        }
    }
    let to_i64 = |v: u64| i64::try_from(v).map_err(|_| "a value does not fit".to_string());
    let concat = eocd as i64 - to_i64(size)? - to_i64(offset)? - if zip64 { 76 } else { 0 };
    let start = to_i64(offset)?
        .checked_add(concat)
        .filter(|&s| s >= 0)
        .ok_or("bad offset for the central directory")?;
    let mut at = start as usize;
    for n in 0..count {
        if at.checked_add(4).and_then(|e| bytes.get(at..e)) != Some(b"PK\x01\x02") {
            return Err(format!("bad magic number for central directory record {n}"));
        }
        let method = le(bytes, at + 10, 2)?;
        let crc = le(bytes, at + 16, 4)?;
        let (mut csize, mut usize_, mut header) = (
            le(bytes, at + 20, 4)?,
            le(bytes, at + 24, 4)?,
            le(bytes, at + 42, 4)?,
        );
        let (nlen, elen, clen) = (
            le(bytes, at + 28, 2)? as usize,
            le(bytes, at + 30, 2)? as usize,
            le(bytes, at + 32, 2)? as usize,
        );
        let name = bytes
            .get(at + 46..at + 46 + nlen)
            .ok_or("name out of range")?;
        let extra = bytes
            .get(at + 46 + nlen..at + 46 + nlen + elen)
            .ok_or("extra field out of range")?;
        let mut x = 0;
        while x + 4 <= extra.len() {
            let (id, len) = (le(extra, x, 2)?, le(extra, x + 2, 2)? as usize);
            let body = extra
                .get(x + 4..x + 4 + len)
                .ok_or("extra record out of range")?;
            if id == 1 {
                let mut o = 0;
                for field in [&mut usize_, &mut csize, &mut header] {
                    if *field == 0xFFFF_FFFF {
                        *field = le(body, o, 8)?;
                        o += 8;
                    }
                }
            }
            x += 4 + len;
        }
        let lfh = to_i64(header)?
            .checked_add(concat)
            .and_then(|v| usize::try_from(v).ok())
            .ok_or("bad local header offset")?;
        if lfh.checked_add(4).and_then(|e| bytes.get(lfh..e)) != Some(b"PK\x03\x04") {
            return Err(format!(
                "bad local header for {}",
                String::from_utf8_lossy(name)
            ));
        }
        let (lnlen, lelen) = (
            le(bytes, lfh + 26, 2)? as usize,
            le(bytes, lfh + 28, 2)? as usize,
        );
        if bytes.get(lfh + 30..lfh + 30 + lnlen) != Some(name) {
            return Err("local header name differs from the directory".into());
        }
        let data_at = lfh + 30 + lnlen + lelen;
        let raw = usize::try_from(csize)
            .ok()
            .and_then(|c| data_at.checked_add(c))
            .and_then(|end| bytes.get(data_at..end))
            .ok_or("data out of range")?;
        let data = match method {
            0 => raw.to_vec(),
            8 => decompress_to_vec(raw).map_err(|e| format!("inflate: {e:?}"))?,
            m => return Err(format!("method {m}")),
        };
        let actual = crc32(&data);
        if data.len() as u64 != usize_ || u64::from(actual) != crc {
            return Err(format!(
                "size or CRC of {} is wrong",
                String::from_utf8_lossy(name)
            ));
        }
        each(name_text(name), data, actual);
        at += 46 + nlen + elen + clen;
    }
    if Some(at)
        != usize::try_from(size)
            .ok()
            .and_then(|z| (start as usize).checked_add(z))
    {
        return Err("the directory is not the size the end record says".into());
    }
    Ok(())
}
