//! Tests for text that meets other text: bytes that read right apart can read differently when an
//! edit makes them touch, and for the text that can stand outside the root element. The model test
//! at the end is the property that matters: after an edit, writing the tree and reading it back
//! gives the same text and the same elements.

use super::*;
use crate::api::error::ErrorCode;

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

/// The first element called `name`, in document order.
fn element(t: &SpanTree, name: &str) -> NodeId {
    let mut stack = vec![t.document().id()];
    while let Some(id) = stack.pop() {
        let n = t.node_ref(id);
        if n.is_element() && n.local_name() == name {
            return id;
        }
        let kids: Vec<NodeId> = n.children().map(|c| c.id()).collect();
        stack.extend(kids.into_iter().rev());
    }
    panic!("no element {name}");
}

#[test]
fn a_carriage_return_and_a_line_feed_that_meet_are_two_line_breaks_still() {
    // The tree reads "x\n" then "\ny": two line breaks. Written as they stand, the bytes `x\r` and
    // `\ny` read back as one.
    let mut t = parse("<a>x\r<b/>\ny</a>");
    assert_eq!(t.root().text_content(), "x\n\ny");
    let b = element(&t, "b");
    t.edit().detach(b).unwrap();
    assert_eq!(t.root().text_content(), "x\n\ny");
    let written = out(&t);
    assert_eq!(written, "<a>x\n\ny</a>");
    assert_eq!(parse(&written).root().text_content(), "x\n\ny");
    // The first text node still reads as it did: only its last byte is written differently.
    assert_eq!(t.root().first_child().unwrap().text(), "x\n");

    // The same when the line feed is text that was added.
    let mut t = parse("<a>x\r<b/></a>");
    let b = element(&t, "b");
    let a = t.root().id();
    {
        let mut ed = t.edit();
        ed.detach(b).unwrap();
        let text = ed.create_text("\ny").unwrap();
        ed.append_child(a, text).unwrap();
    }
    assert_eq!(out(&t), "<a>x\n\ny</a>");

    // A carriage return before a carriage return, and a pair that is one line break already, are
    // not touched: they read back as they stand.
    let mut t = parse("<a>x\r<b/>\ry</a>");
    let b = element(&t, "b");
    t.edit().detach(b).unwrap();
    assert_eq!(out(&t), "<a>x\r\ry</a>");
    assert_eq!(
        parse(&out(&t)).root().text_content(),
        t.root().text_content()
    );
    let mut t = parse("<a>x\r\n<b/>\ny</a>");
    let b = element(&t, "b");
    t.edit().detach(b).unwrap();
    assert_eq!(out(&t), "<a>x\r\n\ny</a>");

    // A carriage return that was written as a reference is a character of its own, and is kept.
    let mut t = parse("<a>x&#13;<b/>\ny</a>");
    let b = element(&t, "b");
    t.edit().detach(b).unwrap();
    assert_eq!(out(&t), "<a>x&#13;\ny</a>");
    assert_eq!(parse(&out(&t)).root().text_content(), "x\r\ny");
}

#[test]
fn text_that_meets_text_never_makes_a_cdata_end() {
    // `]]` then `>` is not allowed in content. The later `>` is written as a reference.
    let mut t = parse("<a>x]]<b/>>y</a>");
    let b = element(&t, "b");
    t.edit().detach(b).unwrap();
    assert_eq!(t.root().text_content(), "x]]>y");
    assert_eq!(out(&t), "<a>x]]&gt;y</a>");
    assert_eq!(parse(&out(&t)).root().text_content(), "x]]>y");
    // One `]` is fine.
    let mut t = parse("<a>x]<b/>>y</a>");
    let b = element(&t, "b");
    t.edit().detach(b).unwrap();
    assert_eq!(out(&t), "<a>x]>y</a>");
    // The split can also fall between the two brackets: `]` before the join, `]>` after it.
    let mut t = parse("<a>x]<b/>]>y</a>");
    let b = element(&t, "b");
    t.edit().detach(b).unwrap();
    assert_eq!(t.root().text_content(), "x]]>y");
    assert_eq!(out(&t), "<a>x]]&gt;y</a>");
    assert!(roxmltree::Document::parse(&out(&t)).is_ok());
}

