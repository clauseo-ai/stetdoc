//! Tests of `Tx::add_part`: the new part, the parts it rewrites, and the generations it moves.

use super::derived::Dep;
use super::tests::*;
use super::*;
use crate::api::error::ErrorCode;
use crate::package::NewPart;
use crate::xml::SpanTree;
use std::cell::Cell;

// ---- add_part -------------------------------------------------------------------------------

const COMMENTS: &str = "word/comments.xml";
const DOC_RELS: &str = "word/_rels/document.xml.rels";
const CONTENT_TYPES: &str = "[Content_Types].xml";

fn comments_part() -> NewPart<'static> {
    NewPart {
        name: COMMENTS,
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
        bytes: format!("<w:comments xmlns:w=\"{W}\"/>").into_bytes(),
        related_from: Some((
            MAIN,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments",
        )),
    }
}

fn targets(tree: &SpanTree) -> Vec<String> {
    tree.root()
        .child_elements()
        .filter_map(|e| e.attr("", "Target").map(|t| t.into_owned()))
        .collect()
}

fn overrides(tree: &SpanTree) -> Vec<String> {
    tree.root()
        .child_elements()
        .filter_map(|e| e.attr("", "PartName").map(|t| t.into_owned()))
        .collect()
}

#[test]
fn an_added_part_is_visible_in_the_transaction_and_not_in_the_base() {
    let bytes = package_bytes(BODY);
    let s = state_from(bytes.clone(), &Limits::default());
    let mut tx = s.begin();
    let rid = tx.add_part(comments_part()).unwrap();
    assert_eq!(rid.as_deref(), Some("rId1"));
    let view = tx.parts();
    let comments = view.id(COMMENTS).unwrap();
    assert!(
        view.tree(comments).is_ok(),
        "parsed from the transaction's package"
    );
    assert_eq!(view.tree(comments).unwrap().root().local_name(), "comments");
    let rels = view.tree(view.id(DOC_RELS).unwrap()).unwrap();
    assert_eq!(
        targets(rels),
        ["comments.xml"],
        "the relationship part was made"
    );
    let types = view.tree(view.id(CONTENT_TYPES).unwrap()).unwrap();
    assert!(overrides(types).contains(&"/word/comments.xml".to_owned()));
    assert!(!s.parts().contains(COMMENTS), "the base has not changed");
    assert!(!s.parts().contains(DOC_RELS));
    tx.discard();
    assert!(!s.parts().contains(COMMENTS));
    assert_eq!(s.to_package().unwrap().write().unwrap(), bytes);
}

#[test]
fn add_part_moves_the_generations_it_changed_and_no_others() {
    let s = state(BODY);
    let (main, core) = (Dep::Part(id(&s, MAIN)), Dep::Part(id(&s, CORE)));
    let types = Dep::Part(id(&s, CONTENT_TYPES));
    let before = [main, core, types, Dep::Package].map(|d| s.parts().generation(d));
    let mut tx = s.begin();
    tx.add_part(comments_part()).unwrap();
    let view = tx.parts();
    let after = [main, core, types, Dep::Package].map(|d| view.generation(d));
    assert_eq!(before[0], after[0], "the main part");
    assert_eq!(before[1], after[1], "an unrelated part");
    assert_ne!(before[2], after[2], "the content types");
    assert_ne!(before[3], after[3], "the set of parts");
    let rels = Dep::Part(view.id(DOC_RELS).unwrap());
    let comments = Dep::Part(view.id(COMMENTS).unwrap());
    assert_eq!(
        view.generation(rels).epoch,
        1,
        "a new part starts at epoch 1"
    );
    assert_eq!(view.generation(comments).epoch, 1);
}

