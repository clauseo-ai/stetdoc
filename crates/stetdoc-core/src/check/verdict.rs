//! What `check::edit` found, and how it says so: the refusal, or what was verified.

use super::document::{SchemaPart, block_ref};
use super::found::Found;
use crate::api::check::{Class, Finding};
use crate::api::error::{ErrorCode, StetError};
use crate::schema::{is_unit_boundary, unit_root};
use crate::state::PartsView;
use crate::wordml::BlockKey;
use crate::xml::NodeId;

/// How many problems that were already there a `Verified` lists.
const CARRIED_LISTED: usize = 10;

/// How many findings the message of a refusal names.
const NAMED: usize = 3;

/// What `edit` looked at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Checked {
    /// Parts the transaction changed.
    pub(crate) parts: usize,
    /// Paragraphs, notes, top-level elements and tables that the dirty flags led to.
    pub(crate) dirty: usize,
    /// Of those, the ones that were the base's element for element.
    pub(crate) equal: usize,
    /// Tables and frames whose own shell did not change.
    pub(crate) shells_skipped: usize,
    /// Units that were validated.
    pub(crate) validated: usize,
}

/// What `edit` verified when it accepted an edit.
#[derive(Debug)]
pub(crate) struct Verified {
    /// What was looked at.
    pub(crate) checked: Checked,
    /// Problems that the changed units already had, located, at most ten.
    pub(crate) carried: Vec<Finding>,
    /// How many there are.
    pub(crate) carried_total: usize,
    /// Notes that the edit adds. They block nothing.
    pub(crate) notes: Vec<Finding>,
    /// The paragraphs and tables the edit changed, found from the trees and not from anything that
    /// was declared: those that are new, were moved, or are not what the base has, and the tables
    /// whose own shell changed. The frame of a story part (its root, the body, the body's last section
    /// properties and what lies between blocks) is here too, keyed by the part's root element, when it is
    /// not what the base has. In part order, and in document order within a part.
    pub(crate) changed: Vec<BlockKey>,
}

/// A finding of the edit, and the place of its part in the list of parts. The finding of a rule
/// that spans parts has none.
struct Reported {
    part: Option<usize>,
    found: Found,
}

/// What the units, the parts and the rules over the document said, as the loop collects it.
#[derive(Default)]
pub(super) struct Verdict {
    pub(super) checked: Checked,
    new: Vec<Reported>,
    /// How many violations are new. The list has the ones that could be located.
    pub(super) new_total: usize,
    carried: Vec<Reported>,
    carried_total: usize,
    notes: Vec<Reported>,
    pub(super) changed: Vec<BlockKey>,
}

impl Verdict {
    /// A finding that the edit adds: a violation, or a note when its code is one (`empty_properties`), which blocks nothing.
    pub(super) fn add_new(&mut self, part: Option<usize>, found: Found) {
        if found.code.class() == Class::Note {
            self.note(part, found);
            return;
        }
        self.new_total += 1;
        self.new.push(Reported { part, found });
    }

    /// A problem that the edit's origin had. A note is legal, so it is no problem and is not carried.
    pub(super) fn carry(&mut self, part: Option<usize>, found: Found) {
        if found.code.class() == Class::Note {
            return;
        }
        self.carried_total += 1;
        self.carried.push(Reported { part, found });
    }

    /// A note that the edit adds: legal, and worth saying.
    pub(super) fn note(&mut self, part: Option<usize>, found: Found) {
        self.notes.push(Reported { part, found });
    }

    /// How many violations are new.
    #[cfg(test)]
    pub(super) fn new_total(&self) -> usize {
        self.new_total
    }

    /// The codes of the new violations, sorted, each once.
    #[cfg(test)]
    pub(super) fn new_codes(&self) -> Vec<crate::api::check::FindingCode> {
        let mut codes: Vec<_> = self.new.iter().map(|r| r.found.code).collect();
        codes.sort();
        codes.dedup();
        codes
    }

    /// The block that the first new violation is in, as a key of the transaction's trees: `None` when there is no new
    /// violation, or it is not in a paragraph or a table (a part, a frame, a note).
    pub(super) fn first_block(
        &self,
        after: &PartsView<'_>,
        parts: &[SchemaPart],
    ) -> Option<BlockKey> {
        let reported = self.new.first()?;
        let part = parts.get(reported.part?)?;
        let tree = after.tree(part.id).ok()?;
        let unit = unit_root(tree, reported.found.node?);
        is_unit_boundary(tree, unit).then(|| BlockKey::new(part.name.clone(), unit))
    }

    /// The verdict: `validation_failed` naming the first findings, or what was verified. `views`
    /// are the base's and the transaction's.
    pub(super) fn conclude(
        self,
        views: (&PartsView<'_>, &PartsView<'_>),
        parts: &[SchemaPart],
    ) -> Result<Verified, StetError> {
        let after = views.1;
        if self.new_total > 0 {
            let named: Vec<String> = self
                .new
                .iter()
                .take(NAMED)
                .map(|r| {
                    let f = finding(after, parts, r);
                    let part = r.part.and_then(|at| parts.get(at));
                    format!(
                        "{}: {}",
                        place(views, part, r.found.node, f.block),
                        sentence(&f.message)
                    )
                })
                .collect();
            let total = self.new_total;
            let more = total.saturating_sub(named.len());
            let tail = if more > 0 {
                format!(" And {more} more.")
            } else {
                String::new()
            };
            return Err(StetError::new(
                ErrorCode::ValidationFailed,
                format!(
                    "The edit would add {total} {}. {}{tail}",
                    if total == 1 {
                        "violation"
                    } else {
                        "violations"
                    },
                    named.join(" ")
                ),
            ));
        }
        Ok(Verified {
            checked: self.checked,
            carried: self
                .carried
                .iter()
                .take(CARRIED_LISTED)
                .map(|r| finding(after, parts, r))
                .collect(),
            carried_total: self.carried_total,
            notes: self
                .notes
                .iter()
                .map(|r| finding(after, parts, r))
                .collect(),
            changed: self.changed,
        })
    }
}

/// The findings as the receipt and the refusal show them.
fn finding(after: &PartsView<'_>, parts: &[SchemaPart], r: &Reported) -> Finding {
    let part = r.part.and_then(|at| parts.get(at));
    let block = part.zip(r.found.node).and_then(|(part, node)| {
        let tree = after.tree(part.id).ok()?;
        block_ref(after, &part.name, tree, node)
    });
    Finding {
        code: r.found.code,
        class: r.found.code.class(),
        part: part.map(|p| p.name.to_string()).unwrap_or_default(),
        block,
        message: r.found.message.clone(),
    }
}

/// Where a finding is, for a person: the ref of its block, a new block, or its part.
fn place(
    (before, after): (&PartsView<'_>, &PartsView<'_>),
    part: Option<&SchemaPart>,
    node: Option<NodeId>,
    block: Option<String>,
) -> String {
    let Some(part) = part else {
        return "the document".to_string();
    };
    if let Some(block) = block {
        return block;
    }
    let new_block = node.is_some_and(|node| {
        let Ok(tree) = after.tree(part.id) else {
            return false;
        };
        let unit = unit_root(tree, node);
        is_unit_boundary(tree, unit)
            && before
                .id(&part.name)
                .and_then(|id| before.tree(id).ok())
                .is_none_or(|old| !old.node_ref(unit).is_element())
    });
    if new_block {
        "a new block".to_string()
    } else {
        part.name.to_string()
    }
}

/// A sentence ends with one full stop.
fn sentence(text: &str) -> String {
    format!("{}.", text.trim_end().trim_end_matches('.'))
}
