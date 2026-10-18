//! Entry-name validation. A name that could escape the package is rejected at open.

/// Converts raw entry-name bytes to a string for lookups. Valid UTF-8 is used as is. Anything
/// else (a CP437 name written without the UTF-8 flag) maps byte to code point, which is
/// one-to-one, so distinct names stay distinct. The raw bytes are what is copied on write.
pub(crate) fn name_to_string(raw: &[u8]) -> String {
    match std::str::from_utf8(raw) {
        Ok(s) => s.to_owned(),
        Err(_) => raw.iter().map(|&b| char::from(b)).collect(),
    }
}

/// Checks a ZIP entry name against traversal and other unsafe forms.
pub(crate) fn validate_entry_name(raw: &[u8]) -> Result<(), String> {
    if raw.is_empty() {
        return Err("empty entry name".into());
    }
    if raw.len() > 1024 {
        return Err(format!(
            "entry name is {} bytes long (limit 1024)",
            raw.len()
        ));
    }
    if raw.iter().any(|&b| b < 0x20 || b == 0x7f) {
        return Err("entry name contains a control character".into());
    }
    if raw.contains(&b'\\') {
        return Err("entry name contains a backslash".into());
    }
    if raw.first() == Some(&b'/') {
        return Err("entry name is absolute".into());
    }
    if let (Some(&a), Some(&b':')) = (raw.first(), raw.get(1))
        && a.is_ascii_alphabetic()
    {
        return Err("entry name starts with a drive letter".into());
    }
    let body = raw.strip_suffix(b"/").unwrap_or(raw);
    for comp in body.split(|&b| b == b'/') {
        match comp {
            b"" => return Err("entry name has an empty path component".into()),
            b"." | b".." => return Err("entry name has a '.' or '..' path component".into()),
            _ => {}
        }
    }
    Ok(())
}

/// Validates a part name given by a caller (no trailing slash, not a directory).
pub(crate) fn validate_part_name(name: &str) -> Result<(), String> {
    if name.ends_with('/') {
        return Err("a part name cannot end with '/'".into());
    }
    validate_entry_name(name.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_names() {
        for bad in [
            "../evil.xml",
            "word/../../evil.xml",
            "/abs/x",
            "C:\\x",
            "word\\document.xml",
            "a\0b",
            "./word/document.xml",
            "a//b",
            "",
        ] {
            assert!(validate_entry_name(bad.as_bytes()).is_err(), "{bad:?}");
        }
        assert!(validate_entry_name(&vec![b'a'; 2000]).is_err());
    }

    #[test]
    fn accepts_normal_names() {
        for ok in [
            "word/document.xml",
            "[Content_Types].xml",
            "word/media/image1.png",
            "word/",
            "caf\u{e9}.xml",
        ] {
            assert!(validate_entry_name(ok.as_bytes()).is_ok(), "{ok:?}");
        }
        assert!(validate_part_name("word/").is_err());
    }
}
