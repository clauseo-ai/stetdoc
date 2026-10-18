//! `stetdoc inspect`: `open` or `resume`, then `inspect`.

use crate::cli::InspectArgs;
use crate::output::{Failure, Outcome};
use crate::target;

/// Runs `inspect`.
pub fn run(args: &InspectArgs) -> Result<Outcome, Failure> {
    let read = target::open_read(&args.input)?;
    let doc = &read.doc;
    let inspection = doc.inspect(&args.target)?;
    Ok(Outcome::of(&inspection.text, &inspection)?.with_notes(read.notes))
}
