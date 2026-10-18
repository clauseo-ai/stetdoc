//! Tests of the ZIP writer.

use super::*;

#[test]
fn a_plain_archive_has_the_expected_signatures_and_counts() {
    let z = Zip::new(vec![
        Entry::new("a.xml", "<a/>"),
        Entry::new("b.xml", "<b/>").stored(),
    ])
    .build();
    assert!(z.starts_with(b"PK\x03\x04"));
    let eocd = z.len() - 22;
    assert_eq!(&z[eocd..eocd + 4], b"PK\x05\x06");
    assert_eq!(u16::from_le_bytes([z[eocd + 10], z[eocd + 11]]), 2);
}

#[test]
fn the_level_decides_the_hint_bits() {
    for (level, bits) in [(9, 0x2u16), (1, 0x6), (2, 0x4), (6, 0x0)] {
        let z = Zip::new(vec![Entry::new("a", "x").level(level)]).build();
        assert_eq!(u16::from_le_bytes([z[6], z[7]]), bits, "level {level}");
    }
}

#[test]
fn a_descriptor_zeroes_the_local_sizes_and_follows_the_data() {
    let z = Zip::new(vec![
        Entry::new("a", "hello")
            .stored()
            .descriptor(Descriptor::Signed),
    ])
    .build();
    assert_eq!(u16::from_le_bytes([z[6], z[7]]) & 8, 8);
    assert_eq!(&z[14..26], &[0u8; 12]);
    let at = 30 + 1 + 5;
    assert_eq!(&z[at..at + 4], b"PK\x07\x08");
}

#[test]
fn full_zip64_adds_the_end_records() {
    let z = Zip::new(vec![Entry::new("a", "hello")])
        .zip64(Zip64::Full)
        .build();
    assert!(z.windows(4).any(|w| w == b"PK\x06\x06"));
    assert!(z.windows(4).any(|w| w == b"PK\x06\x07"));
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn central_records(z: &[u8]) -> Vec<usize> {
    (0..z.len() - 3)
        .filter(|at| &z[*at..*at + 4] == b"PK\x01\x02")
        .collect()
}

#[test]
fn relative_offsets_shift_every_offset_by_the_prefix() {
    let entries = || vec![Entry::new("a.xml", "<a/>"), Entry::new("b.xml", "<b/>")];
    let plain = Zip::new(entries()).prefix(&[b'x'; 100]).build();
    let relative = Zip::new(entries())
        .prefix(&[b'x'; 100])
        .relative_offsets()
        .build();
    assert_eq!(plain.len(), relative.len());
    let (p, r) = (plain.len() - 22, relative.len() - 22);
    assert_eq!(u32_at(&plain, p + 16), u32_at(&relative, r + 16) + 100);
    let (cp, cr) = (central_records(&plain), central_records(&relative));
    for (a, b) in cp.iter().zip(&cr) {
        assert_eq!(u32_at(&plain, a + 42), u32_at(&relative, b + 42) + 100);
    }
    assert_eq!(u32_at(&relative, cr[0] + 42), 0);
}

#[test]
fn a_local_time_can_differ_from_the_central_time() {
    let z = Zip::new(vec![
        Entry::new("a", "x")
            .mtime(Some((1, 2)))
            .local_mtime(Some((3, 4))),
        Entry::new("b", "x").mtime(Some((1, 2))).local_mtime(None),
        Entry::new("c", "x").mtime(Some((1, 2))),
    ])
    .build();
    let central = central_records(&z);
    for c in &central {
        assert_eq!((u16_at(&z, c + 12), u16_at(&z, c + 14)), (1, 2));
    }
    let locals: Vec<usize> = (0..z.len() - 3)
        .filter(|at| &z[*at..*at + 4] == b"PK\x03\x04")
        .collect();
    assert_eq!(
        (u16_at(&z, locals[0] + 10), u16_at(&z, locals[0] + 12)),
        (3, 4)
    );
    assert_eq!(
        (u16_at(&z, locals[1] + 10), u16_at(&z, locals[1] + 12)),
        (0, 0)
    );
    assert_eq!(
        (u16_at(&z, locals[2] + 10), u16_at(&z, locals[2] + 12)),
        (1, 2)
    );
}

#[test]
fn a_declared_compressed_size_is_written_to_both_headers() {
    let z = Zip::new(vec![Entry::new("a", "hello").stored().declared_csize(77)]).build();
    assert_eq!(u32_at(&z, 18), 77);
    assert_eq!(u32_at(&z, central_records(&z)[0] + 20), 77);
}

#[test]
fn a_central_offset_changes_only_the_central_record() {
    let z = Zip::new(vec![
        Entry::new("a", "hello"),
        Entry::new("b", "hello").central_offset(1234),
    ])
    .build();
    let c = central_records(&z);
    assert_eq!(u32_at(&z, c[1] + 42), 1234);
    assert_ne!(u32_at(&z, c[0] + 42), 1234);
}

#[test]
fn end_records_only_leaves_the_entries_without_zip64_fields() {
    let z = Zip::new(vec![Entry::new("a", "hello")])
        .zip64(Zip64::EndRecordsOnly)
        .build();
    let eocd = z.len() - 22;
    assert_eq!(u16_at(&z, eocd + 10), 0xFFFF);
    assert_eq!(u32_at(&z, eocd + 12), 0xFFFF_FFFF);
    assert_eq!(u32_at(&z, eocd + 16), 0xFFFF_FFFF);
    assert_eq!(u16_at(&z, 28), 0, "no extra field in the local header");
    assert_eq!(
        u16_at(&z, central_records(&z)[0] + 30),
        0,
        "none in the central record"
    );
    assert!(z.windows(4).any(|w| w == b"PK\x06\x06"));
    assert!(z.windows(4).any(|w| w == b"PK\x06\x07"));
}
