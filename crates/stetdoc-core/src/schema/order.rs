//! Where a new child element goes: `insert_in_order`, and `reorder` for rewriting a property
//! container.
//!
//! **Contract.** Anything that inserts a child element asks this module for the index. Given
//! the parent's [`Ctx`], the tree and the new element's name, `insert_in_order` returns an index
//! such that the result is valid whenever any index makes it valid, and says in [`Fit`] how good
//! the result is. When none does, it never makes things worse: it adds nothing that the parent's
//! violation keys do not already have, or it refuses (`Fit::NoValidSlot`).
//!
//! **How the position is chosen.**
//!
//! 1. *The sequence.* The parent's children are read the way the validator reads them: each
//!    `mc:AlternateContent` is replaced by the branch Word takes, and each element that
//!    `mc:ProcessContent` keeps the children of by those children (one function in `effective`
//!    serves both, with the `mc:Ignorable` scope read from the parent and its ancestors), and
//!    elements the content model does not name are skipped.
//! 2. *Once.* An element the model lets occur only once is refused (`Fit::NoValidSlot`) when
//!    the parent already has one, even if the parent is invalid and the automaton cannot say.
//!    The generator emits that bit per symbol.
//! 3. *The cost of each position.* For every place the new element could go, the content
//!    model (a DFA) is run over the sequence the way the validator runs it: an element with no
//!    transition is one failure and leaves the state alone, and an end state that does not
//!    accept is one more. A position with cost zero is a valid slot, and `Prefer` picks among
//!    those. That is exact for sequences (`w:pPr`: one slot), for repeated choices (`w:p`
//!    content: many slots) and for models with required elements, and it finds the slot that
//!    repairs a parent that was missing something (`w:tbl` without `w:tblPr`), which rank cannot
//!    do (every symbol of `w:tbl` shares one rank). The work is linear in the children times
//!    the states of the model, with no cap.
//! 4. *A valid parent with no valid slot* refuses (`Fit::NoValidSlot`): whatever the element did
//!    would be new.
//! 5. *A property container* whose children are already invalid is not patched.
//!    `Fit::Reorder(order)` carries the whole rewrite, and `reorder` gives the same for a
//!    removal. The validator keys such a container by the elements that fail, so the rewrite is
//!    accepted when no element fails in it that did not fail before; if the new element does
//!    (it excludes one that is there, or something required is missing before it) the answer is
//!    `Fit::NoValidSlot`.
//! 6. *Content* cannot be rewritten, and the validator keys it by the kinds of failure alone
//!    (some element out of place, something missing). A position is acceptable when it brings
//!    no kind the parent did not already have. Among those the cheapest wins, canonical rank
//!    inversions breaking ties (`Fit::BestEffort`), and when none is acceptable the answer is
//!    `Fit::NoValidSlot`. That is a deliberate exception to rule A (an edit is not refused for a
//!    problem that was in the input): a `w:tr` into a `w:tbl` that has a `w:tblPr` and no
//!    `w:tblGrid` and no rows can go nowhere without a row before the missing grid, a failure
//!    of a kind the table did not have, so it is refused and the caller repairs the table first.
//!
//! An unknown parent ([`Ctx`] absent, or an opaque declaration such as a drawing) gets the
//! caller's preferred position and `Fit::UnknownContext`.
//!

use super::ctx::{Ctx, col_of, column};
use super::effective::{Effective, boundaries, effective_children};
use super::scope::scope_at;
use super::tables::Model;
use super::tree::{Nodes, QName};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

/// Where among the valid slots the caller would like the element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prefer {
    /// The earliest valid slot.
    First,
    /// The latest valid slot (append when the content model allows it).
    Last,
    /// Immediately before the child at this index, or the nearest valid slot to it.
    Before(usize),
    /// Immediately after the child at this index, or the nearest valid slot to it.
    After(usize),
}

