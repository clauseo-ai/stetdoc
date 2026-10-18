//! Cache coherence: after every operation and every commit of the oracle's random edits, every
//! cached value equals a rebuild from scratch, and the check can see a dependency that is declared
//! too narrowly.

use super::derived::Dep;
use super::tests::*;
use super::*;
use crate::wordml::{IdKind, Story, existing_ids, id_parts};

/// A test cache over two parts, and one whose dependencies are too narrow.
#[derive(PartialEq, Debug)]
struct TwoParts(usize, String);

fn two_parts(view: &PartsView<'_>, narrow: bool) -> Arc<TwoParts> {
    let (main, core) = (view.id(MAIN).unwrap(), view.id(CORE).unwrap());
    let deps: &[Dep] = if narrow {
        &[Dep::Part(main)]
    } else {
        &[Dep::Part(main), Dep::Part(core)]
    };
    let build = |_: Option<&TwoParts>| -> Result<TwoParts, StetError> {
        let blocks = view.story(main)?.blocks().len();
        let core = view.tree(core)?.root().text_content();
        Ok(TwoParts(blocks, core))
    };
    view.derived(u64::from(narrow), deps, &build).unwrap()
}

/// Every cached value against a rebuild from scratch. Returns what differs.
pub(super) fn incoherent(view: &PartsView<'_>) -> Vec<String> {
    let mut bad = Vec::new();
    let main = view.id(MAIN).unwrap();
    let name = view.name(main).unwrap();
    let tree = view.tree(main).unwrap();
    let cached = view.story(main).unwrap();
    let fresh = Story::scan(name, crate::wordml::StoryKind::Body, tree).unwrap();
    if !cached.same_as(&fresh) {
        bad.push("story".to_owned());
    }
    let rels = view.rels(main).unwrap();
    let again = super::stories::rels_from_scratch(view, main);
    if *rels != again {
        bad.push("rels".to_owned());
    }
    // The story list and every story in it, including the stories that `add_part` made.
    let list = view.story_specs().unwrap();
    if list.0 != crate::wordml::stories(name, tree, &again) {
        bad.push("story list".to_owned());
    }
    for spec in &list.0 {
        let part = view.id(&spec.part).unwrap();
        let fresh = Story::scan(&spec.part, spec.kind, view.tree(part).unwrap()).unwrap();
        if !view.story(part).unwrap().same_as(&fresh) {
            bad.push(format!("story {}", spec.part));
        }
    }
    for kind in [IdKind::ParaId, IdKind::TextId, IdKind::AnnotationId] {
        let set = view.ids(kind).unwrap();
        let specs = view.story_specs().unwrap();
        let mut found = Vec::new();
        for part in id_parts(kind, &specs.0, &rels) {
            existing_ids(
                kind,
                view.tree(view.id(&part).unwrap()).unwrap(),
                &mut found,
            );
        }
        found.sort_unstable();
        found.dedup();
        if set.len() != found.len() || found.iter().any(|id| !set.contains(*id)) {
            bad.push(format!("ids {kind:?}"));
        }
    }
    let wide = two_parts(view, false);
    let blocks = view.story(main).unwrap().blocks().len();
    let core = view
        .tree(view.id(CORE).unwrap())
        .unwrap()
        .root()
        .text_content();
    if *wide != TwoParts(blocks, core) {
        bad.push("two parts".to_owned());
    }
    bad
}

#[test]
fn the_coherence_check_can_see_a_dependency_that_is_declared_too_narrowly() {
    let s = state(&paragraphs(&["a", "b"]));
    let narrow = two_parts(&s.parts(), true);
    let mut tx = s.begin();
    {
        let tree = tx.part_mut(CORE).unwrap();
        let first = tree
            .root()
            .first_child()
            .unwrap()
            .first_child()
            .unwrap()
            .id();
        tree.edit().set_text(first, "changed").unwrap();
    }
    // The wide cache follows the edit and the narrow one does not, and the check tells them apart.
    let wide = two_parts(&tx.parts(), false);
    let still = two_parts(&tx.parts(), true);
    assert_ne!(wide.1, narrow.1);
    assert!(
        Arc::ptr_eq(&narrow, &still),
        "the narrow cache missed the edit of the core part"
    );
    assert_ne!(
        still.1,
        tx.parts().tree(id(&s, CORE)).unwrap().root().text_content()
    );
    assert_eq!(
        wide.1,
        tx.parts().tree(id(&s, CORE)).unwrap().root().text_content()
    );
}
