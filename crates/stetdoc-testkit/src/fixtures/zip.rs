//! A hand-rolled ZIP writer for fixtures. Every header field, gap and trailer can be set,
//! including the ones a correct writer would never produce.

use flate2::{Compression, Crc, write::DeflateEncoder};
use std::collections::BTreeMap;
use std::io::Write;

mod entry;
pub use entry::{DOS_1980, Entry};

/// How an entry's CRC and sizes are recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Descriptor {
    /// Sizes and CRC are in the local header.
    None,
    /// General-purpose bit 3, zeroed local header, descriptor with the PK 07 08 signature.
    Signed,
    /// Bit 3, descriptor without the signature.
    Unsigned,
    /// Bit 3, descriptor with the signature and 8-byte sizes.
    Zip64Signed,
}

/// ZIP64 behaviour of the whole archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zip64 {
    /// No ZIP64 records.
    Off,
    /// Sentinel fields, ZIP64 extras in local and central records, EOCD64 and locator.
    Full,
    /// Java style: the ZIP64 extra field in local headers only.
    LocalExtraOnly,
    /// Only the end records: sentinel fields in the end record, an EOCD64 record and its locator,
    /// which is how an archive with more than 65,535 small entries is written.
    EndRecordsOnly,
}

fn deflate(data: &[u8], level: u32) -> Vec<u8> {
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::new(level));
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = Crc::new();
    c.update(data);
    c.sum()
}

struct Written {
    name: Vec<u8>,
    flags: u16,
    method: u16,
    time: (u16, u16),
    crc: u32,
    csize: u64,
    usize: u64,
    offset: u64,
    cd_offset: Option<u32>,
    version_made: (u8, u8),
    version_needed: u16,
    extra_central: Vec<u8>,
    comment: Vec<u8>,
    external: u32,
    internal: u16,
    zip64: bool,
}

/// An archive under construction.
#[derive(Clone, Debug)]
pub struct Zip {
    entries: Vec<Entry>,
    comment: Vec<u8>,
    prefix: Vec<u8>,
    trailing: Vec<u8>,
    zip64: Zip64,
    relative: bool,
    cd_order: Option<Vec<usize>>,
    gaps: BTreeMap<usize, Vec<u8>>,
    cd_gap: Vec<u8>,
}

impl Zip {
    /// An archive of these entries in this order.
    pub fn new(entries: Vec<Entry>) -> Zip {
        Zip {
            entries,
            comment: Vec::new(),
            prefix: Vec::new(),
            trailing: Vec::new(),
            zip64: Zip64::Off,
            relative: false,
            cd_order: None,
            gaps: BTreeMap::new(),
            cd_gap: Vec::new(),
        }
    }

    /// An archive comment.
    #[must_use]
    pub fn comment(mut self, comment: &[u8]) -> Zip {
        self.comment = comment.to_vec();
        self
    }

    /// Bytes before the first local header. Offsets stay relative to the start of the file.
    #[must_use]
    pub fn prefix(mut self, prefix: &[u8]) -> Zip {
        self.prefix = prefix.to_vec();
        self
    }

    /// Offsets count from the first local header, not from the start of the file, so a prefix
    /// shifts every offset (as when an archive is appended to a stub and then repaired).
    #[must_use]
    pub fn relative_offsets(mut self) -> Zip {
        self.relative = true;
        self
    }

    /// Bytes after the end record.
    #[must_use]
    pub fn trailing(mut self, trailing: &[u8]) -> Zip {
        self.trailing = trailing.to_vec();
        self
    }

    /// ZIP64 mode of the archive.
    #[must_use]
    pub fn zip64(mut self, mode: Zip64) -> Zip {
        self.zip64 = mode;
        self
    }

    /// The order in which the central directory lists the entries (indexes into entries).
    #[must_use]
    pub fn cd_order(mut self, order: &[usize]) -> Zip {
        self.cd_order = Some(order.to_vec());
        self
    }

    /// Bytes written before the local header of entry `index`.
    #[must_use]
    pub fn gap_before(mut self, index: usize, bytes: &[u8]) -> Zip {
        self.gaps.insert(index, bytes.to_vec());
        self
    }

    /// Bytes between the last entry and the central directory.
    #[must_use]
    pub fn gap_before_central_directory(mut self, bytes: &[u8]) -> Zip {
        self.cd_gap = bytes.to_vec();
        self
    }

