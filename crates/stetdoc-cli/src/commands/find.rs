//! Finding the text of `replace` and `comment`: every place it is, in the scope the flags give, and the refusal that lists
//! them when there is not the number the command needs.

use crate::cli::{FindArgs, QuotesArg};
use crate::output::{Failure, to_json};
use serde_json::Value;
use stetdoc_core::{Document, Edit, ErrorCode, Occurrence, Occurrences, Quotes, Scope, StetError};

/// How many places a refusal lists for one text.
const LISTED: usize = 20;

/// How many places a command needs.
#[derive(Clone, Copy)]
pub enum Wanted {
    /// Exactly one.
    One,
    /// Every one, and at least one.
    All,
    /// Every one, and exactly this many.
    Exactly(u32),
}

/// The scope the flags give.
pub fn scope(args: &FindArgs) -> Result<Scope, Failure> {
    if let Some(range) = &args.range {
        let (from, to) = match range.split_once("..") {
            Some((from, to)) => (from.trim().to_string(), Some(to.trim().to_string())),
            None => (range.trim().to_string(), None),
        };
        if from.is_empty() || to.as_deref() == Some("") {
            return Err(Failure::usage(
                format!("--range {range} is not a range of refs."),
                "Write --range p40..p60, or --range p40 for p40 to the end of its story.",
            ));
        }
        return Ok(Scope::Range { from, to });
    }
    if let Some(clause) = &args.clause {
        return Ok(Scope::Clause(clause.clone()));
    }
    Ok(Scope::Auto)
}

/// The scope in words, for messages: ` in clause 11.1`, ` in p40..p60`, or ` in the document`.
pub fn place_words(scope: &Scope) -> String {
    match scope {
        Scope::Range { from, to: Some(to) } => format!(" in {from}..{to}"),
        Scope::Range { from, to: None } => format!(" in {from} to the end of its story"),
        Scope::Clause(clause) => format!(" in clause {clause}"),
        _ => " in the document".to_string(),
    }
}

/// One place on one line: its ref, its clause or the heading it is under, and the snippet.
pub fn line(hit: &Occurrence) -> String {
    format!("  {}{} {}", hit.block, where_words(hit), hit.snippet)
}

/// Where a place is, after its ref: ` (clause 11.1)`, ` (under "RECITALS")`, or nothing.
fn where_words(hit: &Occurrence) -> String {
    match (&hit.clause, &hit.heading) {
        (Some(clause), _) => format!(" (clause {clause})"),
        (None, Some(heading)) => format!(" (under {heading:?})"),
        (None, None) => String::new(),
    }
}

/// The places, a line each, up to `LISTED`, then how many more there are.
fn lines(hits: &[Occurrence]) -> String {
    let mut out: Vec<String> = hits.iter().take(LISTED).map(line).collect();
    if hits.len() > LISTED {
        out.push(format!("  and {} more", hits.len() - LISTED));
    }
    out.join("\n")
}

/// Why the places found are not the ones a command needs, with its code, or `None` when they are.
pub fn problem(
    found: &Occurrences,
    wanted: Wanted,
    scope: &Scope,
    label: &str,
) -> Option<(ErrorCode, String)> {
    let count = found.hits.len();
    let needle = &found.needle;
    let place = place_words(scope);
    if count == 0 {
        let mut text = format!("{label}{needle:?} is not{place} as it reads now.");
        if !found.deleted.is_empty() {
            text.push_str(&format!(
                " It is only in text that a tracked deletion removes, in:\n{}",
                lines(&found.deleted)
            ));
        }
        return Some((ErrorCode::NotFound, text));
    }
    match wanted {
        Wanted::One if count > 1 => Some((
            ErrorCode::AmbiguousMatch,
            format!(
                "{label}{needle:?} is{place} {count} times, and it must be there once:\n{}",
                lines(&found.hits)
            ),
        )),
        Wanted::Exactly(n) if count != n as usize => Some((
            ErrorCode::CountMismatch,
            format!(
                "{label}{needle:?} is{place} {count} time{}, not {n}:\n{}",
                if count == 1 { "" } else { "s" },
                lines(&found.hits)
            ),
        )),
        _ => None,
    }
}

/// The refusal for one or more problems, with the code of the first and a next step for it. `what` names the text, such
/// as `OLD` or `the anchor`, and `all` says whether the command takes --all and --expect.
pub fn refuse(problems: Vec<(ErrorCode, String)>, what: &str, all: bool) -> Failure {
    let code = problems
        .first()
        .map_or(ErrorCode::NotFound, |(code, _)| *code);
    let next = match code {
        ErrorCode::AmbiguousMatch if all => format!(
            "Nothing was applied. Make {what} longer so it matches once, limit the search with --range or --clause, or pass --all to change every one."
        ),
        ErrorCode::AmbiguousMatch => format!(
            "Nothing was applied. Make {what} longer so it matches once, or limit the search with --range or --clause."
        ),
        ErrorCode::CountMismatch => "Nothing was applied. Check the places listed, then pass the count they show with --expect, or limit the search with --range or --clause.".to_string(),
        _ => format!(
            "Nothing was applied. Write {what} exactly as the document reads now (stetdoc view FILE --find WORD shows it), or widen --range or --clause."
        ),
    };
    let text: Vec<String> = problems.into_iter().map(|(_, text)| text).collect();
    Failure::Core(StetError::new(code, text.join("\n")).with_next(next))
}

/// Where the places of one text are, on one line: `once, at p12 (clause 11.1)` or `14 times, in 9 blocks: p3, p5, ...`.
pub fn summary(found: &Occurrences) -> String {
    match found.hits.as_slice() {
        [one] => format!("once, at {}{}", one.block, where_words(one)),
        hits => {
            let mut blocks: Vec<&str> = Vec::new();
            for hit in hits {
                if !blocks.contains(&hit.block.as_str()) {
                    blocks.push(&hit.block);
                }
            }
            let shown: Vec<&str> = blocks.iter().take(LISTED).copied().collect();
            let more = blocks.len().saturating_sub(shown.len());
            format!(
                "{} times, in {} block{}: {}{}",
                hits.len(),
                blocks.len(),
                if blocks.len() == 1 { "" } else { "s" },
                shown.join(", "),
                if more > 0 {
                    format!(" and {more} more")
                } else {
                    String::new()
                }
            )
        }
    }
}

/// Finds a text in the scope.
pub fn find(doc: &Document, needle: &str, scope: &Scope) -> Result<Occurrences, Failure> {
    Ok(doc.occurrences(needle, scope)?)
}

/// The edit's options that the flags give.
pub fn shape(edit: Edit, args: &FindArgs) -> Edit {
    let edit = edit.quotes(match args.quotes {
        QuotesArg::Match => Quotes::Match,
        QuotesArg::AsTyped => Quotes::AsTyped,
    });
    if args.dry_run { edit.dry_run() } else { edit }
}

/// The places as JSON.
pub fn hits_json(found: &Occurrences) -> Result<Value, Failure> {
    to_json(&found.hits)
}
