//! `stetdoc edit`: a batch from stdin or `--ops`, applied with `edit(Edit::ops)`, to a working copy or to a package.
//!
//! A batch for a .docx may leave out its base: the file is read and written in this one process, as `replace` does, so its
//! refs are read against the file as it is now. A working copy keeps its base, since its refs outlive many edits.

use super::apply::{Made, Where, apply};
use crate::cli::{EditArgs, OffsetsArg, QuotesArg};
use crate::fsio;
use crate::output::{Failure, Outcome};
use stetdoc_core::{Batch, Document, Edit, Limits, Markup, OffsetUnit, Quotes, Stamp};

/// Runs `edit`.
pub fn run(args: &EditArgs) -> Result<Outcome, Failure> {
    let at = Where {
        input: &args.input,
        output: args.output.as_ref(),
        in_place: args.in_place,
        dry_run: args.dry_run,
    };
    let docx = !args.input.is_dir();
    apply(&at, &args.stamp, |doc, stamp| {
        Ok(Made::plain(build(args, stamp, doc, docx)?))
    })
}

/// The markup that --manual and --manual-color ask for. --manual-color alone is enough.
pub fn markup(manual: bool, color: bool) -> Markup {
    match (manual, color) {
        (_, true) => Markup::ManualColor,
        (true, false) => Markup::Manual,
        (false, false) => Markup::Revisions,
    }
}

/// The `Edit` the flags and the batch ask for. With a .docx, a batch without a base takes the document's.
fn build(args: &EditArgs, stamp: Stamp, doc: &Document, docx: bool) -> Result<Edit, Failure> {
    let limits = Limits::default();
    let mut text = match &args.batch {
        Some(text) => text.clone(),
        None => fsio::read_batch(args.ops.as_deref(), limits.max_batch_bytes)?,
    };
    if !has_base(&text) {
        if !docx {
            return Err(Failure::usage(
                format!(
                    "The batch has no base, and a batch for the working copy {} names the base its refs come from.",
                    args.input.display()
                ),
                "Start the batch with the base token of the last view or receipt: a line base: TOKEN, a base field in JSON, or base with the MCP tool's ops.",
            ));
        }
        text = with_base(&text, &doc.base());
    }
    let batch = Batch::parse(&text, &limits)?;
    let mut edit = Edit::ops(batch, stamp)
        .quotes(match args.quotes {
            QuotesArg::Match => Quotes::Match,
            QuotesArg::AsTyped => Quotes::AsTyped,
        })
        .offsets(match args.offsets {
            OffsetsArg::Scalar => OffsetUnit::Scalar,
            OffsetsArg::Utf16 => OffsetUnit::Utf16,
        });
    if args.no_track {
        edit = edit.untracked();
    }
    edit = edit.markup(markup(args.manual, args.manual_color));
    let scope: Vec<String> = args
        .scope
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if !scope.is_empty() {
        edit = edit.scope(scope);
    }
    if args.dry_run {
        edit = edit.dry_run();
    }
    Ok(edit)
}

/// Whether a batch names a base: a `base` key in JSON, or a first line `base:` in block text. Text that is neither is left
/// to the parser to refuse.
fn has_base(text: &str) -> bool {
    let start = text.trim_start();
    match start.chars().next() {
        Some('{') => serde_json::from_str::<serde_json::Value>(start)
            .map_or(true, |v| !v.is_object() || v.get("base").is_some()),
        Some('[') => false,
        Some(_) => start.starts_with("base:"),
        None => true,
    }
}

/// The batch with a base: added to a JSON object, wrapped round a bare list of operations, or as a first line.
fn with_base(text: &str, base: &str) -> String {
    let start = text.trim_start();
    match serde_json::from_str::<serde_json::Value>(start) {
        Ok(serde_json::Value::Object(mut object)) => {
            object.insert("base".into(), base.into());
            serde_json::Value::Object(object).to_string()
        }
        Ok(ops @ serde_json::Value::Array(_)) => {
            serde_json::json!({ "base": base, "ops": ops }).to_string()
        }
        _ => format!("base: {base}\n{text}"),
    }
}
