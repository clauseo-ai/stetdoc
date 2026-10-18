//! Edit simulation: do the two rules for order hold when elements are inserted and removed?
//!
//! * **Rule A.** An edit is never refused because of a problem that was already in the input:
//!   the violation keys of the parent's unit (`validate_unit`: the parent and what is below it,
//!   but not the insides of the paragraphs and tables below it) after an insertion or a removal
//!   are a subset of the keys before, apart from the new element's own problems and from a
//!   removal that takes out something required (that one is refused, rightly).
//! * **Rule B.** A parent that was valid before an edit is valid after it.
//!
//! The simulation works on a real span tree and edits it through its `Editor`: an insertion is
//! `create_element` and `insert_before` at the index `insert_in_order` returns, a rewrite is
//! `detach` and `append_child`. For a sample of parents it inserts the elements the content
//! model names, where `insert_in_order` says (rewriting a property container when the fit is
//! `Fit::Reorder`), and removes each of a sample of children, applying `reorder` to a property
//! container afterwards, and compares every violation key of the parent's unit before and after. `synthetic` runs it on random children of every
//! content model, `corpus` on real documents (ignored by default; it reads a corpus), and the
//! tests below on small documents.

mod corpus;
mod synthetic;
mod tree;
#[cfg(test)]
mod unit;

use super::effective::effective_children;
use super::scope::scope_at;
use super::*;
use crate::xml::NodeId;
use std::collections::BTreeMap;
use tree::{SimTree, Spec, keys_of};

/// Tallies of one simulation run.
#[derive(Default, Debug)]
struct Tally {
    pub inserts: usize,
    pub removals: usize,
    /// Removals refused because they left a required element missing.
    pub refused_removals: usize,
    /// Insertions refused before they reach the tree (`NotAllowed`, `NoValidSlot`).
    pub refused_inserts: usize,
    /// Edits the span tree's editor refused. The simulation asks only for edits it can take, so
    /// any is a finding.
    pub editor_refusals: usize,
    pub parent_clean: usize,
    pub rule_a_failures: Vec<String>,
    pub rule_b_failures: Vec<String>,
    /// `Fit::Valid` where the parent still has an order problem. Checked on synthetic trees.
    pub valid_but_invalid: Vec<String>,
    pub by_fit: BTreeMap<String, usize>,
    pub reordered: usize,
    pub rewritten_removals: usize,
    /// Names the document being simulated, for the failure messages.
    pub context: String,
}

/// The kinds that say the content model is not satisfied by the children.
fn structural(k: ViolationKind) -> bool {
    matches!(
        k,
        ViolationKind::OutOfOrder
            | ViolationKind::TooMany
            | ViolationKind::NonCanonicalOrder
            | ViolationKind::MissingChild
    )
}

impl Tally {
    /// Judges one edit. A removal that takes out something the content model requires
    /// (`required`) is refused, and rightly, whatever it does to the keys: the element is
    /// missing, and what follows it reads as out of order.
    fn judge(
        &mut self,
        label: &dyn Fn() -> String,
        before: &ViolationSet,
        after: &ViolationSet,
        removal: Option<bool>,
    ) -> bool {
        let added = after.added_since(before);
        if let Some(required) = removal
            && (required || added.iter().any(|k| k.kind == ViolationKind::MissingChild))
        {
            self.refused_removals += 1;
            return false;
        }
        if !added.is_empty() {
            self.rule_a_failures
                .push(format!("{}: new {:?}", label(), added.first()));
        }
        if before.is_empty() {
            self.parent_clean += 1;
            if !after.is_empty() {
                self.rule_b_failures.push(format!(
                    "{}: {:?}",
                    label(),
                    after.iter().next().map(|(k, _)| k)
                ));
            }
        }
        true
    }
}

/// Whether every sequence the content model of `ctx` accepts contains the element.
fn mandatory(ctx: Ctx, name: QName<'_>) -> bool {
    let Some(model) = ctx.model() else {
        return false;
    };
    let Some(col) = super::ctx::col_of(&model, name) else {
        return false;
    };
    let mut seen = vec![false; model.states()];
    let mut stack = vec![0usize];
    if let Some(s) = seen.first_mut() {
        *s = true;
    }
    while let Some(q) = stack.pop() {
        if model.accepts(q) {
            return false;
        }
        for c in (0..model.cols()).filter(|c| *c != col) {
            if let Some(t) = model.next(q, c)
                && let Some(s) = seen.get_mut(t)
                && !*s
            {
                *s = true;
                stack.push(t);
            }
        }
    }
    true
}

