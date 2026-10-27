//! `stetdoc compare`: `compare`, then `save` of the redline.

use crate::cli::CompareArgs;
use crate::fsio;
use crate::output::{Failure, Outcome, to_json};
use crate::{stamp, target};
use stetdoc_core::{CompareOptions, FormatId, Limits};

/// Runs `compare`.
pub fn run(args: &CompareArgs) -> Result<Outcome, Failure> {
    fsio::present(&args.original)?;
    fsio::present(&args.revised)?;
    let working_copies: Vec<&std::path::Path> = [&args.original, &args.revised]
        .into_iter()
        .filter(|p| p.is_dir())
        .map(|p| p.as_path())
        .collect();
    fsio::check_output(
        &args.output,
        &[&args.original, &args.revised],
        &working_copies,
    )?;
    let stamp = stamp::resolve(&args.stamp)?;
    let original = target::read_docx(&args.original)?;
    let revised = target::read_docx(&args.revised)?;
    let options = CompareOptions {
        limits: Limits::default(),
        stamp,
        format: FormatId::default(),
    };
    let compared = stetdoc_core::compare(original, revised, &options)?;
    let saved = compared.document.save()?;
    fsio::write_atomic(&args.output, &saved.bytes)?;
    // A .docx can be viewed without unpacking it, and the ledger is what an agent wants of a redline.
    let hint = format!("stetdoc view {} --revisions", args.output.display());
    let told = super::written::retell(
        &compared.document,
        &compared.receipt,
        &saved.bytes,
        &args.output,
        Some(hint),
    )?;
    let json = serde_json::json!({
        "output": args.output.display().to_string(),
        "receipt": told.receipt,
        "saved": to_json(&saved.receipt)?,
        "next": told.next,
    });
    let author = options.stamp.author.clone();
    super::apply::tell_others(
        Outcome::new(told.text, json),
        &compared.document,
        &author,
        &args.output,
    )
}
