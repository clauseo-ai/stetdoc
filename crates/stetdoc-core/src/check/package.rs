//! The package rules: P1 to P3 of `docs/design/check.md` 3.3.
//!
//! `Tx::add_part` registers what it adds and is atomic, so these cannot fail unless it has a bug.
//! They are cheap and they run only when the transaction called it: the defence of the parts that
//! `comments` creates. A part is read through the relationships of the main part, as everywhere in
//! `check`, so a part that nothing relates is not one of them.

use super::document::{SchemaPart, listing};
use super::found::Found;
use super::keys::Key;
use super::verdict::Verdict;
use crate::api::check::FindingCode;
use crate::api::error::StetError;
use crate::state::PartsView;
use crate::wordml::RelTarget;
use std::collections::HashSet;

const CONTENT_TYPES: &str = "[Content_Types].xml";

/// The content types the package declares: the extensions that have a default, and the names that
/// have an override, in lower case.
struct Types {
    defaults: HashSet<String>,
    overrides: HashSet<String>,
}

/// The declarations of `[Content_Types].xml`, or `None` when the view has no such part.
fn types(view: &PartsView<'_>) -> Result<Option<Types>, StetError> {
    let Some(tree) = view.tree_opt(CONTENT_TYPES)? else {
        return Ok(None);
    };
    let mut out = Types {
        defaults: HashSet::new(),
        overrides: HashSet::new(),
    };
    for el in tree.root().child_elements() {
        let attr = |name: &str| el.attr("", name).map(|v| v.to_ascii_lowercase());
        match el.local_name() {
            "Default" => out.defaults.extend(attr("Extension")),
            "Override" => out.overrides.extend(attr("PartName")),
            _ => {}
        }
    }
    Ok(Some(out))
}

/// Whether a part has a content type: an override for its name, or a default for its extension.
fn has_type(types: &Types, name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let extension = lower.rsplit_once('.').map(|(_, e)| e.to_owned());
    types.overrides.contains(&format!("/{lower}"))
        || extension.is_some_and(|e| types.defaults.contains(&e))
}

/// The findings of the package rules for the parts of a document, as the view has them, each with
/// the place of its part in `parts`: a part with no content type, a relationship that names no
/// part, and a relationship id that two relationships of one part share.
pub(super) fn findings(
    view: &PartsView<'_>,
    parts: &[SchemaPart],
) -> Result<Vec<(usize, Found)>, StetError> {
    let declared = types(view)?;
    let mut out = Vec::new();
    for (at, part) in parts.iter().enumerate() {
        if declared
            .as_ref()
            .is_some_and(|types| !has_type(types, &part.name))
        {
            let found = Found::rule(FindingCode::PartWithoutContentType, &part.name, None);
            out.push((at, found));
        }
        let rels = view.rels(part.id)?;
        let mut seen: HashSet<&str> = HashSet::new();
        let mut reported: HashSet<&str> = HashSet::new();
        for rel in rels.iter() {
            let subject = format!("{}#{}", part.name, rel.id);
            if matches!(rel.target, RelTarget::Invalid) {
                let found = Found::rule(FindingCode::RelationshipTargetMissing, &subject, None);
                out.push((at, found));
            }
            if !seen.insert(&rel.id) && reported.insert(&rel.id) {
                let found = Found::rule(FindingCode::RelationshipIdDuplicate, &subject, None);
                out.push((at, found));
            }
        }
    }
    Ok(out)
}

/// The package rules of a transaction that called `add_part`: what they find now, against what
/// they found in the base. A finding the base had is about the package and not about the blocks
/// the edit changed, so it is not new and it is not carried either.
pub(super) fn judge(
    verdict: &mut Verdict,
    (before, after): (&PartsView<'_>, &PartsView<'_>),
    parts: &[SchemaPart],
) -> Result<(), StetError> {
    let old: HashSet<Key> = findings(before, &listing(before)?.parts)?
        .into_iter()
        .map(|(_, found)| found.key)
        .collect();
    for (at, found) in findings(after, parts)? {
        if !old.contains(&found.key) {
            verdict.add_new(Some(at), found);
        }
    }
    Ok(())
}
