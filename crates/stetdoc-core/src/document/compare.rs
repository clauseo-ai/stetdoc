//! The public `compare` entry (design `document-and-ops.md` 1.1, `compare.md` 2.2 and 7.7).
//!
//! `compare` opens both packages and accepts their revisions (`compare::prepare`), writes the redline on an open transaction of
//! the revised state (`compare::redline`, which fails closed: a call that broke after writing is `internal`, and no transaction it
//! cannot vouch for comes back), runs `check::edit` on that transaction, commits it, and builds the receipt. The order is an
//! edit's with a longer first step.
//!
//! What `check` is told: the origins of the compiler's calls, as for an edit (`declared::build`); every block of the redline that
//! received content from the original, with the keys of its block there (`check::unit_keys` on the original's view); and every
//! style or numbering definition and part that the import added, with the keys of its element or whole part there
//! (`check::part_keys`). So a problem the original's content already had is carried into the redline, and one that the redline
//! adds is refused, as `validation_failed`.
//!
//! The redline is a `Document` whose original is the revised input as given, so `save` writes the redline and a working copy keeps
//! both. The receipt has the revisions by author (one author, the stamp's), the moves as refs, compare's warnings, a warning for
//! each block compare could not redline (`compare_not_redlined`) or that rejecting does not give back (`compare_not_restored`),
//! and what `check` verified. Its operations are empty.

use super::declared;
use super::receipt_text;
use super::{Document, Original, check_format};
use crate::api::error::{ErrorCode, StetError};
use crate::api::options::CompareOptions;
use crate::api::receipt::{AuthorRevisions, MoveEntry, Receipt, Validation};
use crate::api::stamp::Stamp;
use crate::api::view::{RevisionFilter, Warning, WarningCode};
use crate::check::{self, Declared};
use crate::compare::{self, CompareReceipt, Exclusion, Prepared, Site};
use crate::revisions::stamp::RevStamp;
use crate::schema::is_unit_boundary;
use crate::state::{PartsView, Tx};
use crate::wordml::BlockKey;
use crate::xml::{NodeId, SpanTree};

/// A redline and the receipt that describes it.
pub struct Compared {
    /// The redline, built on the revised package.
    pub document: Document,
    /// The revisions by author and the warnings. Its operations are empty.
    pub receipt: Receipt,
}

/// Compares two packages and returns a redline of the differences: accepting every revision gives the revised document, and
/// rejecting every revision gives the original, apart from the blocks the receipt's warnings name.
pub fn compare(
    original: Vec<u8>,
    revised: Vec<u8>,
    options: &CompareOptions,
) -> Result<Compared, StetError> {
    check_format(&options.format)?;
    let stamp = RevStamp::new(&options.stamp.author, &options.stamp.date)?;
    let prepared = compare::prepare(original, revised, &options.limits, &stamp)?;
    let compare::Redline { tx, receipt: rc } = compare::redline(&prepared, &stamp)?;
    let told = declared_of(&prepared, &tx, &rc)?;
    let verified = check::edit_located(&tx, &told.declared).map_err(|refusal| refusal.error)?;
    let (next, _committed) = tx.commit()?;
    let document = Document {
        state: next,
        limits: options.limits.clone(),
        format: options.format.clone(),
        // The revised input as given: what the redline was built on, and what a working copy keeps beside it.
        original: Original::Kept(prepared.revised.opened_source()),
    };
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let validation = Validation {
        parts: count(verified.checked.parts),
        units: count(verified.checked.validated),
        carried: verified.carried,
        carried_total: count(verified.carried_total),
        notes: verified.notes,
    };
    let receipt = receipt_of(&document, &rc, &told.merged, validation, &stamp)?;
    Ok(Compared { document, receipt })
}

/// True if the part has a schema, and so blocks whose keys `check` can compute.
fn in_schema(schema: &[std::sync::Arc<str>], part: &str) -> bool {
    schema.iter().any(|name| **name == *part)
}

/// What `check` is told about a redline, and what the receipt says of it.
pub(super) struct Told {
    /// The origins of the calls, and the foreign origins of everything that came from the original.
    pub(super) declared: Declared,
    /// The blocks of the redline whose paragraphs and tables did not pair one to one with those of their block in the original,
    /// so that each was given the keys of all of them (`compare_keys_merged`).
    pub(super) merged: Vec<BlockKey>,
}

