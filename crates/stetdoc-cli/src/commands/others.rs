//! The tracked changes that authors other than the current one made: one or two lines that every receipt of an edit, and the
//! head of a view, carry, so that an agent tells the user about changes it did not make instead of passing over them.
//!
//! The changes are `Document::revisions` with no filter, in document order, so the number of each is the one the ledger
//! (`stetdoc view FILE --revisions`) shows. There is no line when every change is the current author's, or there is none.

use crate::output::Failure;
use serde_json::{Value, json};
use std::path::Path;
use stetdoc_core::{Document, RevisionFilter, RevisionInfo};

/// How many changes the line names.
const NAMED: usize = 3;

/// How many characters of a change's text the line shows.
const SHOWN_CHARS: usize = 40;

/// The changes by other authors, said.
pub struct Others {
    /// One or two lines, ending with a newline.
    pub text: String,
    /// The same as data: the count, the count per author, and the first changes.
    pub json: Value,
}

/// The changes in `doc` whose author is not `author`, or `None` when there are none. With no `author` (a view, when no name
/// is configured) every change counts. `file` is the document, for the command that lists them all.
pub fn others(
    doc: &Document,
    author: Option<&str>,
    file: &Path,
) -> Result<Option<Others>, Failure> {
    let all = doc.revisions(&RevisionFilter::default())?;
    let theirs: Vec<(usize, &RevisionInfo)> = all
        .iter()
        .enumerate()
        .filter(|(_, r)| author.is_none_or(|me| r.author.as_deref() != Some(me)))
        .collect();
    if theirs.is_empty() {
        return Ok(None);
    }
    let mut by_author: Vec<(String, usize)> = Vec::new();
    for (_, change) in &theirs {
        let name = change
            .author
            .clone()
            .unwrap_or_else(|| "no author".to_string());
        match by_author.iter_mut().find(|(n, _)| *n == name) {
            Some((_, count)) => *count += 1,
            None => by_author.push((name, 1)),
        }
    }
    let total = theirs.len();
    let changes = if total == 1 { "change" } else { "changes" };
    let who = match by_author.as_slice() {
        [(name, _)] => format!("by {name}"),
        many => {
            let each: Vec<String> = many.iter().map(|(n, c)| format!("{n} {c}")).collect();
            format!("by others ({})", each.join(", "))
        }
    };
    let named: Vec<String> = theirs
        .iter()
        .take(NAMED)
        .map(|(n, c)| line(*n, c))
        .collect();
    let more = if total > NAMED {
        format!("; all {total}: stetdoc view {} --revisions", quoted(file))
    } else {
        String::new()
    };
    let them = if total == 1 { "it" } else { "them" };
    let text = match author {
        Some(_) => format!(
            "{total} tracked {changes} {who} you did not make: {}{more}.\nTell the user about {them}.\n",
            named.join(", ")
        ),
        // Without a configured author a view cannot tell the agent's own changes from others'.
        None => format!(
            "{total} tracked {changes} {who} already in the document: {}{more}.\nIf you did not make {them}, tell the user about {them}.\n",
            named.join(", ")
        ),
    };
    let json = json!({
        "count": total,
        "by_author": by_author.iter().map(|(n, c)| json!({ "author": n, "count": c })).collect::<Vec<_>>(),
        "first": theirs.iter().take(NAMED).map(|(n, c)| json!({
            "change": n,
            "kind": kind(c),
            "author": c.author,
            "block": c.block,
            "inserted": c.inserted,
            "deleted": c.deleted,
        })).collect::<Vec<_>>(),
    });
    Ok(Some(Others { text, json }))
}

/// `#56 insert p146 "rated at least A-"`.
fn line(number: usize, change: &RevisionInfo) -> String {
    let mut out = format!("#{number} {}", kind(change));
    if let Some(block) = &change.block {
        out.push_str(&format!(" {block}"));
    }
    if let Some(text) = change.inserted.as_ref().or(change.deleted.as_ref()) {
        out.push_str(&format!(" {:?}", short(text)));
    }
    out
}

/// The change's kind in words: `insert` and `delete` as the ledger says them, and the others as their code says them, such
/// as `run format`.
fn kind(change: &RevisionInfo) -> String {
    let code = serde_json::to_value(change.kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    match code.as_str() {
        "insertion" => "insert".to_string(),
        "deletion" => "delete".to_string(),
        other => other.replace('_', " "),
    }
}

/// The text on one line, cut at `SHOWN_CHARS` characters.
fn short(text: &str) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    if flat.chars().count() > SHOWN_CHARS {
        let cut: String = flat.chars().take(SHOWN_CHARS - 1).collect();
        format!("{}…", cut.trim_end())
    } else {
        flat
    }
}

/// A path as a shell reads it back.
fn quoted(path: &Path) -> String {
    let text = path.display().to_string();
    if text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./:=,@+%".contains(c))
    {
        text
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}
