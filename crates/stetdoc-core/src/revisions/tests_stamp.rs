//! Tests for the stamp, the date validator and the ownership rules.

use super::policy::*;
use super::stamp::*;
use crate::api::error::ErrorCode;

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % n.max(1) as u64).unwrap_or(0)
    }
}

fn stamp() -> RevStamp {
    RevStamp::new("Clauseo", "2026-10-05T14:03:27Z").expect("a valid stamp")
}

// ---- the author ---------------------------------------------------------------------------------

#[test]
fn an_author_is_kept_as_given() {
    for a in [
        "Eric White",
        "Alvarez,",
        "clauseo",
        "Zoë Müller",
        "合同 作者",
        "A",
    ] {
        let s = RevStamp::new(a, "2026-10-05T14:03:27").expect(a);
        assert_eq!(s.author(), a);
    }
}

#[test]
fn an_author_that_cannot_be_written_is_refused_with_invalid_payload() {
    let too_long = "x".repeat(MAX_AUTHOR_CHARS + 1);
    for bad in [
        "",
        too_long.as_str(),
        "a\nb",
        "a\tb",
        "a\u{0}b",
        "a\u{7f}b",
        "a\u{ffff}b",
        "a\u{fffe}",
    ] {
        let e = RevStamp::new(bad, "2026-10-05T14:03:27").expect_err(bad);
        assert_eq!(e.code(), ErrorCode::InvalidPayload, "{bad:?}");
        assert!(!e.message().is_empty());
    }
}

#[test]
fn the_longest_author_counts_characters_not_bytes() {
    let ok = "é".repeat(MAX_AUTHOR_CHARS);
    assert!(RevStamp::new(&ok, "2026-10-05T14:03:27").is_ok());
    assert!(RevStamp::new(&format!("{ok}é"), "2026-10-05T14:03:27").is_err());
}

// ---- the date -----------------------------------------------------------------------------------

#[test]
fn a_date_is_kept_as_the_wall_clock_digits_with_z() {
    let cases = [
        ("2026-10-05T14:03:27", "2026-10-05T14:03:27Z"),
        ("2026-10-05T14:03:27Z", "2026-10-05T14:03:27Z"),
        ("2026-10-05T14:03:27.123456Z", "2026-10-05T14:03:27Z"),
        ("2026-10-05T14:03:27.5", "2026-10-05T14:03:27Z"),
        ("2026-10-05T14:03:27+02:00", "2026-10-05T14:03:27Z"),
        ("2026-10-05T14:03:27-07:00", "2026-10-05T14:03:27Z"),
        ("2026-10-05T14:03:27.9+10:30", "2026-10-05T14:03:27Z"),
        ("2026-10-05T14:03:27+14:00", "2026-10-05T14:03:27Z"),
        ("2026-10-05T00:00:00Z", "2026-10-05T00:00:00Z"),
        ("2026-12-31T23:59:59Z", "2026-12-31T23:59:59Z"),
        ("0001-01-01T00:00:00Z", "0001-01-01T00:00:00Z"),
        ("9999-12-31T23:59:59Z", "9999-12-31T23:59:59Z"),
    ];
    for (given, written) in cases {
        assert_eq!(
            normalize_date(given).as_deref().ok(),
            Some(written),
            "{given}"
        );
        assert_eq!(RevStamp::new("a", given).expect(given).date(), written);
    }
}

#[test]
fn leap_years_and_month_lengths_are_checked() {
    for ok in [
        "2024-02-29",
        "2000-02-29",
        "2026-04-30",
        "2026-01-31",
        "2026-02-28",
    ] {
        assert!(normalize_date(&format!("{ok}T01:02:03")).is_ok(), "{ok}");
    }
    for bad in [
        "2023-02-29",
        "1900-02-29",
        "2100-02-29",
        "2026-04-31",
        "2026-02-30",
        "2026-06-31",
        "2026-13-01",
        "2026-00-10",
        "2026-10-00",
        "2026-10-32",
        "0000-01-01",
    ] {
        assert!(normalize_date(&format!("{bad}T01:02:03")).is_err(), "{bad}");
    }
}