#[test]
fn what_depends_on_the_set_of_parts_or_on_the_relationships_goes_stale() {
    let s = state(BODY);
    let builds = Cell::new(0);
    let over = |view: &PartsView<'_>, deps: &[Dep]| {
        view.derived(0, deps, &|_: Option<&String>| {
            builds.set(builds.get() + 1);
            Ok(format!("{}", view.contains(COMMENTS)))
        })
        .unwrap()
    };
    let main = Dep::Part(id(&s, MAIN));
    assert_eq!(*over(&s.parts(), &[Dep::Package]), "false");
    assert_eq!(*over(&s.parts(), &[main]), "false");
    assert_eq!(builds.get(), 2);
    let mut tx = s.begin();
    tx.add_part(comments_part()).unwrap();
    assert_eq!(*over(&tx.parts(), &[Dep::Package]), "true");
    assert_eq!(builds.get(), 3);
    assert_eq!(
        *over(&tx.parts(), &[main]),
        "false",
        "a part the add did not touch stays valid"
    );
    assert_eq!(builds.get(), 3);
    let next = tx.commit().unwrap().0;
    assert_eq!(*over(&next.parts(), &[Dep::Package]), "true");
    assert_eq!(builds.get(), 3, "promoted at commit");
}

#[test]
fn commit_adopts_the_new_package_and_the_old_state_is_unchanged() {
    let bytes = package_bytes(BODY);
    let s = state_from(bytes.clone(), &Limits::default());
    let mut tx = s.begin();
    tx.add_part(comments_part()).unwrap();
    let next = tx.commit().unwrap().0;
    assert!(next.parts().contains(COMMENTS));
    assert_eq!(next.parts().generation(Dep::Package).epoch, 1);
    assert_eq!(s.parts().generation(Dep::Package).epoch, 0);
    assert!(!s.parts().contains(COMMENTS));
    assert_eq!(s.to_package().unwrap().write().unwrap(), bytes);
    let rels = next.parts().tree(id(&next, DOC_RELS)).unwrap();
    assert_eq!(targets(rels), ["comments.xml"]);
    let types = next.parts().tree(id(&next, CONTENT_TYPES)).unwrap();
    assert!(overrides(types).contains(&"/word/comments.xml".to_owned()));
    // Saved and read again, the part is there.
    let saved = next.to_package().unwrap().write().unwrap();
    let reopened = state_from(saved, &Limits::default());
    assert!(reopened.parts().contains(COMMENTS));
    assert_eq!(
        first_text(reopened.parts().tree(id(&reopened, MAIN)).unwrap()),
        "one"
    );
}

#[test]
fn a_failed_add_part_changes_nothing() {
    let s = state(BODY);
    let mut tx = s.begin();
    let mut again = comments_part();
    again.name = MAIN;
    let e = tx.add_part(again).err().unwrap();
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    assert!(tx.added.is_none());
    assert_eq!(tx.index.len(), s.index.len());
    // A good one still works afterwards.
    tx.add_part(comments_part()).unwrap();
    assert!(tx.parts().contains(COMMENTS));
    // And a second failure after a success leaves the success alone.
    let mut dup = comments_part();
    dup.name = "word/COMMENTS.xml";
    assert!(tx.add_part(dup).is_err());
    assert!(tx.parts().contains(COMMENTS));
    assert_eq!(
        tx.index.len(),
        s.index.len() + 2,
        "the part and its relationships part"
    );
}

