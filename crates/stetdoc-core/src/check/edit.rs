//! `check::edit`: step 3 of the commit pipeline. Of the parts a transaction changed, the units that
//! differ from the base are validated, and the edit is refused if any of them has a finding that
//! its origin did not have (`docs/design/check.md` 2.2 to 2.4).

use super::census::{Census, revision_findings};
use super::document::{SchemaPart, census_of, is_story, listing};
use super::examine::examine;
use super::found::Found;
use super::keys::{Key, KeySet, schema_code};
use super::origins::{Declared, Origins, remember};
use super::report::schema_message;
use super::rules::scan_part;
#[cfg(test)]
use super::units::all_units;
use super::units::{Changed, Layout, UnitKind, changed_units, layout_of};
use super::verdict::{Verdict, Verified};
use super::{audit, fields, package};
use crate::api::check::FindingCode;
use crate::api::error::StetError;
use crate::schema::validate_part;
use crate::state::{Dep, PartsView, Tx};
use crate::wordml::BlockKey;
use crate::xml::SpanTree;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

/// How much of the work to do. The shortcuts are exact, which a test shows by running without
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    /// Find the units from the dirty flags, and drop what is the base's.
    Shortcuts,
    /// Every unit of every part that changed.
    #[cfg(test)]
    Everything,
}

/// A census of a changed story part now, and as the base had it, if the base had the part.
struct Counted {
    part: usize,
    was: Option<Arc<Census>>,
    now: Arc<Census>,
}

/// Validates the edit. The parts the transaction changed are found by comparing generations with
/// the base, and for each, the units that differ from the base are validated and compared with
/// their origins. Nothing is read in a part the transaction did not change.
pub(crate) fn edit(tx: &Tx<'_>, declared: &Declared) -> Result<Verified, StetError> {
    run(tx, declared, Mode::Shortcuts)
}

/// A refused edit: the error, and the block that holds the first new violation, if it is in a block.
#[derive(Debug)]
pub(crate) struct Refusal {
    /// What `edit` returns.
    pub(crate) error: StetError,
    /// The block, as a key of the transaction's trees. The error says it in a message, and a caller that must say which of its
    /// operations made the block asks for the block and not for the message.
    pub(crate) block: Option<BlockKey>,
}

/// As `edit`, and when the edit is refused, the block that the first new violation is in.
pub(crate) fn edit_located(tx: &Tx<'_>, declared: &Declared) -> Result<Verified, Box<Refusal>> {
    let refuse = |error: StetError, block: Option<BlockKey>| Box::new(Refusal { error, block });
    let (list, verdict) = collect(tx, declared, Mode::Shortcuts).map_err(|e| refuse(e, None))?;
    let block = verdict.first_block(&tx.parts(), &list);
    verdict
        .conclude((&tx.base().parts(), &tx.parts()), &list)
        .map_err(|e| refuse(e, block))
}

/// The codes the redline never carries on the strength of an input having them (decision 0021): the structure of tables and
/// containers, the schema's shape of an element, the package and the comments. Word repairs or refuses a file with any of them,
/// so one the redline adds is refused even when an input has the same problem elsewhere. A copy of the original's content still
/// keeps its own problems through its foreign origins, as for any import.
const NEVER_CARRIED: [FindingCode; 18] = [
    FindingCode::CellWithoutFinalParagraph,
    FindingCode::TableWithoutRow,
    FindingCode::RowWithoutCell,
    FindingCode::ContainerWithoutBlock,
    FindingCode::SchemaUnexpectedChild,
    FindingCode::SchemaMissingChild,
    FindingCode::SchemaOutOfOrder,
    FindingCode::SchemaUnexpectedText,
    FindingCode::SchemaWrongRoot,
    FindingCode::McPrefixUndeclared,
    FindingCode::RelationshipTargetMissing,
    FindingCode::RelationshipIdDuplicate,
    FindingCode::PartWithoutContentType,
    FindingCode::MarkerUnpaired,
    FindingCode::MarkerWithoutComment,
    FindingCode::ReferenceMissing,
    FindingCode::ReferenceDuplicate,
    FindingCode::CommentIdDuplicate,
];

