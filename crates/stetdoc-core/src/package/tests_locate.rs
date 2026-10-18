//! Tests for where the container says its parts are, against a second reader (`reference_reader`,
//! which follows Python's `zipfile`). `unzip` and `bsdtar` agree with it on these shapes. The
//! rule these tests keep is that whatever this crate opens, the other readers open to the same
//! entries, and that a file they would read differently is refused.

use super::reference_reader as reference;
use super::testzip::{E, Z, docx_entries};
use super::*;
use crate::api::error::ErrorCode;

fn refused(bytes: Vec<u8>) -> StetError {
    match Package::open(bytes, &Limits::default()) {
        Ok(_) => panic!("expected the archive to be refused"),
        Err(e) => e,
    }
}

fn opens(bytes: Vec<u8>) -> Package {
    Package::open(bytes, &Limits::default()).expect("open")
}

/// The same package with one letter of the text changed, so every size and offset is the same.
/// That is what makes the second archive of `two-zips.docx` hard to tell from the first.
fn other_document() -> Vec<E> {
    let mut es = docx_entries();
    let text = es[2].data.clone();
    let text = String::from_utf8(text).unwrap().replace("Hello", "Jello");
    es[2] = E::new("word/document.xml", text.as_bytes());
    es
}

fn eocd_pos(b: &[u8]) -> usize {
    b.windows(4)
        .rposition(|w| w == b"PK\x05\x06")
        .expect("end record")
}

fn put(b: &mut [u8], at: usize, bytes: &[u8]) {
    b[at..at + bytes.len()].copy_from_slice(bytes);
}

/// An archive with ZIP64 records and real values in the end record, so a reader that does not
/// look for the ZIP64 record and one that does read the same directory.
fn zip64_archive() -> (Z, Vec<u8>) {
    let mut z = Z::new(docx_entries());
    z.zip64_end = true;
    z.zip64_real_values = true;
    let b = z.build();
    (z, b)
}

#[test]
fn two_archives_in_one_file_are_refused() {
    // `two-zips.docx`: one archive, then a second whose offsets count from where it starts.
    let first = Z::new(docx_entries()).build();
    let second = Z::new(other_document()).build();
    let both = [first.clone(), second.clone()].concat();
    // Python's `zipfile`, `unzip` and `bsdtar` read the second archive, which is the one with the
    // last end record. A reader that goes through the file from the front reads the first.
    let seen = reference::read(&both).expect("the reference reader reads the second archive");
    let doc = seen.iter().find(|f| f.name == "word/document.xml").unwrap();
    assert!(String::from_utf8_lossy(&doc.data).contains("Jello"));
    let e = refused(both.clone());
    assert_eq!(e.code(), ErrorCode::MalformedInput);
    assert!(
        e.message().contains("start like a ZIP file"),
        "{}",
        e.message()
    );
    // The same second archive behind bytes that do not look like an archive (a stub) is read as
    // the second archive, as the others read it, and comes back unchanged.
    let behind_a_stub = [b"MZ self-extractor".to_vec(), both].concat();
    let p = opens(behind_a_stub.clone());
    let text = p.part("word/document.xml").unwrap().unwrap();
    assert!(String::from_utf8_lossy(&text).contains("Jello"));
    assert_eq!(p.write().unwrap(), behind_a_stub);
    assert_eq!(p.write_rebuilt().unwrap(), behind_a_stub);
    // An edit leaves the second archive readable by the others.
    let mut q = p.clone();
    q.replace_part(
        "word/document.xml",
        b"<w:document xmlns:w=\"urn:w\"/>".to_vec(),
    )
    .unwrap();
    let out = q.write().unwrap();
    let seen = reference::read(&out).expect("the reference reader reads the edited file");
    assert_eq!(seen.len(), 4);
}

#[test]
fn the_directory_has_to_end_where_the_end_record_begins() {
    // Bytes between the central directory and the end record. The end record's offset still points
    // at the directory, which a reader that trusts it would find, and the others look at where the
    // end record is and how large the directory is, so they miss it.
    let mut z = Z::new(docx_entries());
    z.pre_eocd = vec![0xCC; 9];
    let bytes = z.build();
    assert!(reference::read(&bytes).is_err());
    let e = refused(bytes);
    assert_eq!(e.code(), ErrorCode::MalformedInput);
    assert!(e.message().contains("central directory"), "{}", e.message());
}