#[test]
fn a_splice_joins_what_it_keeps_the_same_way() {
    // `replace_text_range` keeps the raw bytes outside the range. "x\ry\nz" reads "x\ny\nz".
    let mut t = parse("<a>x\ry\nz</a>");
    let text = t.root().first_child().unwrap().id();
    t.edit().replace_text_range(text, 2, 3, "").unwrap();
    assert_eq!(t.root().text_content(), "x\n\nz");
    assert_eq!(out(&t), "<a>x\n\nz</a>");

    let mut t = parse("<a>x\ry\nz</a>");
    let text = t.root().first_child().unwrap().id();
    t.edit().replace_text_range(text, 2, 2, "\n").unwrap();
    assert_eq!(t.root().text_content(), "x\n\ny\nz");
    assert_eq!(parse(&out(&t)).root().text_content(), "x\n\ny\nz");

    let mut t = parse("<a>a]]b>c</a>");
    let text = t.root().first_child().unwrap().id();
    t.edit().replace_text_range(text, 3, 4, "").unwrap();
    assert_eq!(t.root().text_content(), "a]]>c");
    assert_eq!(out(&t), "<a>a]]&gt;c</a>");
    // The `]` that is added meets the `>` that was kept.
    let mut t = parse("<a>a]>c</a>");
    let text = t.root().first_child().unwrap().id();
    t.edit().replace_text_range(text, 2, 2, "]").unwrap();
    assert_eq!(t.root().text_content(), "a]]>c");
    assert_eq!(out(&t), "<a>a]]&gt;c</a>");
    // Removing what stood between `]` and `]>` makes them meet.
    let mut t = parse("<a>x]Z]>y</a>");
    let text = t.root().first_child().unwrap().id();
    t.edit().replace_text_range(text, 2, 3, "").unwrap();
    assert_eq!(t.root().text_content(), "x]]>y");
    assert_eq!(out(&t), "<a>x]]&gt;y</a>");
    assert!(roxmltree::Document::parse(&out(&t)).is_ok());
}

#[test]
fn the_root_element_cannot_be_detached() {
    // A document without a root element is not a document: the part written would not parse.
    let mut t = parse("<?xml version=\"1.0\"?>\n<a><b/></a>");
    let root = t.root().id();
    let e = t.edit().detach(root).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
    assert!(e.message().contains("root element"), "{e}");
    assert!(!t.is_dirty());
    // A node that is not in the tree is detached already, and everything else can go.
    let b = t.root().first_child().unwrap().id();
    t.edit().detach(b).unwrap();
    t.edit().detach(b).unwrap();
    assert_eq!(out(&t), "<?xml version=\"1.0\"?>\n<a></a>");
}

#[test]
fn only_white_space_can_be_text_outside_the_root_element() {
    let src = "<?xml version=\"1.0\"?>\n<a/>";
    let doc_text = |t: &SpanTree| {
        t.document()
            .children()
            .find(|c| c.kind() == NodeKind::Text)
            .unwrap()
            .id()
    };

    // The reported case: the line feed after the declaration set to "x" made a part that did not
    // parse.
    let mut t = parse(src);
    let ws = doc_text(&t);
    let e = t.edit().set_text(ws, "x").unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
    assert!(e.message().contains("outside the root element"), "{e}");
    let e = t.edit().replace_text_range(ws, 0, 1, "x").unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
    let e = t.edit().replace_text_range(ws, 1, 1, " x").unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
    for bad in ["&", "<", " a ", "\u{a0}", "\u{e9}"] {
        assert!(t.edit().set_text(ws, bad).is_err(), "{bad:?}");
    }
    assert!(!t.is_dirty(), "a refused edit marks nothing");
    assert_eq!(out(&t), src);

    // White space is allowed, and written as itself.
    t.edit().set_text(ws, "\n\t \r\n").unwrap();
    assert_eq!(out(&t), "<?xml version=\"1.0\"?>\n\t \r\n<a/>");
    assert!(
        SpanTree::parse(
            "again.xml",
            Bytes::from(out(&t).into_bytes()),
            &Limits::default()
        )
        .is_ok()
    );
    t.edit().replace_text_range(ws, 0, 0, "   ").unwrap();
    assert_eq!(out(&t), "<?xml version=\"1.0\"?>   \n\t \r\n<a/>");
    assert_eq!(t.document().text_content(), "   \n\t \n");

    // Text inside the root element is not touched by any of this.
    let mut t = parse("<a>t</a>");
    let inner = t.root().first_child().unwrap().id();
    t.edit().set_text(inner, "any <text> & more").unwrap();
    assert_eq!(out(&t), "<a>any &lt;text&gt; &amp; more</a>");
}