/// `edit_located` for the redline of `compare` (decision 0021). A new violation that the origins could not carry is carried
/// when its code is not in `NEVER_CARRIED` and the whole redline has its key (the code with its detail: the element, the
/// attribute, the value) no more often than the input that has it more often: then the redline has only moved that input's
/// problem into a container of the revised document. `inputs` gives the views of the original and the revised document, each
/// with its revisions accepted, and is asked only when a violation is new.
pub(crate) fn compare_located<'v>(
    tx: &Tx<'_>,
    declared: &Declared,
    inputs: impl FnOnce() -> (PartsView<'v>, PartsView<'v>),
) -> Result<Verified, Box<Refusal>> {
    let refuse = |error: StetError, block: Option<BlockKey>| Box::new(Refusal { error, block });
    let (list, mut verdict) =
        collect(tx, declared, Mode::Shortcuts).map_err(|e| refuse(e, None))?;
    if verdict.refuses() {
        let (original, revised) = inputs();
        let keys = |parts: &PartsView<'_>| document_keys(parts).map_err(|e| refuse(e, None));
        let (a, b, now) = (keys(&original)?, keys(&revised)?, keys(&tx.parts())?);
        verdict.carry_where(&|key| {
            !NEVER_CARRIED.contains(&key.code()) && now.count(key) <= a.count(key).max(b.count(key))
        });
    }
    let block = verdict.first_block(&tx.parts(), &list);
    verdict
        .conclude_as(
            (&tx.base().parts(), &tx.parts()),
            &list,
            super::verdict::Subject::Redline,
        )
        .map_err(|e| refuse(e, block))
}

/// The keys of every part of a document that has a schema, with their counts (`part_keys` of each).
fn document_keys(parts: &PartsView<'_>) -> Result<KeySet, StetError> {
    let mut keys = KeySet::default();
    for part in listing(parts)?.parts {
        keys.extend(&super::origins::part_keys(parts, &part.name)?);
    }
    Ok(keys)
}

pub(super) fn run(tx: &Tx<'_>, declared: &Declared, mode: Mode) -> Result<Verified, StetError> {
    let (list, verdict) = collect(tx, declared, mode)?;
    verdict.conclude((&tx.base().parts(), &tx.parts()), &list)
}

/// Examines the changed parts and says what is new and what was carried.
pub(super) fn collect(
    tx: &Tx<'_>,
    declared: &Declared,
    mode: Mode,
) -> Result<(Vec<SchemaPart>, Verdict), StetError> {
    let (before, after) = (tx.base().parts(), tx.parts());
    let list = listing(&after)?.parts;
    let mut origins = Origins::new(before, after, &list, declared)?;
    let mut verdict = Verdict::default();
    let mut counted: Vec<Counted> = Vec::new();
    // what the audit reads: any part of the comments, and any story, when there is a comment to
    // read or a marker to read in one of them, and the whole package when a part was added
    let package_changed = after.generation(Dep::Package) != before.generation(Dep::Package);
    let mut audit_needed = package_changed;
    let mut story_changed = false;
    for (at, part) in list.iter().enumerate() {
        let (now, then) = (
            after.generation(Dep::Part(part.id)),
            before.generation(Dep::Part(part.id)),
        );
        if now == then {
            continue;
        }
        verdict.checked.parts += 1;
        let tree = after.tree(part.id)?;
        let in_base = before.id(&part.name).is_some();
        // A part that `compare` copied whole from another package keeps the problems it had there.
        let allowed = if in_base {
            KeySet::default()
        } else {
            origins.whole_part(part)
        };
        if in_base && now.epoch == then.epoch {
            let base_tree = before.tree(part.id)?;
            units(
                &mut verdict,
                &mut origins,
                &after,
                (at, part),
                (base_tree, tree),
                mode,
            )?;
        } else {
            whole(&mut verdict, (at, part), tree, &allowed);
        }
        if is_story(part.kind) {
            let was = if in_base {
                Some(census_of(&before, part.id, before.tree(part.id)?)?)
            } else {
                None
            };
            let now = census_of(&after, part.id, tree)?;
            census(&mut verdict, (at, part), &now, (was.as_deref(), &allowed));
            fields::judge(
                &mut verdict,
                &mut origins,
                (&before, &after),
                (at, part),
                tree,
                (&now, was.as_deref()),
            )?;
            story_changed = true;
            audit_needed |= now.has_comment_markers()
                || was.as_deref().is_some_and(Census::has_comment_markers);
            counted.push(Counted { part: at, was, now });
        } else {
            audit_needed |= audit::is_audited(part.kind);
        }
    }
    revisions(&mut verdict, &list, (&before, &after), &counted)?;
    // a story changed where there is a comment: a part may have lost its range or its reference
    if story_changed && !audit_needed {
        audit_needed = audit::has_comments(&list) || audit::has_comments(&listing(&before)?.parts);
    }
    if audit_needed {
        audit::judge(&mut verdict, (&before, &after), &list)?;
    }
    if package_changed {
        package::judge(&mut verdict, (&before, &after), &list)?;
    }
    Ok((list, verdict))
}

