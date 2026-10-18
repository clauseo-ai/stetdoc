//! Tests of the container-layout fixtures: each one has the structure its description claims,
//! read from the bytes with the independent reader and by hand.

use super::hostile::good;
use super::inspect::{self, Rec, extra_ids};
use super::tests::{find, names, part_bytes};

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Where the four-byte signature appears, in order.
fn signatures(b: &[u8], sig: &[u8]) -> Vec<usize> {
    (0..b.len().saturating_sub(3))
        .filter(|at| &b[*at..*at + 4] == sig)
        .collect()
}

fn records(name: &str) -> (Vec<u8>, Vec<Rec>) {
    let bytes = find(name).bytes();
    let records = inspect::read(&bytes).unwrap().records;
    (bytes, records)
}

fn record<'a>(records: &'a [Rec], name: &str) -> &'a Rec {
    records
        .iter()
        .find(|r| r.name == name.as_bytes())
        .unwrap_or_else(|| panic!("no record {name}"))
}

/// The extra field of the local header that the record points at.
fn local_extra<'a>(b: &'a [u8], rec: &Rec) -> &'a [u8] {
    let at = rec.offset as usize;
    let name = usize::from(u16_at(b, at + 26));
    let extra = usize::from(u16_at(b, at + 28));
    &b[at + 30 + name..at + 30 + name + extra]
}

#[test]
fn the_fake_end_record_in_the_comment_carries_the_real_values() {
    let b = find("zip-comment-fake-end-record").bytes();
    let ends = signatures(&b, b"PK\x05\x06");
    assert_eq!(ends.len(), 2, "the real end record and the fake one");
    let (real, fake) = (ends[0], ends[1]);
    assert_eq!(b[real + 8..real + 20], b[fake + 8..fake + 20]);
    assert_eq!(fake + 22 + usize::from(u16_at(&b, fake + 20)), b.len());
    assert_eq!(real + 22 + usize::from(u16_at(&b, real + 20)), b.len());
    assert!(real + 22 < fake, "the fake record sits inside the comment");
    assert_eq!(names(&b).len(), 5);
}

#[test]
fn relative_offsets_count_from_the_first_local_header() {
    let b = find("zip-prefix-stub-relative-offsets").bytes();
    let stub = signatures(&b, b"PK\x03\x04")[0];
    assert!(stub > 64, "a stub of {stub} bytes");
    let cd = signatures(&b, b"PK\x01\x02")[0];
    let eocd = b.len() - 22;
    assert_eq!(u32_at(&b, eocd + 16) as usize, cd - stub);
    assert_eq!(
        u32_at(&b, cd + 42),
        0,
        "the first local header is at offset 0"
    );
    let (_, recs) = records("zip-prefix-stub-relative-offsets");
    assert_eq!(recs.len(), 5);
}

#[test]
fn the_extra_fields_differ_between_the_two_headers() {
    let (b, recs) = records("zip-extras-differ-local-central");
    let ids = |extra: &[u8]| extra_ids(extra);
    assert_eq!(ids(local_extra(&b, &recs[0])), [0xA220]);
    assert!(recs[0].extra.is_empty());
    let (local, central) = (local_extra(&b, &recs[1]), &recs[1].extra);
    assert_eq!((ids(local), ids(central)), (vec![0x5455], vec![0x5455]));
    assert_eq!(
        (local.len(), central.len()),
        (13, 9),
        "9 and 5 bytes of data"
    );
    assert_eq!(ids(local_extra(&b, &recs[2])), [0x7875]);
    assert_eq!(ids(&recs[2].extra), [0x7875]);
    assert_eq!(ids(local_extra(&b, &recs[3])), [0x000A]);
    assert_eq!(ids(local_extra(&b, &recs[4])), [0xCAFE, 0xA220]);
    assert_eq!(ids(&recs[4].extra), [0xCAFE]);
}

#[test]
fn the_hosts_and_attributes_vary_between_entries() {
    let (b, recs) = records("zip-host-attributes-mixed");
    let hosts: Vec<u16> = recs.iter().map(|r| r.version_made >> 8).collect();
    assert_eq!(hosts, [3, 3, 0, 19, 19]);
    let first_cd = signatures(&b, b"PK\x01\x02")[0];
    assert_eq!(u32_at(&b, first_cd + 38) >> 16, 0o100_644);
}

#[test]
fn the_local_time_differs_from_the_central_time_on_every_entry() {
    let (b, recs) = records("zip-dos-time-local-differs-from-central");
    assert_eq!(recs.len(), 5);
    for rec in &recs {
        let at = rec.offset as usize;
        assert_ne!(u16_at(&b, at + 10), rec.time, "{:?}", rec.name);
        assert_ne!(u16_at(&b, at + 12), rec.date, "{:?}", rec.name);
        assert_eq!((rec.time, rec.date), (0, 0x0021));
    }
}

#[test]
fn the_entry_counts_are_exact() {
    let b = find("zip-entries-200").bytes();
    assert_eq!(names(&b).len(), 200);
    let b = find("zip-entries-3000").bytes();
    assert_eq!(names(&b).len(), 3000);
    assert!(b.len() < 400_000, "{}", b.len());
    let b = find("opc-minimal-three-parts").bytes();
    assert_eq!(
        names(&b),
        ["[Content_Types].xml", "_rels/.rels", "word/document.xml"]
    );
}