#[test]
fn only_comments_instructions_and_white_space_can_be_put_under_the_document() {
    let mut t = parse("<?xml version=\"1.0\"?>\n<!--c--><a>text<![CDATA[data]]></a>");
    let doc = t.document().id();
    let root = t.root().id();
    let comment = t
        .document()
        .children()
        .find(|c| c.kind() == NodeKind::Comment)
        .unwrap()
        .id();
    let cdata = t
        .root()
        .children()
        .find(|c| c.kind() == NodeKind::CData)
        .unwrap()
        .id();
    let inner_text = t.root().first_child().unwrap().id();
    let mut ed = t.edit();

    // Text that is not white space, as it is made or as it is copied from inside an element.
    let words = ed.create_text("words").unwrap();
    let e = ed.insert_before(doc, Some(root), words).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
    let copy = ed.clone_subtree(inner_text).unwrap();
    assert_eq!(
        ed.insert_before(doc, Some(root), copy).unwrap_err().code(),
        ErrorCode::InvalidPayload
    );
    // A carriage return made by `create_text` is written as a reference, which is not allowed there.
    let cr = ed.create_text("\r").unwrap();
    assert_eq!(
        ed.insert_before(doc, Some(root), cr).unwrap_err().code(),
        ErrorCode::InvalidPayload
    );
    // A CDATA section.
    let section = ed.clone_subtree(cdata).unwrap();
    let e = ed.insert_before(doc, Some(root), section).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
    // None of that changed the document, and the nodes are still free.
    assert!(t.node_ref(words).parent().is_none());

    // White space and a comment are fine.
    let mut ed = t.edit();
    let spaces = ed.create_text("\n  ").unwrap();
    ed.insert_before(doc, Some(root), spaces).unwrap();
    let copy = ed.clone_subtree(comment).unwrap();
    ed.insert_before(doc, Some(root), copy).unwrap();
    let written = out(&t);
    assert!(written.contains("<!--c-->\n  <!--c--><a>"), "{written}");
    assert!(
        SpanTree::parse(
            "again.xml",
            Bytes::from(written.into_bytes()),
            &Limits::default()
        )
        .is_ok()
    );
}

// ---------------------------------------------------------------------------------------------
// The model: after any edit, writing the tree and reading it back gives the same text and the same
// elements.

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % n.max(1) as u64).unwrap_or(0)
    }
}

/// Text made of the pieces that make trouble when they meet.
fn word(rng: &mut Rng) -> String {
    const PIECES: [&str; 14] = [
        "x", "y", "\r", "\n", "\r\n", "]", "]]", ">", "&", "<", "\u{e9}", " ", "\t", "z",
    ];
    (0..rng.below(4))
        .map(|_| PIECES[rng.below(PIECES.len())])
        .collect()
}

/// Every attached node below the document, parents before children.
fn attached(t: &SpanTree) -> Vec<NodeId> {
    let mut found = Vec::new();
    let mut stack: Vec<NodeId> = t.document().children().map(|c| c.id()).collect();
    while let Some(id) = stack.pop() {
        found.push(id);
        stack.extend(t.node_ref(id).children().map(|c| c.id()));
    }
    found
}