#[test]
fn bytes_that_belong_to_no_entry_are_refused() {
    // `inner-gap.docx`: a reader that goes through the file from the front sees what is in the
    // gap, and one that goes by the directory does not.
    let mut hidden = Z::new(vec![E::new("junk.xml", b"<junk/>")]).build();
    hidden.truncate(hidden.len() - 22 - 46 - "junk.xml".len());
    for (what, gap, entry) in [
        ("between entries", vec![0xAA; 13], 1),
        ("a hidden local header between entries", hidden.clone(), 1),
        ("before the directory", vec![0xBB; 5], 3),
        ("a hidden local header before the directory", hidden, 3),
    ] {
        let mut es = docx_entries();
        es[entry].gap_after = gap;
        let bytes = Z::new(es).build();
        assert!(reference::read(&bytes).is_ok(), "{what}: others read it");
        let e = refused(bytes);
        assert_eq!(e.code(), ErrorCode::MalformedInput, "{what}");
        assert!(
            e.message().contains("belong to no entry"),
            "{what}: {}",
            e.message()
        );
    }
}

#[test]
fn a_zip64_locator_has_to_lead_to_a_record_that_agrees() {
    let (_, good) = zip64_archive();
    assert_eq!(reference::read(&good).unwrap().len(), 4);
    opens(good.clone());
    let eocd = eocd_pos(&good);
    let locator = eocd - 20;
    let record = locator - 56;

    // The record says another count, size or offset than the end record does: Python and
    // `bsdtar` read the record, `unzip` reads the end record.
    for (field, places) in [
        ("count", vec![24, 32]),
        ("size", vec![40]),
        ("offset", vec![48]),
    ] {
        let mut b = good.clone();
        for at in places {
            let old = u64::from_le_bytes(b[record + at..record + at + 8].try_into().unwrap());
            put(&mut b, record + at, &(old + 1).to_le_bytes());
        }
        let e = refused(b);
        assert_eq!(e.code(), ErrorCode::MalformedInput, "{field}");
        assert!(
            e.message().contains("disagrees"),
            "{field}: {}",
            e.message()
        );
    }

    // Bytes between the record and the locator: the record is not where Python looks for it.
    let mut b = good.clone();
    b.splice(locator..locator, [0u8; 4]);
    let e = refused(b);
    assert!(
        e.message().contains("no ZIP64 end record"),
        "{}",
        e.message()
    );

    // A locator with nothing before it, on an end record that has real values.
    let (_, mut b) = zip64_archive();
    let at = eocd_pos(&b) - 20 - 56;
    b.drain(at..at + 56);
    let e = refused(b);
    assert!(
        e.message().contains("no ZIP64 end record"),
        "{}",
        e.message()
    );

    // The locator points at another place than the record: `unzip` and `bsdtar` follow it.
    let mut b = good.clone();
    let stored = u64::from_le_bytes(b[locator + 8..locator + 16].try_into().unwrap());
    put(&mut b, locator + 8, &(stored + 1).to_le_bytes());
    let e = refused(b);
    assert!(
        e.message().contains("ZIP64 locator points somewhere else"),
        "{}",
        e.message()
    );

    // Extensible data after the fixed fields is not supported, and is not read as something else:
    // where the record is not at the place Python looks, and where it says it is longer.
    let mut b = good.clone();
    put(&mut b, record + 4, &(44u64 + 8).to_le_bytes());
    b.splice(locator..locator, [0u8; 8]);
    let e = refused(b);
    assert!(e.message().contains("extensible data"), "{}", e.message());
    let mut b = good.clone();
    put(&mut b, record + 4, &(44u64 + 8).to_le_bytes());
    let e = refused(b);
    assert!(e.message().contains("extensible data"), "{}", e.message());

    // A record or locator that names another disk.
    for at in [record + 16, record + 20, locator + 4] {
        let mut b = good.clone();
        put(&mut b, at, &1u32.to_le_bytes());
        assert_eq!(refused(b).code(), ErrorCode::MalformedInput, "disk at {at}");
    }
    let mut b = good.clone();
    put(&mut b, locator + 16, &2u32.to_le_bytes());
    assert_eq!(refused(b).code(), ErrorCode::MalformedInput);
}

#[test]
fn zip64_records_behind_a_prefix_with_relative_offsets_are_read_where_they_are() {
    for relative in [false, true] {
        let mut z = Z::new(docx_entries());
        z.prefix = b"MZ stub bytes".to_vec();
        z.relative_offsets = relative;
        z.zip64_end = true;
        z.zip64_real_values = true;
        let bytes = z.build();
        let seen = reference::read(&bytes).unwrap();
        assert_eq!(seen.len(), 4, "relative {relative}");
        let p = opens(bytes.clone());
        assert_eq!(p.part_names().count(), 4);
        assert_eq!(p.write().unwrap(), bytes);
        assert_eq!(p.write_rebuilt().unwrap(), bytes);
    }
}

