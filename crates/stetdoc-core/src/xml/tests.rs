use super::*;
use crate::api::error::ErrorCode;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn limits() -> Limits {
    Limits::default()
}

fn parse_bytes(b: &[u8]) -> Result<SpanTree, StetError> {
    SpanTree::parse("test.xml", Bytes::from(b.to_vec()), &limits())
}

fn parse(s: &str) -> SpanTree {
    parse_bytes(s.as_bytes()).expect("parse")
}

/// Round trips through both serializer paths.
fn assert_identity(b: &[u8]) -> SpanTree {
    let mut t = parse_bytes(b).expect("parse");
    assert_eq!(t.serialize(), b, "clean path");
    t.mark_all_tags_dirty();
    assert_eq!(t.serialize(), b, "all-dirty path");
    parse_bytes(b).expect("parse")
}

const DOC: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"w14\"><w:body><w:p w14:paraId=\"0A1B\"><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:r><w:t>Hello &amp; goodbye&#xA0;now</w:t></w:r><w:r><w:t xml:space=\"preserve\"> two </w:t></w:r></w:p><w:p><w:r><w:br/></w:r></w:p><w:sectPr/></w:body></w:document>";

#[test]
fn identity_on_varied_inputs() {
    let cases: Vec<Vec<u8>> = vec![
        DOC.as_bytes().to_vec(),
        // BOM, single-quoted declaration, no standalone, LF endings
        b"\xEF\xBB\xBF<?xml version='1.0' encoding='utf-8'?>\n<a/>".to_vec(),
        b"<a></a>".to_vec(),
        b"<a/>\n".to_vec(),
        // comments and processing instructions around the root
        b"<?xml version=\"1.0\"?>\n<!-- c1 -->\n<?pi data?>\n<r><!-- in -- side --></r>\n<!-- after -->\n".to_vec(),
        // CDATA, references, mixed content
        b"<r>text <![CDATA[ a < b ]]> more &lt;&gt;&amp;&quot;&apos;&#65;&#x42; <e/> tail</r>".to_vec(),
        // attributes: quote styles, spacing, `>` in a value, newline between attributes
        b"<r a = '1'   b=\"x>y\"\n\tc=\"&#10;\" ><e  d='\"'  /></r >".to_vec(),
        // default namespace, odd prefix, nested declaration, redeclared prefix
        b"<ns0:doc xmlns:ns0=\"urn:a\" xmlns=\"urn:d\"><x/><ns0:y xmlns:ns0=\"urn:b\" xmlns:z=\"urn:z\"><z:q z:attr=\"1\"/></ns0:y></ns0:doc>".to_vec(),
        // long text and a deep but legal nesting
        format!("<r>{}</r>", "x".repeat(200_000)).into_bytes(),
        format!("{}{}", "<a>".repeat(100), "</a>".repeat(100)).into_bytes(),
        // non-ASCII
        "<r a=\"caf\u{e9}\">\u{4e2d}\u{6587} \u{1F600}</r>".as_bytes().to_vec(),
    ];
    for c in &cases {
        assert_identity(c);
    }
}

#[test]
fn utf16_identity_and_editing() {
    let xml = "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\r\n<r xmlns=\"urn:x\"><t>h\u{e9}llo \u{1F600}</t><e/></r>";
    for (be, bom) in [(false, true), (true, true), (false, false), (true, false)] {
        let bytes = encoding::utf8_to_utf16(xml.as_bytes(), be, bom);
        let mut t = assert_identity(&bytes);
        let txt = t
            .root()
            .child_elements()
            .next()
            .and_then(|e| e.first_child())
            .expect("text");
        assert_eq!(txt.text(), "h\u{e9}llo \u{1F600}");
        let id = txt.id();
        t.edit().set_text(id, "bye").expect("edit");
        let out = t.serialize();
        // The edit changed one text node; the BOM, byte order and the rest are as before.
        let expect = encoding::utf8_to_utf16(
            xml.replace("h\u{e9}llo \u{1F600}", "bye").as_bytes(),
            be,
            bom,
        );
        assert_eq!(out, expect, "be={be} bom={bom}");
    }
}

