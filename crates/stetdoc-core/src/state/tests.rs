//! Tests of the parts, the transaction and the derived caches. They build small packages with the
//! testkit's fixture builder and read the parts through `PartsView`, the way later modules will.

use super::*;
use crate::api::error::ErrorCode;
use crate::refs::BlockKey;
use crate::xml::{NodeId, SpanTree};
use stetdoc_testkit::fixtures::{
    parts,
    zip::{Entry, Zip},
};

pub(super) const MAIN: &str = "word/document.xml";
pub(super) const CORE: &str = "docProps/core.xml";
pub(super) const BODY: &str =
    "<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p>";

pub(super) fn package_bytes(body: &str) -> Vec<u8> {
    let doc = parts::Doc::new().body(body).bytes();
    Zip::new(parts::entries_doc(&doc)).build()
}

/// A package whose `docProps/core.xml` is not well-formed. It is not a story, so `open` accepts it.
pub(super) fn package_with_bad_core() -> Vec<u8> {
    let doc = parts::Doc::new().body(BODY).bytes();
    let mut entries = parts::entries_doc(&doc);
    entries.retain(|e| e.name_bytes() != b"docProps/core.xml");
    entries.push(Entry::new("docProps/core.xml", "<a></b>"));
    Zip::new(entries).build()
}

pub(super) fn state_from(bytes: Vec<u8>, limits: &Limits) -> State {
    State::open(bytes, limits).unwrap()
}

pub(super) fn state(body: &str) -> State {
    state_from(package_bytes(body), &Limits::default())
}

pub(super) fn id(s: &State, name: &str) -> PartId {
    s.parts().id(name).unwrap()
}

pub(super) fn text_nodes(tree: &SpanTree) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![tree.document().id()];
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

pub(super) fn first_text(tree: &SpanTree) -> String {
    let id = *text_nodes(tree).first().unwrap();
    tree.node_ref(id).text().into_owned()
}

pub(super) fn edit_first_text(tx: &mut Tx<'_>, with: &str) {
    let tree = tx.part_mut(MAIN).unwrap();
    let id = *text_nodes(tree).first().unwrap();
    tree.edit().set_text(id, with).unwrap();
}

/// The keys of the blocks of the body, in order, with their kinds.
pub(super) fn body_blocks(view: &PartsView<'_>) -> Vec<(BlockKey, crate::wordml::BlockKind)> {
    let story = view.story(view.id(MAIN).unwrap()).unwrap();
    story
        .blocks()
        .iter()
        .map(|e| (story.key(e), e.kind))
        .collect()
}

pub(super) fn body_keys(s: &State) -> Vec<BlockKey> {
    body_blocks(&s.parts())
        .into_iter()
        .map(|(k, _)| k)
        .collect()
}

/// `<w:p>` with one run of text.
pub(super) fn para_xml(text: &str) -> String {
    format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>")
}

/// A body of paragraphs with these texts.
pub(super) fn paragraphs(texts: &[&str]) -> String {
    texts.iter().map(|t| para_xml(t)).collect()
}

/// The key of a node of the main part, as the transaction sees it.
pub(super) fn key_in(tx: &Tx<'_>, node: NodeId) -> BlockKey {
    let view = tx.parts();
    let name = view.name(view.id(MAIN).unwrap()).unwrap();
    BlockKey::new(Arc::clone(name), node)
}

/// A new paragraph element with this text, not yet in the tree.
pub(super) fn new_paragraph(tx: &mut Tx<'_>, text: &str) -> NodeId {
    let tree = tx.part_mut(MAIN).unwrap();
    let mut e = tree.edit();
    let p = e.create_element(W, "p").unwrap();
    let r = e.create_element(W, "r").unwrap();
    let t = e.create_element(W, "t").unwrap();
    let content = e.create_text(text).unwrap();
    e.append_child(t, content).unwrap();
    e.append_child(r, t).unwrap();
    e.append_child(p, r).unwrap();
    p
}

/// Puts a new element before the block `before`, in the same parent, and returns its key.
pub(super) fn put_before(tx: &mut Tx<'_>, node: NodeId, before: &BlockKey) -> BlockKey {
    let tree = tx.part_mut(MAIN).unwrap();
    let parent = tree.node_ref(before.node()).parent().unwrap().id();
    tree.edit()
        .insert_before(parent, Some(before.node()), node)
        .unwrap();
    key_in(tx, node)
}

