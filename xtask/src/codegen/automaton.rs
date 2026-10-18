//! Content models: particle tree -> NFA -> DFA -> minimal DFA, plus a rank for each symbol (the
//! canonical child order that `insert_in_order` uses).
//!
//! Symbols are `u32` element name ids. [`ANY`] is the wildcard (`xsd:any`): in the DFA it
//! becomes one extra column, used for any element that is not in the alphabet.

use super::model::{Occurs, Particle};
use super::paths::{Paths, max_once, sortable};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const ANY: u32 = u32::MAX;
pub const NONE: u16 = u16::MAX;

/// Resolves a particle element's declaration name to its element name id.
pub type Resolve<'a> = dyn Fn(&str) -> Result<u32, String> + 'a;

#[derive(Default)]
struct Nfa {
    eps: Vec<Vec<usize>>,
    edges: Vec<Vec<(u32, usize)>>,
}

impl Nfa {
    fn node(&mut self) -> usize {
        self.eps.push(Vec::new());
        self.edges.push(Vec::new());
        self.eps.len() - 1
    }
    fn eps(&mut self, a: usize, b: usize) {
        if let Some(v) = self.eps.get_mut(a) {
            v.push(b);
        }
    }
    fn edge(&mut self, a: usize, sym: u32, b: usize) {
        if let Some(v) = self.edges.get_mut(a) {
            v.push((sym, b));
        }
    }
}

fn occurs_of(p: &Particle) -> Occurs {
    match p {
        Particle::Element { occurs, .. }
        | Particle::Any { occurs }
        | Particle::Seq { occurs, .. }
        | Particle::Choice { occurs, .. }
        | Particle::All { occurs, .. } => *occurs,
    }
}

fn build_once(nfa: &mut Nfa, p: &Particle, res: &Resolve) -> Result<(usize, usize), String> {
    match p {
        Particle::Element { decl, .. } => {
            let sym = res(decl)?;
            let (s, e) = (nfa.node(), nfa.node());
            nfa.edge(s, sym, e);
            Ok((s, e))
        }
        Particle::Any { .. } => {
            let (s, e) = (nfa.node(), nfa.node());
            nfa.edge(s, ANY, e);
            Ok((s, e))
        }
        Particle::Seq { items, .. } => {
            let s = nfa.node();
            let mut cur = s;
            for i in items {
                let (is, ie) = build_occ(nfa, i, res)?;
                nfa.eps(cur, is);
                cur = ie;
            }
            Ok((s, cur))
        }
        Particle::Choice { items, .. } => {
            let (s, e) = (nfa.node(), nfa.node());
            if items.is_empty() {
                nfa.eps(s, e);
            }
            for i in items {
                let (is, ie) = build_occ(nfa, i, res)?;
                nfa.eps(s, is);
                nfa.eps(ie, e);
            }
            Ok((s, e))
        }
        Particle::All { items, .. } => {
            // Unordered: every permutation. Small in practice; refuse a blow-up.
            if items.len() > 6 {
                return Err(format!("xsd:all with {} items", items.len()));
            }
            let (s, e) = (nfa.node(), nfa.node());
            let mut idx: Vec<usize> = (0..items.len()).collect();
            let mut perms = Vec::new();
            permute(&mut idx, 0, &mut perms);
            for perm in perms {
                let mut cur = s;
                for k in perm {
                    if let Some(i) = items.get(k) {
                        let (is, ie) = build_occ(nfa, i, res)?;
                        nfa.eps(cur, is);
                        cur = ie;
                    }
                }
                nfa.eps(cur, e);
            }
            Ok((s, e))
        }
    }
}

fn permute(v: &mut Vec<usize>, k: usize, out: &mut Vec<Vec<usize>>) {
    if k >= v.len() {
        out.push(v.clone());
        return;
    }
    for i in k..v.len() {
        v.swap(k, i);
        permute(v, k + 1, out);
        v.swap(k, i);
    }
}

