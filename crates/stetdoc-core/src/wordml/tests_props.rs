//! Tests for `Props` and `PropsRef`. Each test names what it proves.

use super::names::{W_S, W_T, W14};
use super::props::{Ns, Props, PropsRef};
use crate::api::limits::Limits;
use crate::xml::{Bytes, NodeRef, SpanTree};
use std::borrow::Cow;

fn tree(xml: &str) -> SpanTree {
    SpanTree::parse("t.xml", Bytes::from(xml.as_bytes()), &Limits::default()).unwrap()
}

/// A container with `inner` as its content, in the given `w` namespace.
fn container(inner: &str, w: &str) -> SpanTree {
    tree(&format!(
        "<w:rPr xmlns:w=\"{w}\" xmlns:w14=\"{W14}\">{inner}</w:rPr>"
    ))
}

/// Runs the same check over the owned and the borrowed reading, in both dialects.
fn both(inner: &str, check: impl Fn(&dyn Fn(&str) -> Option<bool>)) {
    for w in [W_T, W_S] {
        let t = container(inner, w);
        let owned = Props::read(t.root());
        check(&|name| owned.on_off(name));
        let borrowed = PropsRef::new(t.root());
        check(&|name| borrowed.on_off(name));
    }
}

#[test]
fn word_and_extension_names_are_kept_apart() {
    // Proves the review's bug is fixed: `w14:shadow` does not answer `shadow`.
    both("<w14:shadow/>", |on_off| assert_eq!(on_off("shadow"), None));
    both("<w14:shadow/><w:shadow w:val=\"0\"/>", |on_off| {
        assert_eq!(on_off("shadow"), Some(false));
    });
    both("<w:shadow/><w14:shadow w:val=\"0\"/>", |on_off| {
        assert_eq!(on_off("shadow"), Some(true));
    });
    let t = container("<w14:shadow w14:blurRad=\"1\"/><w:shadow/>", W_T);
    let p = Props::read(t.root());
    let w14 = p.get_in(&Ns::Ext(W14.into()), "shadow").unwrap();
    assert_eq!(w14.attr_in(&Ns::Ext(W14.into()), "blurRad"), Some("1"));
    assert_eq!(w14.attr("blurRad"), None);
    assert!(p.get("shadow").unwrap().attrs.is_empty());
    assert_eq!(p.get_in(&Ns::Word, "shadow"), p.get("shadow"));
    let r = PropsRef::new(t.root());
    assert_eq!(
        r.get_in(&Ns::Ext(W14.into()), "shadow").unwrap().prefix(),
        "w14"
    );
    assert_eq!(r.get("shadow").unwrap().prefix(), "w");
}

#[test]
fn on_off_follows_the_value() {
    // Proves the on/off table of design 6.1, in both dialects and both readings.
    let cases = [
        ("<w:b/>", Some(true)),
        ("<w:b w:val=\"true\"/>", Some(true)),
        ("<w:b w:val=\"1\"/>", Some(true)),
        ("<w:b w:val=\"on\"/>", Some(true)),
        ("<w:b w:val=\"false\"/>", Some(false)),
        ("<w:b w:val=\"0\"/>", Some(false)),
        ("<w:b w:val=\"off\"/>", Some(false)),
        ("<w:i/>", None),
        ("", None),
    ];
    for (inner, want) in cases {
        both(inner, |on_off| assert_eq!(on_off("b"), want, "{inner}"));
    }
}

#[test]
fn val_and_attr_read_the_word_element() {
    // Proves `val` and `attr` look at the Word element and the Word attribute only.
    for w in [W_T, W_S] {
        let t = container(
            "<w:sz w:val=\"24\"/><w:ind w:left=\"10\" w:hanging=\"5\" left=\"99\"/><w14:sz w:val=\"1\"/>",
            w,
        );
        let p = Props::read(t.root());
        let r = PropsRef::new(t.root());
        assert_eq!(p.val("sz"), Some("24"));
        assert_eq!(r.val("sz").as_deref(), Some("24"));
        assert_eq!(p.attr("ind", "left"), Some("10"));
        assert_eq!(r.attr("ind", "hanging").as_deref(), Some("5"));
        assert_eq!(p.val("ind"), None);
        assert_eq!(p.val("missing"), None);
        assert_eq!(r.val("missing"), None);
        assert_eq!(p.attr("ind", "right"), None);
    }
}

#[test]
fn order_and_duplicates_are_kept() {
    // Proves document order and duplicates are preserved, and `get` returns the first.
    let t = container("<w:b/><w:i/><w:b w:val=\"0\"/>text<!--c--><w:u/>", W_T);
    let p = Props::read(t.root());
    let names: Vec<&str> = p.iter().map(|x| &*x.name).collect();
    assert_eq!(names, ["b", "i", "b", "u"]);
    assert_eq!(p.on_off("b"), Some(true));
    let r = PropsRef::new(t.root());
    assert_eq!(r.iter().count(), 4);
    assert_eq!(r.on_off("b"), Some(true));
    assert!(!p.is_empty());
}

