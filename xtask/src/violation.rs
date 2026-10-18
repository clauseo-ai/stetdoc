//! A problem a check found, with where it is and how to fix it.

use std::fmt;

/// One problem. Every check reports these, so every failure names a file and line and
/// says how to fix it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    /// Path relative to the repository root.
    pub file: String,
    /// One-based line, or 0 when the problem is about the whole file.
    pub line: usize,
    /// What is wrong.
    pub message: String,
    /// What to do about it.
    pub fix: String,
}

impl Violation {
    /// Builds a violation.
    pub fn new(
        file: impl Into<String>,
        line: usize,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) -> Violation {
        Violation {
            file: file.into(),
            line,
            message: message.into(),
            fix: fix.into(),
        }
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}: {}", self.file, self.message)?;
        } else {
            write!(f, "{}:{}: {}", self.file, self.line, self.message)?;
        }
        write!(f, "\n    fix: {}", self.fix)
    }
}

/// The one-based line of a byte offset in `text`.
pub fn line_of_offset(text: &str, offset: usize) -> usize {
    let end = offset.min(text.len());
    text.as_bytes()[..end]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_violation_names_the_file_the_line_and_the_fix() {
        let v = Violation::new(
            "crates/a/src/lib.rs",
            12,
            "something is wrong",
            "do the thing",
        );
        insta::assert_snapshot!(v.to_string(), @r"
        crates/a/src/lib.rs:12: something is wrong
            fix: do the thing
        ");
    }

    #[test]
    fn a_whole_file_violation_has_no_line() {
        let v = Violation::new("README.md", 0, "missing", "add it");
        assert_eq!(v.to_string(), "README.md: missing\n    fix: add it");
    }

    #[test]
    fn offsets_map_to_one_based_lines() {
        let text = "a\nb\nc";
        assert_eq!(line_of_offset(text, 0), 1);
        assert_eq!(line_of_offset(text, 2), 2);
        assert_eq!(line_of_offset(text, 4), 3);
        assert_eq!(line_of_offset(text, 999), 3);
    }
}
