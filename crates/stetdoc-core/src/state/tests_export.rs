//! Tests of `export` and `resume`, of documents with more than one story, and of what happens when
//! the story list changes.

use super::tests::*;
use super::tests_edit::{refs, snapshot};
use super::*;
use crate::api::error::ErrorCode;
use crate::package::NewPart;
use crate::refs::{Claim, NamespaceId, NamespaceOrigin};
use crate::wordml::StoryKind;

fn recorded(e: &Exported) -> Recorded {
    Recorded {
        base: e.base,
        head_sha256: e.head_sha256,
        refs_sha256: e.refs_sha256,
    }
}

fn resume(e: &Exported, original: &[u8]) -> Result<State, StetError> {
    State::resume(
        e.head.clone(),
        &e.refs,
        &recorded(e),
        Some(original),
        &Limits::default(),
    )
}

/// Inserts a paragraph at the end of the header, in the transaction.
fn header_insert(tx: &mut Tx<'_>, text: &str) -> BlockKey {
    let tree = tx.part_mut(HEADER).unwrap();
    let root = tree.root().id();
    let p = {
        let mut e = tree.edit();
        let p = e.create_element(W, "p").unwrap();
        let r = e.create_element(W, "r").unwrap();
        let t = e.create_element(W, "t").unwrap();
        let content = e.create_text(text).unwrap();
        e.append_child(t, content).unwrap();
        e.append_child(r, t).unwrap();
        e.append_child(p, r).unwrap();
        e.append_child(root, p).unwrap();
        p
    };
    let view = tx.parts();
    BlockKey::new(Arc::clone(view.name(view.id(HEADER).unwrap()).unwrap()), p)
}

#[test]
fn stories_are_numbered_in_story_order() {
    let s = state_from(
        package_with_header(&["a", "b"], &["h1", "h2"]),
        &Limits::default(),
    );
    assert_eq!(
        all_refs(&s),
        ["word/document.xml: p1 p2", "word/header1.xml: p3 p4"]
    );
    let list: Arc<StoryList> = s.parts().story_specs().unwrap();
    let kinds: Vec<StoryKind> = list.0.iter().map(|spec| spec.kind).collect();
    assert_eq!(kinds, [StoryKind::Body, StoryKind::Header]);
    // Only the stories were parsed, and the relationships were read as plain data: a hyperlink is
    // external and an image that is not in the package is invalid, and neither fails the open.
    assert_eq!(parsed_parts(&s), [MAIN, HEADER]);
    let rels = s.parts().rels(id(&s, MAIN)).unwrap();
    use crate::wordml::RelTarget;
    assert_eq!(
        rels.get("rId9").unwrap().target,
        RelTarget::Part(HEADER.to_owned())
    );
    assert_eq!(
        rels.get("rId10").unwrap().target,
        RelTarget::External("https://example.com/".to_owned())
    );
    assert_eq!(rels.get("rId11").unwrap().target, RelTarget::Invalid);
}

fn parsed_parts(s: &State) -> Vec<&str> {
    s.slots
        .iter()
        .filter(|slot| slot.cell.get().is_some())
        .map(|slot| &*slot.name)
        .collect()
}