#[test]
fn an_added_part_can_be_staged_and_edited_in_the_same_transaction() {
    let s = state(BODY);
    let mut tx = s.begin();
    tx.add_part(comments_part()).unwrap();
    let tree = tx.part_mut(COMMENTS).unwrap();
    let root = tree.root().id();
    {
        let mut e = tree.edit();
        let c = e.create_element(W, "comment").unwrap();
        e.set_attr(c, W, "id", "0").unwrap();
        e.append_child(root, c).unwrap();
    }
    let next = tx.commit().unwrap().0;
    let saved = next.to_package().unwrap();
    let bytes = saved.part(COMMENTS).unwrap().unwrap();
    assert!(
        String::from_utf8_lossy(&bytes).contains("<w:comment"),
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let reopened = state_from(saved.write().unwrap(), &Limits::default());
    let tree = reopened.parts().tree(id(&reopened, COMMENTS)).unwrap();
    assert_eq!(tree.root().child_elements().count(), 1);
}

/// The kinds of the stories of a view, in story order.
fn story_kinds(tx: &Tx<'_>) -> Vec<crate::wordml::StoryKind> {
    let list = tx.parts().story_specs().unwrap();
    list.0.iter().map(|x| x.kind).collect()
}

#[test]
fn a_second_add_part_in_one_transaction_rebuilds_the_relationships_and_the_story_list() {
    use crate::wordml::StoryKind::{Body, Comments, Footnotes};
    let s = state(&paragraphs(&["a", "b"]));
    let main = id(&s, MAIN);
    let mut tx = s.begin();
    tx.add_part(footnotes_with_paragraph()).unwrap();
    // What the transaction reads between the two calls is cached under the first call's generation.
    assert_eq!(story_kinds(&tx), [Body, Footnotes]);
    assert_eq!(tx.parts().rels(main).unwrap().iter().count(), 1);
    let first = tx.parts().generation(Dep::Package);
    tx.add_part(comments_with_paragraph()).unwrap();
    assert_ne!(
        tx.parts().generation(Dep::Package),
        first,
        "the second call moves the set of parts"
    );
    // Both are rebuilt from the package with the two calls in it.
    assert_eq!(tx.parts().rels(main).unwrap().iter().count(), 2);
    assert_eq!(story_kinds(&tx), [Body, Footnotes, Comments]);
    let (rels, from_scratch) = (
        tx.parts().rels(main).unwrap(),
        super::stories::rels_from_scratch(&tx.parts(), main),
    );
    assert_eq!(*rels, from_scratch);
}

#[test]
fn a_second_add_part_in_one_transaction_mints_the_refs_of_both_new_stories_and_resumes() {
    let original = package_bytes(&paragraphs(&["a", "b"]));
    let s = state_from(original.clone(), &Limits::default());
    let mut tx = s.begin();
    tx.add_part(footnotes_with_paragraph()).unwrap();
    let _ = story_kinds(&tx);
    tx.add_part(comments_with_paragraph()).unwrap();
    // The commit mints a ref for the paragraph of each new story, in story order.
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(
        committed
            .minted
            .iter()
            .map(|(_, r)| r.to_string())
            .collect::<Vec<_>>(),
        ["p3", "p4"]
    );
    assert_eq!(
        all_refs(&next),
        [
            "word/document.xml: p1 p2",
            "word/footnotes.xml: p3",
            "word/comments.xml: p4"
        ]
    );
    // What the state exports resumes: the blob and the head agree, and so do the refs.
    let e = next.export().unwrap();
    let rec = Recorded {
        base: e.base,
        head_sha256: e.head_sha256,
        refs_sha256: e.refs_sha256,
    };
    let back = State::resume(e.head, &e.refs, &rec, Some(&original), &Limits::default()).unwrap();
    assert_eq!(all_refs(&back), all_refs(&next));
    assert_eq!(
        back.parts().rels(id(&back, MAIN)).unwrap().iter().count(),
        2
    );
}

#[test]
fn what_is_cached_between_two_add_parts_does_not_survive_the_commit() {
    let s = state(&paragraphs(&["a"]));
    let count_parts = |view: &PartsView<'_>| -> Arc<usize> {
        view.derived(7, &[Dep::Package], &|_: Option<&usize>| {
            Ok(view.index().len())
        })
        .unwrap()
    };
    let mut tx = s.begin();
    tx.add_part(footnotes_with_paragraph()).unwrap();
    let story_list = tx.parts().story_specs().unwrap();
    let cached = count_parts(&tx.parts());
    assert_eq!(story_list.0.len(), 2);
    tx.add_part(comments_with_paragraph()).unwrap();
    // The commit does not read the part count again, so the entry that the first call left would
    // survive into the next state if the next state's generation of the set of parts did not move
    // twice.
    let (next, _) = tx.commit().unwrap();
    assert_eq!(
        *cached + 1,
        *count_parts(&next.parts()),
        "the entry was built again"
    );
    let list = next.parts().story_specs().unwrap();
    assert!(!Arc::ptr_eq(&story_list, &list));
    assert_eq!(list.0.len(), 3);
    // And what is built in the next state is valid there.
    assert!(Arc::ptr_eq(&list, &next.parts().story_specs().unwrap()));
}

