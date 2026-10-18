//! The keys and the findings of one unit: what the schema and the Word rules say about it.

use super::found::Found;
use super::keys::{Key, KeySet, schema_code};
use super::report::schema_message;
use super::rules::scan_unit;
use super::units::{Layout, Unit, UnitKind, layout_of};
use crate::schema::{PartKind, ctx_at, validate_unit};
use crate::xml::{NodeId, SpanTree};
use std::collections::BTreeMap;

/// What was found in one unit.
#[derive(Debug, Default)]
pub(super) struct Examined {
    /// Every key, once.
    pub(super) keys: KeySet,
    /// The findings, in the order of the schema's report and then the Word rules' scan. A schema
    /// report lists at most `schema::MAX_DETAILS` of its violations, and the keys are complete.
    pub(super) found: Vec<Found>,
    /// How many times each key occurs.
    counts: BTreeMap<Key, usize>,
}

impl Examined {
    /// How many findings have this key.
    pub(super) fn instances(&self, key: &Key) -> usize {
        self.counts.get(key).copied().unwrap_or(0)
    }

    fn count(&mut self, key: Key, n: usize) {
        if n > 0 {
            self.keys.insert(key.clone());
            *self.counts.entry(key).or_default() += n;
        }
    }
}

/// Whether the violations of a unit are only those at the root and at its children. It is the
/// frame of a part that is not a story, and of the notes parts: the root, and the order and the
/// number of what is directly below it. What is below that belongs to the units there.
fn keeps_only_the_top(layout: Layout, unit: Unit) -> bool {
    layout != Layout::Story && unit.kind == UnitKind::Frame
}

/// Validates a unit and reads it with the Word rules. A unit that has no context (it stands in an
/// element the schema skips, or where it does not place one) is not validated by the schema, and
/// is read by the Word rules all the same.
pub(super) fn examine(part: PartKind, tree: &SpanTree, unit: Unit) -> Examined {
    let layout = layout_of(part);
    let mut out = Examined::default();
    if let Some(ctx) = ctx_at(part, tree, unit.root) {
        let report = validate_unit(ctx, tree, unit.root);
        if keeps_only_the_top(layout, unit) {
            let at_top = |node: NodeId| {
                node == unit.root
                    || tree
                        .node_ref(node)
                        .parent()
                        .is_some_and(|p| p.id() == unit.root)
            };
            for v in report.violations.iter().filter(|v| at_top(v.node)) {
                out.count(Key::schema(&v.key), 1);
                out.found.push(Found {
                    key: Key::schema(&v.key),
                    code: schema_code(v.kind),
                    node: Some(v.node),
                    message: schema_message(tree, v),
                });
            }
        } else {
            for (key, n) in report.set.iter() {
                out.count(Key::schema(key), n as usize);
            }
            for v in &report.violations {
                out.found.push(Found {
                    key: Key::schema(&v.key),
                    code: schema_code(v.kind),
                    node: Some(v.node),
                    message: schema_message(tree, v),
                });
            }
        }
    }
    // The frame of a notes part is the root alone, and a part that is not a story has nothing a
    // Word rule is about.
    if layout != Layout::Elements && !keeps_only_the_top(layout, unit) {
        for f in scan_unit(tree, unit.root) {
            out.count(f.key.clone(), 1);
            out.found.push(f);
        }
    }
    out
}