#[test]
fn a_state_with_history_is_the_same_after_export_and_resume() {
    let original = package_with_header(&["a", "b", "c"], &["h1"]);
    let s = state_from(original.clone(), &Limits::default());
    let keys = body_keys(&s);
    // Version 1: a deletion in the body and an insertion in the header.
    let mut tx = s.begin();
    detach(&mut tx, &keys[1]);
    header_insert(&mut tx, "h2");
    let (one, _) = tx.commit().unwrap();
    // Version 2: a merge, and a new paragraph.
    let keys = body_keys(&one);
    let mut tx = one.begin();
    detach(&mut tx, &keys[1]);
    tx.merge(&keys[0], &keys[1], keys[0].clone()).unwrap();
    let n = new_paragraph(&mut tx, "new");
    put_after(&mut tx, n, &keys[0]);
    let (two, _) = tx.commit().unwrap();
    assert_eq!(two.base_token().version(), 2);
    let exported = two.export().unwrap();
    let back = resume(&exported, &original).unwrap();
    assert_eq!(back.base_token(), two.base_token());
    assert_eq!(all_refs(&back), all_refs(&two));
    for token in ["p1", "p2", "p3", "p4", "p5", "p6", "t1"] {
        match (
            two.resolve(Claim::Implied, token),
            back.resolve(Claim::Implied, token),
        ) {
            (Ok(_), Ok(_)) => {}
            (Err(a), Err(b)) => assert_eq!(a, b, "{token}"),
            (a, b) => panic!("{token}: {a:?} against {b:?}"),
        }
    }
    // The head is what was exported, and saving the resumed state gives it back.
    assert_eq!(back.to_package().unwrap().write().unwrap(), exported.head);
    // And exporting it again gives the same bytes, so the saved form is canonical.
    let again = back.export().unwrap();
    assert_eq!(again.refs, exported.refs);
    assert_eq!(again.head, exported.head);
    assert_eq!(again.base, exported.base);
}

#[test]
fn the_commit_after_a_resume_is_the_commit_of_an_unbroken_session() {
    let original = package_bytes(&paragraphs(&["a", "b", "c"]));
    let s = state_from(original.clone(), &Limits::default());
    let mut tx = s.begin();
    detach(&mut tx, &body_keys(&s)[0]);
    let (one, _) = tx.commit().unwrap();
    let resumed = resume(&one.export().unwrap(), &original).unwrap();
    let edit = |state: &State| {
        let keys = body_keys(state);
        let mut tx = state.begin();
        let n = new_paragraph(&mut tx, "after");
        put_after(&mut tx, n, &keys[1]);
        tx.touch(&keys[0]);
        let (next, committed) = tx.commit().unwrap();
        (
            next.base_token(),
            committed
                .minted
                .iter()
                .map(|(_, r)| r.to_string())
                .collect::<Vec<_>>(),
            refs(&next),
        )
    };
    assert_eq!(
        edit(&one),
        edit(&resumed),
        "the same lineage, the same refs"
    );
    // Claims from before the break still work after it.
    let before = one.base_token();
    assert!(resumed.begin().resolve(Claim::Base(&before), "p2").is_ok());
}

#[test]
fn a_saved_form_that_does_not_match_its_digests_is_refused() {
    let original = package_bytes(&paragraphs(&["a", "b"]));
    let s = state_from(original.clone(), &Limits::default());
    let e = s.export().unwrap();
    let refuses = |head: Vec<u8>, refs: Vec<u8>, rec: Recorded, original: Option<&[u8]>| {
        State::resume(head, &refs, &rec, original, &Limits::default())
            .err()
            .map(|e| e.code())
    };
    let good = recorded(&e);
    assert_eq!(
        refuses(
            e.head.clone(),
            e.refs.clone(),
            good.clone(),
            Some(&original)
        ),
        None
    );
    // One byte of the head, or of the ref map.
    let mut head = e.head.clone();
    if let Some(b) = head.get_mut(40) {
        *b ^= 1;
    }
    assert_eq!(
        refuses(head, e.refs.clone(), good.clone(), Some(&original)),
        Some(ErrorCode::CorruptComplement)
    );
    let mut refs = e.refs.clone();
    if let Some(b) = refs.get_mut(30) {
        *b ^= 1;
    }
    assert_eq!(
        refuses(e.head.clone(), refs, good.clone(), Some(&original)),
        Some(ErrorCode::CorruptComplement)
    );
    // A recorded token for another state than the ref map's.
    let mut tx = s.begin();
    tx.touch(&body_keys(&s)[0]);
    let other = tx.commit().unwrap().0.base_token();
    let wrong = Recorded {
        base: other,
        ..good.clone()
    };
    assert_eq!(
        refuses(e.head.clone(), e.refs.clone(), wrong, Some(&original)),
        Some(ErrorCode::CorruptComplement)
    );
    // The original bytes: needed to check an opened namespace, and they must be the original's.
    assert_eq!(
        refuses(e.head.clone(), e.refs.clone(), good.clone(), None),
        Some(ErrorCode::CorruptComplement)
    );
    assert_eq!(
        refuses(
            e.head.clone(),
            e.refs.clone(),
            good.clone(),
            Some(b"another")
        ),
        Some(ErrorCode::CorruptComplement)
    );
}

