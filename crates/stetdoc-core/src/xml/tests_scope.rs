//! Tests for hostile namespace shapes: that parsing stays linear on them, and that the limit on the
//! declarations in scope, which bounds what an edit costs, is enforced when parsing and by the editor.

use super::*;
use crate::api::error::ErrorCode;
use std::time::Duration;

fn parse(s: &str) -> SpanTree {
    SpanTree::parse(
        "test.xml",
        Bytes::from(s.as_bytes().to_vec()),
        &Limits::default(),
    )
    .expect("parse")
}

fn out(t: &SpanTree) -> String {
    String::from_utf8(t.serialize()).expect("utf-8")
}

/// The default limits with no bound on the declarations in scope, to time parsing on shapes that
/// the default limit refuses at once. The bound exists for the editor; the parse is linear
/// without it.
fn unbounded_scope() -> Limits {
    Limits {
        max_xml_namespaces_in_scope: u32::MAX,
        ..Limits::default()
    }
}

/// Runs `work` on another thread and fails the test if it takes longer than `secs` seconds. The
/// shapes below took minutes with quadratic namespace handling and take well under a second now.
fn within<T: Send + 'static>(secs: u64, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx.recv_timeout(Duration::from_secs(secs))
        .expect("parsing did not finish in time")
}

/// `n` sibling elements with `per` declarations each, every URI different.
fn many_uris(n: usize, per: usize) -> String {
    let mut s = String::from("<r>");
    for i in 0..n {
        s.push_str("<a");
        for j in 0..per {
            s.push_str(&format!(" xmlns:p{j}=\"urn:{i}:{j}\""));
        }
        s.push_str(&format!("><p{}:x/></a>", per - 1));
    }
    s.push_str("</r>");
    s
}

/// `levels` nested elements, each declaring the same `per` prefixes again with other URIs, and a
/// child at every level that uses one of them. The URIs repeat every 50 levels, so a part has
/// fewer than the 65,535 it may name.
fn nested_redeclarations(levels: usize, per: usize) -> String {
    let mut s = String::new();
    for level in 0..levels {
        s.push_str("<a");
        for j in 0..per {
            s.push_str(&format!(" xmlns:p{j}=\"urn:{}:{j}\"", level % 50));
        }
        s.push_str(&format!("><p{}:c/><p0:c/>", level % per));
    }
    s.push_str(&"</a>".repeat(levels));
    s
}

#[test]
fn parsing_many_distinct_namespaces_is_not_quadratic() {
    // 65 elements with 1,000 URIs each: 65,000 of the 65,535 a part may name.
    let src = many_uris(65, 1000);
    let t = within(20, move || parse(&src));
    let last = t.root().last_child().unwrap();
    let x = last.first_child().unwrap();
    assert_eq!(x.namespace(), "urn:64:999");
    // One more than a part may name is refused, and promptly.
    let src = many_uris(67, 1000);
    let e = within(20, move || {
        SpanTree::parse("big.xml", Bytes::from(src.into_bytes()), &Limits::default()).err()
    })
    .expect("too many namespaces");
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    assert!(
        e.message().contains("too many distinct XML namespaces"),
        "{e}"
    );
}

#[test]
fn redeclaring_many_prefixes_at_every_level_is_not_quadratic() {
    // 100 levels of 1,024 redeclarations is 102,400 declarations in scope at once.
    let src = nested_redeclarations(100, 1024);
    let t = within(20, move || {
        SpanTree::parse(
            "test.xml",
            Bytes::from(src.into_bytes()),
            &unbounded_scope(),
        )
        .expect("parse")
    });
    let mut deepest = t.root();
    loop {
        let next = deepest.child_elements().find(|c| c.local_name() == "a");
        match next {
            Some(n) => deepest = n,
            None => break,
        }
    }
    // The innermost declaration wins, and an outer scope is back after an inner one closes.
    let kids: Vec<_> = deepest.child_elements().collect();
    assert_eq!(kids[0].namespace(), "urn:49:99");
    assert_eq!(kids[1].namespace(), "urn:49:0");
    let first = t.root().child_elements().find(|c| c.local_name() == "c");
    assert_eq!(first.unwrap().namespace(), "urn:0:0");
}

