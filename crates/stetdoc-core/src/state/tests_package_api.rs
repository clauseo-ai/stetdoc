//! Tests of the story refresh that passes only the blocks touched since the story was built, of
//! `PartsView::content_type`, of `State::opened_bytes` and of `Tx::add_relationship`.

use super::tests::*;
use super::tests_coherence::incoherent;
use super::*;
use crate::api::error::ErrorCode;
use crate::wordml::Story;
use crate::xml::NodeId;

const DOC_RELS: &str = "word/_rels/document.xml.rels";
const LINK: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
const FIELD: &str = "<w:p><w:r><w:t>before </w:t></w:r><w:fldSimple w:instr=\" PAGE \"><w:r><w:t>1</w:t></w:r></w:fldSimple></w:p>";

fn body() -> String {
    format!("{}{FIELD}{}{FIELD}", para_xml("one"), para_xml("two"))
}

/// Writes the first text node of block `at`, and touches the block.
fn write(tx: &mut Tx<'_>, at: usize, text: &str) {
    let key = tx_keys(tx)[at].clone();
    let tree = tx.part_mut(MAIN).unwrap();
    let node = *text_nodes_below(tree, key.node()).first().unwrap();
    tree.edit().set_text(node, text).unwrap();
    tx.touch(&key);
}

fn tx_keys(tx: &Tx<'_>) -> Vec<BlockKey> {
    body_blocks(&tx.parts())
        .into_iter()
        .map(|(k, _)| k)
        .collect()
}

fn text_nodes_below(tree: &crate::xml::SpanTree, below: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![below];
    while let Some(n) = stack.pop() {
        let node = tree.node_ref(n);
        if node.is_element() && node.local_name() == "t" {
            out.extend(node.children().map(|c| c.id()));
        }
        let mark = stack.len();
        stack.extend(node.children().filter(|c| c.is_element()).map(|c| c.id()));
        if let Some(tail) = stack.get_mut(mark..) {
            tail.reverse();
        }
    }
    out
}

fn fields_of(story: &Story, tree: &crate::xml::SpanTree) -> String {
    format!("{:?}", story.fields(tree))
}

#[test]
fn refreshing_with_only_the_blocks_touched_since_the_last_build_equals_a_scan() {
    let s = state(&body());
    let main = id(&s, MAIN);
    let mut tx = s.begin();
    // The field pass has run, so each refresh has an index to carry or to rebuild.
    {
        let view = tx.parts();
        let story = view.story(main).unwrap();
        assert_eq!(story.fields(view.tree(main).unwrap()).len(), 2);
    }
    // A field paragraph, a plain one, the same field paragraph again (already in the touched list),
    // and a second field paragraph, each read after the write.
    for (at, text) in [(1, "x"), (0, "y"), (1, "z"), (3, "w"), (1, "v"), (3, "u")] {
        write(&mut tx, at, text);
        let view = tx.parts();
        let story = view.story(main).unwrap();
        let tree = view.tree(main).unwrap();
        assert_eq!(incoherent(&view), Vec::<String>::new(), "after {at} {text}");
        let fresh = Story::scan(
            view.name(main).unwrap(),
            crate::wordml::StoryKind::Body,
            tree,
        )
        .unwrap();
        assert_eq!(
            fields_of(&story, tree),
            fields_of(&fresh, tree),
            "after {at} {text}"
        );
    }
    // A write that is touched only after the story was read is seen by the read after.
    let key = tx_keys(&tx)[1].clone();
    let before = tx.parts().story(main).unwrap();
    let node = *text_nodes_below(tx.part_mut(MAIN).unwrap(), key.node())
        .first()
        .unwrap();
    tx.part_mut(MAIN)
        .unwrap()
        .edit()
        .set_text(node, "late")
        .unwrap();
    tx.touch(&key);
    let view = tx.parts();
    let after = view.story(main).unwrap();
    assert!(!Arc::ptr_eq(&before, &after));
    assert_eq!(incoherent(&view), Vec::<String>::new());
    let committed = tx.commit().unwrap().0;
    assert_eq!(incoherent(&committed.parts()), Vec::<String>::new());
}