/// Puts a new element after the block `after`, in the same parent, and returns its key.
pub(super) fn put_after(tx: &mut Tx<'_>, node: NodeId, after: &BlockKey) -> BlockKey {
    let tree = tx.part_mut(MAIN).unwrap();
    let at = tree.node_ref(after.node());
    let parent = at.parent().unwrap().id();
    let next = at.next_sibling().map(|n| n.id());
    tree.edit().insert_before(parent, next, node).unwrap();
    key_in(tx, node)
}

/// Takes the element of a block out of the tree. The node keeps its id.
pub(super) fn detach(tx: &mut Tx<'_>, key: &BlockKey) {
    tx.part_mut(MAIN)
        .unwrap()
        .edit()
        .detach(key.node())
        .unwrap();
}

pub(super) const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub(super) const HEADER: &str = "word/header1.xml";

/// A package whose body ends in a section that names one header, with paragraphs in each.
pub(super) fn package_with_header(body: &[&str], header: &[&str]) -> Vec<u8> {
    let document = format!(
        "<w:document xmlns:w=\"{W}\" xmlns:r=\"{R_NS}\"><w:body>{}<w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId9\"/></w:sectPr></w:body></w:document>",
        paragraphs(body)
    );
    let header_xml = format!("<w:hdr xmlns:w=\"{W}\">{}</w:hdr>", paragraphs(header));
    let rels = format!(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId9\" Type=\"{R_NS}/header\" Target=\"header1.xml\"/><Relationship Id=\"rId10\" Type=\"{R_NS}/hyperlink\" Target=\"https://example.com/\" TargetMode=\"External\"/><Relationship Id=\"rId11\" Type=\"{R_NS}/image\" Target=\"media/missing.png\"/></Relationships>"
    );
    let types = parts::content_types().replace(
        "</Types>",
        "<Override PartName=\"/word/header1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml\"/></Types>",
    );
    let mut entries = parts::entries_with(document.as_bytes(), &parts::root_rels(), &types);
    entries.push(Entry::new(HEADER, header_xml));
    entries.push(Entry::new("word/_rels/document.xml.rels", rels));
    Zip::new(entries).build()
}

/// A package with two sections, the first in a paragraph, each naming its own header.
pub(super) fn package_with_two_headers() -> Vec<u8> {
    let sect = |rid: &str| {
        format!("<w:sectPr><w:headerReference w:type=\"default\" r:id=\"{rid}\"/></w:sectPr>")
    };
    let document = format!(
        "<w:document xmlns:w=\"{W}\" xmlns:r=\"{R_NS}\"><w:body>{}<w:p><w:pPr>{}</w:pPr></w:p>{}{}</w:body></w:document>",
        para_xml("a"),
        sect("rId9"),
        para_xml("b"),
        sect("rId12")
    );
    let header = |text: &str| format!("<w:hdr xmlns:w=\"{W}\">{}</w:hdr>", para_xml(text));
    let rels = format!(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId9\" Type=\"{R_NS}/header\" Target=\"header1.xml\"/><Relationship Id=\"rId12\" Type=\"{R_NS}/header\" Target=\"header2.xml\"/></Relationships>"
    );
    let types = parts::content_types().replace(
        "</Types>",
        "<Override PartName=\"/word/header1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml\"/><Override PartName=\"/word/header2.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml\"/></Types>",
    );
    let mut entries = parts::entries_with(document.as_bytes(), &parts::root_rels(), &types);
    entries.push(Entry::new("word/header1.xml", header("first header")));
    entries.push(Entry::new("word/header2.xml", header("second header")));
    entries.push(Entry::new("word/_rels/document.xml.rels", rels));
    Zip::new(entries).build()
}

