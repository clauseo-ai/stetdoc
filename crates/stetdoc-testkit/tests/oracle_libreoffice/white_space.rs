//! Whether a difference in white space only is LibreOffice's: it drops the white space at the ends of a `w:t` that does not say
//! `xml:space="preserve"`, which Word shows (hand check 37), so the difference is LibreOffice's only where the input has such a
//! `w:t` (check's `edge_space_without_preserve`) at the place of the difference. Anywhere else a space that stetdoc has and
//! LibreOffice lacks, or the other way round, is a stetdoc suspect.

use std::path::Path;
use stetdoc_testkit::laws::locality::part;

/// The story parts a difference can be in.
fn story_parts() -> Vec<String> {
    let mut names: Vec<String> = ["document", "footnotes", "endnotes", "comments"]
        .iter()
        .map(|n| format!("word/{n}.xml"))
        .collect();
    for i in 1..=9 {
        names.push(format!("word/header{i}.xml"));
        names.push(format!("word/footer{i}.xml"));
    }
    names
}

/// The words on the two sides of the first space that stetdoc's line `s` has and LibreOffice's line `l` lacks, if that is where
/// they first differ: (the word before, the word after).
fn space_place(s: &str, l: &str) -> Option<(String, String)> {
    let (sc, lc): (Vec<char>, Vec<char>) = (s.chars().collect(), l.chars().collect());
    let i = sc
        .iter()
        .zip(&lc)
        .position(|(a, b)| a != b)
        .unwrap_or(lc.len());
    if sc.get(i) != Some(&' ') || i >= sc.len() {
        return None;
    }
    let before: String = sc[..i].iter().collect();
    let after: String = sc[i + 1..].iter().collect();
    let left = before.rsplit(' ').next().unwrap_or_default().to_string();
    let right = after.split(' ').next().unwrap_or_default().to_string();
    Some((left, right))
}

/// The `w:t` elements of a part, in order: (their text, whether they or the document say `xml:space="preserve"`).
fn texts(xml: &str) -> Vec<(String, bool)> {
    let document_preserves = xml
        .find("<w:document")
        .and_then(|a| xml[a..].find('>').map(|b| &xml[a..a + b]))
        .is_some_and(|tag| tag.contains("xml:space=\"preserve\""));
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find("<w:t") {
        rest = &rest[at + 4..];
        if !(rest.starts_with('>') || rest.starts_with(' ')) {
            continue;
        }
        let Some(end) = rest.find('>') else { break };
        let attrs = &rest[..end];
        if attrs.ends_with('/') {
            continue;
        }
        let body = &rest[end + 1..];
        let Some(close) = body.find("</w:t>") else {
            break;
        };
        let text = body[..close]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&");
        out.push((
            text,
            document_preserves || attrs.contains("xml:space=\"preserve\""),
        ));
        rest = &body[close..];
    }
    out
}

/// True if one word ends the other: a word may be split across runs.
fn tail_of(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && (a.ends_with(b) || b.ends_with(a))
}

fn head_of(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && (a.starts_with(b) || b.starts_with(a))
}

/// True if the input holds, between the words `left` and `right`, a `w:t` without `preserve` whose white space at an end is
/// that space: it ends with white space after `left`, starts with it before `right`, or is white space alone between them.
pub fn edge_space_at(input: &Path, s: &str, l: &str) -> bool {
    let Some((left, right)) = space_place(s, l) else {
        return false;
    };
    let Ok(bytes) = std::fs::read(input) else {
        return false;
    };
    let ws = |c: char| c.is_whitespace();
    story_parts()
        .iter()
        .filter_map(|n| part(&bytes, n))
        .any(|xml| {
            let all = texts(&String::from_utf8_lossy(&xml));
            all.iter().enumerate().any(|(k, (t, preserve))| {
                if *preserve || !(t.starts_with(ws) || t.ends_with(ws)) {
                    return false;
                }
                let trimmed = t.trim();
                if trimmed.is_empty() {
                    let before = k.checked_sub(1).and_then(|j| all.get(j));
                    let after = all.get(k + 1);
                    return before.is_some_and(|(p, _)| {
                        tail_of(p.trim_end().rsplit(' ').next().unwrap_or(""), &left)
                    }) || after.is_some_and(|(n, _)| {
                        head_of(n.trim_start().split(' ').next().unwrap_or(""), &right)
                    });
                }
                let last = trimmed.rsplit(ws).next().unwrap_or("");
                let first = trimmed.split(ws).next().unwrap_or("");
                (t.ends_with(ws) && tail_of(last, &left))
                    || (t.starts_with(ws) && head_of(first, &right))
            })
        })
}

#[cfg(test)]
mod tests {
    use super::{edge_space_at, space_place, texts};
    use stetdoc_testkit::fixtures::{parts, zip::Zip};

    fn input(body: &str, name: &str) -> std::path::PathBuf {
        let doc = parts::Doc::new().body(body).bytes();
        let path = std::env::temp_dir().join(format!(
            "stetdoc-white-space-{}-{name}.docx",
            std::process::id()
        ));
        std::fs::write(&path, Zip::new(parts::entries_doc(&doc)).build()).unwrap();
        path
    }

    #[test]
    fn a_space_difference_is_libreoffices_only_where_the_input_has_an_unpreserved_edge_space() {
        let bare = input(
            "<w:p><w:r><w:t>Unshown </w:t></w:r><w:r><w:t>space</w:t></w:r></w:p>",
            "bare",
        );
        let kept = input(
            "<w:p><w:r><w:t xml:space=\"preserve\">Unshown </w:t></w:r><w:r><w:t>space</w:t></w:r></w:p>",
            "kept",
        );
        assert!(edge_space_at(&bare, "Unshown space", "Unshownspace"));
        // the same space with `preserve`, which LibreOffice keeps: a difference there is stetdoc's
        assert!(!edge_space_at(&kept, "Unshown space", "Unshownspace"));
        // a space elsewhere in the same document is not at that place
        assert!(!edge_space_at(&bare, "Other words", "Otherwords"));
        for p in [bare, kept] {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn the_place_of_a_space_is_the_words_around_it() {
        assert_eq!(
            space_place("Unshown space", "Unshownspace"),
            Some(("Unshown".into(), "space".into()))
        );
        assert_eq!(space_place("one two", "one two"), None);
        // LibreOffice with a space that stetdoc lacks is not the deviation
        assert_eq!(space_place("onetwo", "one two"), None);
    }

    #[test]
    fn the_texts_say_whether_they_keep_their_white_space() {
        let xml = r#"<w:document><w:p><w:r><w:t>Unshown </w:t><w:tab/></w:r><w:r><w:t xml:space="preserve"> b</w:t></w:r><w:tbl/></w:p></w:document>"#;
        assert_eq!(
            texts(xml),
            [("Unshown ".to_string(), false), (" b".to_string(), true)]
        );
    }
}
