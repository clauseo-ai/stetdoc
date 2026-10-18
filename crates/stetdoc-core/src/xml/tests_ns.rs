//! Tests for namespace resolution: that the editor chooses prefixes that mean what the caller asked
//! for where the node ends up. Hostile shapes are in `tests_scope.rs`.

use super::*;
use crate::api::error::ErrorCode;

const XML: &str = "http://www.w3.org/XML/1998/namespace";

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

#[test]
fn a_new_element_never_takes_the_meaning_of_a_hidden_prefix() {
    // The reviewer's case: `p` means urn:A on the root and urn:B on `c`.
    let mut t = parse("<r xmlns:p=\"urn:A\"><c xmlns:p=\"urn:B\"/></r>");
    let c = t.root().first_child().unwrap().id();
    let mut ed = t.edit();
    let n = ed.create_element("urn:A", "n").unwrap();
    ed.append_child(c, n).unwrap();
    let s = out(&t);
    assert_eq!(
        s,
        "<r xmlns:p=\"urn:A\"><c xmlns:p=\"urn:B\"><p1:n xmlns:p1=\"urn:A\"/></c></r>"
    );
    let back = parse(&s);
    let n = back.root().first_child().unwrap().first_child().unwrap();
    assert_eq!((n.namespace(), n.local_name()), ("urn:A", "n"));
    // The same element under the root uses the root's prefix and declares nothing.
    let mut t = parse("<r xmlns:p=\"urn:A\"><c xmlns:p=\"urn:B\"/></r>");
    let root = t.root().id();
    let mut ed = t.edit();
    let n = ed.create_element("urn:A", "n").unwrap();
    ed.append_child(root, n).unwrap();
    assert_eq!(
        out(&t),
        "<r xmlns:p=\"urn:A\"><c xmlns:p=\"urn:B\"/><p:n/></r>"
    );
}

#[test]
fn a_new_attribute_never_takes_the_meaning_of_a_hidden_prefix() {
    let mut t = parse("<r xmlns:p=\"urn:A\"><c xmlns:p=\"urn:B\"/><d/></r>");
    let c = t.root().first_child().unwrap();
    let d = c.next_sibling().unwrap().id();
    let c = c.id();
    let mut ed = t.edit();
    ed.set_attr(c, "urn:A", "x", "1").unwrap();
    ed.set_attr(d, "urn:A", "x", "2").unwrap();
    let s = out(&t);
    assert_eq!(
        s,
        "<r xmlns:p=\"urn:A\"><c xmlns:p=\"urn:B\" xmlns:p1=\"urn:A\" p1:x=\"1\"/><d p:x=\"2\"/></r>"
    );
    let back = parse(&s);
    let c = back.root().first_child().unwrap();
    assert_eq!(c.attr("urn:A", "x").unwrap(), "1");
    assert_eq!(c.next_sibling().unwrap().attr("urn:A", "x").unwrap(), "2");
}

#[test]
fn the_nearest_binding_in_scope_is_used() {
    let mut t = parse(
        "<r xmlns:a=\"urn:A\"><c xmlns:b=\"urn:A\"><d/></c><e xmlns:a=\"urn:X\"><f xmlns:z=\"urn:A\"/></e></r>",
    );
    let c = t.root().first_child().unwrap();
    let d = c.first_child().unwrap().id();
    let e = c.next_sibling().unwrap();
    let f = e.first_child().unwrap().id();
    let mut ed = t.edit();
    for parent in [d, f] {
        let n = ed.create_element("urn:A", "n").unwrap();
        ed.append_child(parent, n).unwrap();
    }
    assert_eq!(
        out(&t),
        "<r xmlns:a=\"urn:A\"><c xmlns:b=\"urn:A\"><d><b:n/></d></c><e xmlns:a=\"urn:X\"><f xmlns:z=\"urn:A\"><z:n/></f></e></r>"
    );
}