#[test]
fn names_namespaces_and_attributes() {
    let t = parse(DOC);
    let root = t.root();
    assert!(root.is(W, "document"));
    assert_eq!(root.name(), "w:document");
    assert_eq!(root.prefix(), "w");
    assert_eq!(root.local_name(), "document");
    let body = root.child_elements().next().unwrap();
    let p = body.child_elements().next().unwrap();
    assert_eq!(
        p.attr(
            "http://schemas.microsoft.com/office/word/2010/wordml",
            "paraId"
        )
        .unwrap(),
        "0A1B"
    );
    assert!(p.attr("", "paraId").is_none());
    let t1 = p.child_elements().nth(1).unwrap();
    assert_eq!(t1.text_content(), "Hello & goodbye\u{a0}now");
    let jc = p
        .child_elements()
        .next()
        .unwrap()
        .child_elements()
        .next()
        .unwrap();
    assert_eq!(jc.attr(W, "val").unwrap(), "center");
    let attrs: Vec<String> = root.attrs().map(|a| a.name().to_owned()).collect();
    assert_eq!(attrs, ["xmlns:w", "xmlns:w14", "xmlns:mc", "mc:Ignorable"]);
    assert_eq!(root.attr(super::MC_NS, "Ignorable").unwrap(), "w14");
    // Prolog nodes exist, and the document has exactly one root element.
    let kinds: Vec<NodeKind> = t.document().children().map(|c| c.kind()).collect();
    assert_eq!(
        kinds,
        [
            NodeKind::ProcessingInstruction,
            NodeKind::Text,
            NodeKind::Element
        ]
    );
}

/// Checks that old and new differ only inside `span` of the old bytes: everything before it and
/// everything after it is the same, and nothing else moved.
fn assert_confined(old: &[u8], new: &[u8], span: (usize, usize)) {
    let suffix = old.len() - span.1;
    assert!(
        new.len() >= span.0 + suffix,
        "the output is shorter than the bytes outside the node"
    );
    assert_eq!(
        old[..span.0],
        new[..span.0],
        "bytes before the node changed"
    );
    assert_eq!(
        old[span.1..],
        new[new.len() - suffix..],
        "bytes after the node changed"
    );
}

#[test]
fn set_text_changes_only_that_node() {
    let mut t = parse(DOC);
    let n = t
        .root()
        .child_elements()
        .next()
        .unwrap()
        .child_elements()
        .next()
        .unwrap()
        .child_elements()
        .nth(1)
        .unwrap();
    let txt = n.first_child().unwrap().first_child().unwrap();
    let (id, span) = (txt.id(), txt.source_span().unwrap());
    t.edit().set_text(id, "A & B <c> \u{e9}\r").unwrap();
    let out = t.serialize();
    assert_confined(DOC.as_bytes(), &out, span);
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("<w:t>A &amp; B &lt;c&gt; \u{e9}&#13;</w:t>"));
    // And it parses back to the text we set.
    let back = parse(&s);
    assert!(back.root().text_content().contains("A & B <c> \u{e9}\r"));
}

#[test]
fn replace_text_range_keeps_other_references() {
    let mut t = parse("<r><t>a&#xA0;b &amp; c</t></r>");
    let txt = t.root().first_child().unwrap().first_child().unwrap();
    let id = txt.id();
    // characters: a, NBSP, b, space, &, space, c. Replace "b " (2..4) only.
    t.edit().replace_text_range(id, 2, 4, "X<").unwrap();
    assert_eq!(
        String::from_utf8(t.serialize()).unwrap(),
        "<r><t>a&#xA0;X&lt;&amp; c</t></r>"
    );
    assert!(t.edit().replace_text_range(id, 0, 99, "").is_err());
}

#[test]
fn set_attr_keeps_style_and_remove_attr_drops_one() {
    let src = "<r a = '1'   b=\"x\" c='3'/>";
    let mut t = parse(src);
    let r = t.root().id();
    t.edit().set_attr(r, "", "a", "it's & \"q\"").unwrap();
    assert_eq!(
        String::from_utf8(t.serialize()).unwrap(),
        "<r a = 'it&apos;s &amp; \"q\"'   b=\"x\" c='3'/>"
    );
    t.edit().set_attr(r, "", "b", "y").unwrap();
    t.edit().set_attr(r, "", "d", "new\t").unwrap();
    assert!(t.edit().remove_attr(r, "", "c").unwrap());
    assert!(!t.edit().remove_attr(r, "", "zzz").unwrap());
    assert_eq!(
        String::from_utf8(t.serialize()).unwrap(),
        "<r a = 'it&apos;s &amp; \"q\"'   b=\"y\" d=\"new&#9;\"/>"
    );
    let back = parse(&String::from_utf8(t.serialize()).unwrap());
    assert_eq!(back.root().attr("", "d").unwrap(), "new\t");
}

