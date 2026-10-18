//! Byte-order marks, UTF-16 and the declared encoding.

use super::errors::malformed;
use crate::api::error::StetError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Encoding {
    Utf8,
    Utf16 { big_endian: bool, bom: bool },
}

/// Detects the encoding from a BOM or from the first bytes of `<?xml`. Returns the encoding
/// and the BOM length (in the original bytes).
pub(crate) fn detect(b: &[u8]) -> Result<(Encoding, usize), StetError> {
    if b.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Ok((Encoding::Utf8, 3));
    }
    if b.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) || b.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        return Err(malformed("UTF-32 is not allowed in a package part"));
    }
    if b.starts_with(&[0xFF, 0xFE]) {
        return Ok((
            Encoding::Utf16 {
                big_endian: false,
                bom: true,
            },
            2,
        ));
    }
    if b.starts_with(&[0xFE, 0xFF]) {
        return Ok((
            Encoding::Utf16 {
                big_endian: true,
                bom: true,
            },
            2,
        ));
    }
    if b.starts_with(&[0x3C, 0x00, 0x3F, 0x00]) {
        bomless_declaration(b, false)?;
        return Ok((
            Encoding::Utf16 {
                big_endian: false,
                bom: false,
            },
            0,
        ));
    }
    if b.starts_with(&[0x00, 0x3C, 0x00, 0x3F]) {
        bomless_declaration(b, true)?;
        return Ok((
            Encoding::Utf16 {
                big_endian: true,
                bom: false,
            },
            0,
        ));
    }
    Ok((Encoding::Utf8, 0))
}

/// UTF-16 without a BOM is accepted only when the part opens with an XML declaration, which
/// no edit removes, so the written part is still detected as UTF-16. XML 1.0 section 4.3.3
/// says UTF-16 entities begin with a BOM. `check_declared` then requires the declaration to
/// name UTF-16.
fn bomless_declaration(b: &[u8], big_endian: bool) -> Result<(), StetError> {
    let unit = |i: usize| {
        let (x, y) = (b.get(2 * i).copied(), b.get(2 * i + 1).copied());
        match (x, y) {
            (Some(x), Some(y)) if big_endian => Some(u16::from_be_bytes([x, y])),
            (Some(x), Some(y)) => Some(u16::from_le_bytes([x, y])),
            _ => None,
        }
    };
    let opens = b"<?xml"
        .iter()
        .enumerate()
        .all(|(i, &c)| unit(i) == Some(u16::from(c)))
        && matches!(unit(5), Some(0x20 | 0x09 | 0x0A | 0x0D));
    if opens {
        Ok(())
    } else {
        Err(malformed(
            "UTF-16 part has no BOM and does not begin with an XML declaration (XML 1.0 section 4.3.3)",
        ))
    }
}

/// Decodes UTF-16 (after any BOM) to UTF-8. A lone surrogate or an odd byte count is an error.
pub(crate) fn utf16_to_utf8(b: &[u8], big_endian: bool) -> Result<Vec<u8>, StetError> {
    if !b.len().is_multiple_of(2) {
        return Err(malformed("UTF-16 part has an odd number of bytes"));
    }
    let units = b.chunks_exact(2).map(|c| {
        let (x, y) = (
            c.first().copied().unwrap_or(0),
            c.get(1).copied().unwrap_or(0),
        );
        if big_endian {
            u16::from_be_bytes([x, y])
        } else {
            u16::from_le_bytes([x, y])
        }
    });
    let mut out = String::with_capacity(b.len() / 2);
    for (i, r) in char::decode_utf16(units).enumerate() {
        match r {
            Ok(c) => out.push(c),
            Err(_) => {
                return Err(malformed(format!(
                    "UTF-16 part has an unpaired surrogate near code unit {i}"
                )));
            }
        }
    }
    Ok(out.into_bytes())
}

/// Encodes UTF-8 back to UTF-16 with the original byte order and BOM.
pub(crate) fn utf8_to_utf16(utf8: &[u8], big_endian: bool, bom: bool) -> Vec<u8> {
    let s = String::from_utf8_lossy(utf8);
    let mut out = Vec::with_capacity(utf8.len() * 2 + 2);
    let mut push = |u: u16| {
        let bytes = if big_endian {
            u.to_be_bytes()
        } else {
            u.to_le_bytes()
        };
        out.extend_from_slice(&bytes);
    };
    if bom {
        push(0xFEFF);
    }
    for u in s.encode_utf16() {
        push(u);
    }
    out
}