/// A comments part with one comment of one paragraph, related from the main part.
pub(super) fn comments_with_paragraph() -> crate::package::NewPart<'static> {
    crate::package::NewPart {
        name: "word/comments.xml",
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
        bytes: format!(
            "<w:comments xmlns:w=\"{W}\"><w:comment w:id=\"0\" w:author=\"a\"><w:p><w:r><w:t>note</w:t></w:r></w:p></w:comment></w:comments>"
        )
        .into_bytes(),
        related_from: Some((MAIN, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments")),
    }
}

/// A footnotes part with one footnote of one paragraph, related from the main part.
pub(super) fn footnotes_with_paragraph() -> crate::package::NewPart<'static> {
    crate::package::NewPart {
        name: "word/footnotes.xml",
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml",
        bytes: format!(
            "<w:footnotes xmlns:w=\"{W}\"><w:footnote w:id=\"1\"><w:p><w:r><w:t>foot</w:t></w:r></w:p></w:footnote></w:footnotes>"
        )
        .into_bytes(),
        related_from: Some((MAIN, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes")),
    }
}

/// The refs of every block of every story, in story order, as `part: refs` lines.
pub(super) fn all_refs(s: &State) -> Vec<String> {
    let layout = s.parts().layout(false).unwrap();
    layout
        .stories
        .iter()
        .map(|story| {
            let refs: Vec<String> = story
                .blocks
                .iter()
                .map(|b| {
                    s.refs
                        .ref_of(&b.key)
                        .map_or("-".to_owned(), |r| r.to_string())
                })
                .collect();
            format!("{}: {}", story.part, refs.join(" "))
        })
        .collect()
}

fn parsed_parts(s: &State) -> Vec<&str> {
    s.slots
        .iter()
        .filter(|slot| slot.cell.get().is_some())
        .map(|slot| &*slot.name)
        .collect()
}

#[test]
fn open_parses_the_stories_and_nothing_else() {
    let s = state(BODY);
    assert_eq!(parsed_parts(&s), [MAIN], "the body is the only story");
    let core = id(&s, CORE);
    assert!(s.parts().tree(core).is_ok());
    assert_eq!(
        parsed_parts(&s),
        [MAIN, CORE],
        "a part is parsed when it is asked for"
    );
}

#[test]
fn a_story_that_does_not_parse_refuses_the_open() {
    let e = State::open(package_bytes("<w:p></w:q>"), &Limits::default())
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::MalformedInput);
    assert!(e.message().starts_with(MAIN), "{}", e.message());
}

#[test]
fn a_package_with_no_main_part_is_refused() {
    let entries = vec![
        Entry::new("[Content_Types].xml", parts::content_types()),
        Entry::new("docProps/core.xml", "<a/>"),
    ];
    let e = State::open(Zip::new(entries).build(), &Limits::default())
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::MalformedInput);
}

#[test]
fn a_part_is_found_ignoring_ascii_case_and_named_as_the_package_spells_it() {
    let s = state(BODY);
    let main = id(&s, MAIN);
    assert_eq!(s.parts().id("WORD/Document.XML"), Some(main));
    assert_eq!(&**s.parts().name(main).unwrap(), MAIN);
    assert!(s.parts().contains(CORE));
    assert!(!s.parts().contains("word/styles.xml"));
    assert!(s.parts().tree_opt("word/styles.xml").unwrap().is_none());
    assert!(s.parts().tree_opt(MAIN).unwrap().is_some());
}

#[test]
fn a_tree_is_parsed_once_and_the_next_state_shares_it() {
    let s = state(BODY);
    let main = id(&s, MAIN);
    let first = s.parts().tree(main).unwrap() as *const SpanTree;
    assert_eq!(s.parts().tree(main).unwrap() as *const SpanTree, first);
    let next = s.begin().commit().unwrap().0;
    assert_eq!(next.parts().tree(main).unwrap() as *const SpanTree, first);
}

#[test]
fn a_failed_parse_is_kept_and_reported_each_time() {
    let s = state_from(package_with_bad_core(), &Limits::default());
    let core = id(&s, CORE);
    let a = s.parts().tree(core).err().unwrap();
    let b = s.parts().tree(core).err().unwrap();
    assert_eq!(a.code(), ErrorCode::MalformedInput);
    assert_eq!(a, b);
    assert!(a.message().starts_with(CORE), "{}", a.message());
}

#[test]
fn a_part_number_that_is_not_the_packages_is_internal() {
    let s = state(BODY);
    let mut index = (*s.index).clone();
    let bogus = index.add("word/extra.xml").unwrap();
    assert_eq!(index.len(), s.index.len() + 1);
    let e = s.parts().tree(bogus).err().unwrap();
    assert_eq!(e.code(), ErrorCode::Internal);
}

#[test]
fn an_unedited_state_saves_the_input_bytes() {
    let bytes = package_bytes(BODY);
    let s = state_from(bytes.clone(), &Limits::default());
    assert_eq!(s.to_package().unwrap().write().unwrap(), bytes);
    // Reading a part does not edit it.
    let main = id(&s, MAIN);
    s.parts().tree(main).unwrap();
    assert_eq!(s.to_package().unwrap().write().unwrap(), bytes);
}

