//! A raw ZIP writer for tests. Every field is under the caller's control, so the tests can
//! build the structures real producers write and the ones attackers write.

use super::codec::{crc32, deflate};

#[derive(Clone)]
pub(super) struct E {
    pub name: Vec<u8>,
    pub data: Vec<u8>,
    pub method: u16,
    pub flags: u16,
    /// `(signature, wide)`: write a data descriptor and set bit 3.
    pub descriptor: Option<(bool, bool)>,
    pub lfh_extra: Vec<u8>,
    pub cd_extra: Vec<u8>,
    pub comment: Vec<u8>,
    pub time: u16,
    pub date: u16,
    /// ZIP64 extra fields in the local and central records.
    pub zip64: bool,
    pub gap_after: Vec<u8>,
    pub version_made: u16,
    pub ext_attr: u32,
    pub lfh_time_date: Option<(u16, u16)>,
}

impl E {
    pub fn new(name: &str, data: &[u8]) -> E {
        E {
            name: name.as_bytes().to_vec(),
            data: data.to_vec(),
            method: 8,
            flags: 0,
            descriptor: None,
            lfh_extra: vec![],
            cd_extra: vec![],
            comment: vec![],
            time: 0x6000,
            date: 0x5A21,
            zip64: false,
            gap_after: vec![],
            version_made: 0x031E,
            ext_attr: 0x81A4_0000,
            lfh_time_date: None,
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct Z {
    pub entries: Vec<E>,
    pub prefix: Vec<u8>,
    pub comment: Vec<u8>,
    pub cd_order: Option<Vec<usize>>,
    pub zip64_end: bool,
    /// With `zip64_end`, write the real count, size and offset in the end record, as writers do when
    /// they only need ZIP64 for an entry, instead of the all-ones values that mean "see the record".
    pub zip64_real_values: bool,
    /// Offsets stored relative to the first local header, as with a prepended stub.
    pub relative_offsets: bool,
    /// Bytes between the central directory and the end record (before any ZIP64 records).
    pub pre_eocd: Vec<u8>,
}

fn p16(o: &mut Vec<u8>, v: u16) {
    o.extend_from_slice(&v.to_le_bytes());
}
fn p32(o: &mut Vec<u8>, v: u32) {
    o.extend_from_slice(&v.to_le_bytes());
}
fn p64(o: &mut Vec<u8>, v: u64) {
    o.extend_from_slice(&v.to_le_bytes());
}

fn extra_record(id: u16, data: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    p16(&mut v, id);
    p16(&mut v, data.len() as u16);
    v.extend_from_slice(data);
    v
}

impl Z {
    pub fn new(entries: Vec<E>) -> Z {
        Z {
            entries,
            ..Z::default()
        }
    }

    pub fn build(&self) -> Vec<u8> {
        let mut out = self.prefix.clone();
        let base = if self.relative_offsets {
            self.prefix.len()
        } else {
            0
        };
        let mut offsets = Vec::new();
        let mut comps = Vec::new();
        for e in &self.entries {
            offsets.push(out.len() - base);
            let comp = if e.method == 8 {
                deflate(&e.data)
            } else {
                e.data.clone()
            };
            let crc = crc32(&e.data);
            let flags = e.flags | if e.descriptor.is_some() { 8 } else { 0 };
            p32(&mut out, 0x0403_4b50);
            p16(&mut out, if e.zip64 { 45 } else { 20 });
            p16(&mut out, flags);
            p16(&mut out, e.method);
            let (lt, ld) = e.lfh_time_date.unwrap_or((e.time, e.date));
            p16(&mut out, lt);
            p16(&mut out, ld);
            let mut extra = Vec::new();
            if e.zip64 {
                let mut d = Vec::new();
                p64(&mut d, e.data.len() as u64);
                p64(&mut d, comp.len() as u64);
                extra.extend(extra_record(1, &d));
            }
            extra.extend_from_slice(&e.lfh_extra);
            if e.descriptor.is_some() {
                p32(&mut out, 0);
                p32(&mut out, 0);
                p32(&mut out, 0);
            } else {
                p32(&mut out, crc);
                p32(&mut out, if e.zip64 { u32::MAX } else { comp.len() as u32 });
                p32(
                    &mut out,
                    if e.zip64 {
                        u32::MAX
                    } else {
                        e.data.len() as u32
                    },
                );
            }
            p16(&mut out, e.name.len() as u16);
            p16(&mut out, extra.len() as u16);
            out.extend_from_slice(&e.name);
            out.extend_from_slice(&extra);
            out.extend_from_slice(&comp);
            if let Some((sig, wide)) = e.descriptor {
                if sig {
                    p32(&mut out, 0x0807_4b50);
                }
                p32(&mut out, crc);
                if wide {
                    p64(&mut out, comp.len() as u64);
                    p64(&mut out, e.data.len() as u64);
                } else {
                    p32(&mut out, comp.len() as u32);
                    p32(&mut out, e.data.len() as u32);
                }
            }
            out.extend_from_slice(&e.gap_after);
            comps.push((comp.len(), crc));
        }
        let cd_start = out.len();
        let order: Vec<usize> = self
            .cd_order
            .clone()
            .unwrap_or_else(|| (0..self.entries.len()).collect());
        for &i in &order {
            let e = &self.entries[i];
            let (csize, crc) = comps[i];
            let flags = e.flags | if e.descriptor.is_some() { 8 } else { 0 };
            let mut extra = Vec::new();
            if e.zip64 {
                let mut d = Vec::new();
                p64(&mut d, e.data.len() as u64);
                p64(&mut d, csize as u64);
                p64(&mut d, offsets[i] as u64);
                extra.extend(extra_record(1, &d));
            }
            extra.extend_from_slice(&e.cd_extra);
            p32(&mut out, 0x0201_4b50);
            p16(&mut out, e.version_made);
            p16(&mut out, if e.zip64 { 45 } else { 20 });
            p16(&mut out, flags);
            p16(&mut out, e.method);
            p16(&mut out, e.time);
            p16(&mut out, e.date);
            p32(&mut out, crc);
            p32(&mut out, if e.zip64 { u32::MAX } else { csize as u32 });
            p32(
                &mut out,
                if e.zip64 {
                    u32::MAX
                } else {
                    e.data.len() as u32
                },
            );
            p16(&mut out, e.name.len() as u16);
            p16(&mut out, extra.len() as u16);
            p16(&mut out, e.comment.len() as u16);
            p16(&mut out, 0);
            p16(&mut out, 0);
            p32(&mut out, e.ext_attr);
            p32(&mut out, if e.zip64 { u32::MAX } else { offsets[i] as u32 });
            out.extend_from_slice(&e.name);
            out.extend_from_slice(&extra);
            out.extend_from_slice(&e.comment);
        }
        let cd_end = out.len();
        let cd_size = cd_end - cd_start;
        out.extend_from_slice(&self.pre_eocd);
        if self.zip64_end {
            let rec = out.len();
            p32(&mut out, 0x0606_4b50);
            p64(&mut out, 44);
            p16(&mut out, 45);
            p16(&mut out, 45);
            p32(&mut out, 0);
            p32(&mut out, 0);
            p64(&mut out, self.entries.len() as u64);
            p64(&mut out, self.entries.len() as u64);
            p64(&mut out, cd_size as u64);
            p64(&mut out, (cd_start - base) as u64);
            p32(&mut out, 0x0706_4b50);
            p32(&mut out, 0);
            p64(&mut out, (rec - base) as u64);
            p32(&mut out, 1);
        }
        p32(&mut out, 0x0605_4b50);
        p16(&mut out, 0);
        p16(&mut out, 0);
        let n = self.entries.len() as u16;
        let sentinels = self.zip64_end && !self.zip64_real_values;
        p16(&mut out, if sentinels { 0xFFFF } else { n });
        p16(&mut out, if sentinels { 0xFFFF } else { n });
        p32(&mut out, if sentinels { u32::MAX } else { cd_size as u32 });
        p32(
            &mut out,
            if sentinels {
                u32::MAX
            } else {
                (cd_start - base) as u32
            },
        );
        p16(&mut out, self.comment.len() as u16);
        out.extend_from_slice(&self.comment);
        out
    }
}

/// Size of the stored blob in the test package.
pub(super) const BLOB_LEN: usize = 300;

/// The smallest package the library treats as a Word document.
pub(super) fn docx_entries() -> Vec<E> {
    let ct = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:r><w:t>Hello</w:t></w:r></w:p><w:p><w:r><w:t xml:space=\"preserve\"> a&#xA0;b </w:t></w:r></w:p></w:body></w:document>".to_string();
    let mut media = E::new(
        "word/media/blob.bin",
        &(0..BLOB_LEN as u32)
            .map(|i| (i * 7 % 251) as u8)
            .collect::<Vec<_>>(),
    );
    media.method = 0;
    vec![
        E::new("[Content_Types].xml", ct),
        E::new("_rels/.rels", rels),
        E::new("word/document.xml", doc.as_bytes()),
        media,
    ]
}