#[test]
fn a_namespace_nobody_binds_is_declared_on_the_root_once() {
    let mut t = parse("<r xmlns:ns=\"urn:taken\"><a><b xmlns:ns=\"urn:inner\"/></a></r>");
    let a = t.root().first_child().unwrap();
    let b = a.first_child().unwrap().id();
    let a = a.id();
    let mut ed = t.edit();
    let n1 = ed.create_element("urn:new", "n1").unwrap();
    let n2 = ed.create_element("urn:new", "n2").unwrap();
    ed.append_child(a, n1).unwrap();
    // Under `b`, where `ns` is redeclared, the root's declaration must still be reachable, so it
    // does not use a prefix that `b` hides.
    ed.append_child(b, n2).unwrap();
    let s = out(&t);
    assert_eq!(
        s,
        "<r xmlns:ns=\"urn:taken\" xmlns:ns1=\"urn:new\"><a><b xmlns:ns=\"urn:inner\"><ns1:n2/></b><ns1:n1/></a></r>"
    );
}

#[test]
fn the_default_namespace_is_used_for_elements_and_not_for_attributes() {
    let mut t = parse("<Types xmlns=\"urn:ct\"><x xmlns=\"\"/></Types>");
    let root = t.root().id();
    let x = t.root().first_child().unwrap().id();
    let mut ed = t.edit();
    let o = ed.create_element("urn:ct", "Override").unwrap();
    ed.set_attr(o, "urn:ct", "attr", "v").unwrap();
    ed.append_child(root, o).unwrap();
    // The element is bare, but an attribute needs a prefix, so the namespace gets one.
    let o2 = ed.create_element("urn:ct", "Other").unwrap();
    ed.append_child(x, o2).unwrap();
    let s = out(&t);
    assert_eq!(
        s,
        "<Types xmlns=\"urn:ct\" xmlns:ns=\"urn:ct\"><x xmlns=\"\"><ns:Other/></x><Override ns:attr=\"v\"/></Types>"
    );
    let back = parse(&s);
    assert_eq!(back.root().last_child().unwrap().namespace(), "urn:ct");
}

#[test]
fn an_element_with_no_namespace_is_refused_where_a_default_namespace_applies() {
    let mut t = parse("<r xmlns=\"urn:d\"><c xmlns=\"\"/></r>");
    let root = t.root().id();
    let c = t.root().first_child().unwrap().id();
    let mut ed = t.edit();
    let n = ed.create_element("", "n").unwrap();
    let e = ed.append_child(root, n).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
    assert!(
        t.node_ref(n).parent().is_none(),
        "a refused insert leaves the node detached"
    );
    assert_eq!(out(&t), "<r xmlns=\"urn:d\"><c xmlns=\"\"/></r>");
    t.edit().append_child(c, n).unwrap();
    assert_eq!(out(&t), "<r xmlns=\"urn:d\"><c xmlns=\"\"><n/></c></r>");
}

#[test]
fn the_xml_prefix_needs_no_declaration() {
    let mut t = parse("<r xmlns:w=\"urn:w\"><w:t/></r>");
    let w_t = t.root().first_child().unwrap().id();
    let mut ed = t.edit();
    ed.set_attr(w_t, XML, "space", "preserve").unwrap();
    let detached = ed.create_element("urn:w", "t").unwrap();
    ed.set_attr(detached, XML, "space", "preserve").unwrap();
    ed.append_child(w_t, detached).unwrap();
    let s = out(&t);
    assert_eq!(
        s,
        "<r xmlns:w=\"urn:w\"><w:t xml:space=\"preserve\"><w:t xml:space=\"preserve\"/></w:t></r>"
    );
    let back = parse(&s);
    assert_eq!(
        back.root()
            .first_child()
            .unwrap()
            .attr(XML, "space")
            .unwrap(),
        "preserve"
    );
    let e = t.edit().bind_namespace(XML, "x", false).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
}

