//! The stetdoc block in a project's `AGENTS.md`: added, replaced in place, or removed, keeping the rest of the file and its
//! line endings.

use crate::output::Failure;
use std::path::{Path, PathBuf};

/// The first and last lines of the block in `AGENTS.md`.
const BEGIN: &str = "<!-- stetdoc:begin -->";
const END: &str = "<!-- stetdoc:end -->";

/// What the block says.
const BLOCK_BODY: &str = "## Word documents (.docx)

This project uses `stetdoc` to read and edit .docx files with real tracked changes. Before you work on a .docx, run
`stetdoc skills get core` and follow it. Never edit a .docx with other tools.";

/// `AGENTS.md` with the block: replaced in place when it is there, appended when it is not. The block's lines end the way the
/// file's do (CRLF when it has any), and every other line is kept byte for byte, mixed endings included.
pub fn with_block(current: &str, path: &Path) -> Result<String, Failure> {
    let eol = if current.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let block = format!("{BEGIN}\n{BLOCK_BODY}\n{END}\n").replace('\n', eol);
    match find(current, path)? {
        Some((start, end)) => Ok(format!("{}{block}{}", &current[..start], &current[end..])),
        None if current.is_empty() => Ok(block),
        None => {
            let sep = if current.ends_with(&format!("{eol}{eol}")) {
                ""
            } else if current.ends_with('\n') {
                eol
            } else {
                // The last line has no ending: end it, then leave a blank line.
                &format!("{eol}{eol}")[..]
            };
            Ok(format!("{current}{sep}{block}"))
        }
    }
}

/// `AGENTS.md` without the block, or `None` when it has none.
pub fn without_block(current: &str, path: &Path) -> Result<Option<String>, Failure> {
    let Some((start, end)) = find(current, path)? else {
        return Ok(None);
    };
    let mut before = &current[..start];
    let after = &current[end..];
    // The blank line init put before an appended block goes with it.
    if after.is_empty() {
        for blank in ["\r\n\r\n", "\n\n"] {
            if before.ends_with(blank) {
                before = &before[..before.len() - blank.len() / 2];
                break;
            }
        }
    }
    Ok(Some(format!("{before}{after}")))
}

/// Where the block is: from the start of the line that is exactly the begin marker to the end of the line (its ending
/// included) that is exactly the end marker. A marker inside other text is not one.
fn find(current: &str, path: &Path) -> Result<Option<(usize, usize)>, Failure> {
    let mut begin = None;
    let mut found = None;
    let mut stray = false;
    let mut offset = 0;
    for line in current.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']).trim();
        let next = offset + line.len();
        match (content, begin) {
            (BEGIN, None) if found.is_none() => begin = Some(offset),
            (END, Some(start)) if found.is_none() => found = Some((start, next)),
            (BEGIN | END, _) => stray = true,
            _ => {}
        }
        offset = next;
    }
    if stray || (begin.is_some() && found.is_none()) {
        return Err(Failure::usage(
            format!(
                "{} has a stetdoc block whose markers do not pair up.",
                path.display()
            ),
            format!(
                "Remove the lines {BEGIN} and {END} and what is between them, then run stetdoc init again."
            ),
        ));
    }
    Ok(found)
}

/// A symlinked `AGENTS.md` is written through, so the link stays a link. A link to nothing is refused, because writing
/// through it would create a file somewhere the user did not choose.
pub fn resolve_link(path: &Path) -> Result<PathBuf, Failure> {
    match path.symlink_metadata() {
        Ok(meta) if meta.file_type().is_symlink() => path.canonicalize().map_err(|_| {
            Failure::usage(
                format!(
                    "{} is a symbolic link to a file that does not exist.",
                    path.display()
                ),
                "Point the link at a file, or remove it, then run stetdoc init again.",
            )
        }),
        _ => Ok(path.to_path_buf()),
    }
}
