//! `stetdoc replace`: each OLD, found in the document as it reads now, becomes NEW, as tracked changes, in one edit built
//! from `Document::occurrences` and applied with `edit(Edit::ops)`, all or nothing.
//!
//! The document is read and written in one process, so the batch names the base it was found at and nothing can go stale
//! between finding and editing. Each place becomes a `replace` operation (a `delete` when NEW is empty) on its paragraph's
//! ref, by its offsets in the Final text. The operations on one paragraph go from its end to its start, so that an earlier
//! one never moves the offsets of a later one.

use super::apply::{Made, Where, apply};
use super::find::{self, Wanted};
use crate::cli::ReplaceArgs;
use crate::output::{Failure, Outcome};
use serde_json::json;
use stetdoc_core::{Batch, DeleteOp, Document, Edit, Op, ReplaceOp, Stamp};

/// Runs `replace`.
pub fn run(args: &ReplaceArgs) -> Result<Outcome, Failure> {
    if !args.pairs.len().is_multiple_of(2) {
        return Err(Failure::usage(
            format!(
                "replace takes OLD NEW pairs, and {} texts were given, so the last OLD has no NEW.",
                args.pairs.len()
            ),
            "Give a NEW after each OLD. An empty NEW (\"\") deletes OLD.",
        ));
    }
    let at = Where {
        input: &args.input,
        output: args.find.output.as_ref(),
        in_place: args.find.in_place,
        dry_run: args.find.dry_run,
        dry_run_needs_output: false,
    };
    apply(&at, &args.find.stamp, |doc, stamp| make(args, doc, stamp))
}

fn make(args: &ReplaceArgs, doc: &Document, stamp: Stamp) -> Result<Made, Failure> {
    let scope = find::scope(&args.find)?;
    let wanted = match (args.all, args.expect) {
        (true, _) => Wanted::All,
        (false, Some(n)) => Wanted::Exactly(n),
        (false, None) => Wanted::One,
    };
    let pairs: Vec<(&String, &String)> = args
        .pairs
        .chunks(2)
        .filter_map(|p| match p {
            [old, new] => Some((old, new)),
            _ => None,
        })
        .collect();
    let several = pairs.len() > 1;
    let mut problems = Vec::new();
    let mut found = Vec::new();
    for (i, (old, new)) in pairs.iter().enumerate() {
        if old.is_empty() {
            return Err(Failure::usage(
                format!(
                    "OLD of pair {} is empty, and replace needs text to find.",
                    i + 1
                ),
                "Give the text to replace as the document reads. To add text, replace a word next to it with that word and the new text.",
            ));
        }
        let hits = find::find(doc, old, &scope)?;
        let label = if several {
            format!("pair {}: ", i + 1)
        } else {
            String::new()
        };
        if let Some(problem) = find::problem(&hits, wanted, &scope, &label) {
            problems.push(problem);
        }
        found.push((i, (*old).clone(), (*new).clone(), hits));
    }
    if !problems.is_empty() {
        return Err(find::refuse(problems, "OLD", true));
    }
    // Every place, grouped by paragraph in the order the paragraphs were first found, each group from its end to its start.
    let mut places: Vec<(String, u32, u32, usize)> = Vec::new();
    for (i, _, _, hits) in &found {
        for hit in &hits.hits {
            places.push((hit.block.clone(), hit.start, hit.end, *i));
        }
    }
    let mut order: Vec<String> = Vec::new();
    for (block, ..) in &places {
        if !order.contains(block) {
            order.push(block.clone());
        }
    }
    places.sort_by(|a, b| {
        let at = |x: &String| order.iter().position(|o| o == x);
        at(&a.0).cmp(&at(&b.0)).then(b.1.cmp(&a.1))
    });
    for pair in places.windows(2) {
        if let [a, b] = pair
            && a.0 == b.0
            && b.2 > a.1
        {
            return Err(Failure::usage(
                format!(
                    "pairs {} and {} overlap in {}, and one text cannot be replaced twice.",
                    b.3.min(a.3) + 1,
                    b.3.max(a.3) + 1,
                    a.0
                ),
                "Give one pair for the text they share.",
            ));
        }
    }
    let ops: Vec<Op> = places
        .iter()
        .map(|(block, start, end, i)| {
            let new = found
                .get(*i)
                .map(|(_, _, new, _)| new.clone())
                .unwrap_or_default();
            if new.is_empty() {
                Op::Delete(DeleteOp {
                    r#ref: block.clone(),
                    to: None,
                    find: None,
                    occurrence: None,
                    start: Some(*start),
                    end: Some(*end),
                })
            } else {
                Op::Replace(ReplaceOp {
                    r#ref: block.clone(),
                    with: new,
                    find: None,
                    occurrence: None,
                    start: Some(*start),
                    end: Some(*end),
                })
            }
        })
        .collect();
    let edit = find::shape(Edit::ops(Batch::new(doc.base(), ops), stamp), &args.find);
    let head: Vec<String> = found
        .iter()
        .map(|(_, old, new, hits)| {
            if new.is_empty() {
                format!("delete {old:?}: {}", find::summary(hits))
            } else {
                format!("replace {old:?} with {new:?}: {}", find::summary(hits))
            }
        })
        .collect();
    let json_pairs = found
        .iter()
        .map(|(_, old, new, hits)| {
            Ok(json!({
                "old": old,
                "new": new,
                "count": hits.hits.len(),
                "hits": find::hits_json(hits)?,
            }))
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    Ok(Made {
        edit,
        head: Some(head.join("\n")),
        extra: vec![("found", serde_json::Value::Array(json_pairs))],
    })
}