    /// Writes the archive.
    pub fn build(&self) -> Vec<u8> {
        let mut out = self.prefix.clone();
        let mut written = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            let cdata = e.cdata.clone().unwrap_or_else(|| {
                if e.method == 8 {
                    deflate(&e.data, e.level)
                } else {
                    e.data.clone()
                }
            });
            let crc = e.crc.unwrap_or_else(|| crc32(&e.data));
            let usize_ = u64::from(e.usize.unwrap_or(e.data.len() as u32));
            let csize = e.csize.map_or(cdata.len() as u64, u64::from);
            let dd = e.descriptor != Descriptor::None;
            let mut flags = e.hint_bits() | e.flags_extra;
            let utf8 = e.utf8.unwrap_or_else(|| e.name.iter().any(|b| *b > 127));
            if utf8 {
                flags |= 0x800;
            }
            if dd {
                flags |= 0x8;
            }
            let z64 = e.zip64 || self.zip64 == Zip64::Full;
            let z64_local = z64 || self.zip64 == Zip64::LocalExtraOnly;
            let mut version_needed = e.version_needed;
            let mut extra_local = e.extra_local.clone();
            if z64_local {
                let mut z = Vec::new();
                z.extend(1u16.to_le_bytes());
                z.extend(16u16.to_le_bytes());
                z.extend(usize_.to_le_bytes());
                z.extend(csize.to_le_bytes());
                z.extend(&extra_local);
                extra_local = z;
                version_needed = version_needed.max(45);
            }
            let (mut l_crc, mut l_cs, mut l_us) = (crc, csize as u32, usize_ as u32);
            if dd {
                (l_crc, l_cs, l_us) = (0, 0, 0);
            } else if self.zip64 == Zip64::Full {
                (l_cs, l_us) = (0xFFFF_FFFF, 0xFFFF_FFFF);
            }
            if let Some(gap) = self.gaps.get(&i) {
                out.extend(gap);
            }
            let offset = out.len() as u64;
            let (time, date) = e.mtime.unwrap_or((0, 0));
            let (local_time, local_date) =
                e.local_time.map_or((time, date), |t| t.unwrap_or((0, 0)));
            let local_name = e.local_name.clone().unwrap_or_else(|| e.name.clone());
            out.extend(b"PK\x03\x04");
            out.extend(version_needed.to_le_bytes());
            out.extend(flags.to_le_bytes());
            out.extend(e.method.to_le_bytes());
            out.extend(local_time.to_le_bytes());
            out.extend(local_date.to_le_bytes());
            out.extend(l_crc.to_le_bytes());
            out.extend(l_cs.to_le_bytes());
            out.extend(l_us.to_le_bytes());
            out.extend((local_name.len() as u16).to_le_bytes());
            out.extend((extra_local.len() as u16).to_le_bytes());
            out.extend(&local_name);
            out.extend(&extra_local);
            out.extend(&cdata);
            match e.descriptor {
                Descriptor::None => {}
                Descriptor::Signed => {
                    out.extend(b"PK\x07\x08");
                    out.extend(crc.to_le_bytes());
                    out.extend((csize as u32).to_le_bytes());
                    out.extend((usize_ as u32).to_le_bytes());
                }
                Descriptor::Unsigned => {
                    out.extend(crc.to_le_bytes());
                    out.extend((csize as u32).to_le_bytes());
                    out.extend((usize_ as u32).to_le_bytes());
                }
                Descriptor::Zip64Signed => {
                    out.extend(b"PK\x07\x08");
                    out.extend(crc.to_le_bytes());
                    out.extend(csize.to_le_bytes());
                    out.extend(usize_.to_le_bytes());
                }
            }
            written.push(Written {
                name: e.name.clone(),
                flags,
                method: e.method,
                time: (time, date),
                crc,
                csize,
                usize: usize_,
                offset,
                cd_offset: e.cd_offset,
                version_made: e.version_made,
                version_needed,
                extra_central: e.extra_central.clone(),
                comment: e.comment.clone(),
                external: e.external,
                internal: e.internal,
                zip64: z64,
            });
        }
        out.extend(&self.cd_gap);
        let base = if self.relative {
            self.prefix.len() as u64
        } else {
            0
        };
        let cd_start = out.len() as u64 - base;
        let order: Vec<usize> = self
            .cd_order
            .clone()
            .unwrap_or_else(|| (0..written.len()).collect());
        let mut cd = Vec::new();
        for i in &order {
            let w = &written[*i];
            let (mut cs, mut us) = (w.csize, w.usize);
            let mut off = w.cd_offset.map_or(w.offset - base, u64::from);
            let mut extra = w.extra_central.clone();
            if w.zip64 {
                let full = self.zip64 == Zip64::Full;
                let mut z = Vec::new();
                if us >= 0xFFFF_FFFF || full {
                    z.extend(us.to_le_bytes());
                    us = 0xFFFF_FFFF;
                }
                if cs >= 0xFFFF_FFFF || full {
                    z.extend(cs.to_le_bytes());
                    cs = 0xFFFF_FFFF;
                }
                if off >= 0xFFFF_FFFF || full {
                    z.extend(off.to_le_bytes());
                    off = 0xFFFF_FFFF;
                }
                let mut e2 = Vec::new();
                e2.extend(1u16.to_le_bytes());
                e2.extend((z.len() as u16).to_le_bytes());
                e2.extend(&z);
                e2.extend(&extra);
                extra = e2;
            }
            cd.extend(b"PK\x01\x02");
            cd.extend(
                ((u16::from(w.version_made.0) << 8) | u16::from(w.version_made.1)).to_le_bytes(),
            );
            cd.extend(w.version_needed.to_le_bytes());
            cd.extend(w.flags.to_le_bytes());
            cd.extend(w.method.to_le_bytes());
            cd.extend(w.time.0.to_le_bytes());
            cd.extend(w.time.1.to_le_bytes());
            cd.extend(w.crc.to_le_bytes());
            cd.extend((cs as u32).to_le_bytes());
            cd.extend((us as u32).to_le_bytes());
            cd.extend((w.name.len() as u16).to_le_bytes());
            cd.extend((extra.len() as u16).to_le_bytes());
            cd.extend((w.comment.len() as u16).to_le_bytes());
            cd.extend(0u16.to_le_bytes());
            cd.extend(w.internal.to_le_bytes());
            cd.extend(w.external.to_le_bytes());
            cd.extend((off as u32).to_le_bytes());
            cd.extend(&w.name);
            cd.extend(&extra);
            cd.extend(&w.comment);
        }
        out.extend(&cd);
        let n = order.len() as u64;
        if matches!(self.zip64, Zip64::Full | Zip64::EndRecordsOnly) {
            let e64 = out.len() as u64 - base;
            out.extend(b"PK\x06\x06");
            out.extend(44u64.to_le_bytes());
            out.extend(45u16.to_le_bytes());
            out.extend(45u16.to_le_bytes());
            out.extend(0u32.to_le_bytes());
            out.extend(0u32.to_le_bytes());
            out.extend(n.to_le_bytes());
            out.extend(n.to_le_bytes());
            out.extend((cd.len() as u64).to_le_bytes());
            out.extend(cd_start.to_le_bytes());
            out.extend(b"PK\x06\x07");
            out.extend(0u32.to_le_bytes());
            out.extend(e64.to_le_bytes());
            out.extend(1u32.to_le_bytes());
            eocd(&mut out, 0xFFFF, 0xFFFF_FFFF, 0xFFFF_FFFF, &self.comment);
        } else {
            eocd(
                &mut out,
                n as u16,
                cd.len() as u32,
                cd_start as u32,
                &self.comment,
            );
        }
        out.extend(&self.trailing);
        out
    }
}

fn eocd(out: &mut Vec<u8>, count: u16, cd_size: u32, cd_offset: u32, comment: &[u8]) {
    out.extend(b"PK\x05\x06");
    out.extend(0u16.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(cd_size.to_le_bytes());
    out.extend(cd_offset.to_le_bytes());
    out.extend((comment.len() as u16).to_le_bytes());
    out.extend(comment);
}

/// The end of central directory record with arbitrary field values.
pub fn raw_eocd(count: u16, cd_size: u32, cd_offset: u32, comment_len: u16) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(b"PK\x05\x06");
    out.extend([0u8; 4]);
    out.extend(count.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(cd_size.to_le_bytes());
    out.extend(cd_offset.to_le_bytes());
    out.extend(comment_len.to_le_bytes());
    out
}

#[cfg(test)]
mod tests;
