//! The format shared by the exception files: one `path | reason` per line.
//!
//! An exception without a reason is a violation, so every exception explains itself.

use crate::violation::Violation;
use std::collections::BTreeMap;

/// One exception.
#[derive(Clone, Debug)]
pub struct Entry {
    /// One-based line in the exceptions file.
    pub line: usize,
    /// Why the exception exists.
    pub reason: String,
}

/// Parses `text`. Blank lines and lines starting with `#` are ignored. `file` and `hint`
/// are used in the violations for malformed lines.
pub fn parse(file: &str, hint: &str, text: &str) -> (BTreeMap<String, Entry>, Vec<Violation>) {
    let mut map = BTreeMap::new();
    let mut problems = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (path, reason) = match line.split_once('|') {
            Some((p, r)) => (p.trim(), r.trim()),
            None => (line, ""),
        };
        if reason.is_empty() {
            problems.push(Violation::new(
                file,
                i + 1,
                format!("the exception for `{path}` has no reason"),
                format!("write it as `path | {hint}`"),
            ));
            continue;
        }
        map.insert(
            path.to_string(),
            Entry {
                line: i + 1,
                reason: reason.to_string(),
            },
        );
    }
    (map, problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_need_a_reason_and_comments_are_skipped() {
        let (map, problems) = parse("x.txt", "why", "# c\n\na.rs | because\nb.rs\nc.rs |  \n");
        assert_eq!(map.len(), 1);
        assert_eq!(map["a.rs"].line, 3);
        assert_eq!(map["a.rs"].reason, "because");
        assert_eq!(problems.len(), 2);
        assert_eq!(problems[0].line, 4);
        assert!(problems[0].fix.contains("path | why"));
    }
}
