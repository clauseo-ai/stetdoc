//! Tests of the derived caches: what is built once, what goes stale, and where a value built in a
//! transaction goes.

use super::derived::Dep;
use super::tests::*;
use super::*;
use crate::api::error::ErrorCode;
use std::cell::Cell;

// ---- derived caches -------------------------------------------------------------------------

/// A cache value that remembers what it was built from, so a test can see what was rebuilt.
#[derive(Debug, PartialEq)]
struct Count {
    text: String,
    built_from_stale: bool,
}

fn first_text_of(view: &PartsView<'_>, name: &str, builds: &Cell<usize>) -> Arc<Count> {
    let part = view.id(name).unwrap();
    let build = |stale: Option<&Count>| -> Result<Count, StetError> {
        builds.set(builds.get() + 1);
        let tree = view.tree(part)?;
        let text = text_nodes(tree)
            .first()
            .map(|n| tree.node_ref(*n).text().into_owned())
            .unwrap_or_default();
        Ok(Count {
            text,
            built_from_stale: stale.is_some(),
        })
    };
    view.derived(0, &[Dep::Part(part)], &build).unwrap()
}

#[test]
fn a_value_is_built_once_per_type_variant_and_dependencies() {
    let s = state(BODY);
    let (main, core) = (id(&s, MAIN), id(&s, CORE));
    let builds = Cell::new(0);
    let a = first_text_of(&s.parts(), MAIN, &builds);
    let b = first_text_of(&s.parts(), MAIN, &builds);
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(builds.get(), 1);
    // The same type over another part is another entry, because the key holds the dependencies.
    let c = first_text_of(&s.parts(), CORE, &builds);
    assert_eq!(builds.get(), 2);
    assert!(!Arc::ptr_eq(&a, &c));
    // A variant is another entry too.
    let by_variant = |variant: u64| {
        s.parts()
            .derived(variant, &[Dep::Part(main)], &|_: Option<&Count>| {
                builds.set(builds.get() + 1);
                Ok(Count {
                    text: variant.to_string(),
                    built_from_stale: false,
                })
            })
            .unwrap()
    };
    assert_eq!(by_variant(7).text, "7");
    assert_eq!(by_variant(8).text, "8");
    assert_eq!(by_variant(7).text, "7");
    assert_eq!(builds.get(), 4);
    // The order and repeats of the dependencies do not matter.
    let two = |deps: &[Dep]| {
        s.parts()
            .derived(1, deps, &|_: Option<&Count>| {
                builds.set(builds.get() + 1);
                Ok(Count {
                    text: String::new(),
                    built_from_stale: false,
                })
            })
            .unwrap()
    };
    let x = two(&[Dep::Part(main), Dep::Part(core)]);
    let y = two(&[Dep::Part(core), Dep::Part(main), Dep::Part(core)]);
    assert!(Arc::ptr_eq(&x, &y));
}

#[test]
fn an_edit_makes_what_depends_on_the_part_stale_and_nothing_else() {
    let s = state(BODY);
    let builds = Cell::new(0);
    let main_before = first_text_of(&s.parts(), MAIN, &builds);
    first_text_of(&s.parts(), CORE, &builds);
    assert_eq!(builds.get(), 2);
    let mut tx = s.begin();
    edit_first_text(&mut tx, "edited");
    let after = first_text_of(&tx.parts(), MAIN, &builds);
    assert_eq!(after.text, "edited");
    assert!(
        after.built_from_stale,
        "the builder is given the stale value"
    );
    assert_eq!(main_before.text, "one");
    first_text_of(&tx.parts(), CORE, &builds);
    assert_eq!(builds.get(), 3, "the other part's entry is still valid");
    // After commit, both entries are in the next state, and neither is built again.
    let next = tx.commit().unwrap().0;
    assert_eq!(first_text_of(&next.parts(), MAIN, &builds).text, "edited");
    first_text_of(&next.parts(), CORE, &builds);
    assert_eq!(builds.get(), 3);
    // The old state still answers for itself.
    assert_eq!(first_text_of(&s.parts(), MAIN, &builds).text, "one");
    assert_eq!(builds.get(), 3);
}

#[test]
fn what_a_transaction_builds_reaches_the_state_only_at_commit() {
    let s = state(BODY);
    let at_open = s.derived.borrow().len();
    let builds = Cell::new(0);
    let mut tx = s.begin();
    edit_first_text(&mut tx, "edited");
    first_text_of(&tx.parts(), MAIN, &builds);
    assert_eq!(builds.get(), 1);
    assert_eq!(
        s.derived.borrow().len(),
        at_open,
        "nothing reached the base"
    );
    tx.discard();
    assert_eq!(s.derived.borrow().len(), at_open);
    let again = first_text_of(&s.parts(), MAIN, &builds);
    assert_eq!(builds.get(), 2);
    assert_eq!(again.text, "one");
    assert!(
        !again.built_from_stale,
        "a discarded build leaves no stale value behind"
    );
}

#[test]
fn a_value_built_in_a_transaction_from_parts_it_did_not_change_is_not_leaked_either() {
    let s = state(BODY);
    let at_open = s.derived.borrow().len();
    let builds = Cell::new(0);
    let tx = s.begin();
    first_text_of(&tx.parts(), CORE, &builds);
    assert_eq!(s.derived.borrow().len(), at_open);
    drop(tx);
}

#[test]
fn a_builder_may_ask_for_another_value() {
    let s = state(BODY);
    let builds = Cell::new(0);
    let view = s.parts();
    let main = view.id(MAIN).unwrap();
    let outer = view
        .derived(0, &[Dep::Part(main)], &|_: Option<&String>| {
            let inner = first_text_of(&view, MAIN, &builds);
            Ok(format!("outer {}", inner.text))
        })
        .unwrap();
    assert_eq!(*outer, "outer one");
    assert_eq!(builds.get(), 1);
}

#[test]
fn a_failed_build_stores_nothing() {
    let s = state(BODY);
    let main = id(&s, MAIN);
    let fails = |_: Option<&String>| -> Result<String, StetError> {
        Err(StetError::new(ErrorCode::NotFound, "no"))
    };
    let at_open = s.derived.borrow().len();
    assert!(s.parts().derived(0, &[Dep::Part(main)], &fails).is_err());
    assert_eq!(s.derived.borrow().len(), at_open);
}

#[test]
fn a_generation_moves_when_the_tree_changes_and_the_package_generation_does_not() {
    let s = state(BODY);
    let main = Dep::Part(id(&s, MAIN));
    let before = s.parts().generation(main);
    let mut tx = s.begin();
    edit_first_text(&mut tx, "edited");
    let during = tx.parts().generation(main);
    assert_ne!(before, during);
    assert_eq!(before.epoch, during.epoch);
    assert_eq!(s.parts().generation(main), before, "the base does not move");
    let next = tx.commit().unwrap().0;
    assert_eq!(next.parts().generation(main), during);
    assert_eq!(
        next.parts().generation(Dep::Package),
        s.parts().generation(Dep::Package)
    );
}