#[test]
fn a_date_that_is_not_a_date_and_time_is_refused() {
    let bad = [
        "",
        "2026-10-05",
        "2026-10-05T",
        "2026-10-05T14",
        "2026-10-05T14:03",
        "2026-10-05 14:03:27",
        "2026-10-05t14:03:27",
        "2026-10-05T14:03:27z",
        "2026-10-05T24:00:00",
        "2026-10-05T14:60:00",
        "2026-10-05T14:03:60",
        "2026-10-05T14:03:27.",
        "2026-10-05T14:03:27.Z",
        "2026-10-05T14:03:27+2:00",
        "2026-10-05T14:03:27+0200",
        "2026-10-05T14:03:27+15:00",
        "2026-10-05T14:03:27+14:30",
        "2026-10-05T14:03:27+02:60",
        "2026-10-05T14:03:27ZZ",
        "2026-10-05T14:03:27Z+02:00",
        "2026-10-05T14:03:27 ",
        " 2026-10-05T14:03:27",
        "26-10-05T14:03:27",
        "２０２６-10-05T14:03:27",
        "2026-1-5T14:03:27",
        "2026/10/05T14:03:27",
        "2026-10-05T14.03.27",
        "yesterday",
    ];
    for b in bad {
        let e = normalize_date(b).expect_err(b);
        assert_eq!(e.code(), ErrorCode::InvalidPayload, "{b:?}");
    }
    assert!(
        normalize_date(&format!("2026-10-05T14:03:27.{}", "1".repeat(100))).is_err(),
        "too long"
    );
}

/// A second reader of dates, written another way, to compare the validator with.
fn reference(date: &str) -> Option<String> {
    let c: Vec<char> = date.chars().collect();
    let num = |from: usize, len: usize| -> Option<u32> {
        let part = c.get(from..from + len)?;
        part.iter()
            .all(|ch| ch.is_ascii_digit())
            .then(|| part.iter().collect::<String>().parse().ok())?
    };
    let at = |i: usize| c.get(i).copied();
    let (year, month, day) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (hour, minute, second) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if at(4)? != '-' || at(7)? != '-' || at(10)? != 'T' || at(13)? != ':' || at(16)? != ':' {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let longest = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if year == 0 || month == 0 || month > 12 || day == 0 || day > longest[month as usize - 1] {
        return None;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let mut rest: String = c.get(19..)?.iter().collect();
    if let Some(after) = rest.strip_prefix('.') {
        let digits = after.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        rest = after.chars().skip(digits).collect();
    }
    let zone_ok = match rest.as_str() {
        "" | "Z" => true,
        z => {
            let zc: Vec<char> = z.chars().collect();
            zc.len() == 6
                && (zc[0] == '+' || zc[0] == '-')
                && zc[1..3].iter().all(char::is_ascii_digit)
                && zc[3] == ':'
                && zc[4..6].iter().all(char::is_ascii_digit)
                && {
                    let h: u32 = zc[1..3].iter().collect::<String>().parse().ok()?;
                    let m: u32 = zc[4..6].iter().collect::<String>().parse().ok()?;
                    h < 14 && m < 60 || (h == 14 && m == 0)
                }
        }
    };
    zone_ok.then(|| c[..19].iter().collect::<String>() + "Z")
}

fn random_date(rng: &mut Rng) -> String {
    let year = 1 + rng.below(9999);
    let month = 1 + rng.below(12);
    let day = 1 + rng.below(31);
    let zones = [
        "",
        "Z",
        ".5",
        ".123Z",
        "+02:00",
        "-07:00",
        ".25-10:30",
        "+14:00",
        "z",
        "+15:00",
        ".",
        "+1:00",
    ];
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}{}",
        rng.below(25),
        rng.below(61),
        rng.below(61),
        zones[rng.below(zones.len())]
    )
}