#[test]
fn new_elements_reuse_the_documents_prefix() {
    // The main namespace is bound to an unusual prefix, and `w14` is not bound at all.
    let src = "<ns0:doc xmlns:ns0=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><ns0:p/></ns0:doc>";
    let mut t = parse(src);
    let p = t.root().first_child().unwrap().id();
    let root = t.root().id();
    let mut ed = t.edit();
    let ins = ed.create_element(W, "ins").unwrap();
    ed.set_attr(ins, W, "id", "7").unwrap();
    let r = ed.create_element(W, "r").unwrap();
    let txt = ed.create_text("x & y").unwrap();
    ed.append_child(r, txt).unwrap();
    ed.append_child(ins, r).unwrap();
    // `p` was self-closing: it gains children, so its start tag changes and it gets an end tag.
    ed.append_child(p, ins).unwrap();
    let _ = root;
    let out = String::from_utf8(t.serialize()).unwrap();
    assert_eq!(
        out,
        "<ns0:doc xmlns:ns0=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><ns0:p><ns0:ins ns0:id=\"7\"><ns0:r>x &amp; y</ns0:r></ns0:ins></ns0:p></ns0:doc>"
    );
    // A namespace the document does not bind is declared on the root when the element is inserted.
    let mut t = parse(src);
    let p = t.root().first_child().unwrap().id();
    let mut ed = t.edit();
    let q = ed.create_element("urn:other", "q").unwrap();
    assert_eq!(t.node_ref(q).name(), "q", "no prefix until it is inserted");
    t.edit().append_child(p, q).unwrap();
    assert_eq!(
        String::from_utf8(t.serialize()).unwrap(),
        "<ns0:doc xmlns:ns0=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:ns=\"urn:other\"><ns0:p><ns:q/></ns0:p></ns0:doc>"
    );
}

#[test]
fn bind_namespace_declares_on_root_and_updates_ignorable() {
    let mut t = parse(DOC);
    let ns = "http://schemas.microsoft.com/office/word/2023/wordml/word16du";
    assert_eq!(t.edit().bind_namespace(W, "w", false).unwrap(), "w");
    assert_eq!(t.edit().bind_namespace(ns, "w16du", true).unwrap(), "w16du");
    assert_eq!(t.edit().bind_namespace(ns, "other", true).unwrap(), "w16du");
    let out = String::from_utf8(t.serialize()).unwrap();
    assert!(out.contains(&format!(" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" xmlns:w16du=\"{ns}\" mc:Ignorable=\"w14 w16du\">")), "{out}");
    // Without mc declared, it is declared too.
    let mut t = parse("<r xmlns:a=\"urn:a\"/>");
    t.edit().bind_namespace("urn:ext", "ext", true).unwrap();
    let out = String::from_utf8(t.serialize()).unwrap();
    assert_eq!(
        out,
        "<r xmlns:a=\"urn:a\" xmlns:ext=\"urn:ext\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"ext\"/>"
    );
    assert!(parse(&out).root().attr(super::MC_NS, "Ignorable").is_some());
}

#[test]
fn detach_insert_and_clone() {
    let mut t = parse("<r><a/> <b>x</b><c/></r>");
    let kids: Vec<NodeId> = t.root().children().map(|c| c.id()).collect();
    let (a, b, c) = (kids[0], kids[2], kids[3]);
    let root = t.root().id();
    let mut ed = t.edit();
    ed.detach(b).unwrap();
    ed.insert_before(root, Some(a), b).unwrap();
    let copy = ed.clone_subtree(b).unwrap();
    ed.append_child(root, copy).unwrap();
    assert!(ed.append_child(root, b).is_err(), "already in the tree");
    assert!(ed.insert_before(b, None, root).is_err(), "cycle");
    ed.detach(c).unwrap();
    assert_eq!(
        String::from_utf8(t.serialize()).unwrap(),
        "<r><b>x</b><a/> <b>x</b></r>"
    );
    // The clone is clean: it serializes from the original bytes (here, the same text).
    assert!(!t.node_ref(copy).is_dirty());
}