#[test]
fn the_deepest_allowed_nesting_of_full_scopes_is_not_quadratic() {
    // 254 nested scopes of 1,024 declarations, the shape that cost 179 microseconds per element.
    let src = nested_redeclarations(254, 1024);
    let t = within(20, move || {
        SpanTree::parse(
            "test.xml",
            Bytes::from(src.into_bytes()),
            &unbounded_scope(),
        )
        .expect("parse")
    });
    assert!(t.root().is_element());
    // One level deeper than the default depth limit is refused, not slow.
    let src = nested_redeclarations(300, 1024);
    let e = within(20, move || {
        SpanTree::parse(
            "deep.xml",
            Bytes::from(src.into_bytes()),
            &unbounded_scope(),
        )
        .err()
    })
    .expect("too deep");
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    assert!(e.message().contains("nesting"), "{e}");
}

/// `levels` nested elements, each declaring `per` prefixes, the same ones at every level.
fn nested_same_prefixes(levels: usize, per: usize) -> String {
    let mut s = String::new();
    for level in 0..levels {
        s.push_str("<a");
        for j in 0..per {
            s.push_str(&format!(" xmlns:p{j}=\"urn:{level}:{j}\""));
        }
        s.push('>');
    }
    s.push_str(&"</a>".repeat(levels));
    s
}

#[test]
fn declarations_in_scope_are_limited_and_the_editor_keeps_to_the_limit() {
    let parse_default = |s: &str| {
        SpanTree::parse(
            "test.xml",
            Bytes::from(s.as_bytes().to_vec()),
            &Limits::default(),
        )
    };
    // Exactly the limit parses, and one more is refused with the offset and the limit.
    assert!(parse_default(&nested_same_prefixes(2, 512)).is_ok());
    let e = parse_default(&nested_same_prefixes(2, 513))
        .err()
        .expect("refused");
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    assert!(
        e.message()
            .contains("more than 1024 namespace declarations are in scope"),
        "{e}"
    );
    // A declaration that a closer one hides still counts: it is what a lookup walks past. Three
    // levels of 400 prefixes have 400 distinct prefixes and 1,200 declarations in scope.
    let e = parse_default(&nested_same_prefixes(3, 400))
        .err()
        .expect("refused");
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    // Siblings do not add up: each scope closes before the next opens.
    let siblings = format!("<r>{}</r>", "<a xmlns:p=\"urn:x\"/>".repeat(5000));
    assert!(parse_default(&siblings).is_ok());
    // The shapes that took seconds with quadratic handling are refused at the second level.
    let src = nested_redeclarations(100, 1024);
    let e = within(20, move || {
        SpanTree::parse("big.xml", Bytes::from(src.into_bytes()), &Limits::default()).err()
    })
    .expect("refused");
    assert_eq!(e.code(), ErrorCode::LimitExceeded);

    // The editor counts what it declares against the same limit, so that what it writes can be
    // parsed again: 1,023 in scope leaves room for one more, and no more than that.
    let src = format!(
        "<r><c{}/></r>",
        (0..1023).fold(String::new(), |mut s, j| {
            s.push_str(&format!(" xmlns:p{j}=\"urn:{j}\""));
            s
        })
    );
    let mut t = parse_default(&src).unwrap();
    let root = t.root().id();
    let mut ed = t.edit();
    let first = ed.create_element("urn:new:1", "e").unwrap();
    ed.append_child(root, first).unwrap();
    let second = ed.create_element("urn:new:2", "e").unwrap();
    let e = ed.append_child(root, second).unwrap_err();
    assert_eq!(e.code(), ErrorCode::LimitExceeded, "{e}");
    assert!(e.message().contains("1024 declarations"), "{e}");
    // What was written parses again, with exactly 1,024 declarations in scope below the root.
    assert!(parse_default(&out(&t)).is_ok());
}

#[test]
fn editing_a_part_with_many_namespaces_stays_fast() {
    let mut t = parse("<r/>");
    let root = t.root().id();
    within(20, move || {
        let mut ed = t.edit();
        // Each new namespace is declared on the root, up to the most an element may hold.
        for i in 0..1024 {
            let e = ed.create_element(&format!("urn:new:{i}"), "e").unwrap();
            ed.append_child(root, e).unwrap();
        }
        let e = ed.create_element("urn:new:1024", "e").unwrap();
        let err = ed.append_child(root, e).unwrap_err();
        assert_eq!(err.code(), ErrorCode::LimitExceeded, "{err}");
        let back = parse(&out(&t));
        assert_eq!(back.root().child_elements().count(), 1024);
    });
}

/// ` xmlns:p0="urn:0" ... ` for `n` prefixes.
fn declarations(n: usize) -> String {
    (0..n)
        .map(|j| format!(" xmlns:p{j}=\"urn:{j}\""))
        .collect::<String>()
}