fn mutate(rng: &mut Rng, s: &str) -> String {
    let mut c: Vec<char> = s.chars().collect();
    if c.is_empty() {
        return String::new();
    }
    let i = rng.below(c.len());
    match rng.below(4) {
        0 => {
            c.remove(i);
        }
        1 => c.insert(
            i,
            ['0', '9', '-', ':', 'T', 'Z', '.', '+', ' ', 'x', '٣'][rng.below(11)],
        ),
        2 => c[i] = ['0', '9', '-', ':', 'T', 'Z', '.', '+', ' ', 'x', '٣'][rng.below(11)],
        _ => c.truncate(i),
    }
    c.into_iter().collect()
}

#[test]
fn the_validator_agrees_with_a_second_reader_on_random_dates_and_their_mutations() {
    let mut rng = Rng(0x0DDB_A11C_AFE5_1234);
    let (mut accepted, mut refused) = (0, 0);
    for _ in 0..30_000 {
        let d = random_date(&mut rng);
        let m = mutate(&mut rng, &d);
        for s in [d.as_str(), m.as_str()] {
            let got = normalize_date(s).ok();
            assert_eq!(got, reference(s), "{s:?}");
            if got.is_some() {
                accepted += 1
            } else {
                refused += 1
            }
        }
    }
    assert!(
        accepted > 2000 && refused > 2000,
        "the generator covers both: {accepted} {refused}"
    );
}

#[test]
fn what_is_written_is_nineteen_characters_and_z() {
    let mut rng = Rng(0xFEED_FACE_0000_0007);
    for _ in 0..5000 {
        if let Ok(w) = normalize_date(&random_date(&mut rng)) {
            assert_eq!(w.len(), 20);
            assert!(w.ends_with('Z'));
            assert_eq!(
                normalize_date(&w).as_deref(),
                Ok(w.as_str()),
                "writing is idempotent"
            );
        }
    }
}

// ---- who owns what ------------------------------------------------------------------------------

#[test]
fn own_pending_is_the_same_author_byte_for_byte() {
    let s = stamp();
    assert!(is_own_pending(Some("Clauseo"), &s));
    assert!(!is_own_pending(Some("clauseo"), &s));
    assert!(!is_own_pending(Some("Clauseo "), &s));
    assert!(!is_own_pending(Some(""), &s));
    assert!(!is_own_pending(None, &s));
}

#[test]
fn created_by_this_transaction_needs_the_author_a_minted_id_and_not_a_split_tail() {
    let s = stamp();
    let minted = 100..110;
    let splits = [104u32, 4];
    assert!(is_created(Some("Clauseo"), 100, &s, &minted, &splits));
    assert!(is_created(Some("Clauseo"), 109, &s, &minted, &splits));
    // not minted
    assert!(!is_created(Some("Clauseo"), 99, &s, &minted, &splits));
    assert!(!is_created(Some("Clauseo"), 110, &s, &minted, &splits));
    assert!(!is_created(Some("Clauseo"), -1, &s, &minted, &splits));
    assert!(!is_created(Some("Clauseo"), i64::MAX, &s, &minted, &splits));
    // the tail of a split, even of our own revision
    assert!(!is_created(Some("Clauseo"), 104, &s, &minted, &splits));
    // another author's tail or copy takes a minted id too, and is not ours
    assert!(!is_created(Some("Eric White"), 101, &s, &minted, &splits));
    assert!(!is_created(None, 101, &s, &minted, &splits));
}

#[test]
fn the_change_tracking_names_are_the_ones_the_copy_rule_strips() {
    for n in [
        "ins",
        "del",
        "moveFrom",
        "moveTo",
        "rPrChange",
        "pPrChange",
        "sectPrChange",
        "tblPrChange",
        "tblPrExChange",
        "tblGridChange",
        "trPrChange",
        "tcPrChange",
        "numberingChange",
        "cellIns",
        "cellDel",
        "cellMerge",
    ] {
        assert!(is_change_tracking(n), "{n}");
    }
    for n in [
        "r", "p", "pPr", "rPr", "sectPr", "t", "delText", "insideH", "ind", "numPr", "INS", "",
    ] {
        assert!(!is_change_tracking(n), "{n}");
    }
    assert_eq!(CHANGE_TRACKING.len(), 16);
}
