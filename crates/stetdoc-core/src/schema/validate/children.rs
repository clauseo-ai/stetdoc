//! Content checks: runs an element's children through its content model.

use super::{Frame, Sink, ViolationKind, key};
use crate::schema::effective::Effective;
use crate::schema::scope::{Ignorable, Skip, skip_of};
use crate::schema::tables::{self, Model, display_name, display_qname};
use crate::schema::tree::SchemaTree;
use std::cmp::Reverse;
use std::collections::BTreeSet;

pub(super) struct ChildCtx<'a> {
    pub(super) model: &'a Model,
    pub(super) parent_label: &'a str,
    pub(super) canonical: bool,
    /// A property container: its order problems are keyed by element and blocker.
    pub(super) reorderable: bool,
    /// The scopes of `mc:Ignorable` and `mc:ProcessContent` that `read` was made in.
    pub(super) chain: &'a [Ignorable],
}

/// The most nodes `visit_known` looks at below one child that its parent does not allow.
pub(in crate::schema) const KNOWN_BUDGET: usize = 10_000;

/// Queues `node` with its own context when it is a paragraph, a run, a table, a row or a cell, and otherwise the nearest such
/// elements below it, which an element that is not allowed (or unknown) would hide from the walk.
///
/// The search looks at `KNOWN_BUDGET` nodes at most, in document order, the queued elements counted but not what is inside
/// them (their own walk checks that). When the budget runs out the search stops there: the paragraphs, runs, tables, rows and
/// cells further down in that element are not checked by their own content model, and nothing says so. The element itself is
/// already reported, as unexpected or unknown, so the part has a finding at that place whatever is inside it, and the budget
/// keeps a large foreign subtree (an XML island with no WordprocessingML in it) to a bounded cost per element.
fn visit_known<T: SchemaTree>(
    tree: &T,
    node: T::Node,
    ign: Option<usize>,
    to_visit: &mut Vec<Frame<T::Node>>,
) {
    let mut stack = vec![node];
    let mut budget = KNOWN_BUDGET;
    while let Some(n) = stack.pop() {
        if budget == 0 {
            return;
        }
        budget -= 1;
        match crate::schema::ctx::known(tree.name(n)) {
            Some(own) => to_visit.push(Frame {
                node: n,
                decl: own.decl(),
                ign,
            }),
            None => stack.extend(tree.children(n).into_iter().rev()),
        }
    }
}

/// Runs the children of `node`, as Word reads them (`read`), through its content model.
pub(super) fn check_children<T: SchemaTree>(
    tree: &T,
    node: T::Node,
    read: Effective<T::Node>,
    cc: &ChildCtx<'_>,
    stack: &mut Vec<Frame<T::Node>>,
    sink: &mut Sink<T::Node>,
) {
    let model = cc.model;
    let skipped = read.skipped;
    let mut state = 0usize;
    let mut seen = vec![false; model.alpha_name.len()];
    let mut consumed = Consumed::default();
    let mut to_visit: Vec<Frame<T::Node>> = Vec::new();
    for item in read.items {
        let (child, scope) = (item.node, item.ign);
        let (ns, local) = tree.name(child);
        let name = tables::ns_by_uri(ns).and_then(|n| tables::name_id(n, local));
        let col = name.and_then(|n| model.col_of(n));
        let Some(use_col) = col.or_else(|| model.wildcard()) else {
            if skip_of(tree, child, cc.chain, scope) != Skip::No {
                continue;
            }
            let kind = if tables::ns_by_uri(ns).is_some() {
                ViolationKind::UnexpectedChild
            } else {
                ViolationKind::UnknownElement
            };
            sink.push(
                key(
                    kind,
                    cc.parent_label,
                    display_qname(ns, local),
                    String::new(),
                ),
                child,
                format!(
                    "not allowed in {}; allowed here: {}",
                    cc.parent_label,
                    expected_names(model, state)
                ),
            );
            // A paragraph, a run, a table, a row or a cell has a content model of its own wherever it stands: its children
            // are still checked by it, as the unit of an edit checks them (`ctx_at`), and so are those of one inside an
            // element that is not allowed here.
            visit_known(tree, child, scope, &mut to_visit);
            continue;
        };
        // An earlier sibling with the same name, whether or not it was accepted: what makes a
        // second copy of an element that may occur once "too many", wherever it stands.
        let earlier = col.is_some_and(|c| seen.get(c).copied().unwrap_or(false));
        if let Some(s) = col.and_then(|c| seen.get_mut(c)) {
            *s = true;
        }
        match model.next(state, use_col) {
            Some(next) => {
                state = next;
                if let Some(c) = col {
                    consumed.push(model, c);
                    if let Some(&d) = model.alpha_decl.get(c) {
                        to_visit.push(Frame {
                            node: child,
                            decl: d,
                            ign: scope,
                        });
                    }
                }
            }
            None => {
                let repeated = earlier && col.is_some_and(|c| model.is_once(c));
                let blocking = consumed.blocker(model, col);
                let mut kind = if repeated {
                    ViolationKind::TooMany
                } else {
                    ViolationKind::OutOfOrder
                };
                if cc.canonical {
                    kind = ViolationKind::NonCanonicalOrder;
                }
                // In content (a table, a paragraph) a misplaced element cannot be fixed by
                // rewriting the parent, and any new element placed in an already invalid parent
                // can land in a new bad place. So the key says only that this parent has an order
                // problem; the report still names the element.
                let subject = if cc.reorderable {
                    display_qname(ns, local)
                } else {
                    String::new()
                };
                let why = if repeated {
                    "a second copy".to_string()
                } else if blocking.is_empty() {
                    "excluded by another element".to_string()
                } else {
                    format!("after {blocking}")
                };
                sink.push(
                    key(kind, cc.parent_label, subject, String::new()),
                    child,
                    format!(
                        "not allowed here in {} ({why}); allowed here: {}",
                        cc.parent_label,
                        expected_names(model, state)
                    ),
                );
                // Still check the element itself, so one misplaced child does not hide
                // problems inside it.
                if let Some(&d) = col.and_then(|c| model.alpha_decl.get(c)) {
                    to_visit.push(Frame {
                        node: child,
                        decl: d,
                        ign: scope,
                    });
                }
            }
        }
    }
    if !model.accepts(state) {
        let first = first_expected(model, state);
        // What the model needs next depends on where the sequence went wrong, and supplying
        // one required element changes it. The key says only that something required is
        // missing; the detail names it.
        sink.push(
            key(
                ViolationKind::MissingChild,
                cc.parent_label,
                String::new(),
                String::new(),
            ),
            node,
            format!("{} ends before a required {first}", cc.parent_label),
        );
    }
    // Elements of mc:AlternateContent branches that were not taken: check their own content.
    for (s, scope) in skipped {
        let (ns, local) = tree.name(s);
        let col = tables::ns_by_uri(ns)
            .and_then(|n| tables::name_id(n, local))
            .and_then(|n| model.col_of(n));
        if let Some(&d) = col.and_then(|c| model.alpha_decl.get(c)) {
            to_visit.push(Frame {
                node: s,
                decl: d,
                ign: scope,
            });
        }
    }
    // Depth-first in document order: the stack pops the last pushed first.
    stack.extend(to_visit.into_iter().rev());
}