/// A part that is not a clone of the base's, because `add_part` made it: it has no origin, so
/// everything in it is new.
fn whole(
    verdict: &mut Verdict,
    (at, part): (usize, &SchemaPart),
    tree: &SpanTree,
    allowed: &KeySet,
) {
    let report = validate_part(part.kind, tree);
    for v in &report.violations {
        let found = Found {
            key: Key::schema(&v.key),
            code: schema_code(v.kind),
            node: Some(v.node),
            message: schema_message(tree, v),
        };
        if allowed.contains(&found.key) {
            verdict.carry(Some(at), found);
        } else {
            verdict.add_new(Some(at), found);
        }
    }
    // The instances that the report counts and does not list are new, unless their key came with the part.
    for (key, count) in report.set.iter() {
        if allowed.contains(&Key::schema(key)) {
            continue;
        }
        let listed = report.violations.iter().filter(|v| v.key == *key).count();
        verdict.new_total += (count as usize).saturating_sub(listed);
    }
    if is_story(part.kind) {
        for found in scan_part(tree) {
            if allowed.contains(&found.key) {
                verdict.carry(Some(at), found);
            } else {
                verdict.add_new(Some(at), found);
            }
        }
    }
}

/// The units of a part that is a clone of the base's: the ones that differ are validated, and
/// each is compared with its origin.
fn units(
    verdict: &mut Verdict,
    origins: &mut Origins<'_>,
    after: &PartsView<'_>,
    (at, part): (usize, &SchemaPart),
    (base_tree, tree): (&SpanTree, &SpanTree),
    mode: Mode,
) -> Result<(), StetError> {
    let changed: Changed = match mode {
        Mode::Shortcuts => changed_units(base_tree, tree, part.kind),
        #[cfg(test)]
        Mode::Everything => Changed {
            units: all_units(tree, part.kind),
            ..Changed::default()
        },
    };
    verdict.checked.dirty += changed.dirty;
    verdict.checked.equal += changed.equal;
    verdict.checked.shells_skipped += changed.shells_skipped;
    for unit in changed.units {
        let examined = examine(part.kind, tree, unit);
        remember(after, part, unit, &examined.keys);
        verdict.checked.validated += 1;
        // The frame of a story part holds what no block does, the body's last section properties among
        // them, so the second pass of the scope guard reads it too.
        let story_frame = unit.kind == UnitKind::Frame && layout_of(part.kind) == Layout::Story;
        if story_frame || matches!(unit.kind, UnitKind::Paragraph | UnitKind::Table) {
            verdict
                .changed
                .push(BlockKey::new(part.name.clone(), unit.root));
        }
        let allowed = origins.of(part, unit)?;
        let new: KeySet = examined
            .keys
            .iter()
            .filter(|k| !allowed.contains(k))
            .cloned()
            .collect();
        for key in new.iter() {
            let listed = examined.found.iter().filter(|f| f.key == *key).count();
            verdict.new_total += examined.instances(key).saturating_sub(listed);
        }
        // A counted key that the origin has is new in the instances past the origin's count: the
        // later ones in the unit, since which copy is the extra one cannot be told.
        let mut seen: BTreeMap<&Key, usize> = BTreeMap::new();
        let extra: Vec<bool> = examined
            .found
            .iter()
            .map(|f| {
                if new.contains(&f.key) || !f.key.is_counted() {
                    return false;
                }
                let n = seen.entry(&f.key).or_insert(0);
                *n += 1;
                *n > allowed.count(&f.key)
            })
            .collect();
        for key in examined
            .keys
            .iter()
            .filter(|k| k.is_counted() && !new.contains(k))
        {
            let excess = examined.instances(key).saturating_sub(allowed.count(key));
            let listed = seen
                .get(key)
                .map_or(0, |n| n.saturating_sub(allowed.count(key)));
            verdict.new_total += excess.saturating_sub(listed);
        }
        for (found, extra) in examined.found.into_iter().zip(extra) {
            if extra || new.contains(&found.key) {
                verdict.add_new(Some(at), found);
            } else {
                verdict.carry(Some(at), found);
            }
        }
    }
    Ok(())
}