#[test]
fn the_last_end_record_is_the_one_and_its_comment_has_to_fit() {
    let good = Z::new(docx_entries()).build();
    // A second end record at the end whose comment length reaches past the end of the file. The
    // other readers take it (its signature is the last one) and see an empty archive.
    let mut b = good.clone();
    b.extend(b"PK\x05\x06");
    b.extend([0u8; 16]);
    b.extend(500u16.to_le_bytes());
    assert_eq!(reference::read(&b).unwrap().len(), 0);
    let e = refused(b);
    assert_eq!(e.code(), ErrorCode::MalformedInput);
    assert!(e.message().contains("runs past the end"), "{}", e.message());

    // The end record of the archive itself, with a comment length that is too large.
    let mut b = good.clone();
    let at = eocd_pos(&b) + 20;
    put(&mut b, at, &500u16.to_le_bytes());
    assert_eq!(refused(b).code(), ErrorCode::MalformedInput);

    // A signature in the last bytes of the file, too close to the end to be a record.
    let mut b = good.clone();
    b.extend(b"PK\x05\x06");
    assert_eq!(refused(b).code(), ErrorCode::MalformedInput);

    // A signature in the comment that makes a whole record is the end record, so it has to
    // describe the archive: this one does not, and the file is refused.
    let mut z = Z::new(docx_entries());
    let mut fake = b"PK\x05\x06".to_vec();
    fake.extend([0u8; 16]);
    fake.extend(0u16.to_le_bytes());
    z.comment = fake;
    let bytes = z.build();
    assert_eq!(reference::read(&bytes).unwrap().len(), 0);
    assert_eq!(refused(bytes).code(), ErrorCode::MalformedInput);
}

// ---------------------------------------------------------------------------------------------
// Whatever opens is what the other reader sees.

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n.max(1) as u64).unwrap_or(0)
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next().to_le_bytes()[3]).collect()
    }
}

fn bases() -> Vec<(&'static str, Vec<u8>)> {
    let plain = || Z::new(docx_entries());
    let mut v = vec![("plain", plain().build())];
    let mut z = plain();
    z.prefix = b"MZ stub bytes".to_vec();
    v.push(("prefix", z.build()));
    z.relative_offsets = true;
    v.push(("relative", z.build()));
    let mut z = plain();
    z.comment = b"an archive comment".to_vec();
    v.push(("comment", z.build()));
    for (name, real) in [("zip64 sentinels", false), ("zip64 real", true)] {
        let mut z = plain();
        z.zip64_end = true;
        z.zip64_real_values = real;
        v.push((name, z.build()));
        z.prefix = b"MZ".to_vec();
        z.relative_offsets = true;
        v.push((name, z.build()));
    }
    let mut es = docx_entries();
    for e in &mut es {
        e.descriptor = Some((true, false));
    }
    v.push(("descriptors", Z::new(es).build()));
    let mut es = docx_entries();
    for e in &mut es {
        e.zip64 = true;
    }
    v.push(("zip64 entries", Z::new(es).build()));
    let mut z = plain();
    z.cd_order = Some(vec![3, 1, 2, 0]);
    v.push(("directory order", z.build()));
    v
}

/// Offsets in a built archive where one structure ends and the next begins.
fn boundaries(b: &[u8]) -> Vec<usize> {
    let mut at = vec![0, b.len()];
    for sig in [
        &b"PK\x03\x04"[..],
        b"PK\x01\x02",
        b"PK\x05\x06",
        b"PK\x06\x06",
        b"PK\x06\x07",
    ] {
        at.extend(
            b.windows(4)
                .enumerate()
                .filter(|(_, w)| *w == sig)
                .map(|(i, _)| i),
        );
    }
    at.extend(b.len().checked_sub(22 + 20 + 56));
    at
}

