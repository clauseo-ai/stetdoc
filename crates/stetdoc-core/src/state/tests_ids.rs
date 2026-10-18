//! Tests of the ids the state mints and the sets of ids a document uses.

use super::tests::*;
use super::*;
use crate::api::error::ErrorCode;
use crate::wordml::IdKind;
use stetdoc_testkit::fixtures::{parts, zip::Zip};

const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";

/// A package whose body has paragraphs with these `w14:paraId` values.
fn with_para_ids(ids: &[&str], extra: &str) -> State {
    let body: String = ids
        .iter()
        .map(|id| format!("<w:p w14:paraId=\"{id}\"><w:r><w:t>x</w:t></w:r></w:p>"))
        .collect();
    let doc = parts::Doc::new()
        .root_attrs(&format!(" xmlns:w14=\"{W14}\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\""))
        .body(&format!("{body}{extra}"))
        .bytes();
    state_from(
        Zip::new(parts::entries_doc(&doc)).build(),
        &Limits::default(),
    )
}

fn mint(s: &State, kind: IdKind, part: &str, n: usize) -> Vec<u32> {
    let mut tx = s.begin();
    (0..n).map(|_| tx.mint_id(kind, part).unwrap()).collect()
}

#[test]
fn a_minted_id_comes_from_inputs_only() {
    let a = with_para_ids(&["00000010"], "");
    let b = with_para_ids(&["00000010"], "");
    let first = mint(&a, IdKind::ParaId, MAIN, 3);
    assert_eq!(
        first,
        mint(&b, IdKind::ParaId, MAIN, 3),
        "the same bytes and requests, the same ids"
    );
    assert_eq!(
        first,
        mint(&a, IdKind::ParaId, MAIN, 3),
        "a dry run and the real edit agree"
    );
    assert!(
        first.iter().all(|id| (1..=0x7FFF_FFFF).contains(id)),
        "{first:?}"
    );
    assert_eq!(
        first.iter().collect::<std::collections::HashSet<_>>().len(),
        3,
        "each request is different"
    );
    // The kind, the part and the version are inputs.
    assert_ne!(first[0], mint(&a, IdKind::TextId, MAIN, 1)[0]);
    assert_ne!(first[0], mint(&a, IdKind::ParaId, CORE, 1)[0]);
    let mut tx = a.begin();
    tx.touch(&body_keys(&a)[0]);
    let (next, _) = tx.commit().unwrap();
    assert_ne!(
        first[0],
        mint(&next, IdKind::ParaId, MAIN, 1)[0],
        "another version, another id"
    );
    // Other bytes, another namespace, other ids.
    let c = with_para_ids(&["00000011"], "");
    assert_ne!(first[0], mint(&c, IdKind::ParaId, MAIN, 1)[0]);
}

#[test]
fn a_thousand_ids_are_distinct_and_avoid_what_the_document_uses() {
    let used = ["00000001", "00000002", "7FFFFFFF", "00000003"];
    let s = with_para_ids(&used, "");
    let set: Arc<IdSet> = s.parts().ids(IdKind::ParaId).unwrap();
    assert_eq!(set.len(), 4);
    assert!(set.contains(2) && set.contains(0x7FFF_FFFF) && !set.contains(4));
    assert_eq!(set.max(), Some(0x7FFF_FFFF));
    let minted = mint(&s, IdKind::ParaId, MAIN, 1000);
    let distinct: std::collections::HashSet<u32> = minted.iter().copied().collect();
    assert_eq!(distinct.len(), 1000);
    assert!(minted.iter().all(|id| !set.contains(*id) && *id != 0));
}

#[test]
fn ids_in_both_branches_of_alternate_content_are_used() {
    let fallback = "<mc:AlternateContent><mc:Choice Requires=\"w14\"><w:p w14:paraId=\"0000000A\"><w:r><w:t>c</w:t></w:r></w:p></mc:Choice><mc:Fallback><w:p w14:paraId=\"0000000B\"><w:r><w:t>f</w:t></w:r></w:p></mc:Fallback></mc:AlternateContent>";
    let s = with_para_ids(&["00000001"], fallback);
    let set = s.parts().ids(IdKind::ParaId).unwrap();
    assert!(set.contains(0xA), "the choice branch");
    assert!(
        set.contains(0xB),
        "the fallback branch, where Word repeats a text box's ids"
    );
}

#[test]
fn annotation_ids_are_reported_and_not_minted() {
    let marks = "<w:p><w:bookmarkStart w:id=\"7\" w:name=\"a\"/><w:bookmarkEnd w:id=\"7\"/></w:p><w:p><w:ins w:id=\"12\" w:author=\"a\" w:date=\"2024-01-01T00:00:00Z\"><w:r><w:t>t</w:t></w:r></w:ins></w:p>";
    let s = state(marks);
    let set = s.parts().ids(IdKind::AnnotationId).unwrap();
    assert_eq!(set.max(), Some(12));
    let e = s.begin().mint_id(IdKind::AnnotationId, MAIN).unwrap_err();
    assert_eq!(e.code(), ErrorCode::Internal);
}

#[test]
fn an_id_asked_for_a_part_that_does_not_exist_is_internal() {
    let s = state(BODY);
    let e = s
        .begin()
        .mint_id(IdKind::ParaId, "word/nowhere.xml")
        .unwrap_err();
    assert_eq!(e.code(), ErrorCode::Internal);
}

#[test]
fn the_set_of_ids_is_a_cache_over_the_parts_it_reads() {
    let s = with_para_ids(&["00000001"], "");
    let a = s.parts().ids(IdKind::ParaId).unwrap();
    assert!(
        Arc::ptr_eq(&a, &s.parts().ids(IdKind::ParaId).unwrap()),
        "cached"
    );
    let mut tx = s.begin();
    {
        let tree = tx.part_mut(MAIN).unwrap();
        let first = tree
            .root()
            .child_elements()
            .next()
            .unwrap()
            .first_child()
            .unwrap()
            .id();
        tree.edit()
            .set_attr(first, W14, "paraId", "00000099")
            .unwrap();
    }
    let during = tx.parts().ids(IdKind::ParaId).unwrap();
    assert!(
        during.contains(0x99) && !during.contains(1),
        "the transaction sees its own edit"
    );
    assert!(a.contains(1) && !a.contains(0x99), "and the base does not");
    let (next, _) = tx.commit().unwrap();
    assert!(next.parts().ids(IdKind::ParaId).unwrap().contains(0x99));
    assert!(s.parts().ids(IdKind::ParaId).unwrap().contains(1));
}