#[test]
fn removing_all_children_keeps_the_pair() {
    let mut t = parse("<r><a>x</a></r>");
    let a = t.root().first_child().unwrap();
    let x = a.first_child().unwrap().id();
    t.edit().detach(x).unwrap();
    assert_eq!(String::from_utf8(t.serialize()).unwrap(), "<r><a></a></r>");
}

#[test]
fn hostile_xml_is_rejected_with_codes() {
    let mal = |s: &[u8]| {
        let e = parse_bytes(s)
            .err()
            .unwrap_or_else(|| panic!("should fail: {:?}", String::from_utf8_lossy(s)));
        assert_eq!(
            e.code(),
            ErrorCode::MalformedInput,
            "{} for {:?}",
            e,
            String::from_utf8_lossy(s)
        );
    };
    mal(b"<!DOCTYPE a [<!ENTITY x \"y\">]><a>&x;</a>");
    mal(b"<!DOCTYPE a><a/>");
    mal(b"<?xml version=\"1.0\"?><!DOCTYPE a SYSTEM \"file:///etc/passwd\"><a/>");
    mal(b"<a><b></a></b>");
    mal(b"<a><b>");
    mal(b"");
    mal(b"   ");
    mal(b"<a/><b/>");
    mal(b"<a/>text");
    mal(b"text<a/>");
    mal(b"<a b=\"<\"/>");
    mal(b"<a b=\"x/>");
    mal(b"<a><!-- never closed </a>");
    mal(b"<a>&nbsp;</a>");
    mal(b"<a>&amp</a>");
    mal(b"<a>a & b</a>");
    mal(b"<a b='1' b='2'/>");
    mal(b"<a b=1/>");
    mal(b"<a b='1'c='2'/>");
    mal(b"<a><![CDATA[ x </a>");
    mal(b"<a/></a>");
    mal(b"<x:a/>");
    mal(b"<a x:b='1'/>");
    mal(b"<a>\xFF\xFE</a>");
    mal(b"<?xml version=\"1.0\" encoding=\"windows-1252\"?><a/>");
    mal(b"<a/><?xml version=\"1.0\"?>");
    mal(b"<a><?xml version=\"1.0\"?></a>");
    mal(b"\xFF\xFE\x00\x00<a/>");
    mal(&encoding::utf8_to_utf16(
        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><a/>",
        false,
        true,
    ));
    // a lone surrogate in a UTF-16 part
    let mut bad16 = encoding::utf8_to_utf16(b"<a>x</a>", false, true);
    bad16.splice(8..8, [0x00, 0xD8]);
    mal(&bad16);
    mal(&[0xFF, 0xFE, b'<']);
}

#[test]
fn depth_and_attribute_limits() {
    let deep = format!("{}{}", "<a>".repeat(300), "</a>".repeat(300));
    let e = parse_bytes(deep.as_bytes()).err().expect("too deep");
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    let ok = format!("{}{}", "<a>".repeat(256), "</a>".repeat(256));
    assert!(parse_bytes(ok.as_bytes()).is_ok());
    // 100,000 levels must fail fast and cleanly, not overflow the stack.
    let huge = "<a>".repeat(100_000);
    assert_eq!(
        parse_bytes(huge.as_bytes()).err().map(|e| e.code()),
        Some(ErrorCode::LimitExceeded)
    );
    let many: String = (0..2000).map(|i| format!(" a{i}=\"1\"")).collect();
    let e = parse_bytes(format!("<r{many}/>").as_bytes())
        .err()
        .expect("too many attributes");
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    let few: String = (0..1000).map(|i| format!(" a{i}=\"1\"")).collect();
    assert_identity(format!("<r{few}/>").as_bytes());
    // Duplicates among many attributes are found too.
    let mut dup: Vec<String> = (0..50).map(|i| format!(" a{i}=\"1\"")).collect();
    dup.push(" a7=\"2\"".to_string());
    assert_eq!(
        parse_bytes(format!("<r{}/>", dup.concat()).as_bytes())
            .err()
            .map(|e| e.code()),
        Some(ErrorCode::MalformedInput)
    );
}

