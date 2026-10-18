//! More edge fixtures about the layout of the ZIP container: where the end record is found,
//! where offsets count from, which extra fields and attributes the two headers carry, and how
//! many entries there are. All are valid archives that must open and round-trip.

use super::parts::{DECL, Doc, entries, zeros_part};
use super::zip::{Entry, Zip};
use super::{Expect, Fixture, edge_fixture as fx};

const OK: Expect = Expect::Ok;

/// A stub that stands in for a self-extractor.
fn stub() -> Vec<u8> {
    let mut s = b"MZ\x90\x00".to_vec();
    s.extend(b"This stub stands in for a self-extractor.\r\n");
    s.extend([0u8; 40]);
    s
}

fn field(id: u16, data: &[u8]) -> Vec<u8> {
    let mut e = Vec::new();
    e.extend(id.to_le_bytes());
    e.extend((data.len() as u16).to_le_bytes());
    e.extend(data);
    e
}

/// Local and central extra fields that differ, entry by entry, the way real writers differ.
fn extras_that_differ() -> Vec<u8> {
    let a220 = field(0xA220, &[0u8; 12]);
    let mut t_local = vec![3u8];
    t_local.extend(1_577_836_800u32.to_le_bytes());
    t_local.extend(1_577_836_800u32.to_le_bytes());
    let t_local = field(0x5455, &t_local);
    let mut t_central = vec![3u8];
    t_central.extend(1_577_836_800u32.to_le_bytes());
    let t_central = field(0x5455, &t_central);
    let mut ids = vec![1u8, 4];
    ids.extend(501u32.to_le_bytes());
    ids.push(4);
    ids.extend(20u32.to_le_bytes());
    let ids = field(0x7875, &ids);
    let mut ntfs = Vec::new();
    ntfs.extend(0u32.to_le_bytes());
    ntfs.extend(1u16.to_le_bytes());
    ntfs.extend(24u16.to_le_bytes());
    for _ in 0..3 {
        ntfs.extend(132_223_104_000_000_000u64.to_le_bytes());
    }
    let ntfs = field(0x000A, &ntfs);
    let unknown = field(0xCAFE, b"hello");
    let es = entries()
        .into_iter()
        .enumerate()
        .map(|(i, e)| match i {
            0 => e.extra_local(&a220),
            1 => e.extra_local(&t_local).extra_central(&t_central),
            2 => e.extra_both(&ids),
            3 => e.extra_both(&ntfs),
            _ => {
                let mut local = unknown.clone();
                local.extend(&a220);
                e.extra_local(&local).extra_central(&unknown)
            }
        })
        .collect();
    Zip::new(es).build()
}

/// Version made by and the external attributes, as different hosts write them.
fn host_attributes() -> Vec<u8> {
    let es = entries()
        .into_iter()
        .enumerate()
        .map(|(i, e)| match i {
            0 => e.host(3, 20, 0, 0o100_644 << 16),
            1 => e.host(3, 20, 0, 0o100_755 << 16),
            2 => e.host(0, 20, 0, 0x20),
            3 => e.host(19, 63, 0, 0o100_644 << 16),
            _ => e.host(19, 63, 1, 0o100_755 << 16),
        })
        .collect();
    Zip::new(es).build()
}

/// The local header says one time and the central directory another, on every entry.
fn times_that_differ() -> Vec<u8> {
    let local = (12 << 11 | 30 << 5 | 5, (2021 - 1980) << 9 | 6 << 5 | 15);
    let es = entries()
        .into_iter()
        .map(|e| e.mtime(Some((0, 0x0021))).local_mtime(Some(local)))
        .collect();
    Zip::new(es).build()
}

fn many_entries(total: usize, stored: bool) -> Vec<u8> {
    let mut es = entries();
    let extra = total - es.len();
    es.extend((0..extra).map(|i| {
        let e = Entry::new(format!("word/extra/{i:04}.xml"), "<a/>");
        if stored { e.stored() } else { e }
    }));
    Zip::new(es).build()
}

/// Ten parts of exactly 1 MiB of zeros, each deflated to about a kilobyte.
fn ten_zero_megabytes() -> Vec<u8> {
    let mut es = entries();
    es.extend((0..10).map(|i| zeros_part(&format!("word/media/z{i}.bin"), 1 << 20)));
    Zip::new(es).build()
}

/// Three parts, with content types and relationships that name nothing else.
fn minimal_package() -> Vec<u8> {
    let content_types = format!(
        "{DECL}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>"
    );
    let rels = format!(
        "{DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>"
    );
    Zip::new(vec![
        Entry::new("[Content_Types].xml", content_types),
        Entry::new("_rels/.rels", rels),
        Entry::new("word/document.xml", Doc::new().bytes()),
    ])
    .build()
}

/// The container-layout edge fixtures.
pub(super) fn list() -> Vec<Fixture> {
    vec![
        fx(
            "zip-prefix-stub-relative-offsets",
            "bytes before the first local header and every offset, the central directory offset included, counted from the first local header",
            OK,
            || {
                Zip::new(entries())
                    .prefix(&stub())
                    .relative_offsets()
                    .build()
            },
        ),
        fx(
            "zip-extras-differ-local-central",
            "local extra fields that differ from the central ones: 0xA220 padding, 0x5455 times of 9 bytes local and 5 central, 0x7875, 0x000A and an unknown id",
            OK,
            extras_that_differ,
        ),
        fx(
            "zip-host-attributes-mixed",
            "version made by and external attributes from Unix (host 3), FAT (host 0) and macOS (host 19) writers in one archive",
            OK,
            host_attributes,
        ),
        fx(
            "zip-dos-time-local-differs-from-central",
            "the DOS time and date in the local header differ from the central directory on every entry",
            OK,
            times_that_differ,
        ),
        fx("zip-entries-200", "exactly 200 entries", OK, || {
            many_entries(200, false)
        }),
        fx(
            "zip-entries-3000",
            "3000 entries, mostly tiny stored parts, which is 290 KB of headers",
            OK,
            || many_entries(3000, true),
        ),
        fx(
            "zip-ten-1mib-zero-entries",
            "ten parts of exactly 1 MiB of zeros, each deflated to about a kilobyte: valid under the default limits, and over the limit on total inflated bytes when a test lowers it",
            OK,
            ten_zero_megabytes,
        ),
        fx(
            "opc-minimal-three-parts",
            "only [Content_Types].xml, _rels/.rels and word/document.xml, with nothing that points at a missing part",
            OK,
            minimal_package,
        ),
    ]
}
