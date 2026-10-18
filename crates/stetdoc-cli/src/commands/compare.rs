//! `stetdoc compare`: `compare`, then `save` of the redline.

use crate::cli::CompareArgs;
use crate::fsio;
use crate::output::{Failure, Outcome, to_json};
use crate::{stamp, target};
use stetdoc_core::{CompareOptions, FormatId, Limits};

/// Runs `compare`.
pub fn run(args: &CompareArgs) -> Result<Outcome, Failure> {
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
    let json = serde_json::json!({
        "output": args.output.display().to_string(),
        "receipt": to_json(&compared.receipt)?,
        "saved": to_json(&saved.receipt)?,
    });
    let text = format!(
        "{}\nwrote {}\n",
        compared.receipt.text.trim_end(),
        args.output.display()
    );
    Ok(Outcome::new(text, json))
}