/// No input panics. Every prefix and many single-byte mutations either parse and round trip, or
/// fail with an error code.
#[test]
fn mutation_sweep_never_panics() {
    let seeds: Vec<Vec<u8>> = vec![
        DOC.as_bytes().to_vec(),
        b"\xEF\xBB\xBF<?xml version='1.0'?>\n<a b='1'><![CDATA[x]]><!--c--><?p d?><c/>&#x41;</a>"
            .to_vec(),
        encoding::utf8_to_utf16(
            "<?xml version=\"1.0\" encoding=\"UTF-16\"?><a>h\u{e9}</a>".as_bytes(),
            true,
            true,
        ),
    ];
    for seed in &seeds {
        for cut in 0..=seed.len() {
            check_no_panic(seed.get(..cut).unwrap_or(&[]));
        }
        for i in 0..seed.len() {
            for flip in [0x01u8, 0x20, 0xFF] {
                let mut m = seed.clone();
                if let Some(b) = m.get_mut(i) {
                    *b ^= flip;
                }
                check_no_panic(&m);
            }
        }
    }
}

fn check_no_panic(b: &[u8]) {
    if let Ok(mut t) = parse_bytes(b) {
        assert_eq!(t.serialize(), b);
        t.mark_all_tags_dirty();
        assert_eq!(t.serialize(), b);
        let _ = t.root().text_content();
    }
}

#[test]
fn us_ascii_parts_are_accepted_and_edits_stay_ascii() {
    let src = "<?xml version=\"1.0\" encoding=\"US-ASCII\"?>\n<r a=\"x\"><t>plain</t></r>";
    let mut t = assert_identity(src.as_bytes());
    let txt = t.root().first_child().unwrap().first_child().unwrap().id();
    t.edit().set_text(txt, "caf\u{e9} \u{1F600}").unwrap();
    let r = t.root().id();
    t.edit().set_attr(r, "", "b", "\u{4e2d}").unwrap();
    let out = t.serialize();
    assert!(out.is_ascii(), "{}", String::from_utf8_lossy(&out));
    let s = String::from_utf8(out).unwrap();
    assert!(
        s.contains("<t>caf&#xE9; &#x1F600;</t>") && s.contains(" b=\"&#x4E2D;\""),
        "{s}"
    );
    // A non-ASCII byte under an ASCII declaration is refused, and so is windows-1252.
    let bad = "<?xml version=\"1.0\" encoding=\"us-ascii\"?><r>caf\u{e9}</r>";
    assert_eq!(
        parse_bytes(bad.as_bytes()).err().map(|e| e.code()),
        Some(ErrorCode::MalformedInput)
    );
    let w = "<?xml version=\"1.0\" encoding=\"windows-1252\"?><r>plain</r>";
    assert_eq!(
        parse_bytes(w.as_bytes()).err().map(|e| e.code()),
        Some(ErrorCode::MalformedInput)
    );
}

/// The locality law at the XML level: whatever one edit does, the bytes outside the edited
/// element are the bytes of the input.
#[test]
fn every_single_edit_is_confined_to_its_node() {
    let src = DOC;
    let base = parse(src);
    // Collect element and text nodes with their spans.
    let mut nodes: Vec<(NodeId, NodeKind, (usize, usize))> = Vec::new();
    let mut stack = vec![base.root()];
    while let Some(n) = stack.pop() {
        if let Some(span) = n.source_span() {
            nodes.push((n.id(), n.kind(), span));
        }
        stack.extend(n.children());
    }
    assert!(nodes.len() > 12, "{} nodes", nodes.len());
    let old = src.as_bytes();
    for (id, kind, span) in nodes {
        match kind {
            NodeKind::Text => {
                let mut t = base.clone();
                t.edit().set_text(id, "new & text").unwrap();
                assert_confined(old, &t.serialize(), span);
            }
            NodeKind::Element => {
                // set an attribute, change one, remove one
                let mut t = base.clone();
                t.edit().set_attr(id, "", "added", "1").unwrap();
                assert_confined(old, &t.serialize(), span);
                let mut t = base.clone();
                if id == base.root().id() {
                    // A document has one root element, so it cannot be taken out.
                    assert!(t.edit().detach(id).is_err());
                } else {
                    t.edit().detach(id).unwrap();
                    assert_confined(old, &t.serialize(), span);
                }
                // insert a new element as the last child
                let mut t = base.clone();
                {
                    let mut e = t.edit();
                    let w = e.create_element(W, "ins").unwrap();
                    e.append_child(id, w).unwrap();
                }
                assert_confined(old, &t.serialize(), span);
            }
            _ => {}
        }
    }
}

