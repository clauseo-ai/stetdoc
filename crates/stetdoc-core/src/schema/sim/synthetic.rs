//! The two rules on random children of every content model, for insertion and for removal.
//!
//! For every declaration with a content model, random child sequences (some valid, some
//! shuffled, some at random; some with an unknown element, some with an `mc:AlternateContent`
//! wrapper in them) are built, and every element the model names is inserted where
//! `insert_in_order` says, and a sample of the children removed, with the rewrite of a property
//! container where the contract asks for it. After each edit the violation keys of the parent
//! are compared with those before: nothing new (Rule A), and a parent that was valid is valid
//! (Rule B). Where the fit is `Fit::Valid` the parent must also have no order problem left, so
//! that insertion and validation provably read the same sequence.

use super::super::generated::names::{DECLS, NAMESPACES};
use super::super::tables::{self, Model};
use super::*;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

fn spec(name: &(String, String)) -> Spec {
    Spec {
        ns: name.0.clone(),
        local: name.1.clone(),
        attrs: Vec::new(),
        kids: Vec::new(),
    }
}

fn mc(local: &str, attrs: Vec<(String, String, String)>, kids: Vec<Spec>) -> Spec {
    Spec {
        ns: MC_NS.to_string(),
        local: local.to_string(),
        attrs,
        kids,
    }
}

/// An `mc:AlternateContent` whose branches hold elements the model names. The Choice requires a
/// namespace the tables know, or not, so that either branch can be the one Word reads. A branch
/// holds one element where the parent is a property container, which is what Word writes (the
/// two renderings of one property), and one or two in content.
fn wrapper(rng: &mut Rng, names: &[(String, String)], container: bool) -> Spec {
    let pick = |rng: &mut Rng| spec(&names[rng.below(names.len())]);
    let requires = if rng.below(2) == 0 { "w14" } else { "x" };
    let mut choice_kids = vec![pick(rng)];
    if !container && rng.below(2) == 0 {
        choice_kids.push(pick(rng));
    }
    let mut kids = vec![mc(
        "Choice",
        vec![(String::new(), "Requires".to_string(), requires.to_string())],
        choice_kids,
    )];
    if rng.below(3) != 0 {
        kids.push(mc("Fallback", Vec::new(), vec![pick(rng)]));
    }
    mc("AlternateContent", Vec::new(), kids)
}

/// A random child sequence over the model's alphabet: a walk of the automaton, the same walk
/// shuffled, or a uniform draw.
fn random_children(
    rng: &mut Rng,
    model: &Model,
    names: &[(String, String)],
    container: bool,
) -> Vec<Spec> {
    let cols = model.alpha_name.len();
    let len = rng.below(7);
    let mode = rng.below(3);
    let mut seq: Vec<usize> = Vec::new();
    if mode == 2 {
        seq.extend((0..len).map(|_| rng.below(cols)));
    } else {
        let mut state = 0usize;
        for _ in 0..len {
            let outs: Vec<usize> = (0..cols)
                .filter(|c| model.next(state, *c).is_some())
                .collect();
            if outs.is_empty() {
                break;
            }
            let c = outs[rng.below(outs.len())];
            seq.push(c);
            state = model.next(state, c).unwrap_or(state);
        }
        if mode == 1 {
            for i in (1..seq.len()).rev() {
                let j = rng.below(i + 1);
                seq.swap(i, j);
            }
        }
    }
    seq.into_iter()
        .map(|c| match rng.below(10) {
            0 => Spec {
                ns: "urn:example:unknown".to_string(),
                local: "thing".to_string(),
                attrs: Vec::new(),
                kids: Vec::new(),
            },
            1 => wrapper(rng, names, container),
            _ => spec(&names[c]),
        })
        .collect()
}

/// The expanded name of the element a declaration is for.
fn decl_name(decl: u16) -> Option<(String, String)> {
    let (name, ..) = tables::decl_row(decl)?;
    let (ns, local) = tables::name_row(*name)?;
    Some((
        NAMESPACES.get(usize::from(*ns))?.0.to_string(),
        local.to_string(),
    ))
}

#[test]
fn rules_a_and_b_hold_on_random_children_of_every_content_model() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut tally = Tally::default();
    let mut identities = 0usize;
    for decl in 0..DECLS.len() as u16 {
        let Some(model) = Model::of_decl(decl) else {
            continue;
        };
        let ctx = Ctx::from_decl(decl);
        let names = ctx.child_names();
        let Some(root) = decl_name(decl) else {
            continue;
        };
        if names.is_empty() {
            continue;
        }
        tally.context = ctx.label();
        for _ in 0..12 {
            let kids = random_children(&mut rng, &model, &names, ctx.is_reorderable());
            let mut tree = SimTree::from_spec((&root.0, &root.1), kids);
            let top = tree.root();
            // Rewriting a container that is in order changes nothing, so a caller may do it
            // after every removal.
            let in_order = !keys_of(&tree, ctx, top, None)
                .iter()
                .any(|(k, _)| structural(k.kind) && k.parent == ctx.label());
            if ctx.is_reorderable() && in_order {
                let n = tree.children(top).len();
                assert_eq!(
                    reorder(Some(ctx), &tree.tree, top, None),
                    Some((0..n).map(Some).collect()),
                    "{} is in order but reorder moves it",
                    ctx.label()
                );
                identities += 1;
            }
            simulate_parent(&mut tree, ctx, top, 8, true, &mut tally);
        }
    }
    let summary = format!(
        "inserts {} removals {} reordered {} refused removals {} by fit {:?}",
        tally.inserts, tally.removals, tally.reordered, tally.refused_removals, tally.by_fit
    );
    assert!(tally.inserts > 10_000, "{summary}");
    assert!(tally.removals > 3_000, "{summary}");
    assert!(identities > 200, "{identities} {summary}");
    assert_eq!(tally.editor_refusals, 0, "{summary}");
    let first = |v: &[String]| v.iter().take(12).cloned().collect::<Vec<_>>();
    assert!(
        tally.valid_but_invalid.is_empty(),
        "Valid left an order problem {} times: {:#?}\n{summary}",
        tally.valid_but_invalid.len(),
        first(&tally.valid_but_invalid)
    );
    assert!(
        tally.rule_b_failures.is_empty(),
        "rule B failed {} times: {:#?}\n{summary}",
        tally.rule_b_failures.len(),
        first(&tally.rule_b_failures)
    );
    assert!(
        tally.rule_a_failures.is_empty(),
        "rule A failed {} times: {:#?}\n{summary}",
        tally.rule_a_failures.len(),
        first(&tally.rule_a_failures)
    );
}