#[test]
fn each_call_of_add_part_gives_the_parts_it_rewrites_an_epoch_they_never_had() {
    let s = state(&paragraphs(&["a"]));
    let types = id(&s, CONTENT_TYPES);
    let base = s.parts().generation(Dep::Part(types)).epoch;
    let package = s.parts().generation(Dep::Package).epoch;
    let mut tx = s.begin();
    tx.add_part(footnotes_with_paragraph()).unwrap();
    let footnotes = tx.parts().id("word/footnotes.xml").unwrap();
    assert_eq!(tx.parts().generation(Dep::Part(types)).epoch, base + 1);
    assert_eq!(
        tx.parts().generation(Dep::Part(footnotes)).epoch,
        1,
        "a new part starts at its call"
    );
    assert_eq!(tx.parts().generation(Dep::Package).epoch, package + 1);
    tx.add_part(comments_with_paragraph()).unwrap();
    let comments = tx.parts().id("word/comments.xml").unwrap();
    assert_eq!(
        tx.parts().generation(Dep::Part(types)).epoch,
        base + 2,
        "rewritten again"
    );
    assert_eq!(
        tx.parts().generation(Dep::Part(footnotes)).epoch,
        1,
        "not rewritten by the second"
    );
    assert_eq!(tx.parts().generation(Dep::Part(comments)).epoch, 2);
    assert_eq!(tx.parts().generation(Dep::Package).epoch, package + 2);
    let during: Vec<_> = [types, footnotes, comments]
        .iter()
        .map(|p| tx.parts().generation(Dep::Part(*p)))
        .collect();
    let (next, _) = tx.commit().unwrap();
    // The next state has the values the transaction showed, and a later transaction goes on from them.
    let after: Vec<_> = [types, footnotes, comments]
        .iter()
        .map(|p| next.parts().generation(Dep::Part(*p)))
        .collect();
    assert_eq!(after, during);
    assert_eq!(next.parts().generation(Dep::Package).epoch, package + 2);
    let mut again = next.begin();
    again.add_part(NewPart {
        name: "word/people.xml",
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.people+xml",
        bytes: b"<w15:people xmlns:w15=\"http://schemas.microsoft.com/office/word/2012/wordml\"/>".to_vec(),
        related_from: Some((MAIN, "http://schemas.microsoft.com/office/2011/relationships/people")),
    })
    .unwrap();
    assert_eq!(again.parts().generation(Dep::Part(types)).epoch, base + 3);
    assert_eq!(again.parts().generation(Dep::Package).epoch, package + 3);
}

#[test]
fn a_failed_second_add_part_leaves_the_first_as_it_was() {
    let s = state(&paragraphs(&["a"]));
    let mut tx = s.begin();
    tx.add_part(footnotes_with_paragraph()).unwrap();
    let epoch = tx.parts().generation(Dep::Package);
    let before = tx.index.len();
    let mut again = footnotes_with_paragraph();
    again.name = "word/FOOTNOTES.xml";
    assert!(tx.add_part(again).is_err());
    assert_eq!(
        tx.parts().generation(Dep::Package),
        epoch,
        "a refused call does not count"
    );
    assert_eq!(tx.index.len(), before);
    assert_eq!(tx.added.as_ref().map(|a| a.calls), Some(1));
}