/// How good the chosen index is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fit {
    /// The children plus the new element satisfy the content model.
    Valid,
    /// The children were already invalid and no position repairs them, but at the index the
    /// new element brings no kind of failure the parent did not have. Of those positions it is
    /// the cheapest.
    BestEffort,
    /// The element is in the content model, but it cannot be added without adding a violation
    /// of its own: it is a second copy of one that may occur once, or it excludes an element
    /// that is there (the other alternative of a choice), or the parent was valid and no slot
    /// keeps it so, or every position brings a kind of failure the parent did not have.
    /// Callers refuse the insertion (a property is replaced, not added twice). The index is
    /// the preferred position.
    NoValidSlot,
    /// The parent is a property container (`w:pPr`, `w:tblPr`, ...) whose children are out of
    /// order, so no position can add nothing. The caller rewrites the container's children in
    /// this order: `Some(i)` is the existing child at index `i`, `None` is the new element. The
    /// container is the element being edited, so nothing else is touched, and the old order
    /// problem goes away instead of being repeated.
    ///
    /// The same holds for a removal: after taking a child out of a property container, call
    /// [`reorder`] with no new element and apply the order it returns. It is the identity when
    /// the container is in order, so it is safe to call after every removal.
    Reorder(Vec<Option<usize>>),
    /// The content model does not allow this element in this parent.
    NotAllowed,
    /// The parent has no schema (unknown or opaque). The index follows `Prefer`.
    UnknownContext,
}

/// The result of [`insert_in_order`]: where to insert, and how good the position is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    /// Insert before the child currently at this index; the child count appends. For
    /// [`Fit::Reorder`] it is where the new element lands in the rewritten order.
    pub index: usize,
    /// How good `index` is; callers treat anything but [`Fit::Valid`] as a warning.
    pub fit: Fit,
}

fn target(prefer: Prefer, n: usize) -> usize {
    match prefer {
        Prefer::First => 0,
        Prefer::Last => n,
        Prefer::Before(i) => i.min(n),
        Prefer::After(i) => i.saturating_add(1).min(n),
    }
}

/// The candidate closest to the preferred position. Candidates are ascending.
fn choose(candidates: &[usize], prefer: Prefer, n: usize) -> usize {
    let t = target(prefer, n);
    match prefer {
        Prefer::First => candidates.first().copied(),
        Prefer::Last => candidates.last().copied(),
        _ => candidates
            .iter()
            .copied()
            .min_by_key(|c| (c.abs_diff(t), *c)),
    }
    .unwrap_or(t)
}

/// The kinds of failure of a content model over a sequence, as the validator keys them for
/// content: some element is not where the model puts it (out of order, a second copy of one
/// that may occur once, excluded by another), or a required element is missing at the end.
const ORDER: u8 = 1;
const MISSING: u8 = 2;

/// What inserting the new element at each position of the sequence would cost.
struct Scores {
    /// Failures of the content model over the sequence with the new element at each position,
    /// an end state that does not accept counted as one.
    cost: Vec<u32>,
    /// The kinds of those failures, as a bit set.
    classes: Vec<u8>,
    /// Pairs the new element would put out of canonical rank order, at each position.
    inversions: Vec<u32>,
    /// The cost and the kinds of the sequence as it is.
    input: u32,
    input_classes: u8,
}