/// The elements the content model has accepted so far, kept as much as naming the blocker of one
/// that cannot be placed needs. Each element costs the same however many came before it: a part
/// with thousands of misplaced children must not take time quadratic in them.
#[derive(Default)]
struct Consumed {
    /// The ranks that an accepted element has.
    ranks: BTreeSet<u16>,
    /// The accepted element read furthest: the highest rank, the earliest of equals.
    furthest: Option<(u16, Reverse<usize>)>,
}

impl Consumed {
    fn push(&mut self, model: &Model, col: usize) {
        let rank = model.alpha_rank.get(col).copied().unwrap_or(0);
        self.ranks.insert(rank);
        self.furthest = self.furthest.max(Some((rank, Reverse(col))));
    }

    /// The earlier sibling that makes `col` unplaceable, for the message: the one the sequence
    /// has got furthest with. Empty when an element that ranks like `col` was read before it:
    /// the two exclude each other (the alternatives of a choice).
    fn blocker(&self, model: &Model, col: Option<usize>) -> String {
        if let Some(c) = col
            && self
                .ranks
                .contains(&model.alpha_rank.get(c).copied().unwrap_or(0))
        {
            return String::new();
        }
        self.furthest
            .and_then(|(_, Reverse(c))| model.alpha_name.get(c))
            .map(|n| display_name(*n))
            .unwrap_or_default()
    }
}

pub(super) fn outgoing(model: &Model, state: usize) -> Vec<usize> {
    (0..model.alpha_name.len())
        .filter(|c| model.next(state, *c).is_some())
        .collect()
}

pub(super) fn expected_names(model: &Model, state: usize) -> String {
    let cols = outgoing(model, state);
    let mut names: Vec<String> = cols
        .iter()
        .take(6)
        .filter_map(|c| model.alpha_name.get(*c).map(|n| display_name(*n)))
        .collect();
    if cols.len() > 6 {
        names.push("...".into());
    }
    if names.is_empty() {
        "nothing".into()
    } else {
        names.join(", ")
    }
}

/// The element a content model needs next: the one with the lowest canonical rank.
pub(super) fn first_expected(model: &Model, state: usize) -> String {
    outgoing(model, state)
        .into_iter()
        .min_by_key(|c| (model.alpha_rank.get(*c).copied().unwrap_or(0), *c))
        .and_then(|c| model.alpha_name.get(c))
        .map(|n| display_name(*n))
        .unwrap_or_else(|| "?".into())
}
