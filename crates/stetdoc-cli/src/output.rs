//! What a command returns, and how it is printed: data on stdout, diagnostics on stderr, and the exit status from the error
//! table.

use serde::Serialize;
use serde_json::Value;
use stetdoc_core::{ErrorCode, StetError};

/// What a command that ran prints.
pub struct Outcome {
    /// The text for a person or an agent.
    pub text: String,
    /// The same as data, for `--json`.
    pub json: Value,
    /// Notes for stderr, printed before the data.
    pub notes: Vec<String>,
    /// The exit status: 0 apart from `check`.
    pub exit: u8,
}

impl Outcome {
    /// An outcome with exit status 0 and no notes.
    pub fn new(text: impl Into<String>, json: Value) -> Outcome {
        Outcome {
            text: text.into(),
            json,
            notes: Vec::new(),
            exit: 0,
        }
    }

    /// Adds notes for stderr.
    #[must_use]
    pub fn with_notes(mut self, notes: Vec<String>) -> Outcome {
        self.notes.extend(notes);
        self
    }

    /// The outcome of a core type with a `text` field: the text, and the whole as JSON.
    pub fn of<T: Serialize>(text: &str, data: &T) -> Result<Outcome, Failure> {
        Ok(Outcome::new(text, to_json(data)?))
    }
}

/// Serializes a value for `--json`.
pub fn to_json<T: Serialize>(data: &T) -> Result<Value, Failure> {
    serde_json::to_value(data).map_err(|e| {
        Failure::Core(StetError::new(
            ErrorCode::Internal,
            format!("internal: could not serialize the result as JSON: {e}"),
        ))
    })
}

/// Why a command failed: always a code of the one error table, so the exit status, the JSON and the text agree.
#[derive(Debug)]
pub enum Failure {
    /// A failure with a code from the error table.
    Core(StetError),
}

impl From<StetError> for Failure {
    fn from(error: StetError) -> Failure {
        Failure::Core(error)
    }
}

impl Failure {
    /// A usage error: exit 2 with `invalid_payload`, the code of a request that is malformed.
    pub fn usage(message: impl Into<String>, next: impl Into<String>) -> Failure {
        Failure::Core(StetError::new(ErrorCode::InvalidPayload, message).with_next(next))
    }

    /// A failure of the file system: `io_error`, exit 1.
    pub fn io(message: impl Into<String>, next: impl Into<String>) -> Failure {
        Failure::Core(StetError::new(ErrorCode::IoError, message).with_next(next))
    }

    /// The error.
    pub fn error(&self) -> &StetError {
        match self {
            Failure::Core(error) => error,
        }
    }

    /// The process exit status.
    pub fn exit(&self) -> u8 {
        self.error().code().exit_class().number()
    }

    /// The failure as data: `{"error": {...}}`, the shape napi throws.
    pub fn to_json(&self) -> Value {
        let error = serde_json::to_value(self.error()).unwrap_or(Value::Null);
        serde_json::json!({ "error": error })
    }

    /// The failure as text: `error[code]: message`, then `next: hint`. The operation and the readable file's place are said
    /// once: a message that already names its file (`document.md line 4: ...`) gets no second copy of the place.
    pub fn to_text(&self) -> String {
        let error = self.error();
        let mut text = format!("error[{}]: {}", error.code().as_str(), error.message());
        if let Some(index) = error.op_index() {
            text.push_str(&format!(" (operation {index})"));
        }
        // A message that names its file already says where, in its own words ("document.md line 4: ...", or "document.md: p4
        // is on lines 7 and 8"), so the place is added only to a message that does not name the file. A refusal of a whole
        // file carries the file and no line.
        if let Some(file) = error.file()
            && !error.message().contains(file)
        {
            match error.line() {
                Some(line) => text.push_str(&format!(" ({file} line {line})")),
                None => text.push_str(&format!(" ({file})")),
            }
        }
        format!("{text}\nnext: {}", error.next())
    }
}

/// The message of an argument error: the paragraph before clap's usage lines, on one line.
pub fn argument_message(error: &clap::Error) -> String {
    let rendered = error.render().to_string();
    let message = rendered
        .lines()
        .take_while(|line| !line.trim().is_empty())
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ");
    message.trim_start_matches("error: ").to_string()
}

/// Prints a warning on stderr, for a diagnostic that does not stop the command.
pub fn warn(message: &str) {
    eprintln!("{message}");
}

/// Prints an outcome and returns its exit status.
pub fn print_outcome(outcome: &Outcome, json: bool) -> u8 {
    for note in &outcome.notes {
        eprintln!("{note}");
    }
    if json {
        println!("{}", pretty(&outcome.json));
    } else if !outcome.text.is_empty() {
        let text = outcome.text.trim_end_matches('\n');
        println!("{text}");
    }
    outcome.exit
}

/// Prints a failure and returns its exit status.
pub fn print_failure(failure: &Failure, json: bool) -> u8 {
    if json {
        println!("{}", pretty(&failure.to_json()));
    } else {
        eprintln!("{}", failure.to_text());
    }
    failure.exit()
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::Failure;
    use stetdoc_core::{ErrorCode, StetError};

    fn text(error: StetError) -> String {
        Failure::Core(error).to_text()
    }

    #[test]
    fn a_location_already_in_the_message_is_not_said_twice() {
        let error = StetError::new(
            ErrorCode::ReadonlyRegion,
            "document.md line 4: p4 is read-only.",
        )
        .at_line("document.md", 4);
        let shown = text(error);
        assert_eq!(shown.matches("document.md line 4").count(), 1, "{shown}");
        assert!(
            shown.starts_with("error[readonly_region]: document.md line 4: p4 is read-only.\n")
        );
    }

    #[test]
    fn a_message_that_names_its_file_in_its_own_words_gets_no_suffix() {
        let error = StetError::new(
            ErrorCode::InvalidPayload,
            "document.md: p4 is on lines 7 and 8. To add a paragraph write a line with no ref.",
        )
        .at_line("document.md", 8);
        let shown = text(error);
        assert_eq!(shown.matches("document.md").count(), 1, "{shown}");
    }

    #[test]
    fn a_location_the_message_does_not_name_is_added_once() {
        let error = StetError::new(ErrorCode::NotFound, "p4: `ninety` is not in p4.")
            .at_line("document.md", 7);
        assert_eq!(
            text(error).lines().next(),
            Some("error[not_found]: p4: `ninety` is not in p4. (document.md line 7)")
        );
    }

    #[test]
    fn an_operation_index_is_said_once() {
        let error = StetError::new(ErrorCode::StaleRef, "p4 was changed.").at_op(2);
        assert_eq!(
            text(error).lines().next(),
            Some("error[stale_ref]: p4 was changed. (operation 2)")
        );
    }
}