impl Scores {
    fn new(model: &Model, cols: &[Option<usize>], new_col: usize) -> Scores {
        let m = cols.len();
        let states = model.states();
        let at = |v: &[u32], i: usize| v.get(i).copied().unwrap_or(0);
        let class_at = |v: &[u8], i: usize| v.get(i).copied().unwrap_or(0);
        // Before each position: the state, the failures so far and their kinds.
        let mut state_at: Vec<usize> = Vec::with_capacity(m + 1);
        let mut fails_at: Vec<u32> = Vec::with_capacity(m + 1);
        let mut kinds_at: Vec<u8> = Vec::with_capacity(m + 1);
        let (mut state, mut fails, mut kinds) = (0usize, 0u32, 0u8);
        for col in cols {
            state_at.push(state);
            fails_at.push(fails);
            kinds_at.push(kinds);
            if let Some(c) = col {
                match model.next(state, *c) {
                    Some(t) => state = t,
                    None => {
                        fails += 1;
                        kinds |= ORDER;
                    }
                }
            }
        }
        state_at.push(state);
        fails_at.push(fails);
        kinds_at.push(kinds);
        // From the end back: what reading the rest from state `s` costs, and of which kinds.
        let mut rest: Vec<u32> = (0..states).map(|s| u32::from(!model.accepts(s))).collect();
        let mut rest_kinds: Vec<u8> = (0..states)
            .map(|s| if model.accepts(s) { 0 } else { MISSING })
            .collect();
        let (mut spare, mut spare_kinds) = (rest.clone(), rest_kinds.clone());
        let mut cost = vec![0u32; m + 1];
        let mut classes = vec![0u8; m + 1];
        for e in (0..=m).rev() {
            let s = state_at.get(e).copied().unwrap_or(0);
            let (with_new, with_kinds) = match model.next(s, new_col) {
                Some(t) => (at(&rest, t), class_at(&rest_kinds, t)),
                None => (1 + at(&rest, s), ORDER | class_at(&rest_kinds, s)),
            };
            if let Some(slot) = cost.get_mut(e) {
                *slot = at(&fails_at, e) + with_new;
            }
            if let Some(slot) = classes.get_mut(e) {
                *slot = class_at(&kinds_at, e) | with_kinds;
            }
            if e > 0
                && let Some(Some(c)) = cols.get(e - 1)
            {
                for s in 0..states {
                    let (v, k) = match model.next(s, *c) {
                        Some(t) => (at(&rest, t), class_at(&rest_kinds, t)),
                        None => (1 + at(&rest, s), ORDER | class_at(&rest_kinds, s)),
                    };
                    if let Some(slot) = spare.get_mut(s) {
                        *slot = v;
                    }
                    if let Some(slot) = spare_kinds.get_mut(s) {
                        *slot = k;
                    }
                }
                std::mem::swap(&mut rest, &mut spare);
                std::mem::swap(&mut rest_kinds, &mut spare_kinds);
            }
        }
        let (input, input_classes) = (at(&rest, 0), class_at(&rest_kinds, 0));
        // Inversions against the canonical rank, the tie-break between equal costs.
        let rank_of = |c: usize| model.alpha_rank.get(c).copied();
        let ranks: Vec<Option<u16>> = cols.iter().map(|c| c.and_then(rank_of)).collect();
        let mut inversions = vec![0u32; m + 1];
        if let Some(r) = rank_of(new_col) {
            // Known elements before the position that rank after the new one.
            let mut greater = 0u32;
            for p in 0..=m {
                if let Some(slot) = inversions.get_mut(p) {
                    *slot += greater;
                }
                if let Some(Some(x)) = ranks.get(p)
                    && *x > r
                {
                    greater += 1;
                }
            }
            // Known elements from the position on that rank before the new one.
            let mut less = 0u32;
            for p in (0..=m).rev() {
                if let Some(Some(x)) = ranks.get(p)
                    && *x < r
                {
                    less += 1;
                }
                if let Some(slot) = inversions.get_mut(p) {
                    *slot += less;
                }
            }
        }
        Scores {
            cost,
            classes,
            inversions,
            input,
            input_classes,
        }
    }
}

/// Inserts `new` into the children of `parent`, an element with context `ctx`.
///
/// The result is an index into the parent's element children, `tree.children(parent)`; the
/// caller performs the insertion. See the module docs for how it is chosen.
pub fn insert_in_order<T: Nodes>(
    ctx: Option<Ctx>,
    tree: &T,
    parent: T::Node,
    new: QName<'_>,
    prefer: Prefer,
) -> Placement {
    let mut scope = scope_at(tree, parent, true);
    let eff = effective_children(tree, parent, &mut scope.chain, scope.ign);
    let n = eff.raw;
    let refuse = |fit: Fit| Placement {
        index: target(prefer, n),
        fit,
    };
    let Some(model) = ctx.and_then(Ctx::model) else {
        return refuse(Fit::UnknownContext);
    };
    let Some(new_col) = column(&model, new) else {
        return refuse(Fit::NotAllowed);
    };
    let cols: Vec<Option<usize>> = eff
        .items
        .iter()
        .map(|i| column(&model, tree.name(i.node)))
        .collect();
    if model.is_once(new_col) && cols.contains(&Some(new_col)) {
        return refuse(Fit::NoValidSlot);
    }
    let scores = Scores::new(&model, &cols, new_col);
    let bounds = boundaries(&eff);
    let at = |e: usize| bounds.get(e).copied().unwrap_or(0);
    let cost = |p: usize| scores.cost.get(at(p)).copied().unwrap_or(u32::MAX);
    // A position that keeps the parent valid.
    let valid: Vec<usize> = (0..=n).filter(|p| cost(*p) == 0).collect();
    if !valid.is_empty() {
        return Placement {
            index: choose(&valid, prefer, n),
            fit: Fit::Valid,
        };
    }
    // The parent was valid and nothing keeps it so: whatever the element does is new.
    if scores.input == 0 {
        return refuse(Fit::NoValidSlot);
    }
    if ctx.is_some_and(Ctx::is_reorderable)
        && let Some(order) = reorder_children(&model, tree, &eff, Some(new))
    {
        let mut raw_cols: Vec<Vec<Option<usize>>> = vec![Vec::new(); n];
        for (item, col) in eff.items.iter().zip(&cols) {
            if let Some(slot) = raw_cols.get_mut(item.from) {
                slot.push(*col);
            }
        }
        let rewritten: Vec<(Option<usize>, bool)> = order
            .iter()
            .flat_map(|slot| match slot {
                Some(i) => raw_cols
                    .get(*i)
                    .map(|v| v.iter().map(|c| (*c, false)).collect())
                    .unwrap_or_default(),
                None => vec![(Some(new_col), true)],
            })
            .collect();
        let index = order.iter().position(Option::is_none).unwrap_or(n);
        // A container is rewritten when it is touched. The validator keys a property
        // container by the elements that fail, so the rewrite is fine when no element fails in
        // it that did not fail before. If one does (the new element excludes an element that is
        // there, or a required one is missing before it) the edit would add a violation of its
        // own.
        let after = failing(&model, rewritten.iter().map(|(c, _)| *c));
        let before = failing(&model, cols.iter().copied());
        let fit = if after.0.is_subset(&before.0) && (!after.1 || before.1) {
            Fit::Reorder(order)
        } else {
            Fit::NoValidSlot
        };
        return Placement { index, fit };
    }
    // Content cannot be rewritten. A position is acceptable when the parent has no kind of
    // failure there that it did not have before: the validator keys content by kind alone.
    let key =
        |p: usize| -> (u32, u32) { (cost(p), scores.inversions.get(at(p)).copied().unwrap_or(0)) };
    let acceptable: Vec<usize> = (0..=n)
        .filter(|p| {
            let classes = scores.classes.get(at(*p)).copied().unwrap_or(u8::MAX);
            classes & !scores.input_classes == 0
        })
        .collect();
    let Some(best) = acceptable.iter().map(|p| key(*p)).min() else {
        return refuse(Fit::NoValidSlot);
    };
    let slots: Vec<usize> = acceptable.into_iter().filter(|p| key(*p) == best).collect();
    Placement {
        index: choose(&slots, prefer, n),
        fit: Fit::BestEffort,
    }
}