fn element_names(t: &SpanTree) -> Vec<String> {
    let mut names = Vec::new();
    let mut stack = vec![t.document().id()];
    while let Some(id) = stack.pop() {
        let n = t.node_ref(id);
        if n.is_element() {
            names.push(n.name().to_owned());
        }
        let kids: Vec<NodeId> = n.children().map(|c| c.id()).collect();
        stack.extend(kids.into_iter().rev());
    }
    names
}

/// One edit, chosen from what the tree holds, before anything is borrowed to make it.
enum Op {
    Detach(NodeId),
    SetText(NodeId, String),
    Replace(NodeId, usize, usize, String),
    InsertText(NodeId, Option<NodeId>, String),
    InsertCopy(NodeId, NodeId),
    Move(NodeId, NodeId),
}

fn choose(t: &SpanTree, rng: &mut Rng) -> Option<Op> {
    let nodes = attached(t);
    let root = t.root().id();
    let doc = t.document().id();
    let texts: Vec<NodeId> = nodes
        .iter()
        .copied()
        .filter(|id| t.node_ref(*id).kind() == NodeKind::Text)
        .collect();
    let elements: Vec<NodeId> = nodes
        .iter()
        .copied()
        .filter(|id| t.node_ref(*id).is_element())
        .collect();
    // Not the root: a document without one is not a document.
    let not_root = |rng: &mut Rng| {
        let id = nodes[rng.below(nodes.len())];
        (id != root).then_some(id)
    };
    let element = |rng: &mut Rng| elements[rng.below(elements.len())];
    match rng.below(6) {
        // The root too: detaching it is refused, which the check after the edit does not see.
        0 => Some(Op::Detach(nodes[rng.below(nodes.len())])),
        1 if !texts.is_empty() => Some(Op::SetText(texts[rng.below(texts.len())], word(rng))),
        2 if !texts.is_empty() => {
            let id = texts[rng.below(texts.len())];
            let len = t.node_ref(id).text().chars().count();
            let a = rng.below(len + 1);
            let b = a + rng.below(len + 1 - a);
            Some(Op::Replace(id, a, b, word(rng)))
        }
        3 => {
            let parent = if rng.below(8) == 0 { doc } else { element(rng) };
            let kids: Vec<NodeId> = t.node_ref(parent).children().map(|c| c.id()).collect();
            let before =
                (!kids.is_empty() && rng.below(2) == 0).then(|| kids[rng.below(kids.len())]);
            Some(Op::InsertText(parent, before, word(rng)))
        }
        4 => Some(Op::InsertCopy(not_root(rng)?, element(rng))),
        _ => Some(Op::Move(not_root(rng)?, element(rng))),
    }
}

fn apply(t: &mut SpanTree, op: Op) -> Result<&'static str, crate::api::error::StetError> {
    let mut ed = t.edit();
    match op {
        Op::Detach(id) => ed.detach(id).map(|()| "detach"),
        Op::SetText(id, s) => ed.set_text(id, &s).map(|()| "set_text"),
        Op::Replace(id, a, b, s) => ed
            .replace_text_range(id, a, b, &s)
            .map(|()| "replace_text_range"),
        Op::InsertText(parent, before, s) => {
            let text = ed.create_text(&s)?;
            ed.insert_before(parent, before, text)
                .map(|()| "insert text")
        }
        Op::InsertCopy(id, parent) => {
            let copy = ed.clone_subtree(id)?;
            ed.insert_before(parent, None, copy).map(|()| "insert copy")
        }
        Op::Move(id, parent) => {
            ed.detach(id)?;
            // An element cannot go inside itself, and then the node stays where it is put back.
            ed.insert_before(parent, None, id).map(|()| "move")
        }
    }
}