/// The findings a changed story part makes by itself: bookmark ids that repeat, ranges that do not
/// pair. A finding the part already made is carried.
fn census(
    verdict: &mut Verdict,
    (at, part): (usize, &SchemaPart),
    now: &Census,
    (was, allowed): (Option<&Census>, &KeySet),
) {
    let old: HashSet<Key> = was
        .map(|c| c.part_findings(&part.name))
        .unwrap_or_default()
        .into_iter()
        .map(|f| f.key)
        .collect();
    for found in now.part_findings(&part.name) {
        if old.contains(&found.key) || allowed.contains(&found.key) {
            verdict.carry(Some(at), found);
        } else {
            verdict.add_new(Some(at), found);
        }
    }
}

/// Revision ids are unique across the story parts, so a part cannot be judged alone. They are read
/// only when the ids of a changed part are not the ones it had.
fn revisions(
    verdict: &mut Verdict,
    parts: &[SchemaPart],
    (before, after): (&PartsView<'_>, &PartsView<'_>),
    counted: &[Counted],
) -> Result<(), StetError> {
    let moved = counted.iter().any(|c| {
        c.was
            .as_ref()
            .is_none_or(|w| w.revisions() != c.now.revisions())
    });
    if !moved {
        return Ok(());
    }
    let (mut now_all, mut was_all): (Vec<Arc<Census>>, Vec<Arc<Census>>) = (Vec::new(), Vec::new());
    for (at, part) in parts.iter().enumerate().filter(|(_, p)| is_story(p.kind)) {
        match counted.iter().find(|c| c.part == at) {
            Some(c) => {
                now_all.push(c.now.clone());
                was_all.extend(c.was.clone());
            }
            None => {
                now_all.push(census_of(after, part.id, after.tree(part.id)?)?);
                if before.id(&part.name).is_some() {
                    was_all.push(census_of(before, part.id, before.tree(part.id)?)?);
                }
            }
        }
    }
    let refs =
        |all: &[Arc<Census>]| revision_findings(&all.iter().map(|c| &**c).collect::<Vec<_>>());
    let was: HashSet<Key> = refs(&was_all).into_iter().map(|f| f.key).collect();
    for found in refs(&now_all) {
        if was.contains(&found.key) {
            verdict.carry(None, found);
        } else {
            verdict.add_new(None, found);
        }
    }
    Ok(())
}