/// The columns that fail when `cols` is read in order (an element with no transition leaves the
/// state alone), and whether the end state fails to accept.
fn failing(model: &Model, cols: impl Iterator<Item = Option<usize>>) -> (BTreeSet<usize>, bool) {
    let mut state = 0usize;
    let mut failed = BTreeSet::new();
    for c in cols.flatten() {
        match model.next(state, c) {
            Some(t) => state = t,
            None => {
                failed.insert(c);
            }
        }
    }
    (failed, !model.accepts(state))
}

/// The children of a property container in schema order, with `new` (if any) in its place.
///
/// Each entry is `Some(i)` for the existing child at index `i` of `tree.children(parent)`, or
/// `None` for the new element. A child has the range of ranks of the elements Word reads in its
/// place (one rank for an element, a range for an `mc:AlternateContent` that stands for
/// several). One child goes before another when its highest rank is below the other's lowest;
/// children that nothing orders against each other, the ones that rank alike or whose ranges
/// overlap, keep their order. So children that may interleave are not swapped, and a wrapper
/// that spans ranks is not pulled apart from what it already sits between. A child the model
/// does not name (a foreign or extension element, or a wrapper that stands for none) keeps its
/// position relative to the known child before it and moves with it. The new element goes where
/// the content model likes it best, the latest such place. Returns `None` when `ctx` is not a
/// property container, or does not name `new`.
///
/// For a container in order and no new element the result is the identity, so callers can
/// apply it after every removal.
pub fn reorder<T: Nodes>(
    ctx: Option<Ctx>,
    tree: &T,
    parent: T::Node,
    new: Option<QName<'_>>,
) -> Option<Vec<Option<usize>>> {
    let ctx = ctx.filter(|c| c.is_reorderable())?;
    let model = ctx.model()?;
    let mut scope = scope_at(tree, parent, true);
    let eff = effective_children(tree, parent, &mut scope.chain, scope.ign);
    reorder_children(&model, tree, &eff, new)
}

/// A known child and the unknown ones that follow it, with the lowest and the highest rank among
/// the elements the model reads for them (a wrapper can stand for several).
pub(super) struct Unit {
    pub(super) members: Vec<usize>,
    pub(super) lo: u16,
    pub(super) hi: u16,
}