/// What `check` is told about the redline: the origins of the calls, and the foreign origins of everything that came from the
/// original, each with the keys it had there.
pub(super) fn declared_of(
    prepared: &Prepared,
    tx: &Tx<'_>,
    rc: &CompareReceipt,
) -> Result<Told, StetError> {
    let (before, after) = (tx.base().parts(), tx.parts());
    let mut declared = declared::build(before, after, &rc.declared.origins)?;
    let ours = check::schema_parts(&tx.parts())?;
    let theirs: PartsView<'_> = prepared.original.parts();
    let their_schema = check::schema_parts(&theirs)?;
    let after = tx.parts();
    let mut merged = Vec::new();
    for (block, source) in &rc.declared.imports {
        if in_schema(&ours, block.part()) && in_schema(&their_schema, source.part()) {
            let (units, paired) = imported_units(&theirs, &after, block, source)?;
            declared.foreign.extend(units);
            if !paired {
                merged.push(block.clone());
            }
        }
    }
    for def in &rc.declared.definitions {
        if !in_schema(&ours, &def.part) || !in_schema(&their_schema, &def.source_part) {
            continue;
        }
        match (def.node, def.source_node) {
            (Some(node), Some(source)) => {
                let keys =
                    check::unit_keys(&theirs, &BlockKey::new(def.source_part.clone(), source))?;
                declared
                    .foreign
                    .push((BlockKey::new(def.part.clone(), node), keys));
            }
            (None, _) => {
                let keys = check::part_keys(&theirs, &def.source_part)?;
                declared.parts.push((def.part.clone(), keys));
            }
            (Some(_), None) => {}
        }
    }
    Ok(Told { declared, merged })
}

/// The units at or below `node`, in document order: the block itself, and the paragraphs and tables inside it, each a unit of
/// its own for `check`.
fn unit_roots(tree: &SpanTree, node: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n == node || is_unit_boundary(tree, n) {
            out.push(n);
        }
        let kids: Vec<NodeId> = tree.node_ref(n).child_elements().map(|c| c.id()).collect();
        stack.extend(kids.into_iter().rev());
    }
    out
}

/// The foreign origins of one block of the redline that holds content of the original: the block and each paragraph or table
/// inside it (a table's cells hold units of their own), each with the keys of its unit in the original. The import keeps the
/// paragraphs and tables of a block in their order, so the units pair one to one; when they do not (a copy that left something
/// out), every unit of the copy gets the keys of every unit of the source, which is all the copy can have come from, and the
/// second value is false so that the receipt says so.
fn imported_units(
    theirs: &PartsView<'_>,
    ours: &PartsView<'_>,
    block: &BlockKey,
    source: &BlockKey,
) -> Result<(Vec<(BlockKey, check::KeySet)>, bool), StetError> {
    let internal = || {
        StetError::new(
            ErrorCode::Internal,
            "internal: the part of an imported block is not in the document",
        )
    };
    let their_tree = theirs.tree(theirs.id(source.part()).ok_or_else(internal)?)?;
    let our_tree = ours.tree(ours.id(block.part()).ok_or_else(internal)?)?;
    if !our_tree.node_ref(block.node()).is_element() {
        return Ok((Vec::new(), true));
    }
    let from = unit_roots(their_tree, source.node());
    let to = unit_roots(our_tree, block.node());
    let keys_of =
        |node: NodeId| check::unit_keys(theirs, &BlockKey::new(source.part().clone(), node));
    if from.len() == to.len() {
        let paired = from
            .iter()
            .zip(&to)
            .map(|(f, t)| Ok((BlockKey::new(block.part().clone(), *t), keys_of(*f)?)))
            .collect::<Result<Vec<_>, StetError>>()?;
        return Ok((paired, true));
    }
    let mut all = check::KeySet::default();
    for f in &from {
        all.extend(&keys_of(*f)?);
    }
    let merged = to
        .into_iter()
        .map(|t| (BlockKey::new(block.part().clone(), t), all.clone()))
        .collect();
    Ok((merged, false))
}

/// The code of one of compare's warnings.
fn warning_code(code: &str) -> WarningCode {
    WarningCode::from_name(code).unwrap_or(WarningCode::ReadAnomaly)
}