#[test]
fn attributes_on_a_detached_element_get_their_prefix_at_insertion() {
    let mut t = parse("<r xmlns:w=\"urn:w\"><a xmlns:w=\"urn:other\"/></r>");
    let root = t.root().id();
    let a = t.root().first_child().unwrap().id();
    let mut ed = t.edit();
    let n = ed.create_element("urn:w", "n").unwrap();
    ed.set_attr(n, "urn:w", "val", "1").unwrap();
    ed.set_attr(n, "urn:w", "val", "2").unwrap(); // a second set changes the same attribute
    ed.set_attr(n, "", "plain", "3").unwrap();
    // Cloned before it is in the tree, the node is still unchosen, and is chosen where it lands.
    let n2 = ed.clone_subtree(n).unwrap();
    assert_eq!(t.node_ref(n).attrs().count(), 2);
    t.edit().append_child(root, n).unwrap();
    t.edit().append_child(a, n2).unwrap();
    let s = out(&t);
    assert_eq!(
        s,
        "<r xmlns:w=\"urn:w\"><a xmlns:w=\"urn:other\"><w1:n xmlns:w1=\"urn:w\" w1:val=\"2\" plain=\"3\"/></a><w:n w:val=\"2\" plain=\"3\"/></r>"
    );
    let back = parse(&s);
    let found: Vec<_> = all_elements(&back)
        .into_iter()
        .filter(|e| e.local_name() == "n")
        .collect();
    assert_eq!(found.len(), 2);
    for el in found {
        assert_eq!(el.namespace(), "urn:w");
        assert_eq!(el.attr("urn:w", "val").unwrap(), "2");
    }
}

#[test]
fn moved_nodes_keep_their_meaning_even_inside_a_new_element() {
    let src = "<r xmlns:p=\"urn:A\"><x><p:y/></x><c xmlns:p=\"urn:B\"/></r>";
    let mut t = parse(src);
    let x = t.root().first_child().unwrap();
    let y = x.first_child().unwrap().id();
    let c = x.next_sibling().unwrap().id();
    let mut ed = t.edit();
    ed.detach(y).unwrap();
    // The moved node is put into a new element first, and that element goes where `p` means
    // something else. The check looks inside the new element too, and declares `p` on the moved
    // node, so that it still means urn:A; the new element gets a prefix of its own.
    let holder = ed.create_element("urn:A", "holder").unwrap();
    ed.append_child(holder, y).unwrap();
    ed.append_child(c, holder).unwrap();
    let s = out(&t);
    assert_eq!(
        s,
        "<r xmlns:p=\"urn:A\"><x></x><c xmlns:p=\"urn:B\"><p1:holder xmlns:p1=\"urn:A\"><p:y xmlns:p=\"urn:A\"/></p1:holder></c></r>"
    );
    let back = parse(&s);
    for el in all_elements(&back) {
        if matches!(el.local_name(), "holder" | "y") {
            assert_eq!(el.namespace(), "urn:A", "{}", el.local_name());
        }
    }
}

#[test]
fn a_moved_default_namespace_and_attribute_prefixes_are_declared_where_they_change() {
    // `k` and the default namespace mean one thing where the node was and another where it goes.
    let src = "<r xmlns=\"urn:D\" xmlns:k=\"urn:K\"><a><n k:v=\"1\"><m/></n></a><b xmlns=\"urn:E\" xmlns:k=\"urn:L\"/></r>";
    let mut t = parse(src);
    let a = t.root().first_child().unwrap();
    let n = a.first_child().unwrap().id();
    let b = a.next_sibling().unwrap().id();
    let mut ed = t.edit();
    ed.detach(n).unwrap();
    ed.append_child(b, n).unwrap();
    let s = out(&t);
    assert_eq!(
        s,
        "<r xmlns=\"urn:D\" xmlns:k=\"urn:K\"><a></a><b xmlns=\"urn:E\" xmlns:k=\"urn:L\"><n xmlns=\"urn:D\" xmlns:k=\"urn:K\" k:v=\"1\"><m/></n></b></r>"
    );
    let back = parse(&s);
    for el in all_elements(&back) {
        if matches!(el.local_name(), "n" | "m") {
            assert_eq!(el.namespace(), "urn:D", "{}", el.local_name());
        }
        if el.local_name() == "n" {
            assert_eq!(el.attr("urn:K", "v").unwrap(), "1");
        }
    }
}

