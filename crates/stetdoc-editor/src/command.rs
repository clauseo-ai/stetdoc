//! The commands of Claude's text editor tool, as the model sends them.

use serde::Deserialize;
use stetdoc_core::{ErrorCode, StetError};

/// One call of the text editor tool (`text_editor_20250728`, tool name `str_replace_based_edit_tool`): its `input`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Command {
    /// Show a file with line numbers, or list the files when the path is `/` or empty.
    View {
        /// The file, such as `document.md` or `/document.md`.
        path: String,
        /// The first and last line to show, from 1. A last line of -1 means the end of the file.
        #[serde(default)]
        view_range: Option<[i64; 2]>,
    },
    /// Replace the one place where `old_str` appears with `new_str`.
    StrReplace {
        /// The file.
        path: String,
        /// The text to find. It must appear exactly once.
        old_str: String,
        /// The text that replaces it. Empty deletes it.
        #[serde(default)]
        new_str: String,
    },
    /// Insert text after a line.
    Insert {
        /// The file.
        path: String,
        /// The line after which the text goes, from 1; 0 puts it at the top.
        insert_line: u32,
        /// The text, as whole lines.
        #[serde(alias = "new_str")]
        insert_text: String,
    },
    /// Create a file. The readable files are a projection of the document, so this is `unsupported`.
    Create {
        /// The file.
        path: String,
        /// Its text.
        #[serde(default)]
        file_text: String,
    },
    /// Undo the last edit of a file, which the 20250728 tool no longer offers. It is `unsupported`.
    UndoEdit {
        /// The file.
        path: String,
    },
}

impl Command {
    /// Reads the `input` of a `tool_use` block. A shape the tool does not send is `invalid_payload`.
    pub fn parse(input: &str) -> Result<Command, StetError> {
        serde_json::from_str(input).map_err(|error| describe(&error))
    }

    /// The path the command names.
    pub fn path(&self) -> &str {
        match self {
            Command::View { path, .. }
            | Command::StrReplace { path, .. }
            | Command::Insert { path, .. }
            | Command::Create { path, .. }
            | Command::UndoEdit { path } => path,
        }
    }
}

/// What is wrong with an input that does not parse. Half of a surrogate pair is named in words, because the model can write
/// one (an escape such as `\ud83d` with no second half) and a JSON syntax error would not tell it what to fix.
fn describe(error: &serde_json::Error) -> StetError {
    let text = error.to_string();
    if text.contains("surrogate") || text.contains("hex escape") {
        StetError::new(
            ErrorCode::InvalidPayload,
            format!(
                "The text editor command has text with an unpaired surrogate (half of a UTF-16 character, such as one half \
                 of an emoji) at line {} column {}, which is not text.",
                error.line(),
                error.column()
            ),
        )
        .with_next("Write the text again with whole characters, then retry.")
    } else {
        StetError::new(
            ErrorCode::InvalidPayload,
            format!("The text editor command is not valid: {text}."),
        )
        .with_next(
            "Send one of view, str_replace or insert, with the parameters the tool describes.",
        )
    }
}
