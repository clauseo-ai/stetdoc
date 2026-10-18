//! Other programs that read a document, as independent readers of what a tracked change means. Each is
//! `None` where the program is not installed, so a test that uses one skips there and runs where the tool is. pandoc is here;
//! LibreOffice, which also loads, converts and accepts and rejects all tracked changes, is [`libreoffice`].

pub mod libreoffice;

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

/// What pandoc does with tracked changes (`--track-changes`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackChanges {
    /// Insertions are kept and deletions are dropped.
    Accept,
    /// Insertions are dropped and deletions are kept.
    Reject,
}

/// The plain text that pandoc reads from a `.docx` with its tracked changes accepted or rejected, one line of
/// text for each paragraph, or `None` where pandoc is not installed or cannot read the document.
pub fn pandoc_text(docx: &[u8], mode: TrackChanges) -> Option<String> {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let installed = Command::new("pandoc")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()?;
    if !installed.success() {
        return None;
    }
    let path = std::env::temp_dir().join(format!(
        "stetdoc-pandoc-{}-{}.docx",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, docx).ok()?;
    let flag = match mode {
        TrackChanges::Accept => "--track-changes=accept",
        TrackChanges::Reject => "--track-changes=reject",
    };
    let out = Command::new("pandoc")
        .args(["-f", "docx", "-t", "plain", "--wrap=none", flag])
        .arg(&path)
        .output();
    let _ = std::fs::remove_file(&path);
    let out = out.ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_pandoc_is_none_and_a_present_one_reads_a_document() {
        let docx = crate::fixtures::zip::Zip::new(crate::fixtures::parts::entries()).build();
        match pandoc_text(&docx, TrackChanges::Accept) {
            None => {}
            Some(text) => assert!(text.contains("Hello edge"), "{text:?}"),
        }
    }
}
