//! The `json/1` adapter: the neutral model serialized, and parsed back for the tests.
//!
//! It is for `--json`, and for tests that must not depend on any syntax: the alignment and refusal
//! tests of `files` run on drafts built by hand and on this format, so a bug in the parser of `stet/1`
//! cannot hide a bug in `to_batch`. It is not a working-copy format, and no file is ever rendered as
//! JSON for an agent to edit. Its shape is the model's shape (design 5.8), so a change to a field is
//! additive or a new version.

use super::json_value::{FromJson, Json, JsonError, ToJson};
use super::{Format, Rendered, top_paragraphs};
use crate::api::error::{ErrorCode, StetError};
use crate::api::limits::Limits;
use crate::projection::draft::{Draft, Policy};
use crate::projection::draft_build::{FileGroup, file_draft};
use crate::projection::model::{Body, Output, Projection, Story};

/// The deepest nesting of a JSON file that is read. The model nests inlines inside inlines, and a
/// file that nests deeper than this is not one a renderer wrote.
const MAX_DEPTH: usize = 192;

/// The `json/1` format.
pub(crate) struct JsonFormat;

const POLICY: Policy = Policy {
    files: [
        "document.json",
        "notes.json",
        "headers.json",
        "comments.json",
    ],
    view: "view.json",
};

fn text_of(projection: &Projection) -> String {
    let mut out = String::new();
    projection.to_json().write(&mut out);
    out
}

impl Format for JsonFormat {
    fn id(&self) -> &'static str {
        "json/1"
    }

    fn policy(&self) -> Policy {
        POLICY
    }

    fn render(&self, projection: &Projection) -> Rendered {
        let mut files = Vec::new();
        match &projection.head.output {
            Output::View { .. } => {
                files.push((POLICY.view.to_string(), text_of(projection)));
            }
            Output::Files => {
                let stories = projection.stories();
                for group in FileGroup::ALL {
                    let of_group: Vec<Story> = stories
                        .iter()
                        .filter(|story| FileGroup::of(story.kind) == group)
                        .cloned()
                        .collect();
                    if of_group.is_empty() {
                        continue;
                    }
                    let part = Projection {
                        head: projection.head.clone(),
                        body: Body::Stories(of_group),
                        appendix: Vec::new(),
                    };
                    if let Some(name) = POLICY.files.get(group.index()) {
                        files.push(((*name).to_string(), text_of(&part)));
                    }
                }
            }
        }
        Rendered {
            files,
            wraps: Vec::new(),
        }
    }

    fn render_paragraphs(&self, projection: &Projection) -> Vec<String> {
        top_paragraphs(projection)
            .into_iter()
            .map(|(_, paragraph)| {
                let mut text = String::new();
                paragraph.to_json().write(&mut text);
                text
            })
            .collect()
    }

    fn parse(&self, files: &[(String, String)], limits: &Limits) -> Result<Draft, StetError> {
        let mut draft = Draft::default();
        for (name, text) in files {
            let known = name == POLICY.view || POLICY.files.contains(&name.as_str());
            if !known {
                continue;
            }
            if u64::try_from(text.len()).map_or(true, |len| len > limits.max_entry_bytes) {
                return Err(StetError::new(
                    ErrorCode::LimitExceeded,
                    format!(
                        "{name} is larger than the limit of {} bytes for one file.",
                        limits.max_entry_bytes
                    ),
                ));
            }
            let projection = Json::parse(text, MAX_DEPTH)
                .and_then(|json| Projection::from_json(&json))
                .map_err(|error| refused(name, &error))?;
            let stories: Vec<&Story> = projection.stories().iter().collect();
            draft
                .files
                .push(file_draft(name, &projection.head, &stories));
        }
        Ok(draft)
    }
}

fn refused(name: &str, error: &JsonError) -> StetError {
    let place = error
        .at
        .map_or_else(String::new, |at| format!(" at byte {at}"));
    StetError::new(
        ErrorCode::InvalidPayload,
        format!("{name} is not valid json/1: {}{place}.", error.message),
    )
    .with_next("Export the files again, and do not edit a json/1 file by hand.")
}