#[test]
fn ten_parts_of_zeros_make_exactly_ten_mebibytes() {
    let (b, recs) = records("zip-ten-1mib-zero-entries");
    let zeros: Vec<&Rec> = recs
        .iter()
        .filter(|r| r.name.starts_with(b"word/media/z"))
        .collect();
    assert_eq!(zeros.len(), 10);
    assert!(zeros.iter().all(|r| r.usize == 1 << 20));
    assert!(b.len() < 30_000, "{}", b.len());
    let data = inspect::extract(&b, zeros[0]).unwrap();
    assert!(data.iter().all(|b| *b == 0));
}

#[test]
fn the_offset_and_name_faults_are_in_the_bytes() {
    let (_, recs) = records("zip-path-control-char");
    assert!(recs.iter().any(|r| r.name == b"word/evil\x01.xml"));
    let (_, recs) = records("zip-path-dot-prefix");
    assert!(recs.iter().any(|r| r.name == b"./word/document.xml"));
    let (_, recs) = records("zip-path-name-65535-bytes");
    assert_eq!(recs.last().map(|r| r.name.len()), Some(65_535));

    let (b, recs) = records("zip-overlap-two-records-share-header");
    let last = recs.last().unwrap();
    assert_eq!(last.name, b"word/dup.xml");
    assert_eq!(last.offset, recs[0].offset);
    assert_ne!(
        &b[30..30 + last.name.len()],
        &last.name[..],
        "the local name differs"
    );

    let (b, recs) = records("zip-overlap-entry-runs-into-next");
    let end = recs[0].offset + 30 + recs[0].name.len() as u64 + recs[0].csize;
    assert!(
        end > recs[1].offset + 30 + recs[1].name.len() as u64,
        "runs through entry 1"
    );
    assert!(end <= b.len() as u64);

    let (b, recs) = records("zip-entry-offset-past-eof");
    assert!(record(&recs, "word/document.xml").offset > b.len() as u64);
}

#[test]
fn the_size_and_disk_faults_are_in_the_end_record_and_the_headers() {
    let b = find("zip-ten-byte-file").bytes();
    assert_eq!(b.len(), 10);

    let b = find("zip-cd-size-past-file").bytes();
    let eocd = b.len() - 22;
    assert!(u32_at(&b, eocd + 12) as usize > b.len());

    let (b, recs) = records("zip-csize-past-file");
    assert!(record(&recs, "word/document.xml").csize > b.len() as u64);

    let b = find("zip-multi-disk").bytes();
    let eocd = b.len() - 22;
    assert_eq!((u16_at(&b, eocd + 4), u16_at(&b, eocd + 6)), (1, 1));
}

#[test]
fn seventy_thousand_entries_are_counted_by_the_zip64_end_record_only() {
    let b = find("zip-entries-70000-zip64-count").bytes();
    let eocd = b.len() - 22;
    assert_eq!(u16_at(&b, eocd + 10), 0xFFFF);
    assert_eq!(u32_at(&b, eocd + 16), 0xFFFF_FFFF);
    assert_eq!(signatures(&b, b"PK\x06\x07").len(), 1);
    let archive = inspect::read(&b).unwrap();
    assert!(archive.zip64);
    assert_eq!(archive.records.len(), 70_005);
    assert!(
        archive.records.iter().all(|r| r.extra.is_empty()),
        "no zip64 extra field on any entry"
    );
    assert!(b.len() < 8_000_000, "{}", b.len());
}

#[test]
fn the_declared_sizes_disagree_with_the_data_in_the_way_claimed() {
    let real = part_bytes(&good(), "word/document.xml").len();
    for (name, delta) in [
        ("zip-usize-smaller-than-real", -5i64),
        ("zip-usize-larger-than-real", 5),
    ] {
        let (_, recs) = records(name);
        let rec = record(&recs, "word/document.xml");
        assert_eq!(rec.usize as i64, real as i64 + delta, "{name}");
    }
    let (b, recs) = records("zip-stored-size-too-big");
    let rec = record(&recs, "word/document.xml");
    assert_eq!(rec.method, 0);
    assert_eq!((rec.csize, rec.usize), (real as u64 + 16, real as u64 + 16));
    assert_eq!(inspect::extract(&b, rec).unwrap_err(), "crc mismatch");
    for (name, method) in [
        ("zip-unsupported-method-12-bzip2", 12),
        ("zip-unsupported-method-14-lzma", 14),
    ] {
        let (_, recs) = records(name);
        assert_eq!(record(&recs, "word/document.xml").method, method, "{name}");
    }
}

#[test]
fn the_two_bombs_are_small_on_disk_and_exactly_as_large_as_claimed() {
    let (b, recs) = records("zip-bomb-1000mib-declared");
    let rec = record(&recs, "word/media/bomb.bin");
    assert_eq!(rec.usize, 1000 << 20);
    assert!(b.len() < 2 << 20, "{}", b.len());
    assert!(
        rec.usize / rec.csize >= 500,
        "ratio {}",
        rec.usize / rec.csize
    );

    let (b, recs) = records("zip-ratio-20mib-in-media-part");
    let rec = record(&recs, "word/media/bomb.bin");
    assert_eq!(rec.usize, 20 << 20);
    assert!(
        rec.usize >= 16 << 20,
        "a part of 16 MiB or more is held to the ratio limit"
    );
    assert!(
        rec.usize / rec.csize >= 500,
        "ratio {}",
        rec.usize / rec.csize
    );
    assert!(b.len() < 100_000, "{}", b.len());
    let data = inspect::extract(&b, rec).unwrap();
    assert_eq!(data.len(), 20 << 20);
    assert!(data.iter().all(|b| *b == 0));
}