#[test]
fn one_declaration_serves_the_name_and_the_attributes_of_a_moved_element() {
    // `c:y` and `c:v` are both urn:B where they are, and `c` means urn:A under `s`: one declaration
    // on `y` keeps both.
    let src = "<r xmlns:c=\"urn:A\"><a xmlns:c=\"urn:B\"><c:y c:v=\"1\"/></a><s><c:x/></s></r>";
    let mut t = parse(src);
    let a = t.root().first_child().unwrap();
    let y = a.first_child().unwrap().id();
    let x = a.next_sibling().unwrap().first_child().unwrap().id();
    let mut ed = t.edit();
    ed.detach(y).unwrap();
    ed.append_child(x, y).unwrap();
    let s = out(&t);
    assert!(
        s.contains("<c:x><c:y xmlns:c=\"urn:B\" c:v=\"1\"/></c:x>"),
        "{s}"
    );
    let back = parse(&s);
    let moved = all_elements(&back)
        .into_iter()
        .find(|e| e.local_name() == "y")
        .unwrap();
    assert_eq!(moved.namespace(), "urn:B");
    assert_eq!(moved.attr("urn:B", "v").unwrap(), "1");
}

#[test]
fn a_subtree_that_declares_its_own_prefixes_can_move() {
    let src = "<r xmlns:q=\"urn:Q\"><a><x xmlns:z=\"urn:Z\"><z:y z:k=\"1\"/><q:w/></x></a><b xmlns:q=\"urn:other\"/></r>";
    let mut t = parse(src);
    let a = t.root().first_child().unwrap();
    let x = a.first_child().unwrap().id();
    let b = a.next_sibling().unwrap().id();
    let mut ed = t.edit();
    ed.detach(x).unwrap();
    // `z` is declared inside the subtree and moves with it. `q` is not, and `b` hides it, so `q`
    // is declared on the element that writes it, which keeps it meaning urn:Q.
    ed.append_child(b, x).unwrap();
    assert_eq!(
        out(&t),
        "<r xmlns:q=\"urn:Q\"><a></a><b xmlns:q=\"urn:other\"><x xmlns:z=\"urn:Z\"><z:y z:k=\"1\"/><q:w xmlns:q=\"urn:Q\"/></x></b></r>"
    );
}

#[test]
fn the_root_start_tag_gains_only_declarations() {
    let src = "<?xml version=\"1.0\"?>\n<r a=\"1\"   xmlns:w=\"urn:w\" b='2'>\n  <w:p/>\n</r>\n";
    let mut t = parse(src);
    let root = t.root().id();
    let p = t.root().child_elements().next().unwrap().id();
    let mut ed = t.edit();
    let n = ed.create_element("urn:ext", "n").unwrap();
    ed.append_child(p, n).unwrap();
    ed.bind_namespace("urn:ext2", "e2", true).unwrap();
    let _ = root;
    let s = out(&t);
    assert_eq!(
        s,
        "<?xml version=\"1.0\"?>\n<r a=\"1\"   xmlns:w=\"urn:w\" xmlns:ns=\"urn:ext\" xmlns:e2=\"urn:ext2\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" b='2' mc:Ignorable=\"e2\">\n  <w:p><ns:n/></w:p>\n</r>\n"
    );
}

/// A small deterministic generator, so the random test below is the same on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const URIS: [&str; 6] = [
    "urn:u0", "urn:u1", "urn:u2", "urn:u3", "urn:new1", "urn:new2",
];
const PREFIXES: [&str; 4] = ["a", "b", "c", "ns"];

/// One element of a random document: declarations that bind some prefixes, sometimes again to
/// other URIs than an outer element did, a name that uses a prefix in scope, and `o` set to the
/// element's number.
fn random_element(
    rng: &mut Rng,
    scope: &mut Vec<String>,
    depth: usize,
    count: &mut usize,
    out: &mut String,
) {
    let mark = scope.len();
    let mut decls = String::new();
    if depth == 0 || rng.below(10) < 4 {
        for _ in 0..1 + rng.below(2) {
            let p = PREFIXES[rng.below(PREFIXES.len())];
            if !decls.contains(&format!(" xmlns:{p}=")) {
                decls.push_str(&format!(" xmlns:{p}=\"{}\"", URIS[rng.below(4)]));
                scope.push(p.to_owned());
            }
        }
    }
    let name = match scope.last() {
        Some(p) if rng.below(4) != 0 => format!("{p}:e"),
        _ => "e".to_owned(),
    };
    *count += 1;
    out.push_str(&format!("<{name}{decls} o=\"{count}\">"));
    if depth < 5 {
        for _ in 0..rng.below(4) {
            random_element(rng, scope, depth + 1, count, out);
        }
    }
    out.push_str(&format!("</{name}>"));
    scope.truncate(mark);
}