/// Nothing recurses on input depth: a document nested far deeper than any limit we ship parses,
/// serializes, clones, edits and drops without growing the stack. (POI's `deep-table-cell.docx`
/// nests 15,005 levels, which overflows a recursive parser. This goes ten times deeper.)
#[test]
fn deep_documents_do_not_recurse() {
    const DEPTH: usize = 150_000;
    let src = format!("{}x{}", "<a>".repeat(DEPTH), "</a>".repeat(DEPTH));
    let lim = Limits {
        max_xml_depth: 1_000_000,
        ..Limits::default()
    };
    let mut t =
        SpanTree::parse("test.xml", Bytes::from(src.clone().into_bytes()), &lim).expect("parse");
    assert_eq!(t.serialize(), src.as_bytes());
    assert_eq!(t.root().text_content(), "x");
    t.mark_all_tags_dirty();
    assert_eq!(t.serialize(), src.as_bytes());
    // Clone the whole subtree, edit the innermost text, and serialize the edited tree.
    let root = t.root().id();
    let copy = t.edit().clone_subtree(root).expect("clone");
    let mut inner = t.root();
    while let Some(c) = inner.first_child() {
        inner = c;
    }
    let text = inner.id();
    t.edit().set_text(text, "y").expect("set_text");
    let out = t.serialize();
    assert_eq!(out.len(), src.len());
    assert!(t.node_ref(copy).is_element());
    assert_eq!(out.iter().filter(|&&b| b == b'y').count(), 1);
    // And the shipped default refuses it cleanly.
    let e = SpanTree::parse("test.xml", Bytes::from(src.into_bytes()), &limits()).err();
    assert_eq!(e.map(|e| e.code()), Some(ErrorCode::LimitExceeded));
}

#[test]
fn namespace_lookup_text_test_and_node_equality() {
    let t = parse(
        "<r xmlns=\"urn:d\" xmlns:a=\"urn:a\"><x xmlns:a=\"urn:inner\" xmlns=\"\"><y/></x><z>  </z><w> t </w></r>",
    );
    let root = t.root();
    let x = root.first_child().unwrap();
    let y = x.first_child().unwrap();
    assert_eq!(root.namespace_of_prefix("a").as_deref(), Some("urn:a"));
    assert_eq!(y.namespace_of_prefix("a").as_deref(), Some("urn:inner"));
    assert_eq!(root.namespace_of_prefix("").as_deref(), Some("urn:d"));
    assert_eq!(
        y.namespace_of_prefix("").as_deref(),
        Some(""),
        "default undeclared"
    );
    assert_eq!(y.namespace_of_prefix("nope"), None);
    assert!(
        y.namespace_of_prefix("xml")
            .is_some_and(|u| u.ends_with("/XML/1998/namespace"))
    );
    let decls: Vec<bool> = root.attrs().map(|a| a.is_namespace_declaration()).collect();
    assert_eq!(decls, [true, true]);
    let z = x.next_sibling().unwrap();
    let w = z.next_sibling().unwrap();
    assert!(!z.has_text(), "whitespace only");
    assert!(w.has_text());
    assert!(!root.has_text());
    assert_eq!(root.first_child().unwrap(), x);
    assert_ne!(x, z);
    let other = parse("<r/>");
    assert_ne!(other.root(), root);
}

#[test]
fn node_and_attribute_count_is_limited() {
    let src = format!("<r>{}</r>", "<a b=\"1\"/>".repeat(1000));
    let parse_with = |max_xml_nodes: u32| {
        let lim = Limits {
            max_xml_nodes,
            ..Limits::default()
        };
        SpanTree::parse("test.xml", Bytes::from(src.clone().into_bytes()), &lim)
    };
    // The document node, the root, 1,000 elements and 1,000 attributes make 2,002.
    assert!(parse_with(2002).is_ok());
    let e = parse_with(2001).err().expect("one over the limit");
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    assert!(e.message().starts_with("test.xml: "), "{e}");
    assert_eq!(
        parse_with(1500).err().map(|e| e.code()),
        Some(ErrorCode::LimitExceeded)
    );
}