fn build_occ(nfa: &mut Nfa, p: &Particle, res: &Resolve) -> Result<(usize, usize), String> {
    let o = occurs_of(p);
    let s = nfa.node();
    let mut cur = s;
    for _ in 0..o.min {
        let (is, ie) = build_once(nfa, p, res)?;
        nfa.eps(cur, is);
        cur = ie;
    }
    let e = nfa.node();
    match o.max {
        None => {
            let (is, ie) = build_once(nfa, p, res)?;
            nfa.eps(cur, is);
            nfa.eps(ie, is);
            nfa.eps(ie, e);
            nfa.eps(cur, e);
        }
        Some(max) => {
            let mut exits = vec![cur];
            for _ in o.min..max {
                let (is, ie) = build_once(nfa, p, res)?;
                nfa.eps(cur, is);
                cur = ie;
                exits.push(cur);
            }
            for x in exits {
                nfa.eps(x, e);
            }
        }
    }
    Ok((s, e))
}

/// A minimal DFA over `alphabet` plus, when `any`, one wildcard column at index
/// `alphabet.len()`. State 0 is the start state. `trans[state * cols + col]` is the next
/// state or [`NONE`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dfa {
    pub alphabet: Vec<u32>,
    pub any: bool,
    pub trans: Vec<u16>,
    pub accept: Vec<bool>,
    /// Canonical order of each alphabet symbol (equal for symbols that may interleave).
    pub rank: Vec<u16>,
    /// Whether each alphabet symbol occurs at most once in every accepted sequence.
    pub once: Vec<bool>,
    /// Whether the children can be rewritten in rank order (a property container).
    pub reorderable: bool,
}

impl Dfa {
    pub fn cols(&self) -> usize {
        self.alphabet.len() + usize::from(self.any)
    }
    pub fn states(&self) -> usize {
        self.accept.len()
    }
    pub fn next(&self, state: usize, col: usize) -> u16 {
        self.trans
            .get(state * self.cols() + col)
            .copied()
            .unwrap_or(NONE)
    }
}

fn closure(nfa: &Nfa, set: &BTreeSet<usize>) -> BTreeSet<usize> {
    let mut out = set.clone();
    let mut stack: Vec<usize> = set.iter().copied().collect();
    while let Some(n) = stack.pop() {
        for &m in nfa.eps.get(n).map(Vec::as_slice).unwrap_or(&[]) {
            if out.insert(m) {
                stack.push(m);
            }
        }
    }
    out
}

pub fn compile(p: &Particle, res: &Resolve) -> Result<Dfa, String> {
    let mut nfa = Nfa::default();
    let (start, end) = build_occ(&mut nfa, p, res)?;
    let mut alphabet: Vec<u32> = nfa
        .edges
        .iter()
        .flatten()
        .map(|e| e.0)
        .filter(|s| *s != ANY)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    alphabet.sort();
    let any = nfa.edges.iter().flatten().any(|e| e.0 == ANY);
    let cols = alphabet.len() + usize::from(any);

    let mut ids: BTreeMap<BTreeSet<usize>, usize> = BTreeMap::new();
    let mut sets: Vec<BTreeSet<usize>> = Vec::new();
    let init = closure(&nfa, &BTreeSet::from([start]));
    ids.insert(init.clone(), 0);
    sets.push(init);
    let mut rows: Vec<Vec<Option<usize>>> = Vec::new();
    let mut i = 0;
    while i < sets.len() {
        let set = sets.get(i).cloned().unwrap_or_default();
        let mut row = Vec::with_capacity(cols);
        for col in 0..cols {
            let sym = alphabet.get(col).copied().unwrap_or(ANY);
            let mut moved = BTreeSet::new();
            for &n in &set {
                for &(es, t) in nfa.edges.get(n).map(Vec::as_slice).unwrap_or(&[]) {
                    if es == sym || es == ANY {
                        moved.insert(t);
                    }
                }
            }
            if moved.is_empty() {
                row.push(None);
                continue;
            }
            let c = closure(&nfa, &moved);
            let id = match ids.get(&c) {
                Some(id) => *id,
                None => {
                    let id = sets.len();
                    ids.insert(c.clone(), id);
                    sets.push(c);
                    id
                }
            };
            row.push(Some(id));
        }
        rows.push(row);
        i += 1;
    }
    let accept: Vec<bool> = sets.iter().map(|s| s.contains(&end)).collect();
    let dfa = minimize(&alphabet, any, &rows, &accept)?;
    let rank = ranks(&dfa);
    let paths = Paths::new(&dfa);
    let once = max_once(&paths);
    let reorderable = sortable(&paths, &rank);
    Ok(Dfa {
        rank,
        once,
        reorderable,
        ..dfa
    })
}