fn all_elements(t: &SpanTree) -> Vec<NodeRef<'_>> {
    let mut out = vec![];
    let mut stack = vec![t.root()];
    while let Some(n) = stack.pop() {
        out.push(n);
        stack.extend(n.child_elements());
    }
    out
}

/// What a created element should look like once written and read again.
struct Made {
    k: usize,
    uri: String,
    attrs: Vec<(String, String)>,
}

fn add_attrs(t: &mut SpanTree, rng: &mut Rng, n: NodeId, made: &mut Made) {
    for j in 0..rng.below(3) {
        let uri = URIS[rng.below(URIS.len())];
        let local = format!("x{j}");
        t.edit()
            .set_attr(n, uri, &local, &format!("v{}", made.k))
            .unwrap();
        made.attrs.retain(|(u, l)| !(u == uri && *l == local));
        made.attrs.push((uri.to_owned(), local));
    }
}

/// Writes the tree and reads it again: every created element and attribute must be in the
/// namespace it was made in, and every original element must still be there.
fn check(t: &SpanTree, originals: usize, live: &[Made], log: &str) {
    let written = out(t);
    let back = SpanTree::parse(
        "back.xml",
        Bytes::from(written.clone().into_bytes()),
        &Limits::default(),
    )
    .unwrap_or_else(|e| panic!("the output does not parse: {e}\n{written}\n{log}"));
    let elements = all_elements(&back);
    assert_eq!(elements.len(), originals + live.len(), "{written}\n{log}");
    for m in live {
        let el = elements
            .iter()
            .find(|e| e.attr("", "k").is_some_and(|v| v == m.k.to_string()))
            .unwrap_or_else(|| panic!("created element {} is missing\n{written}\n{log}", m.k));
        assert_eq!(el.namespace(), m.uri, "element {}\n{written}\n{log}", m.k);
        assert_eq!(el.local_name(), "n");
        for (uri, local) in &m.attrs {
            assert_eq!(
                el.attr(uri, local).as_deref(),
                Some(format!("v{}", m.k).as_str()),
                "attribute {local} of element {}\n{written}\n{log}",
                m.k
            );
        }
        let data = el.attrs().filter(|a| !a.is_namespace_declaration()).count();
        assert_eq!(data, 1 + m.attrs.len(), "element {}\n{written}\n{log}", m.k);
    }
}

#[test]
fn random_edits_always_write_the_namespaces_that_were_asked_for() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut inserted = 0usize;
    for round in 0..400 {
        let mut src = String::new();
        random_element(&mut rng, &mut Vec::new(), 0, &mut 0, &mut src);
        let mut t = parse(&src);
        let originals = all_elements(&t).len();
        let mut live: Vec<Made> = Vec::new();
        let mut log = format!("round {round}\n{src}");
        let mut next_k = 0usize;
        for _ in 0..10 {
            // One new element, or a new element that holds another one while still detached.
            let nested = rng.below(4) == 0;
            let count = if nested { 2 } else { 1 };
            let mut batch: Vec<(NodeId, Made)> = Vec::new();
            for _ in 0..count {
                let uri = if rng.below(8) == 0 {
                    String::new()
                } else {
                    URIS[rng.below(URIS.len())].to_owned()
                };
                let n = t.edit().create_element(&uri, "n").unwrap();
                let mut made = Made {
                    k: next_k,
                    uri,
                    attrs: vec![],
                };
                next_k += 1;
                t.edit().set_attr(n, "", "k", &made.k.to_string()).unwrap();
                if rng.below(2) == 0 {
                    add_attrs(&mut t, &mut rng, n, &mut made);
                }
                batch.push((n, made));
            }
            if let [(outer, _), (inner, _)] = batch.as_slice() {
                t.edit().append_child(*outer, *inner).unwrap();
            }
            let targets = all_elements(&t);
            let target = targets[rng.below(targets.len())];
            let (target_id, kids): (NodeId, Vec<NodeId>) =
                (target.id(), target.children().map(|c| c.id()).collect());
            let before =
                (!kids.is_empty() && rng.below(2) == 0).then(|| kids[rng.below(kids.len())]);
            let top = batch[0].0;
            log.push_str(&format!(
                "insert {:?} under {:?}\n",
                batch
                    .iter()
                    .map(|(_, m)| (m.k, m.uri.clone()))
                    .collect::<Vec<_>>(),
                t.node_ref(target_id).name()
            ));
            match t.edit().insert_before(target_id, before, top) {
                Ok(()) => {
                    inserted += batch.len();
                    for (n, mut made) in batch {
                        if rng.below(2) == 0 {
                            add_attrs(&mut t, &mut rng, n, &mut made);
                        }
                        live.push(made);
                    }
                }
                Err(e) => {
                    // Only an element with no namespace under a default namespace is refused.
                    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}\n{log}");
                    assert!(batch.iter().any(|(_, m)| m.uri.is_empty()), "{e}\n{log}");
                    assert!(t.node_ref(top).parent().is_none());
                    // A node that was never inserted does not exist in the output.
                }
            }
            check(&t, originals, &live, &log);
        }
    }
    assert!(
        inserted > 1500,
        "the test should insert plenty of elements: {inserted}"
    );
}

