//! What each command does to a `Document`.

use crate::command::Command;
use crate::render::{matches_listing, numbered, snippets};
use stetdoc_core::{Document, Edit, ErrorCode, ReadableFiles, Receipt, Stamp, StetError};

/// What a command that worked returns: the text for the model, and the receipt of an edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    /// The text of the `tool_result`.
    pub text: String,
    /// The receipt, for `str_replace` and `insert`.
    pub receipt: Option<Receipt>,
}

/// What a command that failed returns: the error, and for `ambiguous_match` the listing of every match with context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The error, boxed so that a `Result` of a command stays small.
    pub error: Box<StetError>,
    /// More lines for the model, such as each match of an ambiguous `old_str` with a line of context on each side.
    pub detail: Option<String>,
}

impl From<StetError> for Failure {
    fn from(error: StetError) -> Failure {
        Failure {
            error: Box::new(error),
            detail: None,
        }
    }
}

/// Runs one command on the document's readable files. `str_replace` and `insert` commit at once, as one tracked
/// `Edit::files` signed by `stamp`; `view` changes nothing.
///
/// `prefix` is the virtual directory the files appear in, such as `/documents/acme/`: a path names `document.md` as
/// `/documents/acme/document.md`, the directory itself lists the files, and a path outside it is `not_found`. An empty
/// prefix puts the files at the root, where a leading `/` is optional.
pub fn run(
    doc: &mut Document,
    command: &Command,
    stamp: &Stamp,
    prefix: &str,
) -> Result<Output, Failure> {
    let files = doc.files()?;
    let paths = Paths::new(prefix);
    let find = |path: &str| paths.find(&files, path);
    match command {
        Command::View { path, view_range } => {
            if paths.is_root(path) {
                return Ok(view_only(paths.listing(&files)));
            }
            let (name, text) = find(path)?;
            Ok(view_only(numbered(text, range(name, text, *view_range)?)))
        }
        Command::StrReplace {
            path,
            old_str,
            new_str,
        } => {
            let (name, text) = find(path)?;
            if old_str.is_empty() {
                return Err(invalid("old_str is empty. Copy the text to replace from the file.").into());
            }
            let starts: Vec<usize> = text.match_indices(old_str.as_str()).map(|(at, _)| at).collect();
            match starts.as_slice() {
                [] => Err(StetError::new(
                    ErrorCode::NotFound,
                    format!("old_str does not appear in {name}."),
                )
                .with_next(
                    "View the file and copy old_str exactly, including the ref at the start of the line and every space.",
                )
                .into()),
                [at] => {
                    let mut edited = String::with_capacity(text.len() + new_str.len());
                    edited.push_str(text.get(..*at).unwrap_or_default());
                    edited.push_str(new_str);
                    edited.push_str(text.get(at + old_str.len()..).unwrap_or_default());
                    commit(doc, &files, name, edited, stamp, &paths)
                }
                many => {
                    let lines: Vec<usize> = many.iter().map(|at| line_of(text, *at)).collect();
                    let first = lines.first().copied().unwrap_or(1);
                    let error = StetError::new(
                        ErrorCode::AmbiguousMatch,
                        format!(
                            "old_str appears {} times in {name}, so the place to replace is not unique.",
                            many.len()
                        ),
                    )
                    .at_line(name, first as u32);
                    Err(Failure {
                        error: Box::new(error),
                        detail: Some(matches_listing(text, &lines)),
                    })
                }
            }
        }
        Command::Insert {
            path,
            insert_line,
            insert_text,
        } => {
            let (name, text) = find(path)?;
            let edited = inserted(name, text, *insert_line as usize, insert_text)?;
            commit(doc, &files, name, edited, stamp, &paths)
        }
        Command::Create { path, .. } => Err(StetError::new(
            ErrorCode::Unsupported,
            format!("{path} cannot be created: the readable files are a view of the document, and a new file has no place in it."),
        )
        .with_next("Edit document.md (or another file that view / lists) with str_replace or insert instead.")
        .into()),
        Command::UndoEdit { path } => Err(StetError::new(
            ErrorCode::Unsupported,
            format!("undo_edit is not offered, so {path} was not changed."),
        )
        .with_next("Every edit is a tracked change: undo one by replacing the text again with str_replace.")
        .into()),
    }
}

/// The whole answer to one `tool_use` block: the text of the `tool_result`, whether it is an error, and the receipt when an
/// edit committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    /// The text for the model.
    pub text: String,
    /// Whether the command failed. Nothing was changed when it did.
    pub is_error: bool,
    /// The receipt of the edit, when `str_replace` or `insert` committed one.
    pub receipt: Option<Receipt>,
}