/// The receipt of a comparison, built from the committed redline.
fn receipt_of(
    document: &Document,
    rc: &CompareReceipt,
    merged: &[BlockKey],
    validation: Validation,
    stamp: &RevStamp,
) -> Result<Receipt, StetError> {
    let parts = document.state.parts();
    // Every key here is a block of the redline: compare never hands over a key of the original to name.
    let shown = |key: &BlockKey| parts.ref_of(key).map(|r| r.to_string());
    let mut warnings: Vec<Warning> = Vec::new();
    let warn = |code: WarningCode, block: Option<String>, message: String| Warning {
        code,
        block,
        story: None,
        message,
        next: None,
    };
    let (in_original, in_revised) = rc.accepted;
    if in_original + in_revised > 0 {
        warnings.push(warn(
            WarningCode::CompareRevisionsAccepted,
            None,
            format!(
                "the revisions of the inputs were accepted before they were compared: {in_original} in the original and {in_revised} in the revised document."
            ),
        ));
    }
    for w in &rc.warnings {
        // Compare counts the blocks it could not redline in one warning; the receipt names each, with its ref, below.
        if w.code == "compare_not_redlined" {
            continue;
        }
        warnings.push(warn(
            warning_code(w.code),
            None,
            format!("{} ({}).", w.message, w.count),
        ));
    }
    for e in &rc.exclusions {
        warnings.push(exclusion_warning(e, &shown));
    }
    for block in merged {
        let at = shown(block);
        let message = format!(
            "{} holds a copy of the original whose paragraphs and tables did not pair one to one with the original's, so each of them may keep any problem that any of the original's had.",
            at.as_deref().unwrap_or("a block")
        );
        warnings.push(warn(WarningCode::CompareKeysMerged, at, message));
    }
    let named = |key: &BlockKey| shown(key).unwrap_or_else(|| "a block".to_string());
    let moves: Vec<MoveEntry> = rc
        .moves
        .iter()
        .map(|m| MoveEntry {
            from: named(&m.from),
            to: named(&m.to),
            text: m.text.clone(),
        })
        .collect();
    let revisions = document.revisions(&RevisionFilter::default())?;
    let mut receipt = Receipt {
        base: document.base(),
        dry_run: false,
        tracked: true,
        markup: crate::api::options::Markup::Revisions,
        stamp: Stamp::new(stamp.author(), stamp.date()),
        operations: Vec::new(),
        revisions: AuthorRevisions::group(&revisions),
        comments: Vec::new(),
        retired: Vec::new(),
        added: Vec::new(),
        added_count: 0,
        moves,
        blocks: Vec::new(),
        blocks_omitted: 0,
        warnings,
        validation,
        text: String::new(),
    };
    receipt.text = receipt_text::compose_compared(&receipt);
    Ok(receipt)
}

/// The warning for a block that compare could not redline. It names a block of the redline, the block itself or the one its copy
/// would have stood next to, or no block, and then describes the original's block by its first words.
pub(super) fn exclusion_warning(
    e: &Exclusion,
    shown: &dyn Fn(&BlockKey) -> Option<String>,
) -> Warning {
    let why = e.reason.trim_end_matches('.');
    let restored = e.kind == "not_restored";
    let code = if restored {
        WarningCode::CompareNotRestored
    } else {
        WarningCode::CompareNotRedlined
    };
    let theirs = if e.text.is_empty() {
        "a block of the original that has no text".to_string()
    } else {
        format!("the original's block that starts \"{}\"", e.text)
    };
    let (block, message) = match &e.site {
        Site::Block(key) => {
            let at = shown(key);
            let name = at.clone().unwrap_or_else(|| "a block".to_string());
            let message = if restored {
                format!(
                    "rejecting the changes does not give {name} back as the original has it: {why}."
                )
            } else {
                format!("{name} is not shown as changes: {why}.")
            };
            (at, message)
        }
        Site::Beside(key) => {
            let at = shown(key);
            let name = at.clone().unwrap_or_else(|| "a block".to_string());
            let message = if restored {
                format!(
                    "rejecting the changes does not give back {theirs}, whose copy would have stood next to {name}: {why}."
                )
            } else {
                format!("{theirs}, next to {name}, is not shown as changes: {why}.")
            };
            (at, message)
        }
        Site::Original => {
            let message = if restored {
                format!("rejecting the changes does not give back {theirs}: {why}.")
            } else {
                format!("{theirs} is not shown as changes: {why}.")
            };
            (None, message)
        }
    };
    Warning {
        code,
        block,
        story: None,
        message,
        next: None,
    }
}