fn try_parse(s: &str) -> Result<SpanTree, crate::api::error::StetError> {
    SpanTree::parse(
        "test.xml",
        Bytes::from(s.as_bytes().to_vec()),
        &Limits::default(),
    )
}

#[test]
fn two_attributes_with_one_expanded_name_are_malformed() {
    // Two prefixes bound to one namespace give the same expanded name twice, which Namespaces in
    // XML 1.0 forbids (Attributes Unique). The fuzzer found it: `remove_attr` removed one and the
    // other was still there. Few attributes and many take different paths in the check.
    let two = r#"<r xmlns:p="urn:n" xmlns:q="urn:n" p:a="1" q:a="2"/>"#;
    assert_eq!(
        try_parse(two).err().map(|e| e.code()),
        Some(ErrorCode::MalformedInput)
    );
    let mut wide: Vec<String> = (0..20).map(|i| format!(" p:a{i}=\"1\"")).collect();
    wide.push(" q:a7=\"2\"".to_string());
    let wide = format!("<r xmlns:p=\"urn:n\" xmlns:q=\"urn:n\"{}/>", wide.concat());
    assert_eq!(
        try_parse(&wide).err().map(|e| e.code()),
        Some(ErrorCode::MalformedInput)
    );
    // The same local name in two namespaces, or prefixed and not, is fine, and so are two
    // prefixes for one namespace with different local names.
    for ok in [
        r#"<r xmlns:p="urn:n" xmlns:q="urn:m" p:a="1" q:a="2" a="3"/>"#,
        r#"<r xmlns:p="urn:n" xmlns:q="urn:n" p:a="1" q:b="2"/>"#,
    ] {
        assert_eq!(out(&parse(ok)), ok);
    }
    let mut t = parse(r#"<r xmlns:p="urn:n" xmlns:q="urn:n" p:a="1" q:b="2"/>"#);
    let root = t.root().id();
    assert!(t.edit().remove_attr(root, "urn:n", "a").expect("remove"));
    assert!(!t.edit().remove_attr(root, "urn:n", "a").expect("remove"));
    assert_eq!(out(&t), r#"<r xmlns:p="urn:n" xmlns:q="urn:n" q:b="2"/>"#);
}

#[test]
fn a_move_out_of_a_nested_declaration_declares_the_prefix_on_the_moved_element() {
    let src = "<r xmlns:q=\"urn:1\"><a xmlns:z=\"urn:z\"><z:x/></a><b/></r>";
    let mut t = parse(src);
    let a = t.root().first_child().unwrap();
    let x = a.first_child().unwrap().id();
    let b = a.next_sibling().unwrap().id();
    let mut ed = t.edit();
    ed.detach(x).unwrap();
    ed.append_child(b, x).unwrap();
    let s = out(&t);
    assert_eq!(
        s,
        "<r xmlns:q=\"urn:1\"><a xmlns:z=\"urn:z\"></a><b><z:x xmlns:z=\"urn:z\"/></b></r>"
    );
    assert_eq!(t.node_ref(x).namespace(), "urn:z");
}