#[test]
fn a_head_that_no_longer_opens_is_corrupt_and_a_limit_stays_a_limit() {
    let original = package_bytes(&paragraphs(&["a"]));
    let s = state_from(original.clone(), &Limits::default());
    let e = s.export().unwrap();
    // Garbage whose digest was recorded: the original opened, so this is damage.
    let garbage = b"this is not a zip file".to_vec();
    let mut rec = recorded(&e);
    rec.head_sha256 = crate::refs::sha256(&garbage);
    let err = State::resume(garbage, &e.refs, &rec, Some(&original), &Limits::default())
        .err()
        .unwrap();
    assert_eq!(err.code(), ErrorCode::CorruptComplement);
    assert!(
        err.message().contains("no longer opens"),
        "{}",
        err.message()
    );
    // A limit that the head is over is not damage.
    let tight = Limits {
        max_entry_bytes: 50,
        ..Limits::default()
    };
    let err = State::resume(
        e.head.clone(),
        &e.refs,
        &recorded(&e),
        Some(&original),
        &tight,
    )
    .err()
    .unwrap();
    assert_eq!(err.code(), ErrorCode::LimitExceeded, "{err}");
}

#[test]
fn text_changed_in_the_head_is_caught_by_the_alignment_even_with_matching_digests() {
    let original = package_bytes(&paragraphs(&["alpha", "beta"]));
    let s = state_from(original.clone(), &Limits::default());
    let e = s.export().unwrap();
    // The agent edits a paragraph in the head package, and records the new digest as well.
    let other = package_bytes(&paragraphs(&["alpha", "BETA"]));
    let mut rec = recorded(&e);
    rec.head_sha256 = crate::refs::sha256(&other);
    let err = State::resume(other, &e.refs, &rec, Some(&original), &Limits::default())
        .err()
        .unwrap();
    assert_eq!(err.code(), ErrorCode::CorruptComplement);
    assert!(
        err.message().contains("its text is not the text it had"),
        "{}",
        err.message()
    );
}

#[test]
fn two_blocks_that_differ_only_in_properties_are_left_to_the_digests() {
    // Fingerprints cover text, so two empty paragraphs with different styles can swap places in the
    // head without the alignment noticing. The digest of the head catches it. An agent that also
    // rewrites the digest has made itself another working copy, which is its own business.
    let style = |name: &str| format!("<w:p><w:pPr><w:pStyle w:val=\"{name}\"/></w:pPr></w:p>");
    let original = package_bytes(&format!("{}{}", style("A"), style("B")));
    let s = state_from(original.clone(), &Limits::default());
    let e = s.export().unwrap();
    let swapped = package_bytes(&format!("{}{}", style("B"), style("A")));
    let err = State::resume(
        swapped.clone(),
        &e.refs,
        &recorded(&e),
        Some(&original),
        &Limits::default(),
    )
    .err()
    .unwrap();
    assert_eq!(
        err.code(),
        ErrorCode::CorruptComplement,
        "the head digest differs"
    );
    let mut rec = recorded(&e);
    rec.head_sha256 = crate::refs::sha256(&swapped);
    assert!(State::resume(swapped, &e.refs, &rec, Some(&original), &Limits::default()).is_ok());
}