#[test]
fn staging_clones_the_part_and_leaves_the_base_alone() {
    let bytes = package_bytes(BODY);
    let s = state_from(bytes.clone(), &Limits::default());
    let main = id(&s, MAIN);
    let mut tx = s.begin();
    edit_first_text(&mut tx, "edited");
    assert_eq!(first_text(tx.parts().tree(main).unwrap()), "edited");
    assert_eq!(first_text(s.parts().tree(main).unwrap()), "one");
    assert_eq!(first_text(tx.base().parts().tree(main).unwrap()), "one");
    tx.discard();
    assert_eq!(first_text(s.parts().tree(main).unwrap()), "one");
    assert_eq!(s.to_package().unwrap().write().unwrap(), bytes);
}

#[test]
fn only_a_part_that_is_asked_for_is_copied() {
    let s = state(BODY);
    let mut tx = s.begin();
    edit_first_text(&mut tx, "edited");
    let staged: Vec<PartId> = tx.staged.keys().copied().collect();
    assert_eq!(staged, [id(&s, MAIN)]);
}

#[test]
fn commit_builds_the_next_state_and_the_old_one_is_unchanged() {
    let bytes = package_bytes(BODY);
    let s = state_from(bytes.clone(), &Limits::default());
    let (main, core) = (id(&s, MAIN), id(&s, CORE));
    s.parts().tree(core).unwrap();
    let mut tx = s.begin();
    edit_first_text(&mut tx, "edited");
    let next = tx.commit().unwrap().0;
    assert_eq!(first_text(next.parts().tree(main).unwrap()), "edited");
    assert_eq!(first_text(s.parts().tree(main).unwrap()), "one");
    assert_eq!(s.to_package().unwrap().write().unwrap(), bytes);
    let saved = next.to_package().unwrap().write().unwrap();
    assert_ne!(saved, bytes);
    let reopened = state_from(saved, &Limits::default());
    assert_eq!(first_text(reopened.parts().tree(main).unwrap()), "edited");
    // The part nobody changed is the same cell in both states, so it was not copied.
    let old = s.slot(core).unwrap();
    let new = next.slot(core).unwrap();
    assert!(Arc::ptr_eq(&old.cell, &new.cell));
    assert!(!Arc::ptr_eq(
        &s.slot(main).unwrap().cell,
        &next.slot(main).unwrap().cell
    ));
}

#[test]
fn a_part_that_was_edited_is_serialized_into_the_package_and_other_parts_are_copied_raw() {
    let bytes = package_bytes(BODY);
    let s = state_from(bytes.clone(), &Limits::default());
    let mut tx = s.begin();
    edit_first_text(&mut tx, "edited");
    let next = tx.commit().unwrap().0;
    let main = next.slot(id(&next, MAIN)).unwrap();
    let a = main.serialized().unwrap();
    assert_eq!(
        Some(a.clone()),
        main.serialized(),
        "the same bytes each time"
    );
    assert!(String::from_utf8_lossy(&a).contains("edited"));
    assert!(
        next.slot(id(&next, CORE)).unwrap().serialized().is_none(),
        "an unedited part is not serialized"
    );
    let original = Package::open(bytes, &Limits::default()).unwrap();
    let saved = next.to_package().unwrap();
    for name in [CORE, "word/_rels/document.xml.rels", "[Content_Types].xml"] {
        if original.contains(name) {
            assert_eq!(
                saved.part(name).unwrap(),
                original.part(name).unwrap(),
                "{name}"
            );
        }
    }
}

pub(super) const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// Every node reachable from the document node, with its name and its parent.
fn snapshot(tree: &SpanTree) -> Vec<(NodeId, String, Option<NodeId>)> {
    let mut out = Vec::new();
    let mut stack = vec![tree.document().id()];
    while let Some(n) = stack.pop() {
        let node = tree.node_ref(n);
        out.push((n, node.name().to_owned(), node.parent().map(|p| p.id())));
        stack.extend(node.children().map(|c| c.id()));
    }
    out
}

