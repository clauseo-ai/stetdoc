//! The author and the date of an edit. The author comes from `--author`, then `STETDOC_AUTHOR`, then the name the user
//! confirmed with `stetdoc init` (the config file), and is never a default the user did not confirm. The date comes from
//! `--date`, then `STETDOC_DATE`, then the clock in the zone `TZ` names.

use crate::cli::StampArgs;
use crate::clock;
use crate::config;
use crate::output::{self, Failure};
use stetdoc_core::Stamp;

/// The variable that gives the author when `--author` is absent.
pub const AUTHOR_VAR: &str = "STETDOC_AUTHOR";
/// The variable that gives the date when `--date` is absent.
pub const DATE_VAR: &str = "STETDOC_DATE";

/// The stamp of a command that writes revisions. A missing author is a usage error.
pub fn resolve(args: &StampArgs) -> Result<Stamp, Failure> {
    let author = args
        .author
        .clone()
        .or_else(|| env(AUTHOR_VAR))
        .or_else(config::author)
        .ok_or_else(|| {
            Failure::usage(
                "No author: every revision carries one, and stetdoc never picks one for you.",
                format!("Run stetdoc init to save your name, or set {AUTHOR_VAR}, or pass --author \"Your Name\"."),
            )
        })?;
    let date = match args.date.clone().or_else(|| env(DATE_VAR)) {
        Some(date) => date,
        None => {
            let (date, warning) = clock::now_local();
            if let Some(warning) = warning {
                output::warn(&warning);
            }
            date
        }
    };
    Ok(Stamp::new(author, date))
}

/// The author a command that writes revisions would use when given no `--author`, or `None` when none is configured: what
/// a command that writes nothing (`view`) takes as the current author.
pub fn author() -> Option<String> {
    env(AUTHOR_VAR).or_else(config::author)
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}
