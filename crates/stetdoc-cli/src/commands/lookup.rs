//! `stetdoc lookup`: `open` or `resume`, then `lookup`.

use crate::cli::LookupArgs;
use crate::output::{Failure, Outcome};
use crate::target;

/// Runs `lookup`.
pub fn run(args: &LookupArgs) -> Result<Outcome, Failure> {
    let read = target::open_read(&args.input)?;
    let doc = &read.doc;
    let found = doc.lookup(&args.query)?;
    Ok(Outcome::of(&found.text, &found)?.with_notes(read.notes))
}