/// Inserts the elements the content model of `parent` names and removes a sample of its
/// children, checking the two rules for each. `max_items` caps how many of each are tried.
/// With `strict`, `Fit::Valid` must also leave the parent without an order problem.
fn simulate_parent(
    tree: &mut SimTree,
    ctx: Ctx,
    parent: NodeId,
    max_items: usize,
    strict: bool,
    tally: &mut Tally,
) {
    let before = keys_of(tree, ctx, parent, None);
    let context = tally.context.clone();
    // What the parent's children were, for a message: written only when an edit fails.
    let describe =
        |tree: &SimTree| -> String { format!("{} children {}", ctx.label(), tree.shape(parent)) };
    let names = ctx.child_names();
    let step = (names.len() / max_items.max(1)).max(1);
    for (nn, name) in names.iter().enumerate() {
        if nn % step != 0 {
            continue;
        }
        let saved = tree.save(parent);
        let placed = tree.insert(
            Some(ctx),
            parent,
            (name.0.as_str(), name.1.as_str()),
            Prefer::Last,
        );
        let outcome = match placed {
            Some((fit, new))
                if !matches!(
                    fit,
                    Fit::NotAllowed | Fit::NoValidSlot | Fit::UnknownContext
                ) =>
            {
                Some((fit, keys_of(tree, ctx, parent, Some(new))))
            }
            _ => None,
        };
        tree.restore(saved);
        let Some((fit, after)) = outcome else {
            tally.refused_inserts += 1;
            continue;
        };
        tally.inserts += 1;
        let fit_name = match &fit {
            Fit::Reorder(_) => "Reorder".to_string(),
            other => format!("{other:?}"),
        };
        *tally.by_fit.entry(fit_name.clone()).or_insert(0) += 1;
        if matches!(fit, Fit::Reorder(_)) {
            tally.reordered += 1;
        }
        let label = || format!("{} +{} ({fit_name}) in {context}", describe(tree), name.1);
        tally.judge(&label, &before, &after, None);
        // A type that can contain itself (`w:frameset`) has the same label at every level, so
        // the label does not say whose problem a key is.
        let recursive = ctx
            .child_names()
            .iter()
            .any(|(ns, local)| ctx.child((ns, local)) == Some(ctx));
        if strict && !recursive && fit == Fit::Valid {
            let label = ctx.label();
            if after
                .iter()
                .any(|(k, _)| structural(k.kind) && k.parent == label)
            {
                tally
                    .valid_but_invalid
                    .push(format!("{} +{}", describe(tree), name.1));
            }
        }
    }
    let count = tree.children(parent).len();
    let step = (count / max_items.max(1)).max(1);
    for index in (0..count).step_by(step) {
        let saved = tree.save(parent);
        let removed = tree.children(parent).get(index).map(|c| tree.qname(*c));
        let removed_name = removed.clone().map(|r| r.1).unwrap_or_default();
        // The last element of a kind that every valid parent has, whether the child is that
        // element or an mc:AlternateContent that stands for it.
        let mut scope = scope_at(&tree.tree, parent, true);
        let read = effective_children(&tree.tree, parent, &mut scope.chain, scope.ign);
        let names: Vec<(usize, (String, String))> = read
            .items
            .iter()
            .map(|i| (i.from, tree.qname(i.node)))
            .collect();
        let required = names
            .iter()
            .filter(|(from, _)| *from == index)
            .any(|(_, (ns, local))| {
                let same = names
                    .iter()
                    .filter(|(_, n)| n == &(ns.clone(), local.clone()));
                mandatory(ctx, (ns, local)) && same.count() == 1
            });
        let rewritten = tree.remove(Some(ctx), parent, index);
        let after = keys_of(tree, ctx, parent, None);
        tree.restore(saved);
        let label = || format!("{} -{removed_name} in {context}", describe(tree));
        if tally.judge(&label, &before, &after, Some(required)) {
            tally.removals += 1;
            if rewritten {
                tally.rewritten_removals += 1;
            }
        }
    }
    tally.editor_refusals += std::mem::take(&mut tree.refused_by_editor);
}

/// Runs [`simulate_parent`] on every `stride`-th element of a part.
fn simulate(xml: &[u8], part: PartKind, stride: usize, max_items: usize, tally: &mut Tally) {
    let Some(mut base) = SimTree::parse(xml) else {
        return;
    };
    for parent in base.elements().into_iter().step_by(stride.max(1)) {
        let Some(ctx) = base.ctx_of(part, parent) else {
            continue;
        };
        if ctx.child_names().is_empty() {
            continue;
        }
        simulate_parent(&mut base, ctx, parent, max_items, false, tally);
    }
}