fn mutate(base: &[u8], other: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut b = base.to_vec();
    let marks = boundaries(&b);
    let mark = |rng: &mut Rng| marks[rng.below(marks.len())];
    match rng.below(10) {
        0 => {
            let from = b.len().saturating_sub(100);
            let n = 1 + rng.below(3);
            for _ in 0..n {
                let at = from + rng.below(b.len() - from);
                b[at] = rng.next().to_le_bytes()[0];
            }
        }
        1 => {
            // A field of the end record, the ZIP64 record or the locator, set to a value that
            // sits at an edge.
            let eocd = eocd_pos(&b);
            let (at, width) = match rng.below(4) {
                0 => (eocd + 8 + 2 * rng.below(2), 2),
                1 => (eocd + 12 + 4 * rng.below(2), 4),
                2 => (eocd.saturating_sub(20) + 8, 8),
                _ => (eocd.saturating_sub(76) + 24 + 8 * rng.below(4), 8),
            };
            if at + width <= b.len() {
                let old = u64::from_le_bytes({
                    let mut w = [0u8; 8];
                    w[..width].copy_from_slice(&b[at..at + width]);
                    w
                });
                let new = match rng.below(6) {
                    0 => 0,
                    1 => u64::MAX,
                    2 => old.wrapping_add(1),
                    3 => old.wrapping_sub(1),
                    4 => old.wrapping_add(b.len() as u64 / 2),
                    _ => rng.next(),
                };
                b[at..at + width].copy_from_slice(&new.to_le_bytes()[..width]);
            }
        }
        2 => {
            let at = mark(rng);
            let n = 1 + rng.below(24);
            let junk = rng.bytes(n);
            b.splice(at..at, junk);
        }
        3 => {
            let at = mark(rng).min(b.len());
            let end = (at + 1 + rng.below(24)).min(b.len());
            b.drain(at..end);
        }
        4 => b = [base, other].concat(),
        5 => b = [other, base].concat(),
        6 => {
            // A signature, and fields that make it a record, in a place readers look: after the
            // archive, or inside a comment.
            let mut fake = b"PK\x05\x06".to_vec();
            fake.extend(rng.bytes(16));
            // The comment length fits (nothing follows) or reaches past the end of the file.
            fake.extend(if rng.below(2) == 0 {
                vec![0, 0]
            } else {
                rng.bytes(2)
            });
            let at = if rng.below(2) == 0 {
                b.len()
            } else {
                mark(rng)
            };
            b.splice(at..at, fake);
        }
        7 => {
            // The same, for a ZIP64 record and locator.
            let mut fake = b"PK\x06\x06".to_vec();
            fake.extend(44u64.to_le_bytes());
            fake.extend(rng.bytes(44));
            fake.extend(b"PK\x06\x07");
            fake.extend(rng.bytes(16));
            let at = mark(rng);
            b.splice(at..at, fake);
        }
        8 => {
            // Another byte of a central record changes, a count, a length or an offset.
            let cds: Vec<usize> = b
                .windows(4)
                .enumerate()
                .filter(|(_, w)| *w == b"PK\x01\x02")
                .map(|(i, _)| i)
                .collect();
            let c = cds[rng.below(cds.len())];
            let at = c + [10, 16, 20, 24, 28, 30, 32, 42][rng.below(8)];
            if at < b.len() {
                b[at] = rng.next().to_le_bytes()[0];
            }
        }
        _ => {
            let at = b.len();
            let n = 1 + rng.below(40);
            b.splice(at..at, rng.bytes(n));
        }
    }
    b
}

#[test]
fn whatever_opens_is_what_the_other_reader_sees() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let all = bases();
    let (mut opened, mut refused_count) = (0usize, 0usize);
    for (name, base) in &all {
        for round in 0..400 {
            let other = &all[rng.below(all.len())].1;
            let mutated = mutate(base, other, &mut rng);
            let Ok(p) = Package::open(mutated.clone(), &Limits::default()) else {
                refused_count += 1;
                continue;
            };
            opened += 1;
            let mut ours = Vec::new();
            let mut unreadable = false;
            for part in p.part_names() {
                match p.part(part) {
                    Ok(Some(bytes)) => ours.push((part.to_owned(), bytes.to_vec())),
                    _ => unreadable = true,
                }
            }
            if unreadable {
                // A part whose data does not check out is refused when it is read, and the other
                // reader refuses the archive for it: nothing to compare.
                continue;
            }
            let theirs = match reference::read(&mutated) {
                Ok(t) => t,
                Err(e) => panic!(
                    "{name}, round {round}: opened here, and the other reader refuses it: {e}"
                ),
            };
            let theirs: Vec<(String, Vec<u8>)> =
                theirs.into_iter().map(|f| (f.name, f.data)).collect();
            assert_eq!(ours, theirs, "{name}, round {round}: different entries");
            // And an edit leaves an archive that reads the same way, with the part changed.
            if let Some((victim, _)) = ours.first() {
                let mut q = p.clone();
                q.replace_part(victim, b"<replaced/>".to_vec()).unwrap();
                let out = q.write().unwrap();
                let seen = reference::read(&out).unwrap_or_else(|e| {
                    panic!("{name}, round {round}: the other reader refuses the edited file: {e}")
                });
                assert_eq!(seen.len(), ours.len(), "{name}, round {round}");
                for (f, (n, data)) in seen.iter().zip(&ours) {
                    assert_eq!(&f.name, n);
                    if n == victim {
                        assert_eq!(f.data, b"<replaced/>");
                    } else {
                        assert_eq!(&f.data, data, "{name}, round {round}: {n}");
                    }
                }
            }
        }
    }
    // The sweep has to reach both outcomes, or it checks nothing.
    assert!(opened > 200, "only {opened} mutated archives opened");
    assert!(refused_count > 200, "only {refused_count} were refused");
}