/// Moore partition refinement, then renumber in breadth-first order so the output is
/// canonical.
fn minimize(
    alphabet: &[u32],
    any: bool,
    rows: &[Vec<Option<usize>>],
    accept: &[bool],
) -> Result<Dfa, String> {
    let n = rows.len();
    let cols = alphabet.len() + usize::from(any);
    let mut class: Vec<usize> = accept.iter().map(|a| usize::from(*a)).collect();
    loop {
        let mut sig: BTreeMap<(usize, Vec<Option<usize>>), usize> = BTreeMap::new();
        let mut next = vec![0; n];
        for s in 0..n {
            let key = (
                class.get(s).copied().unwrap_or(0),
                rows.get(s)
                    .map(|r| {
                        r.iter()
                            .map(|t| t.and_then(|t| class.get(t).copied()))
                            .collect()
                    })
                    .unwrap_or_default(),
            );
            let len = sig.len();
            let id = *sig.entry(key).or_insert(len);
            if let Some(slot) = next.get_mut(s) {
                *slot = id;
            }
        }
        let same = sig.len() == class.iter().collect::<BTreeSet<_>>().len();
        class = next;
        if same {
            break;
        }
    }
    // Representative per class, renumbered by BFS from the start state.
    let mut order: Vec<usize> = Vec::new();
    let mut seen: BTreeMap<usize, usize> = BTreeMap::new();
    let mut queue = VecDeque::new();
    let start_class = class.first().copied().unwrap_or(0);
    seen.insert(start_class, 0);
    order.push(0);
    queue.push_back(0usize);
    while let Some(rep) = queue.pop_front() {
        for col in 0..cols {
            if let Some(t) = rows.get(rep).and_then(|r| r.get(col)).copied().flatten() {
                let c = class.get(t).copied().unwrap_or(0);
                if let std::collections::btree_map::Entry::Vacant(v) = seen.entry(c) {
                    v.insert(order.len());
                    order.push(t);
                    queue.push_back(t);
                }
            }
        }
    }
    if order.len() > usize::from(NONE) {
        return Err("DFA has too many states".into());
    }
    let mut trans = Vec::with_capacity(order.len() * cols);
    let mut acc = Vec::new();
    for rep in &order {
        acc.push(accept.get(*rep).copied().unwrap_or(false));
        for col in 0..cols {
            let t = rows.get(*rep).and_then(|r| r.get(col)).copied().flatten();
            trans.push(match t {
                None => NONE,
                Some(t) => {
                    let c = class.get(t).copied().unwrap_or(0);
                    seen.get(&c).copied().unwrap_or(0) as u16
                }
            });
        }
    }
    Ok(Dfa {
        alphabet: alphabet.to_vec(),
        any,
        trans,
        accept: acc,
        rank: Vec::new(),
        once: Vec::new(),
        reorderable: false,
    })
}