/// Puts the units in order. A unit goes before another when its highest rank is below the other's
/// lowest; units that this does not order keep their places relative to each other. That is the
/// first unit, in the current order, that none of the units still to be placed must precede,
/// taken again and again. For units of one rank it is a stable sort by rank.
///
/// A unit can be placed once its lowest rank is no higher than the lowest `hi` among the units
/// not yet placed, and that bound only rises, so one pass over the units by `lo` and a heap of
/// the ones that can be placed give the same answer in O(u log u). The rule used to be applied
/// by scanning, which took O(u²) steps for each unit placed and O(u³) when the children came
/// in reverse order.
pub(super) fn sort_units(units: Vec<Unit>) -> Vec<Unit> {
    let mut by_lo: Vec<usize> = (0..units.len()).collect();
    by_lo.sort_by_key(|i| units.get(*i).map_or(0, |u| u.lo));
    let mut his: BTreeMap<u16, usize> = BTreeMap::new();
    for u in &units {
        *his.entry(u.hi).or_insert(0) += 1;
    }
    let mut slots: Vec<Option<Unit>> = units.into_iter().map(Some).collect();
    let mut ready: BinaryHeap<Reverse<usize>> = BinaryHeap::new();
    let mut next = 0usize;
    let mut sorted: Vec<Unit> = Vec::with_capacity(slots.len());
    while let Some((&floor, _)) = his.first_key_value() {
        while let Some(&i) = by_lo.get(next) {
            if slots
                .get(i)
                .and_then(Option::as_ref)
                .is_some_and(|u| u.lo > floor)
            {
                break;
            }
            ready.push(Reverse(i));
            next += 1;
        }
        // The unit that holds the floor has `lo <= hi = floor`, so it is ready: this finds one.
        let Some(Reverse(i)) = ready.pop() else {
            break;
        };
        let Some(unit) = slots.get_mut(i).and_then(Option::take) else {
            continue;
        };
        if let Some(count) = his.get_mut(&unit.hi) {
            *count -= 1;
            if *count == 0 {
                his.remove(&unit.hi);
            }
        }
        sorted.push(unit);
    }
    // Only a broken invariant leaves anything here; keep the units rather than lose them.
    sorted.extend(slots.into_iter().flatten());
    sorted
}

fn reorder_children<T: Nodes>(
    model: &Model,
    tree: &T,
    eff: &Effective<T::Node>,
    new: Option<QName<'_>>,
) -> Option<Vec<Option<usize>>> {
    let rank_of =
        |name: QName<'_>| col_of(model, name).and_then(|c| model.alpha_rank.get(c).copied());
    // The columns the model sees for each child, and the range of their ranks.
    let mut raw_cols: Vec<Vec<Option<usize>>> = vec![Vec::new(); eff.raw];
    let mut spans: Vec<Option<(u16, u16)>> = vec![None; eff.raw];
    for item in &eff.items {
        let name = tree.name(item.node);
        if let Some(cols) = raw_cols.get_mut(item.from) {
            cols.push(column(model, name));
        }
        if let (Some(r), Some(span)) = (rank_of(name), spans.get_mut(item.from)) {
            *span = Some(span.map_or((r, r), |(lo, hi)| (lo.min(r), hi.max(r))));
        }
    }
    let mut head: Vec<usize> = Vec::new();
    let mut units: Vec<Unit> = Vec::new();
    for (i, span) in spans.into_iter().enumerate() {
        match (span, units.last_mut()) {
            (Some((lo, hi)), _) => units.push(Unit {
                members: vec![i],
                lo,
                hi,
            }),
            (None, Some(last)) => last.members.push(i),
            (None, None) => head.push(i),
        }
    }
    let sorted = sort_units(units);
    let mut order: Vec<Option<usize>> = head.iter().map(|i| Some(*i)).collect();
    // The columns in that order, and where each unit starts in them.
    let mut cols: Vec<Option<usize>> = head
        .iter()
        .flat_map(|i| raw_cols.get(*i).cloned().unwrap_or_default())
        .collect();
    let mut starts = vec![cols.len()];
    for u in &sorted {
        order.extend(u.members.iter().map(|i| Some(*i)));
        for i in &u.members {
            cols.extend(raw_cols.get(*i).cloned().unwrap_or_default());
        }
        starts.push(cols.len());
    }
    if let Some(q) = new {
        let new_col = col_of(model, q)?;
        let scores = Scores::new(model, &cols, new_col);
        // Between units (after the head), where the model likes it best, the latest such place.
        let at = (0..=sorted.len())
            .min_by_key(|u| {
                let e = starts.get(*u).copied().unwrap_or(0);
                (
                    scores.cost.get(e).copied().unwrap_or(u32::MAX),
                    scores.inversions.get(e).copied().unwrap_or(0),
                    std::cmp::Reverse(*u),
                )
            })
            .unwrap_or(sorted.len());
        let slot = head.len()
            + sorted
                .iter()
                .take(at)
                .map(|u| u.members.len())
                .sum::<usize>();
        order.insert(slot.min(order.len()), None);
    }
    Some(order)
}