fn parse_again(t: &SpanTree) -> Result<SpanTree, crate::api::error::StetError> {
    SpanTree::parse("again.xml", Bytes::from(t.serialize()), &Limits::default())
}

#[test]
fn an_insert_counts_the_declarations_that_come_with_the_subtree() {
    // `e` carries 300 declarations. A copy of it, or `e` itself moved, brings them along.
    let src = format!("<r><e{}/><x/></r>", declarations(300));
    let first = |t: &SpanTree| t.root().first_child().unwrap().id();

    // Siblings do not stack: a hundred copies side by side have 300 in scope each.
    let mut t = parse(&src);
    let (root, e) = (t.root().id(), first(&t));
    {
        let mut ed = t.edit();
        for _ in 0..100 {
            let copy = ed.clone_subtree(e).unwrap();
            ed.append_child(root, copy).unwrap();
        }
    }
    assert!(parse_again(&t).is_ok());

    // Copies inside copies do, hidden declarations included: the third has 900 in scope, and the
    // fourth would have 1,200, over the limit of 1,024. Before the count, this went on until the
    // depth limit, at 256 levels of 300.
    let mut t = parse(&src);
    let (e, x) = (first(&t), t.root().last_child().unwrap().id());
    let mut at = x;
    let mut accepted = 0;
    let mut refused = None;
    {
        let mut ed = t.edit();
        for _ in 0..10 {
            let copy = ed.clone_subtree(e).unwrap();
            match ed.append_child(at, copy) {
                Ok(()) => {
                    accepted += 1;
                    at = copy;
                }
                Err(err) => {
                    refused = Some((err, copy));
                    break;
                }
            }
        }
    }
    let (err, copy) = refused.expect("the fourth copy is refused");
    assert_eq!(accepted, 3);
    assert_eq!(err.code(), ErrorCode::LimitExceeded, "{err}");
    assert!(
        err.message()
            .contains("1200 namespace declarations in scope"),
        "{err}"
    );
    assert!(err.message().contains("limit of 1024"), "{err}");
    // Nothing was linked: the copy is free, and the part that is written parses again.
    assert!(t.node_ref(copy).parent().is_none());
    assert!(parse_again(&t).is_ok());

    // The reported case: a copy of a root that has 1,000 declarations, put under any element.
    let src = format!("<r{}><c/></r>", declarations(1000));
    let mut t = parse(&src);
    let (root, c) = (t.root().id(), t.root().first_child().unwrap().id());
    let mut ed = t.edit();
    let copy = ed.clone_subtree(root).unwrap();
    let err = ed.append_child(c, copy).unwrap_err();
    assert_eq!(err.code(), ErrorCode::LimitExceeded, "{err}");
    assert!(!t.is_dirty());

    // A node that is moved brings its declarations too. `e` goes under `b`, where 800 are in scope.
    let src = format!(
        "<r><a{}><b/></a><e{}/></r>",
        declarations(800),
        declarations(300)
    );
    let mut t = parse(&src);
    let a = t.root().first_child().unwrap();
    let (b, e) = (
        a.first_child().unwrap().id(),
        a.next_sibling().unwrap().id(),
    );
    let mut ed = t.edit();
    ed.detach(e).unwrap();
    let err = ed.append_child(b, e).unwrap_err();
    assert_eq!(err.code(), ErrorCode::LimitExceeded, "{err}");
    // Back where there are none in scope, it goes in.
    let root = t.root().id();
    t.edit().append_child(root, e).unwrap();
    assert!(parse_again(&t).is_ok());
}

#[test]
fn a_chain_built_apart_is_counted_when_it_joins_the_document() {
    // Where the parent is not in the document yet nothing is counted, because nothing is known
    // about what is above it. The whole chain is measured when its top is put in.
    let src = format!("<r><e{}/></r>", declarations(300));
    let mut t = parse(&src);
    let (root, e) = (t.root().id(), t.root().first_child().unwrap().id());
    let mut ed = t.edit();
    let copies: Vec<NodeId> = (0..4).map(|_| ed.clone_subtree(e).unwrap()).collect();
    for pair in copies.windows(2) {
        ed.append_child(pair[0], pair[1]).unwrap();
    }
    let err = ed.append_child(root, copies[0]).unwrap_err();
    assert_eq!(err.code(), ErrorCode::LimitExceeded, "{err}");
    assert!(err.message().contains("1200"), "{err}");
    // Three of them are within the limit.
    ed.detach(copies[3]).unwrap();
    ed.append_child(root, copies[0]).unwrap();
    assert!(parse_again(&t).is_ok());
}