#[test]
fn after_any_edit_the_written_part_reads_back_as_the_tree() {
    let starts = [
        "<?xml version=\"1.0\"?>\r\n<!--c--><a>x\r<b/>\ny<c>]]<d/>>z</c>\r<e/>\r\n<f>t</f></a>\n",
        "<a>1\r\n<b>]<c/>]</b>\n\r<d/>&gt;<e/>\r</a>",
        "<a><![CDATA[\r]]>\r<b/>\n<c/>]]<d/></a>",
        "<?xml version=\"1.0\"?>\n<a>\u{e9}\r<b/>\r<c/>\n\n<d>\r</d>]]<!--k--></a>\n",
        // `]` and `]>` on either side of an element: a detach or a splice makes them meet.
        "<a>p]<b/>]>q<c>]<d/>]>r</c>]<e/>]>s</a>",
    ];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut checked = 0usize;
    for (case, src) in starts.iter().enumerate() {
        for round in 0..200 {
            let mut t = parse(src);
            for step in 0..60 {
                let Some(op) = choose(&t, &mut rng) else {
                    continue;
                };
                // An edit that is refused changes nothing, and the check below covers that too.
                let Ok(what) = apply(&mut t, op) else {
                    continue;
                };
                let written = t.serialize();
                let back = SpanTree::parse("back.xml", Bytes::from(written.clone()), &Limits::default())
                    .unwrap_or_else(|e| {
                        panic!(
                            "case {case}, round {round}, step {step} ({what}): the part written after the edit does not parse: {e}\n{:?}",
                            String::from_utf8_lossy(&written)
                        )
                    });
                // An independent parser checks that the written part is well-formed XML.
                if let Err(e) = roxmltree::Document::parse(&String::from_utf8_lossy(&written)) {
                    panic!(
                        "case {case}, round {round}, step {step} ({what}): the written part is not well-formed XML: {e}\n{:?}",
                        String::from_utf8_lossy(&written)
                    );
                }
                assert_eq!(
                    back.document().text_content(),
                    t.document().text_content(),
                    "case {case}, round {round}, step {step} ({what}): the text changed\n{:?}\n{}",
                    String::from_utf8_lossy(&written),
                    dump(&t)
                );
                assert_eq!(
                    element_names(&back),
                    element_names(&t),
                    "case {case}, round {round}, step {step} ({what})"
                );
                if let Err(e) = clean_nodes_are_written_as_they_were(&t, &back) {
                    panic!(
                        "case {case}, round {round}, step {step} ({what}): {e}\n{:?}\n{}",
                        String::from_utf8_lossy(&written),
                        dump(&t)
                    );
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 3000, "only {checked} edits were checked");
}

/// A node that an edit left clean is written as the bytes it had in the source, in the order the tree
/// has them. Text that meets text is not left clean, because the writer can write either of the
/// two differently (`join_bytes`), so this holds without an exception for it.
fn clean_nodes_are_written_as_they_were(t: &SpanTree, back: &SpanTree) -> Result<(), String> {
    let written = back.document().source_bytes();
    let mut pos = 0usize;
    let mut stack = vec![t.document()];
    while let Some(node) = stack.pop() {
        if node.kind() != NodeKind::Document && !node.is_dirty() {
            let bytes = node.source_bytes();
            let found = memchr::memmem::find(&written[pos..], bytes);
            let Some(at) = found else {
                return Err(format!(
                    "a clean {:?} node {:?} is not in the part after byte {pos}",
                    node.kind(),
                    String::from_utf8_lossy(bytes)
                ));
            };
            pos += at + bytes.len();
            continue;
        }
        let mark = stack.len();
        stack.extend(node.children());
        stack[mark..].reverse();
    }
    Ok(())
}

/// The nodes of the tree and the bytes each holds, for a failure message.
fn dump(t: &SpanTree) -> String {
    let mut lines = Vec::new();
    let mut stack = vec![(t.document().id(), 0usize)];
    while let Some((id, depth)) = stack.pop() {
        let n = t.node_ref(id);
        let raw = t.str_of(t.node(id.0).span);
        let shown = if n.is_element() {
            n.name().to_owned()
        } else {
            format!("{raw:?}")
        };
        lines.push(format!("{}{:?} {shown}", "  ".repeat(depth), n.kind()));
        let kids: Vec<NodeId> = n.children().map(|c| c.id()).collect();
        stack.extend(kids.into_iter().rev().map(|k| (k, depth + 1)));
    }
    lines.join("\n")
}