/// Reads a `tool_use` input exactly as the model sent it, runs it with the files under `prefix`, and renders the answer.
pub fn respond(doc: &mut Document, input: &str, stamp: &Stamp, prefix: &str) -> Response {
    let result = Command::parse(input)
        .map_err(Failure::from)
        .and_then(|command| run(doc, &command, stamp, prefix));
    let (text, is_error) = crate::render::tool_result(&result);
    Response {
        text,
        is_error,
        receipt: result.ok().and_then(|output| output.receipt),
    }
}

fn view_only(text: String) -> Output {
    Output {
        text,
        receipt: None,
    }
}

fn invalid(message: &str) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message)
}

/// Where the files appear: the prefix of every path, normalized to start and end with `/`, or empty.
struct Paths {
    prefix: String,
}

impl Paths {
    fn new(prefix: &str) -> Paths {
        let trimmed = prefix.trim().trim_matches('/');
        let prefix = if trimmed.is_empty() {
            String::new()
        } else {
            format!("/{trimmed}/")
        };
        Paths { prefix }
    }

    /// How the model sees a file's name.
    fn show(&self, name: &str) -> String {
        format!("{}{name}", self.prefix)
    }

    fn is_root(&self, path: &str) -> bool {
        let path = path.trim();
        if self.prefix.is_empty() {
            matches!(path, "" | "/" | ".")
        } else {
            path == self.prefix || path == self.prefix.trim_end_matches('/')
        }
    }

    fn listing(&self, files: &ReadableFiles) -> String {
        let mut out = String::from("The readable files of this document:\n");
        for (name, text) in &files.files {
            out.push_str(&format!(
                "{} ({} lines)\n",
                self.show(name),
                text.lines().count()
            ));
        }
        out.push_str(
            "Each str_replace or insert is applied to the document at once, as tracked changes.\n",
        );
        out
    }

    /// The file a path names.
    fn find<'a>(
        &self,
        files: &'a ReadableFiles,
        path: &str,
    ) -> Result<(&'a str, &'a str), StetError> {
        let path = path.trim();
        let wanted = if self.prefix.is_empty() {
            Some(path.trim_start_matches('/'))
        } else {
            path.strip_prefix(self.prefix.as_str())
        };
        wanted
            .and_then(|wanted| files.files.iter().find(|(name, _)| name == wanted))
            .map(|(name, text)| (name.as_str(), text.as_str()))
            .ok_or_else(|| {
                let names: Vec<String> = files
                    .files
                    .iter()
                    .map(|(name, _)| self.show(name))
                    .collect();
                StetError::new(
                    ErrorCode::NotFound,
                    format!(
                        "There is no file {path}. The files are {}.",
                        names.join(", ")
                    ),
                )
                .with_next(if self.prefix.is_empty() {
                    "View / to list the files, then name one of them.".to_string()
                } else {
                    format!(
                        "View {} to list the files, then name one of them.",
                        self.prefix
                    )
                })
            })
    }
}

/// The lines a view shows, from 1 and inclusive.
fn range(
    name: &str,
    text: &str,
    view_range: Option<[i64; 2]>,
) -> Result<(usize, usize), StetError> {
    let count = text.lines().count();
    let Some([start, end]) = view_range else {
        return Ok((1, count));
    };
    let last = if end == -1 { count as i64 } else { end };
    if start < 1 || start > count.max(1) as i64 || last < start {
        return Err(invalid(&format!(
            "view_range [{start}, {end}] is not a range of {name}, which has {count} lines."
        )));
    }
    Ok((start as usize, (last as usize).min(count)))
}

/// The one-based line a byte offset is on.
fn line_of(text: &str, at: usize) -> usize {
    text.get(..at)
        .map_or(0, |before| before.matches('\n').count())
        + 1
}

/// The text with `insert` placed after line `after` as whole lines.
fn inserted(name: &str, text: &str, after: usize, insert: &str) -> Result<String, StetError> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    if after > lines.len() {
        return Err(invalid(&format!(
            "insert_line {after} is past the end of {name}, which has {} lines.",
            lines.len()
        )));
    }
    let mut out = String::with_capacity(text.len() + insert.len() + 2);
    for line in lines.iter().take(after) {
        out.push_str(line);
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(insert);
    if !insert.ends_with('\n') && after < lines.len() {
        out.push('\n');
    }
    for line in lines.iter().skip(after) {
        out.push_str(line);
    }
    Ok(out)
}

/// Applies the edited file as one tracked `Edit::files`, and shows the touched blocks in the new file.
fn commit(
    doc: &mut Document,
    files: &ReadableFiles,
    name: &str,
    edited: String,
    stamp: &Stamp,
    paths: &Paths,
) -> Result<Output, Failure> {
    let mut edit = files.clone();
    for (file, text) in &mut edit.files {
        if file == name {
            *text = edited;
            break;
        }
    }
    let receipt = doc.edit(Edit::files(edit, stamp.clone()))?;
    let after = doc.files()?;
    let text = snippets(name, &paths.show(name), &receipt, &after);
    Ok(Output {
        text,
        receipt: Some(receipt),
    })
}