/// Canonical order of the alphabet. `b -> a` when `a` can occur after `b`. Symbols in one
/// strongly connected component may interleave and share a rank. A component's rank is the
/// length of the longest chain of components that must come before it, so components that
/// nothing orders against each other (the alternatives of a choice, which never occur
/// together) share a rank too, and sorting a container by rank never swaps them.
fn ranks(dfa: &Dfa) -> Vec<u16> {
    let n = dfa.alphabet.len();
    let states = dfa.states();
    let cols = dfa.cols();
    // Symbols that can still be consumed from each state (reachability fixpoint).
    let mut after: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); states];
    loop {
        let mut changed = false;
        for s in 0..states {
            for col in 0..n {
                let t = dfa.next(s, col);
                if t == NONE {
                    continue;
                }
                let mut add: BTreeSet<usize> = BTreeSet::from([col]);
                add.extend(after.get(usize::from(t)).cloned().unwrap_or_default());
                if let Some(cur) = after.get_mut(s) {
                    let before = cur.len();
                    cur.extend(add);
                    changed |= cur.len() != before;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut follow: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    for s in 0..states {
        for col in 0..n.min(cols) {
            let t = dfa.next(s, col);
            if t == NONE {
                continue;
            }
            if let Some(f) = follow.get_mut(col) {
                f.extend(after.get(usize::from(t)).cloned().unwrap_or_default());
            }
        }
    }
    // Transitive closure (n is small: tens of symbols).
    let mut reach = follow.clone();
    loop {
        let mut changed = false;
        for a in 0..n {
            let cur: Vec<usize> = reach
                .get(a)
                .map(|r| r.iter().copied().collect())
                .unwrap_or_default();
            for b in cur {
                let extra = reach.get(b).cloned().unwrap_or_default();
                if let Some(r) = reach.get_mut(a) {
                    let before = r.len();
                    r.extend(extra);
                    changed |= r.len() != before;
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Component of a: itself plus every b with a ->* b and b ->* a.
    let mut comp: Vec<usize> = vec![usize::MAX; n];
    let mut comps: Vec<Vec<usize>> = Vec::new();
    for a in 0..n {
        if comp.get(a).copied() != Some(usize::MAX) {
            continue;
        }
        let id = comps.len();
        let mut members = vec![a];
        for b in (a + 1)..n {
            let ab = reach.get(a).is_some_and(|r| r.contains(&b));
            let ba = reach.get(b).is_some_and(|r| r.contains(&a));
            if ab && ba {
                members.push(b);
            }
        }
        for &m in &members {
            if let Some(c) = comp.get_mut(m) {
                *c = id;
            }
        }
        comps.push(members);
    }
    // Longest chain of components before each one, by Kahn's algorithm over the components.
    let k = comps.len();
    let mut indeg = vec![0usize; k];
    let mut out_edges: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); k];
    for a in 0..n {
        for &b in reach.get(a).into_iter().flatten() {
            let (ca, cb) = (
                comp.get(a).copied().unwrap_or(0),
                comp.get(b).copied().unwrap_or(0),
            );
            if ca != cb
                && out_edges.get_mut(ca).is_some_and(|s| s.insert(cb))
                && let Some(d) = indeg.get_mut(cb)
            {
                *d += 1;
            }
        }
    }
    let mut ready: Vec<usize> = (0..k)
        .filter(|c| indeg.get(*c).copied() == Some(0))
        .collect();
    let mut rank_of_comp = vec![0u16; k];
    while let Some(c) = ready.pop() {
        let here = rank_of_comp.get(c).copied().unwrap_or(0);
        for &d in out_edges.get(c).into_iter().flatten() {
            if let Some(r) = rank_of_comp.get_mut(d) {
                *r = (*r).max(here.saturating_add(1));
            }
            if let Some(x) = indeg.get_mut(d) {
                *x -= 1;
                if *x == 0 {
                    ready.push(d);
                }
            }
        }
    }
    (0..n)
        .map(|a| {
            rank_of_comp
                .get(comp.get(a).copied().unwrap_or(0))
                .copied()
                .unwrap_or(0)
        })
        .collect()
}