#[test]
fn content_type_is_the_override_or_the_default_and_none_when_unregistered() {
    let s = state(BODY);
    let view = s.parts();
    assert_eq!(
        view.content_type(id(&s, MAIN)).unwrap().as_deref(),
        Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml")
    );
    // Every part of the fixture package has one.
    for name in view.index().names() {
        let part = view.id(name).unwrap();
        assert!(view.content_type(part).unwrap().is_some(), "{name}");
    }
    // A part added in a transaction has the type it was added with, and the base does not see it.
    let mut tx = s.begin();
    tx.add_part(NewPart {
        name: "word/extra.bin",
        content_type: "application/x-extra",
        bytes: vec![1, 2, 3],
        related_from: None,
    })
    .unwrap();
    let extra = tx.parts().id("word/extra.bin").unwrap();
    assert_eq!(
        tx.parts().content_type(extra).unwrap().as_deref(),
        Some("application/x-extra")
    );
    assert!(s.parts().id("word/extra.bin").is_none());
}

#[test]
fn opened_bytes_is_the_size_of_the_input_and_no_commit_changes_it() {
    let bytes = package_bytes(BODY);
    let s = state_from(bytes.clone(), &Limits::default());
    assert_eq!(s.opened_bytes(), bytes.len());
    let mut tx = s.begin();
    edit_first_text(&mut tx, "changed text that is longer than before");
    tx.add_relationship(MAIN, LINK, "https://example.com/")
        .unwrap();
    let next = tx.commit().unwrap().0;
    assert_eq!(next.opened_bytes(), bytes.len());
    assert_eq!(s.opened_bytes(), bytes.len());
}

#[test]
fn add_relationship_writes_an_external_target_and_names_no_part() {
    let s = state(BODY);
    let before = s.parts().generation(Dep::Package);
    let mut tx = s.begin();
    let a = tx
        .add_relationship(MAIN, LINK, "https://example.com/a?x=1&y=2")
        .unwrap();
    let b = tx
        .add_relationship(MAIN, LINK, "mailto:someone@example.com")
        .unwrap();
    assert_ne!(a, b);
    let view = tx.parts();
    let rels = view.rels(id(&s, MAIN)).unwrap();
    let found: Vec<_> = rels.iter().filter(|r| r.rel_type == LINK).collect();
    assert_eq!(found.len(), 2);
    assert!(matches!(
        &found.first().unwrap().target,
        crate::wordml::RelTarget::External(t) if t == "https://example.com/a?x=1&y=2"
    ));
    let parsed = view.package().relationships(MAIN).unwrap();
    assert!(
        parsed
            .items
            .iter()
            .filter(|r| r.rel_type == LINK)
            .all(|r| r.external)
    );
    assert_eq!(
        view.index().len(),
        s.parts().index().len() + 1,
        "only the relationships part is new, and no part was made for the target"
    );
    assert!(
        view.content_type(view.id(DOC_RELS).unwrap())
            .unwrap()
            .is_some()
    );
    assert_ne!(view.generation(Dep::Package), before);
    assert!(
        s.parts()
            .rels(id(&s, MAIN))
            .unwrap()
            .iter()
            .all(|r| r.rel_type != LINK),
        "the base is unchanged"
    );
    // It commits, saves and opens again with the relationships in place.
    let next = tx.commit().unwrap().0;
    let saved = next.to_package().unwrap().write().unwrap();
    let reopened = state_from(saved, &Limits::default());
    let again = reopened.parts().rels(id(&reopened, MAIN)).unwrap();
    assert_eq!(again.iter().filter(|r| r.rel_type == LINK).count(), 2);
}

#[test]
fn add_relationship_from_the_package_makes_the_root_relationships_part_grow_and_a_missing_source_is_refused()
 {
    let s = state(BODY);
    let mut tx = s.begin();
    let rid = tx
        .add_relationship("", LINK, "https://example.com/")
        .unwrap();
    let first = tx.parts().package().relationships("").unwrap();
    assert!(first.get(&rid).is_some_and(|r| r.external));
    let err = tx
        .add_relationship("word/nothing.xml", LINK, "https://example.com/")
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::InvalidPayload);
    let parsed = tx.parts().package().relationships("").unwrap();
    assert!(
        parsed
            .items
            .iter()
            .any(|r| r.external && r.target == "https://example.com/")
    );
}

#[test]
fn a_failed_add_relationship_changes_nothing() {
    let s = state(BODY);
    let mut tx = s.begin();
    let epoch = tx.parts().generation(Dep::Package);
    assert!(tx.add_relationship("word/nothing.xml", LINK, "x").is_err());
    assert_eq!(tx.parts().generation(Dep::Package), epoch);
    assert!(tx.added.is_none());
}
