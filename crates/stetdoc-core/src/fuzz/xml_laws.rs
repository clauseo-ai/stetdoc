//! The `xml` target: parse, serialize clean and with every tag dirty, then make one edit.

use super::xml_edits::{apply, plan};
use super::{fail, split};
use crate::api::error::{ErrorCode, StetError};
use crate::api::limits::Limits;
use crate::xml::{Bytes, NodeId, NodeKind, NodeRef, SpanTree};

/// The name every part is parsed under, which starts every error message about it.
const PART: &str = "word/document.xml";

/// Runs the target on one input, and aborts when a law breaks.
pub(super) fn run(data: &[u8]) {
    if let Err(why) = check(data, &Limits::default()) {
        fail(&why);
    }
}

/// Checks the laws of `SpanTree` on one input under `limits`.
///
/// A part that parses must be read without a panic, serialize to its input, and do so again with
/// every start tag dirty. Then one edit, chosen by the first four bytes of the input, is made on
/// it: what the edit says must hold in the tree, the part it writes must parse again as that same
/// tree, and every clean node must be in the output as it was in the input.
pub(super) fn check(data: &[u8], limits: &Limits) -> Result<(), String> {
    let Some((head, doc)) = split(data) else {
        return Ok(());
    };
    let mut tree = match SpanTree::parse(PART, Bytes::from(doc), limits) {
        Ok(tree) => tree,
        Err(e) => return refused(&e),
    };
    read_everything(&tree)?;
    if tree.serialize() != doc {
        return Err("an unedited tree does not serialize to its input".into());
    }
    let mut dirty = tree.clone();
    dirty.mark_all_tags_dirty();
    if dirty.serialize() != doc {
        return Err("the tree with every start tag dirty does not serialize to its input".into());
    }
    let Some(edit) = plan(&tree, head) else {
        return Ok(());
    };
    apply(&mut tree, &edit)?;
    check_output(&tree, doc, limits)
}

/// A refusal of a part must be `malformed_input` or `limit_exceeded`, and must name the part.
pub(super) fn refused(e: &StetError) -> Result<(), String> {
    if !matches!(
        e.code(),
        ErrorCode::MalformedInput | ErrorCode::LimitExceeded
    ) {
        return Err(format!(
            "parse failed with {:?}, which is not how hostile XML is refused: {e}",
            e.code()
        ));
    }
    if !e.message().starts_with(&format!("{PART}: ")) {
        return Err(format!("an error does not name the part: {e}"));
    }
    Ok(())
}

/// `node` and everything below it, in document order, with each node's depth below `node`. The
/// walk keeps its own stack, so a deep tree cannot overflow this one.
pub(super) fn walk(node: NodeRef<'_>) -> Vec<(NodeRef<'_>, usize)> {
    let mut out = Vec::new();
    let mut stack = vec![(node, 0usize)];
    while let Some((n, depth)) = stack.pop() {
        out.push((n, depth));
        let mark = stack.len();
        stack.extend(n.children().map(|c| (c, depth + 1)));
        if let Some(tail) = stack.get_mut(mark..) {
            tail.reverse();
        }
    }
    out
}

pub(super) fn of_kind<'a>(all: &[(NodeRef<'a>, usize)], kind: NodeKind) -> Vec<NodeRef<'a>> {
    all.iter()
        .map(|(n, _)| *n)
        .filter(|n| n.kind() == kind)
        .collect()
}

pub(super) fn count_elements(node: NodeRef<'_>) -> usize {
    walk(node).iter().filter(|(n, _)| n.is_element()).count()
}

/// Every node of a tree with its kind and, for an element, its name. A node keeps its identity for
/// the life of the tree: an edit never renumbers a node or gives its handle to another.
pub(super) fn identities(tree: &SpanTree) -> Vec<(NodeId, NodeKind, String)> {
    let all = walk(tree.document());
    all.iter()
        .map(|(n, _)| (n.id(), n.kind(), n.name().to_owned()))
        .collect()
}

/// Checks that every node in `before` is still the same node.
pub(super) fn identities_are_kept(
    tree: &SpanTree,
    before: &[(NodeId, NodeKind, String)],
) -> Result<(), String> {
    for (id, kind, name) in before {
        let now = tree.node_ref(*id);
        if now.kind() != *kind || now.name() != name {
            return Err(format!(
                "the handle of a {kind:?} node named {name:?} now means a {:?} node named {:?}",
                now.kind(),
                now.name()
            ));
        }
    }
    Ok(())
}

/// Reads everything a tree can be asked, which must not panic, and checks that the links between
/// nodes agree.
fn read_everything(tree: &SpanTree) -> Result<(), String> {
    for (node, _) in walk(tree.document()) {
        let _ = (node.kind(), node.is_dirty(), node.has_text(), node.text());
        if let Some((start, end)) = node.source_span()
            && (start > end || node.source_bytes().len() != end - start)
        {
            return Err(format!(
                "source_span {start}..{end} does not match source_bytes"
            ));
        }
        if node.children().any(|c| c.parent() != Some(node)) {
            return Err("a child's parent is not the node it is a child of".into());
        }
        let kids: Vec<NodeRef<'_>> = node.children().collect();
        for (i, kid) in kids.iter().enumerate() {
            let before = i.checked_sub(1).and_then(|j| kids.get(j)).copied();
            if kid.prev_sibling() != before {
                return Err("a child's previous sibling is not the child before it".into());
            }
        }
        if !node.is_element() {
            continue;
        }
        let _ = (
            node.name(),
            node.prefix(),
            node.local_name(),
            node.namespace(),
            node.namespace_of_prefix(node.prefix()),
        );
        for attr in node.attrs() {
            let _ = (
                attr.name(),
                attr.local_name(),
                attr.namespace(),
                attr.value(),
                attr.raw_value(),
            );
            if !attr.is_namespace_declaration()
                && node.attr(attr.namespace(), attr.local_name()).is_none()
            {
                return Err(format!(
                    "attribute {:?} of {:?} cannot be found by its own name",
                    attr.name(),
                    node.name()
                ));
            }
        }
    }
    let _ = tree.root().text_content();
    Ok(())
}

