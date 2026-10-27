//! `stetdoc edit`: a batch from stdin or `--ops`, applied with `edit(Edit::ops)`, to a working copy or to a package.

use super::apply::{Made, Where, apply};
use crate::cli::{EditArgs, OffsetsArg, QuotesArg};
use crate::fsio;
use crate::output::{Failure, Outcome};
use stetdoc_core::{Batch, Edit, Limits, OffsetUnit, Quotes, Stamp};

/// Runs `edit`.
pub fn run(args: &EditArgs) -> Result<Outcome, Failure> {
    let at = Where {
        input: &args.input,
        output: args.output.as_ref(),
        in_place: args.in_place,
        dry_run: args.dry_run,
        dry_run_needs_output: true,
    };
    apply(&at, &args.stamp, |_, stamp| {
        Ok(Made::plain(build(args, stamp)?))
    })
}

/// The `Edit` the flags and the batch ask for.
pub fn build(args: &EditArgs, stamp: Stamp) -> Result<Edit, Failure> {
    let limits = Limits::default();
    let text = match &args.batch {
        Some(text) => text.clone(),
        None => fsio::read_batch(args.ops.as_deref(), limits.max_batch_bytes)?,
    };
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
