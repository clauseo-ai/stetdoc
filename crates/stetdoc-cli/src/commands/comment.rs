//! `stetdoc comment`: a comment anchored to a text, found once (or at every place, with --all or --expect) in the document as it reads now, in one edit built from
//! `Document::occurrences` and applied with `edit(Edit::ops)` as a `comment` operation on the paragraph's ref and offsets.

use super::apply::{Made, Where, apply};
use super::find::{self, Wanted};
use crate::cli::CommentArgs;
use crate::output::{Failure, Outcome};
use serde_json::json;
use stetdoc_core::{Batch, CommentOp, Edit, Op};

/// Runs `comment`.
pub fn run(args: &CommentArgs) -> Result<Outcome, Failure> {
    if args.text.trim().is_empty() {
        return Err(Failure::usage(
            "The comment is empty.",
            "Give the comment's text after the anchor.",
        ));
    }
    if args.anchor.is_empty() {
        return Err(Failure::usage(
            "The anchor is empty, and comment needs text to anchor to.",
            "Give the text to comment on as the document reads.",
        ));
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
        let found = find::find(doc, &args.anchor, &scope, &args.find)?;
        if let Some((code, mut text)) = find::problem(&found, wanted, &scope, "") {
            let hint = find::case_hint(doc, &found, &scope, &args.find, "")?;
            if let Some(hint) = &hint {
                text.push('\n');
                text.push_str(hint);
            }
            return Err(find::refuse(
                vec![(code, text)],
                "the anchor",
                "comment on every one",
                hint.is_some(),
            ));
        }
        if found.hits.is_empty() {
            return Err(find::refuse(
                Vec::new(),
                "the anchor",
                "comment on every one",
                false,
            ));
        }
        // One comment for each place, in document order. A comment adds no text, so no place moves another's offsets.
        let ops: Vec<Op> = found
            .hits
            .iter()
            .map(|hit| {
                Op::Comment(CommentOp {
                    r#ref: hit.block.clone(),
                    text: args.text.clone(),
                    initials: None,
                    find: None,
                    occurrence: None,
                    start: Some(hit.start),
                    end: Some(hit.end),
                })
            })
            .collect();
        let edit = find::shape(Edit::ops(Batch::new(doc.base(), ops), stamp), &args.find);
        Ok(Made {
            edit,
            head: Some(format!(
                "comment on {:?}: {}",
                args.anchor,
                find::summary(&found)
            )),
            extra: vec![(
                "found",
                json!([{ "anchor": args.anchor, "count": found.hits.len(), "hits": find::hits_json(&found)? }]),
            )],
        })
    })
}