#[test]
fn children_are_read_to_depth_four_and_no_further() {
    // Proves the depth bound: levels one to four are read, a fifth is dropped.
    let t = tree(&format!(
        "<w:pPr xmlns:w=\"{W_T}\"><w:l1><w:l2><w:l3><w:l4><w:l5/></w:l4></w:l3></w:l2></w:l1></w:pPr>"
    ));
    let p = Props::read(t.root());
    let l4 = p
        .get("l1")
        .and_then(|x| x.child("l2"))
        .and_then(|x| x.child("l3"))
        .and_then(|x| x.child("l4"))
        .unwrap();
    assert!(l4.children.is_empty());
}

#[test]
fn the_mark_run_properties_and_change_snapshots_are_reached() {
    // Proves the shapes the design names fit in four levels.
    let t = tree(&format!(
        "<w:pPr xmlns:w=\"{W_T}\"><w:pBdr><w:top w:val=\"single\"/></w:pBdr>\
         <w:rPr><w:rPrChange w:id=\"1\"><w:rPr><w:b/></w:rPr></w:rPrChange></w:rPr>\
         <w:pPrChange w:id=\"2\"><w:pPr><w:pBdr><w:left w:val=\"dotted\"/></w:pBdr></w:pPr></w:pPrChange></w:pPr>"
    ));
    let p = Props::read(t.root());
    assert_eq!(
        p.get("pBdr")
            .and_then(|b| b.child("top"))
            .and_then(|x| x.val()),
        Some("single")
    );
    let b = p
        .get("rPr")
        .and_then(|x| x.child("rPrChange"))
        .and_then(|x| x.child("rPr"))
        .and_then(|x| x.child("b"));
    assert!(b.is_some_and(|b| b.on_off()));
    let left = p
        .get("pPrChange")
        .and_then(|x| x.child("pPr"))
        .and_then(|x| x.child("pBdr"))
        .and_then(|x| x.child("left"));
    assert_eq!(left.and_then(|x| x.val()), Some("dotted"));
}

#[test]
fn attributes_keep_their_namespace_and_drop_declarations() {
    // Proves attribute namespaces (none, Word, other) and that `xmlns` declarations are not data.
    let t = tree(&format!(
        "<w:rPr xmlns:w=\"{W_S}\" xmlns:x=\"urn:x\"><w:rFonts w:ascii=\"A\" x:ascii=\"B\" hint=\"C\" xmlns:y=\"urn:y\"/></w:rPr>"
    ));
    let p = Props::read(t.root());
    let f = p.get("rFonts").unwrap();
    assert_eq!(f.attrs.len(), 3);
    assert_eq!(f.attr("ascii"), Some("A"));
    assert_eq!(f.attr_in(&Ns::Ext("urn:x".into()), "ascii"), Some("B"));
    assert_eq!(f.attr_in(&Ns::None, "hint"), Some("C"));
    assert_eq!(f.ns, Ns::Word);
}

#[test]
fn an_empty_container_reads_as_empty() {
    // Proves the empty case for both readings.
    let t = container("", W_T);
    let p = Props::read(t.root());
    assert!(p.is_empty());
    assert_eq!(p.on_off("b"), None);
    assert_eq!(p, Props::new());
    assert_eq!(PropsRef::new(t.root()).iter().count(), 0);
    assert_eq!(PropsRef::new(t.root()).to_owned(), Props::new());
}

#[test]
fn props_ref_to_owned_equals_the_owned_reading() {
    // Proves the two readings agree.
    let t = container(
        "<w:b/><w14:glow w14:rad=\"3\"><w14:schemeClr w14:val=\"x\"/></w14:glow>",
        W_T,
    );
    assert_eq!(PropsRef::new(t.root()).to_owned(), Props::read(t.root()));
}

#[test]
fn props_ref_does_not_allocate() {
    // The zero-allocation claim, shown by construction because the core forbids `unsafe` and so a
    // counting allocator cannot be written. The reader is one `NodeRef` (a reference and an id),
    // it is `Copy`, and `val` hands back `Cow::Borrowed` when the value has no character reference.
    assert_eq!(
        std::mem::size_of::<PropsRef<'_>>(),
        std::mem::size_of::<NodeRef<'_>>()
    );
    let t = container("<w:sz w:val=\"24\"/><w:b/>", W_T);
    let r = PropsRef::new(t.root());
    let copy = r;
    assert!(matches!(copy.val("sz"), Some(Cow::Borrowed("24"))));
    assert!(matches!(r.attr("sz", "val"), Some(Cow::Borrowed("24"))));
    assert_eq!(r.on_off("b"), Some(true));
}
