//! The default format, `stet/1`: the readable files of a working copy and the pages of a view.
//!
//! The files are `document.md` (the body), `notes.md`, `headers.md` and `comments.md`, and a file is
//! written only when its story exists. The extension is for editors and harnesses, which treat `.md` as
//! text. The content is not Markdown, and nothing renders it. Every file starts with a header, a legend
//! and, when there are any, the authors, a summary and the warnings. A paragraph is one logical line:
//! its ref, its tags in braces, and its text, with every non-text thing a marker in braces and every
//! character that a person cannot see for what it is written as an escape (design 5.2 to 5.6).
//!
//! The knobs of design 5.7 (layout, change markup, table syntax, ref syntax) are one function each in
//! `layout.rs`, `render_inline.rs`, `render_structure.rs` and `refs_syntax.rs`, and the default arm of each
//! is what is built. The others are built when the experiments ask for them.

mod escape;
mod invisible;
mod layout;
mod parse;
mod parse_inline;
mod parse_lines;
mod parse_structure;
mod refs_syntax;
mod render;
mod render_inline;
mod render_other;
mod render_structure;
mod tokens;
mod words;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_escape;
#[cfg(test)]
mod tests_lines;
#[cfg(test)]
mod tests_parse;

use super::{BaselineParts, Format, Rendered};
use crate::api::error::StetError;
use crate::api::limits::Limits;
use crate::projection::draft::{Draft, Policy};
use crate::projection::model::Projection;

/// The `stet/1` format.
pub(crate) struct StetFormat;

impl Format for StetFormat {
    fn id(&self) -> &'static str {
        "stet/1"
    }

    fn policy(&self) -> Policy {
        render::POLICY
    }

    fn render(&self, projection: &Projection) -> Rendered {
        render::render(projection)
    }

    fn baseline(&self, model: Projection) -> BaselineParts {
        render::baseline(self, model)
    }

    fn render_paragraphs(&self, projection: &Projection) -> Vec<String> {
        render::paragraph_texts(projection)
    }

    fn parse(&self, files: &[(String, String)], limits: &Limits) -> Result<Draft, StetError> {
        parse::parse_files(&render::POLICY.files, files, limits)
    }
}