/// What the declaration adds to the detected encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Declared {
    /// Nothing, or a name that agrees with the bytes.
    Default,
    /// `us-ascii`. Accepted when every byte is ASCII, and edits then write non-ASCII
    /// characters as character references.
    AsciiOnly,
}

/// Checks the declared encoding against the detected one. OPC allows only UTF-8 and UTF-16.
pub(crate) fn check_declared(declared: Option<&str>, enc: Encoding) -> Result<Declared, StetError> {
    let Some(d) = declared else {
        if let Encoding::Utf16 { bom: false, .. } = enc {
            return Err(malformed(
                "UTF-16 part has no BOM and its XML declaration names no encoding (XML 1.0 section 4.3.3)",
            ));
        }
        return Ok(Declared::Default);
    };
    let d = d.to_ascii_lowercase();
    let ok = match enc {
        Encoding::Utf8 => d == "utf-8" || d == "utf8",
        Encoding::Utf16 { big_endian, .. } => {
            d == "utf-16" || d == "utf16" || d == if big_endian { "utf-16be" } else { "utf-16le" }
        }
    };
    if ok {
        Ok(Declared::Default)
    } else if enc == Encoding::Utf8 && (d == "us-ascii" || d == "ascii") {
        Ok(Declared::AsciiOnly)
    } else {
        Err(malformed(format!(
            "the XML declaration names encoding {d:?}, but package parts must be UTF-8 or UTF-16 and this one is {}",
            match enc {
                Encoding::Utf8 => "UTF-8",
                Encoding::Utf16 { .. } => "UTF-16",
            }
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_round_trip() {
        let s = "<a>h\u{e9}llo \u{1F600}</a>";
        for be in [false, true] {
            for bom in [false, true] {
                let enc = utf8_to_utf16(s.as_bytes(), be, bom);
                let (e, n) = detect(&enc).unwrap();
                if bom {
                    assert_eq!(
                        e,
                        Encoding::Utf16 {
                            big_endian: be,
                            bom
                        }
                    );
                }
                let back = utf16_to_utf8(enc.get(n..).unwrap(), be).unwrap();
                assert_eq!(back, s.as_bytes());
            }
        }
        assert!(utf16_to_utf8(&[0x00, 0xD8, 0x41, 0x00], false).is_err());
        assert!(utf16_to_utf8(&[0x41], false).is_err());
    }

    #[test]
    fn bomless_utf16_needs_a_declaration_naming_utf16() {
        for be in [false, true] {
            let enc = |s: &str| utf8_to_utf16(s.as_bytes(), be, false);
            let bomless = Encoding::Utf16 {
                big_endian: be,
                bom: false,
            };
            let ok = enc("<?xml version=\"1.0\" encoding=\"UTF-16\"?><a/>");
            assert_eq!(detect(&ok).unwrap(), (bomless, 0));
            assert!(check_declared(Some("UTF-16"), bomless).is_ok());
            // A processing instruction is not a declaration: an edit could remove it.
            assert!(detect(&enc("<?xmen x?><a/>")).is_err());
            assert!(detect(&enc("<?xml")).is_err());
            assert!(check_declared(None, bomless).is_err());
            assert!(check_declared(Some("UTF-8"), bomless).is_err());
            // With a BOM, no declaration is needed.
            let with_bom = Encoding::Utf16 {
                big_endian: be,
                bom: true,
            };
            assert_eq!(
                detect(&utf8_to_utf16(b"<a/>", be, true)).unwrap(),
                (with_bom, 2)
            );
            assert!(check_declared(None, with_bom).is_ok());
        }
    }

    #[test]
    fn declared_must_agree() {
        assert!(check_declared(Some("UTF-8"), Encoding::Utf8).is_ok());
        assert!(check_declared(Some("windows-1252"), Encoding::Utf8).is_err());
        assert!(
            check_declared(
                Some("utf-16"),
                Encoding::Utf16 {
                    big_endian: false,
                    bom: true
                }
            )
            .is_ok()
        );
        assert!(
            check_declared(
                Some("utf-16be"),
                Encoding::Utf16 {
                    big_endian: false,
                    bom: true
                }
            )
            .is_err()
        );
        assert!(
            check_declared(
                Some("utf-8"),
                Encoding::Utf16 {
                    big_endian: false,
                    bom: true
                }
            )
            .is_err()
        );
    }
}