#[test]
fn removing_a_header_reference_retires_the_header_refs() {
    let s = state_from(
        package_with_header(&["a"], &["h1", "h2"]),
        &Limits::default(),
    );
    let mut tx = s.begin();
    {
        let tree = tx.part_mut(MAIN).unwrap();
        let sect = tree
            .root()
            .child_elements()
            .next()
            .unwrap()
            .child_elements()
            .last()
            .unwrap()
            .id();
        let reference = tree.node_ref(sect).first_child().unwrap().id();
        tree.edit().detach(reference).unwrap();
    }
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(
        next.parts().story_specs().unwrap().0.len(),
        1,
        "the header is not a story now"
    );
    assert_eq!(
        committed
            .retired
            .iter()
            .map(|(r, _)| r.to_string())
            .collect::<Vec<_>>(),
        ["p2", "p3"]
    );
    assert_eq!(
        next.resolve(Claim::Implied, "p2").unwrap_err().message(),
        "p2 was deleted in version 1."
    );
    assert!(next.resolve(Claim::Implied, "p1").is_ok());
    // The header part is still in the package, and is not a story, so asking for its story is a bug.
    assert!(next.parts().contains(HEADER));
    assert_eq!(
        next.parts().story(id(&next, HEADER)).err().unwrap().code(),
        ErrorCode::Internal
    );
}

#[test]
fn adding_a_comments_part_makes_a_new_story_and_mints_its_refs_at_commit() {
    let s = state(&paragraphs(&["a", "b"]));
    let mut tx = s.begin();
    tx.add_part(comments_with_paragraph()).unwrap();
    // In the transaction the story list already has the comments part.
    let kinds: Vec<StoryKind> = tx
        .parts()
        .story_specs()
        .unwrap()
        .0
        .iter()
        .map(|x| x.kind)
        .collect();
    assert_eq!(kinds, [StoryKind::Body, StoryKind::Comments]);
    assert_eq!(
        s.parts().story_specs().unwrap().0.len(),
        1,
        "and the base's has not"
    );
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(committed.minted.len(), 1);
    assert_eq!(
        committed.minted.first().map(|(_, r)| r.to_string()),
        Some("p3".to_owned())
    );
    assert_eq!(
        all_refs(&next),
        ["word/document.xml: p1 p2", "word/comments.xml: p3"]
    );
    assert_eq!(next.base_token().version(), 1);
    // The new state survives export and resume.
    let original = package_bytes(&paragraphs(&["a", "b"]));
    let back = resume(&next.export().unwrap(), &original).unwrap();
    assert_eq!(all_refs(&back), all_refs(&next));
    assert_eq!(back.base_token(), next.base_token());
}