#[test]
fn node_ids_survive_the_clone_and_every_edit() {
    let s = state("<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p><w:p/>");
    let main = id(&s, MAIN);
    let before = snapshot(s.parts().tree(main).unwrap());
    assert!(before.len() > 10);
    let mut tx = s.begin();
    let tree = tx.part_mut(MAIN).unwrap();
    assert_eq!(snapshot(tree), before, "a clone has the same ids");
    let body = tree.root().child_elements().next().unwrap().id();
    let paragraphs: Vec<NodeId> = tree
        .node_ref(body)
        .child_elements()
        .filter(|c| c.local_name() == "p")
        .map(|c| c.id())
        .collect();
    let texts = text_nodes(tree);
    {
        let mut e = tree.edit();
        e.set_text(texts[0], "edited").unwrap();
        let copy = e.clone_subtree(paragraphs[0]).unwrap();
        e.insert_before(body, paragraphs.get(2).copied(), copy)
            .unwrap();
        e.detach(paragraphs[1]).unwrap();
        let fresh = e.create_element(W, "p").unwrap();
        e.append_child(body, fresh).unwrap();
    }
    for (node, name, parent) in &before {
        let now = tree.node_ref(*node);
        assert_eq!(now.name(), name, "{node:?}");
        if *node == paragraphs[1] {
            assert!(now.parent().is_none(), "the detached paragraph");
        } else {
            assert_eq!(now.parent().map(|p| p.id()), *parent, "{node:?}");
        }
    }
    assert!(
        tree.node_ref(paragraphs[1]).first_child().is_some(),
        "a detached node keeps its subtree"
    );
    assert_eq!(
        snapshot(s.parts().tree(main).unwrap()),
        before,
        "the base is untouched"
    );
}

#[test]
fn package_plumbing_and_unknown_names_cannot_be_staged() {
    let s = state(BODY);
    let mut tx = s.begin();
    for name in [
        "[Content_Types].xml",
        "_rels/.rels",
        "word/_rels/document.xml.rels",
        "no/such.xml",
    ] {
        let e = tx.part_mut(name).err().unwrap();
        assert_eq!(e.code(), ErrorCode::Internal, "{name}");
        assert!(e.message().starts_with("internal:"), "{name}");
    }
    assert!(tx.staged.is_empty());
}

#[test]
fn a_part_that_cannot_be_read_cannot_be_staged() {
    let s = state_from(package_with_bad_core(), &Limits::default());
    let mut tx = s.begin();
    let e = tx.part_mut(CORE).err().unwrap();
    assert_eq!(e.code(), ErrorCode::MalformedInput);
    assert!(tx.staged.is_empty());
}

#[test]
fn commit_refuses_a_part_over_the_size_limit_and_changes_nothing() {
    let limits = Limits {
        max_entry_bytes: 4000,
        ..Limits::default()
    };
    let bytes = package_bytes(BODY);
    let s = state_from(bytes.clone(), &limits);
    let mut tx = s.begin();
    edit_first_text(&mut tx, &"x".repeat(5000));
    let e = tx.commit().err().unwrap();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    assert!(e.message().contains(MAIN), "{}", e.message());
    assert_eq!(s.to_package().unwrap().write().unwrap(), bytes);
    assert_eq!(first_text(s.parts().tree(id(&s, MAIN)).unwrap()), "one");
}

#[test]
fn a_commit_that_staged_nothing_is_the_same_state() {
    let s = state(BODY);
    let next = s.begin().commit().unwrap().0;
    let main = id(&s, MAIN);
    assert_eq!(
        s.slot(main).unwrap().generation,
        next.slot(main).unwrap().generation
    );
    assert!(Arc::ptr_eq(
        &s.slot(main).unwrap().cell,
        &next.slot(main).unwrap().cell
    ));
}

#[test]
fn a_state_is_send_and_not_sync() {
    fn send<T: Send>() {}
    send::<State>();
    // The usual trick for "not Sync": this names `some_item` through two impls, which is ambiguous,
    // and so does not compile, exactly when the type is `Sync`.
    trait AmbiguousIfSync<A> {
        fn some_item() {}
    }
    impl<T: ?Sized> AmbiguousIfSync<()> for T {}
    impl<T: ?Sized + Sync> AmbiguousIfSync<u8> for T {}
    let _ = <State as AmbiguousIfSync<_>>::some_item;
}

#[test]
fn a_package_without_content_types_or_root_relationships_is_refused() {
    let doc = parts::Doc::new().body(BODY).bytes();
    for (without, mentions) in [
        ("[Content_Types].xml", "no part has a content type"),
        (
            "_rels/.rels",
            "nothing says which part is the main document",
        ),
    ] {
        let mut entries = parts::entries_doc(&doc);
        entries.retain(|e| e.name_bytes() != without.as_bytes());
        let e = State::open(Zip::new(entries).build(), &Limits::default())
            .err()
            .unwrap();
        assert_eq!(e.code(), ErrorCode::MalformedInput, "{without}");
        assert!(
            e.message().contains(without) && e.message().contains(mentions),
            "{}",
            e.message()
        );
    }
}