/// Checks the part the tree writes: it must parse again, as the same tree, and serialize to
/// itself, and every clean node must be in it as it was in the input.
pub(super) fn check_output(tree: &SpanTree, doc: &[u8], limits: &Limits) -> Result<(), String> {
    read_everything(tree)?;
    let out = tree.serialize();
    if tree.serialize() != out {
        return Err("serialize is not deterministic".into());
    }
    if !tree.is_dirty() && out != doc {
        return Err("a tree with no edit does not serialize to its input".into());
    }
    let again = SpanTree::parse(PART, Bytes::from(out.clone()), limits)
        .map_err(|e| format!("the part an edit wrote does not parse: {e}"))?;
    if again.serialize() != out {
        return Err("the part an edit wrote does not serialize to itself".into());
    }
    let mut dirty = again.clone();
    dirty.mark_all_tags_dirty();
    if dirty.serialize() != out {
        return Err(
            "the part an edit wrote, with every start tag dirty, does not serialize to itself"
                .into(),
        );
    }
    let (wrote, read) = (dump(tree), dump(&again));
    if let Some(at) = (0..wrote.len().max(read.len())).find(|&i| wrote.get(i) != read.get(i)) {
        return Err(format!(
            "the part an edit wrote reads back differently at node {at}: the tree had {:?} and the part has {:?}",
            shown(wrote.get(at)),
            shown(read.get(at))
        ));
    }
    clean_nodes_are_in_the_output(tree, &again)
}

fn shown(line: Option<&String>) -> String {
    line.map_or("nothing", |l| l.as_str())
        .chars()
        .take(200)
        .collect()
}

/// The tree as lines, one per node, that do not depend on prefixes, the order of attributes or how
/// text is split into nodes: a node's depth and kind, an element's namespace, name and sorted
/// attributes, and decoded text.
pub(super) fn dump(tree: &SpanTree) -> Vec<String> {
    fn flush(text: &mut Option<(usize, String)>, lines: &mut Vec<String>) {
        if let Some((depth, t)) = text.take()
            && !t.is_empty()
        {
            lines.push(format!("{depth} text {t:?}"));
        }
    }
    let mut lines = Vec::new();
    let mut text: Option<(usize, String)> = None;
    for (node, depth) in walk(tree.document()).into_iter().skip(1) {
        if node.kind() == NodeKind::Text {
            let t = node.text().into_owned();
            match &mut text {
                Some((d, acc)) if *d == depth => acc.push_str(&t),
                _ => {
                    flush(&mut text, &mut lines);
                    text = Some((depth, t));
                }
            }
            continue;
        }
        flush(&mut text, &mut lines);
        lines.push(match node.kind() {
            NodeKind::Element => {
                let mut attrs: Vec<String> = node
                    .attrs()
                    .filter(|a| !a.is_namespace_declaration())
                    .map(|a| format!("{{{}}}{}={:?}", a.namespace(), a.local_name(), a.value()))
                    .collect();
                attrs.sort();
                format!(
                    "{depth} element {{{}}}{} {}",
                    node.namespace(),
                    node.local_name(),
                    attrs.join(" ")
                )
            }
            NodeKind::CData => format!("{depth} cdata {:?}", node.text()),
            NodeKind::Comment => format!("{depth} comment {:?}", node.text()),
            _ => format!(
                "{depth} other {:?}",
                String::from_utf8_lossy(node.source_bytes())
            ),
        });
    }
    flush(&mut text, &mut lines);
    lines
}

/// An edit changes only what it touches: every clean node of the edited tree must be in the part
/// it wrote, byte for byte, and in the order the tree has them. There is no exception for text that
/// meets text: a text node that an edit makes meet another one (by detaching what lay between the
/// two, or by inserting text beside text) is dirty, because the two are written joined, and a clean
/// one is written as it was read. The part is read through `again`, which holds its UTF-8 form
/// whatever the encoding of the input was.
pub(super) fn clean_nodes_are_in_the_output(
    edited: &SpanTree,
    again: &SpanTree,
) -> Result<(), String> {
    let haystack = again.document().source_bytes();
    let mut pos = 0usize;
    let mut stack = vec![edited.document()];
    while let Some(node) = stack.pop() {
        if node.kind() != NodeKind::Document && !node.is_dirty() {
            let bytes = node.source_bytes();
            if bytes.is_empty() {
                continue;
            }
            let rest = haystack.get(pos..).unwrap_or(&[]);
            let Some(at) = memchr::memmem::find(rest, bytes) else {
                return Err(format!(
                    "a clean {:?} node of {} bytes ({:?}) is not in the part after byte {pos}",
                    node.kind(),
                    bytes.len(),
                    String::from_utf8_lossy(bytes.get(..bytes.len().min(60)).unwrap_or(bytes))
                ));
            };
            pos += at + bytes.len();
            continue;
        }
        let mark = stack.len();
        stack.extend(node.children());
        if let Some(tail) = stack.get_mut(mark..) {
            tail.reverse();
        }
    }
    Ok(())
}
