//! `stetdoc comment`: a comment anchored to a text, found once in the document as it reads now, in one edit built from
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
        dry_run_needs_output: false,
    };
    apply(&at, &args.find.stamp, |doc, stamp| {
        let scope = find::scope(&args.find)?;
        let found = find::find(doc, &args.anchor, &scope)?;
        if let Some(problem) = find::problem(&found, Wanted::One, &scope, "") {
            return Err(find::refuse(vec![problem], "the anchor", false));
        }
        let hit = found
            .hits
            .first()
            .ok_or_else(|| find::refuse(Vec::new(), "the anchor", false))?;
        let op = Op::Comment(CommentOp {
            r#ref: hit.block.clone(),
            text: args.text.clone(),
            initials: None,
            find: None,
            occurrence: None,
            start: Some(hit.start),
            end: Some(hit.end),
        });
        let edit = find::shape(
            Edit::ops(Batch::new(doc.base(), vec![op]), stamp),
            &args.find,
        );
        Ok(Made {
            edit,
            head: Some(format!(
                "comment on {:?}: {}",
                args.anchor,
                find::summary(&found)
            )),
            extra: vec![(
                "found",
                json!([{ "anchor": args.anchor, "count": 1, "hits": find::hits_json(&found)? }]),
            )],
        })
    })
}