#[test]
fn the_digest_of_a_commit_covers_the_parts_that_add_part_rewrote() {
    let s = state(&paragraphs(&["a"]));
    let add = |name: &'static str| {
        let mut tx = s.begin();
        tx.add_part(NewPart {
            name,
            content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
            bytes: format!("<w:comments xmlns:w=\"{W}\"/>").into_bytes(),
            related_from: Some((MAIN, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments")),
        })
        .unwrap();
        tx.commit().unwrap().0.base_token()
    };
    // Same parts added: the same lineage. Different names: the content types and the relationships
    // differ, and so does the lineage, though no tree changed.
    assert_eq!(add("word/comments.xml"), add("word/comments.xml"));
    assert_ne!(add("word/comments.xml"), add("word/comments2.xml"));
    assert_eq!(add("word/comments.xml").version(), 1);
}

#[test]
fn a_snapshot_of_a_state_does_not_change_when_a_transaction_that_added_a_part_is_discarded() {
    let s = state(&paragraphs(&["a"]));
    let before = snapshot(&s);
    let mut tx = s.begin();
    tx.add_part(comments_with_paragraph()).unwrap();
    tx.discard();
    assert_eq!(snapshot(&s), before);
}

#[test]
fn a_state_opened_under_a_derived_namespace_is_resumed_without_the_original_bytes() {
    // `compare` builds its redline under a namespace derived from its inputs, and the ref map
    // records how, so that `resume` can check it by recomputing it.
    let a = NamespaceId::of_package(b"original", 1);
    let b = NamespaceId::of_package(b"revised", 1);
    let origin = NamespaceOrigin::Derived {
        label: "compare".into(),
        parents: vec![a, b],
        extra: b"stamp".to_vec(),
    };
    let namespace = NamespaceId::derived("compare", &[a, b], b"stamp");
    let bytes = package_bytes(&paragraphs(&["a", "b"]));
    let s = State::open_in(namespace, origin, bytes, &Limits::default()).unwrap();
    assert_eq!(s.base_token().namespace(), namespace);
    assert_eq!(refs(&s), ["p1", "p2"]);
    let e = s.export().unwrap();
    let back = State::resume(
        e.head.clone(),
        &e.refs,
        &recorded(&e),
        None,
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(back.base_token(), s.base_token());
    assert_eq!(refs(&back), ["p1", "p2"]);
    // A batch written for the document that was opened from the same bytes is refused: another
    // namespace, so no ref of the redline can be taken for one of the revised document.
    let opened = state(&paragraphs(&["a", "b"])).base_token();
    assert_eq!(
        back.begin()
            .resolve(Claim::Base(&opened), "p1")
            .unwrap_err()
            .code(),
        ErrorCode::StaleRef
    );
}

#[test]
fn a_story_is_keyed_by_its_part_and_not_by_its_place_in_the_list() {
    let s = state_from(package_with_two_headers(), &Limits::default());
    let (first, second) = (id(&s, "word/header1.xml"), id(&s, "word/header2.xml"));
    let list = s.parts().story_specs().unwrap();
    let parts: Vec<&str> = list.0.iter().map(|x| &*x.part).collect();
    assert_eq!(parts, [MAIN, "word/header1.xml", "word/header2.xml"]);
    let before = s.parts().story(second).unwrap();
    assert_eq!(&**before.part(), "word/header2.xml");
    // Remove the first section's header reference. The second header moves up the list, and its
    // story is still its own, which a key by position would get wrong.
    let mut tx = s.begin();
    {
        let tree = tx.part_mut(MAIN).unwrap();
        let reference = {
            let mut found = None;
            let mut stack = vec![tree.document().id()];
            while let Some(n) = stack.pop() {
                let node = tree.node_ref(n);
                let named = node.attr(R_NS, "id");
                if node.is_element()
                    && node.local_name() == "headerReference"
                    && named.as_deref() == Some("rId9")
                {
                    found = Some(n);
                }
                stack.extend(node.children().map(|c| c.id()));
            }
            found.unwrap()
        };
        tree.edit().detach(reference).unwrap();
    }
    let after_list = tx.parts().story_specs().unwrap();
    let parts: Vec<&str> = after_list.0.iter().map(|x| &*x.part).collect();
    assert_eq!(parts, [MAIN, "word/header2.xml"]);
    let during = tx.parts().story(second).unwrap();
    assert_eq!(&**during.part(), "word/header2.xml");
    assert_eq!(during.blocks().len(), before.blocks().len());
    assert!(
        tx.parts().story(first).is_err(),
        "the first header is not a story now"
    );
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(
        &**next
            .parts()
            .story(id(&next, "word/header2.xml"))
            .unwrap()
            .part(),
        "word/header2.xml"
    );
    assert_eq!(
        committed
            .retired
            .iter()
            .map(|(r, _)| r.to_string())
            .collect::<Vec<_>>(),
        ["p4"]
    );
    assert_eq!(
        all_refs(&next),
        ["word/document.xml: p1 p2 p3", "word/header2.xml: p5"]
    );
}

#[test]
fn a_part_is_found_through_a_relationship_and_a_missing_one_is_none() {
    let s = state_from(package_with_header(&["a"], &["h"]), &Limits::default());
    let main = id(&s, MAIN);
    let header = format!("{R_NS}/header");
    let found = s.parts().related_part(main, &|t| t == header).unwrap();
    assert_eq!(found, Some(id(&s, HEADER)));
    // A relationship of another type, an external one and one that targets no part find nothing.
    let none = |kind: &str| {
        let wanted = format!("{R_NS}/{kind}");
        s.parts().related_part(main, &|t| t == wanted).unwrap()
    };
    assert_eq!(none("styles"), None);
    assert_eq!(none("hyperlink"), None, "an external target is not a part");
    assert_eq!(none("image"), None, "a target that names no part");
}
