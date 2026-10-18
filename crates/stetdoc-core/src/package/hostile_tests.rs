//! Hostile input: every case must fail cleanly with a stable error code, never panic.

use super::testzip::{BLOB_LEN, E, Z, docx_entries};
use super::*;
use crate::api::error::ErrorCode;

fn limits() -> Limits {
    Limits::default()
}

fn open_err(bytes: Vec<u8>) -> StetError {
    match Package::open(bytes, &limits()) {
        Ok(_) => panic!("expected open to fail"),
        Err(e) => e,
    }
}

fn base() -> Vec<u8> {
    Z::new(docx_entries()).build()
}

/// Offset of the n-th central record of an archive with no comment and no ZIP64 records.
fn cd_pos(b: &[u8], n: usize) -> usize {
    let eocd = b.len() - 22;
    let mut pos = u32::from_le_bytes(b[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
    for _ in 0..n {
        let nl = u16::from_le_bytes([b[pos + 28], b[pos + 29]]) as usize;
        let el = u16::from_le_bytes([b[pos + 30], b[pos + 31]]) as usize;
        let cl = u16::from_le_bytes([b[pos + 32], b[pos + 33]]) as usize;
        pos += 46 + nl + el + cl;
    }
    pos
}

fn set32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

#[test]
fn not_a_zip() {
    for bytes in [
        vec![],
        vec![1u8; 10],
        b"this is not a zip file, just text".repeat(20),
        vec![0u8; 70_000],
    ] {
        assert_eq!(open_err(bytes).code(), ErrorCode::MalformedInput);
    }
    let mut ole = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    ole.extend(vec![0u8; 600]);
    // A password-protected .docx or a legacy .doc: unsupported, with a hint a user can act on.
    let e = open_err(ole);
    assert_eq!(e.code(), ErrorCode::Unsupported);
    assert!(
        e.message().contains("password-protected .docx"),
        "{}",
        e.message()
    );
    assert!(e.next().contains("remove the password"), "{}", e.next());
}

#[test]
fn truncated_archives() {
    let b = base();
    for cut in [
        b.len() - 1,
        b.len() - 22,
        b.len() - 40,
        b.len() / 2,
        100,
        30,
        4,
    ] {
        assert_eq!(
            open_err(b[..cut].to_vec()).code(),
            ErrorCode::MalformedInput,
            "cut at {cut}"
        );
    }
}

#[test]
fn unsafe_entry_names() {
    let names: Vec<Vec<u8>> = vec![
        b"../evil.xml".to_vec(),
        b"word/../../evil.xml".to_vec(),
        b"/abs/evil.xml".to_vec(),
        b"C:\\evil.xml".to_vec(),
        b"word\\document.xml".to_vec(),
        b"a\0b.xml".to_vec(),
        b"a\x01b.xml".to_vec(),
        vec![b'a'; 70_000],
        b"./word/document.xml".to_vec(),
        b"word//document.xml".to_vec(),
    ];
    for name in names {
        let mut es = docx_entries();
        let mut e = E::new("x", b"data");
        e.name = name.clone();
        es.push(e);
        let r = std::panic::catch_unwind(|| Z::new(es.clone()).build());
        let Ok(bytes) = r else { continue }; // a name too long for the 16-bit length cannot be built
        let err = open_err(bytes);
        assert_eq!(
            err.code(),
            ErrorCode::MalformedInput,
            "{:?}",
            String::from_utf8_lossy(&name)
        );
    }
}

#[test]
fn duplicate_names() {
    for (a, b) in [("word/a.xml", "word/a.xml"), ("word/a.xml", "WORD/A.XML")] {
        let z = Z::new(vec![E::new(a, b"1"), E::new(b, b"2")]);
        assert_eq!(
            open_err(z.build()).code(),
            ErrorCode::MalformedInput,
            "{a} {b}"
        );
    }
}

#[test]
fn local_header_name_differs_from_central() {
    let mut b = base();
    // The first local header's name is at offset 30.
    b[30] ^= 0x01;
    let e = open_err(b);
    assert_eq!(e.code(), ErrorCode::MalformedInput);
    assert!(e.message().contains("local header name"), "{}", e.message());
}

#[test]
fn overlapping_entries() {
    // Two central records pointing at the same local header.
    let mut b = base();
    let first_off = u32::from_le_bytes(
        b[cd_pos(&b, 0) + 42..cd_pos(&b, 0) + 46]
            .try_into()
            .unwrap(),
    );
    let at = cd_pos(&b, 1) + 42;
    set32(&mut b, at, first_off);
    assert_eq!(open_err(b).code(), ErrorCode::MalformedInput);

    // An entry whose compressed size runs into the next entry.
    let mut b = base();
    let at = cd_pos(&b, 0) + 20;
    set32(&mut b, at, 10_000);
    let e = open_err(b);
    assert!(
        e.code() == ErrorCode::MalformedInput || e.code() == ErrorCode::LimitExceeded,
        "{e}"
    );
}

#[test]
fn offsets_and_counts_out_of_range() {
    let eocd = |b: &Vec<u8>| b.len() - 22;
    // central directory offset beyond the file
    let mut b = base();
    let at = eocd(&b) + 16;
    set32(&mut b, at, 0x7FFF_0000);
    assert_eq!(open_err(b).code(), ErrorCode::MalformedInput);
    // entry offset beyond the file
    let mut b = base();
    let at = cd_pos(&b, 2) + 42;
    set32(&mut b, at, 0x7FFF_0000);
    assert_eq!(open_err(b).code(), ErrorCode::MalformedInput);
    // central directory size larger than the file
    let mut b = base();
    let at = eocd(&b) + 12;
    set32(&mut b, at, 0x00FF_FFFF);
    assert_eq!(open_err(b).code(), ErrorCode::MalformedInput);
    // more entries than records
    let mut b = base();
    let at = eocd(&b) + 10;
    b[at] = 9;
    b[at - 2] = 9;
    assert_eq!(open_err(b).code(), ErrorCode::MalformedInput);
    // compressed size larger than the file
    let mut b = base();
    let at = cd_pos(&b, 3) + 20;
    set32(&mut b, at, 0x0010_0000);
    assert!(matches!(
        open_err(b).code(),
        ErrorCode::MalformedInput | ErrorCode::LimitExceeded
    ));
}

#[test]
fn multi_disk() {
    let mut b = base();
    let at = b.len() - 22 + 4;
    b[at] = 1;
    assert_eq!(open_err(b).code(), ErrorCode::MalformedInput);
}

fn part_err(b: Vec<u8>, name: &str) -> StetError {
    let p = Package::open(b, &limits()).expect("open succeeds");
    p.part(name).expect_err("part must fail")
}

#[test]
fn corrupt_parts_fail_when_read_not_when_opened() {
    // bad CRC
    let mut b = base();
    let at = cd_pos(&b, 2) + 16;
    b[at] ^= 0xFF;
    assert_eq!(
        part_err(b, "word/document.xml").code(),
        ErrorCode::MalformedInput
    );
    // declared size smaller and larger than the real one
    for delta in [-3i64, 3] {
        let mut b = base();
        let at = cd_pos(&b, 2) + 24;
        let cur = u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as i64;
        set32(&mut b, at, (cur + delta) as u32);
        assert_eq!(
            part_err(b, "word/document.xml").code(),
            ErrorCode::MalformedInput,
            "delta {delta}"
        );
    }
    // corrupt deflate stream: damage the data of the document entry
    let mut b = base();
    let lfh = u32::from_le_bytes(
        b[cd_pos(&b, 2) + 42..cd_pos(&b, 2) + 46]
            .try_into()
            .unwrap(),
    ) as usize;
    let data = lfh + 30 + b"word/document.xml".len();
    for i in 0..6 {
        b[data + i] = 0xFF;
    }
    assert_eq!(
        part_err(b, "word/document.xml").code(),
        ErrorCode::MalformedInput
    );
    // stored entry whose sizes disagree
    let mut b = base();
    let at = cd_pos(&b, 3) + 24;
    set32(&mut b, at, BLOB_LEN as u32 - 1);
    assert_eq!(
        part_err(b, "word/media/blob.bin").code(),
        ErrorCode::MalformedInput
    );
}

/// Sets the compression method of entry `i` in both its central record and its local header.
fn set_method(b: &mut [u8], i: usize, method: u8) {
    let cd = cd_pos(b, i);
    b[cd + 10] = method;
    let lfh = u32::from_le_bytes(b[cd + 42..cd + 46].try_into().unwrap()) as usize;
    b[lfh + 8] = method;
}

#[test]
fn a_local_header_that_says_another_method_than_the_central_record_is_refused_at_open() {
    // Readers that take the method from one and readers that take it from the other would inflate
    // the entry differently.
    for (central, local) in [(8u8, 0u8), (0, 8), (8, 99)] {
        let mut b = base();
        let cd = cd_pos(&b, 2);
        b[cd + 10] = central;
        let lfh = u32::from_le_bytes(b[cd + 42..cd + 46].try_into().unwrap()) as usize;
        b[lfh + 8] = local;
        let e = open_err(b);
        assert_eq!(e.code(), ErrorCode::MalformedInput, "{central} {local}");
        assert!(e.message().contains("method"), "{e}");
    }
}

#[test]
fn encrypted_and_unsupported_methods() {
    let mut b = base();
    let at = cd_pos(&b, 2) + 8;
    b[at] |= 1;
    assert_eq!(
        part_err(b, "word/document.xml").code(),
        ErrorCode::Unsupported
    );
    for m in [12u8, 14, 99] {
        let mut b = base();
        set_method(&mut b, 2, m);
        assert_eq!(
            part_err(b, "word/document.xml").code(),
            ErrorCode::Unsupported,
            "method {m}"
        );
    }
    // Nothing else is affected, and the package still writes back unchanged.
    let mut b = base();
    set_method(&mut b, 2, 99);
    let p = Package::open(b.clone(), &limits()).unwrap();
    assert_eq!(p.write().unwrap(), b);
    assert_eq!(p.write_rebuilt().unwrap(), b);
}

#[test]
fn bombs_and_limits() {
    // A deflate bomb: 20 MiB of zeros in about 20 KiB.
    let mut es = docx_entries();
    es.push(E::new("word/media/bomb.bin", &vec![0u8; 20 << 20]));
    let b = Z::new(es).build();
    let strict = Limits {
        max_compression_ratio: 100,
        ..Limits::default()
    };
    // `open` reads no data, so the package opens. Reading the bomb fails, and nothing is inflated.
    let p = Package::open(b.clone(), &strict).expect("open does not inflate");
    let e = p.part("word/media/bomb.bin").expect_err("ratio limit");
    assert_eq!(e.code(), ErrorCode::LimitExceeded, "{e}");
    assert!(e.message().contains("bomb.bin"));
    // The default limit (500:1) refuses it too: this is about 1000:1.
    let p = Package::open(b.clone(), &limits()).unwrap();
    assert_eq!(
        p.part("word/media/bomb.bin").err().map(|e| e.code()),
        Some(ErrorCode::LimitExceeded)
    );
    // Other parts still read, and the package still writes back unchanged.
    assert!(p.part("word/document.xml").unwrap().is_some());
    assert_eq!(p.write().unwrap(), b);
    // A big, highly compressible part under the entry-size limit but over the ratio is refused only
    // when the ratio limit is set below it.
    let lax = Limits {
        max_compression_ratio: 5000,
        ..Limits::default()
    };
    assert_eq!(
        Package::open(b.clone(), &lax)
            .unwrap()
            .part("word/media/bomb.bin")
            .unwrap()
            .unwrap()
            .len(),
        20 << 20
    );
    // Entry size, total size, entry count and archive size.
    let small = |f: fn(&mut Limits)| {
        let mut l = Limits::default();
        f(&mut l);
        Package::open(base(), &l).err().map(|e| e.code())
    };
    assert_eq!(
        small(|l| l.max_entry_bytes = 100),
        Some(ErrorCode::LimitExceeded)
    );
    assert_eq!(
        small(|l| l.max_total_bytes = 200),
        Some(ErrorCode::LimitExceeded)
    );
    assert_eq!(small(|l| l.max_entries = 3), Some(ErrorCode::LimitExceeded));
    assert_eq!(
        small(|l| l.max_archive_bytes = 100),
        Some(ErrorCode::LimitExceeded)
    );
    // A central record that claims 3 GB for a tiny entry.
    let mut b = base();
    let at = cd_pos(&b, 2) + 24;
    set32(&mut b, at, 3_000_000_000);
    assert_eq!(open_err(b).code(), ErrorCode::LimitExceeded);
}

#[test]
fn many_entries_limit_is_checked_before_the_directory_is_read() {
    let es: Vec<E> = (0..3000)
        .map(|i| E::new(&format!("p/{i}.xml"), b"<a/>"))
        .collect();
    let b = Z::new(es).build();
    assert!(Package::open(b.clone(), &limits()).is_ok());
    let l = Limits {
        max_entries: 100,
        ..Limits::default()
    };
    assert_eq!(
        Package::open(b, &l).err().map(|e| e.code()),
        Some(ErrorCode::LimitExceeded)
    );
}

#[test]
fn a_fake_end_record_in_the_comment_wins_as_it_does_elsewhere() {
    let mut z = Z::new(docx_entries());
    let mut c = b"PK\x05\x06".to_vec();
    c.extend([0u8; 18]);
    c.extend(b" tail");
    z.comment = c;
    // The last end record signature in the file wins, as it does for Python's `zipfile`, `unzip` and
    // `bsdtar`, so what a package says it holds is what those tools see: here, nothing. Behind a
    // stub that is an empty package, and the bytes come back as they went in.
    z.prefix = b"MZ stub".to_vec();
    let bytes = z.build();
    let p = Package::open(bytes.clone(), &limits()).unwrap();
    assert_eq!(p.part_names().count(), 0);
    assert_eq!(p.write().unwrap(), bytes);
    assert_eq!(p.write_rebuilt().unwrap(), bytes);
    // Without the stub, the bytes in front of the empty archive start like a ZIP file: they are an
    // archive of their own, which a reader that goes through the file from the front would read.
    z.prefix.clear();
    let e = open_err(z.build());
    assert_eq!(e.code(), ErrorCode::MalformedInput);
    assert!(
        e.message().contains("start like a ZIP file"),
        "{}",
        e.message()
    );
}

#[test]
fn a_later_end_record_that_fails_is_not_skipped_for_an_earlier_one() {
    // A file that holds one valid archive and, after it, a second directory with its own end
    // record, whose entries overlap. Python's `zipfile`, `unzip` and `bsdtar` read the last end
    // record and so see the second; falling back to the first would show this package one archive
    // and them another.
    let first = base();
    let eocd1 = first.len() - 22;
    let cd_start = u32::from_le_bytes(first[eocd1 + 16..eocd1 + 20].try_into().unwrap()) as usize;
    let cd_size = u32::from_le_bytes(first[eocd1 + 12..eocd1 + 16].try_into().unwrap()) as usize;
    let mut second_cd = first[cd_start..cd_start + cd_size].to_vec();
    // The first entry's compressed size now reaches over the entries after it.
    second_cd[20..24].copy_from_slice(&5000u32.to_le_bytes());
    let mut b = first.clone();
    let cd2_at = b.len();
    b.extend_from_slice(&second_cd);
    let mut end = first[eocd1..].to_vec();
    end[12..16].copy_from_slice(&(cd_size as u32).to_le_bytes());
    end[16..20].copy_from_slice(&(cd2_at as u32).to_le_bytes());
    b.extend_from_slice(&end);
    assert!(
        Package::open(first, &limits()).is_ok(),
        "the first archive is a good one"
    );
    let e = open_err(b);
    assert_eq!(e.code(), ErrorCode::MalformedInput, "{e}");
}

/// No input panics. Truncating or flipping any byte of an archive either opens it, reads every
/// part and writes it back through both paths without error, or fails with an error code.
#[test]
fn mutation_sweep_never_panics() {
    let mut shapes: Vec<Vec<u8>> = vec![base()];
    let mut z = Z::new(docx_entries());
    for e in &mut z.entries {
        e.descriptor = Some((true, false));
    }
    z.comment = b"comment".to_vec();
    z.cd_order = Some(vec![3, 1, 0, 2]);
    shapes.push(z.build());
    let mut z = Z::new(docx_entries());
    for e in &mut z.entries {
        e.zip64 = true;
    }
    z.zip64_end = true;
    shapes.push(z.build());
    let mut n = 0usize;
    for seed in &shapes {
        let mut probe = |b: Vec<u8>| {
            n += 1;
            if let Ok(p) = Package::open(b, &limits()) {
                let names: Vec<String> = p.part_names().map(str::to_owned).collect();
                for name in &names {
                    let _ = p.part(name);
                }
                let _ = p.write();
                // Rewriting is the slow part (it compresses), so only some mutations take that path.
                if n.is_multiple_of(7) {
                    let _ = p.write_rebuilt();
                    let mut q = p.clone();
                    if q.replace_part("word/document.xml", b"<a/>".to_vec())
                        .is_ok()
                    {
                        let _ = q.write();
                    }
                }
            }
        };
        for cut in 0..seed.len() {
            probe(seed[..cut].to_vec());
        }
        // Every byte near a header or the end records, every other byte elsewhere. (The debug build
        // inflates slowly, and the whole sweep takes about a second in release.)
        for i in 0..seed.len() {
            if i % 2 != 0 && i >= 200 && i + 150 < seed.len() {
                continue;
            }
            for flip in [0x01u8, 0x80, 0xFF] {
                let mut m = seed.clone();
                m[i] ^= flip;
                probe(m);
            }
        }
    }
}
