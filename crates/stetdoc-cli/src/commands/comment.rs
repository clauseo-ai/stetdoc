//! `stetdoc comment`: comments anchored to texts, each found once (or at every place, with --all or --expect) in the
//! document as it reads now, in one edit built from `Document::occurrences` and applied with `edit(Edit::ops)` as
//! `comment` operations on the paragraphs' refs and offsets, all or nothing.

use super::apply::{Made, Where, apply};
use super::find::{self, Wanted};
use crate::cli::CommentArgs;
use crate::output::{Failure, Outcome};
use serde_json::json;
use stetdoc_core::{Batch, CommentOp, Edit, Op};

/// Runs `comment`.
pub fn run(args: &CommentArgs) -> Result<Outcome, Failure> {
    if !args.pairs.len().is_multiple_of(2) {
        return Err(Failure::usage(
            format!(
                "comment takes ANCHOR TEXT pairs, and {} texts were given, so the last anchor has no comment.",
                args.pairs.len()
            ),
            "Give the comment's text after each anchor.",
        ));
    }
    let pairs: Vec<(&String, &String)> = args
        .pairs
        .chunks(2)
        .filter_map(|p| match p {
            [anchor, text] => Some((anchor, text)),
            _ => None,
        })
        .collect();
    let several = pairs.len() > 1;
    for (i, (anchor, text)) in pairs.iter().enumerate() {
        let which = if several {
            format!(" of pair {}", i + 1)
        } else {
            String::new()
        };
        if text.trim().is_empty() {
            return Err(Failure::usage(
                format!("The comment{which} is empty."),
                "Give the comment's text after the anchor.",
            ));
        }
        if anchor.is_empty() {
            return Err(Failure::usage(
                format!("The anchor{which} is empty, and comment needs text to anchor to."),
                "Give the text to comment on as the document reads.",
            ));
        }
    }
    let at = Where {
        input: &args.input,
        output: args.find.output.as_ref(),
        in_place: args.find.in_place,
        dry_run: args.find.dry_run,
    };
    let wanted = match (args.all, args.expect) {
        (true, _) => Wanted::All,
        (false, Some(n)) => Wanted::Exactly(n),
        (false, None) => Wanted::One,
    };
    apply(&at, &args.find.stamp, |doc, stamp| {
        let scope = find::scope(&args.find)?;
        let mut problems = Vec::new();
        let mut case = false;
        let mut found = Vec::new();
        for (i, (anchor, _)) in pairs.iter().enumerate() {
            let hits = find::find(doc, anchor, &scope, &args.find)?;
            let label = if several {
                format!("pair {}: ", i + 1)
            } else {
                String::new()
            };
            if let Some((code, mut text)) = find::problem(&hits, wanted, &scope, &label) {
                if let Some(hint) = find::case_hint(doc, &hits, &scope, &args.find, &label)? {
                    case = true;
                    text.push('\n');
                    text.push_str(&hint);
                }
                problems.push((code, text));
            }
            found.push(hits);
        }
        if !problems.is_empty() {
            let example = find::example(&found);
            return Err(find::refuse(
                problems,
                "the anchor",
                "comment on every one",
                case,
                example.as_deref(),
            ));
        }
        // One comment for each place of each anchor. A comment adds no text, so no place moves another's offsets.
        let mut ops: Vec<Op> = Vec::new();
        for ((_, text), hits) in pairs.iter().zip(&found) {
            ops.extend(hits.hits.iter().map(|hit| {
                Op::Comment(CommentOp {
                    r#ref: hit.block.clone(),
                    text: (*text).clone(),
                    initials: None,
                    find: None,
                    occurrence: None,
                    start: Some(hit.start),
                    end: Some(hit.end),
                })
            }));
        }
        let edit = find::shape(Edit::ops(Batch::new(doc.base(), ops), stamp), &args.find);
        let head: Vec<String> = pairs
            .iter()
            .zip(&found)
            .map(|((anchor, _), hits)| format!("comment on {anchor:?}: {}", find::summary(hits)))
            .collect();
        let json_found = pairs
            .iter()
            .zip(&found)
            .map(|((anchor, _), hits)| {
                Ok(json!({ "anchor": anchor, "count": hits.hits.len(), "hits": find::hits_json(hits)? }))
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        Ok(Made {
            edit,
            head: Some(head.join("\n")),
            extra: vec![("found", serde_json::Value::Array(json_found))],
        })
    })
}
