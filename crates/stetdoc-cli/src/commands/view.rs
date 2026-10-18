//! `stetdoc view`: `open` or `resume`, then `view`.

use crate::cli::{ModeArg, StoryArg, ViewArgs};
use crate::output::{Failure, Outcome};
use crate::target;
use stetdoc_core::{
    Budget, CommentFilter, Cursor, Mode, RevisionFilter, Scope, StoryName, ViewSpec,
};

/// Runs `view`.
pub fn run(args: &ViewArgs) -> Result<Outcome, Failure> {
    let read = target::open_read(&args.input)?;
    let doc = &read.doc;
    let view = doc.view(&spec(args)?)?;
    let mut text = view.text.clone();
    if let Some(next) = &view.next {
        text.push_str(&format!("\n-- more: {}\n", next_command(next.as_str())));
    }
    Ok(Outcome::of(&text, &view)?.with_notes(read.notes))
}

/// The `ViewSpec` the flags ask for.
fn spec(args: &ViewArgs) -> Result<ViewSpec, Failure> {
    let scope = if args.outline {
        Scope::Outline { depth: args.depth }
    } else if let Some(range) = &args.range {
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
        Scope::Range { from, to }
    } else if let Some(clause) = &args.clause {
        Scope::Clause(clause.clone())
    } else if let Some(needle) = &args.find {
        Scope::Find {
            needle: needle.clone(),
            context: args.context,
        }
    } else if args.revisions {
        Scope::Revisions(RevisionFilter {
            author: args.author.clone(),
            kind: args.kind.clone(),
        })
    } else if args.comments {
        let resolved = match (args.resolved, args.open) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        };
        Scope::Comments(CommentFilter {
            author: args.author.clone(),
            resolved,
        })
    } else if let Some(story) = args.story {
        Scope::Story(match story {
            StoryArg::Body => StoryName::Body,
            StoryArg::Notes => StoryName::Notes,
            StoryArg::Headers => StoryName::Headers,
            StoryArg::Comments => StoryName::Comments,
        })
    } else {
        Scope::Auto
    };
    if args.author.is_some() && !args.revisions && !args.comments {
        return Err(Failure::usage(
            "--author filters --revisions or --comments, and neither was given.",
            "Add --revisions or --comments.",
        ));
    }
    Ok(ViewSpec {
        scope,
        mode: match args.mode {
            ModeArg::Final => Mode::Final,
            ModeArg::Original => Mode::Original,
            ModeArg::Redline => Mode::Redline,
        },
        budget: Budget(args.budget),
        after: args.after.as_ref().map(Cursor::new),
        notes: !args.no_notes,
        ..ViewSpec::default()
    })
}

/// The command that shows the next page: this one, with `--after` set to the cursor.
fn next_command(cursor: &str) -> String {
    let mut words: Vec<String> = vec!["stetdoc".to_string()];
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--after" {
            args.next();
        } else if !arg.starts_with("--after=") {
            words.push(arg);
        }
    }
    words.push("--after".to_string());
    words.push(cursor.to_string());
    words.iter().map(|w| quote(w)).collect::<Vec<_>>().join(" ")
}

/// A word as a POSIX shell reads it back.
fn quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=,@+%".contains(c));
    if plain {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}
