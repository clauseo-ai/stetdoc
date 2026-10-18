//! Wrapping what is already in the tree in a revision: the content of a paragraph, the rows of a table and the marks of the
//! paragraphs in them (design `compare.md`, 6.1). These are the node-level helpers of `mark_inserted` and `place_deleted`.
//!
//! A run is wrapped where it stands, so the wrapper goes inside a hyperlink, a smart tag or a content control that holds the
//! run and never around it (the schema has no `w:ins` around a hyperlink). Consecutive runs, and the range markers between
//! them, share one wrapper. An insertion leaves the `separate` run of a field outside the wrapper, as Word writes it
//! (`RP020`), and a deletion holds the whole field. A deletion renames `w:t` to `w:delText`.

use super::compiler::Compiler;
use super::errors::internal;
use super::insert_run::write_revision_attrs;
use super::names::{is_w, wns};
use super::props_edit::{ensure_chain, find_chain, place};
use super::tree_ops::{child_ids, create_w, insert, move_to, parent_of};
use crate::api::error::StetError;
use crate::wordml::NodeId;
use crate::xml::{NodeRef, SpanTree};
use std::sync::Arc;

/// Which revision wraps the content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Rev {
    /// `w:ins`.
    Ins,
    /// `w:del`.
    Del,
}

impl Rev {
    /// The element's local name.
    pub(super) fn local(self) -> &'static str {
        match self {
            Rev::Ins => "ins",
            Rev::Del => "del",
        }
    }
}

/// The wrappers that hold runs and are entered, not wrapped.
fn is_run_holder(n: &NodeRef<'_>) -> bool {
    is_w(n, "hyperlink")
        || is_w(n, "smartTag")
        || is_w(n, "customXml")
        || is_w(n, "fldSimple")
        || is_w(n, "dir")
        || is_w(n, "bdo")
}

/// True for a run that holds only the `separate` of a complex field.
pub(super) fn is_separate_run(n: &NodeRef<'_>) -> bool {
    if !is_w(n, "r") {
        return false;
    }
    let mut content = n.child_elements().filter(|c| !is_w(c, "rPr"));
    match (content.next(), content.next()) {
        (Some(only), None) => {
            is_w(&only, "fldChar")
                && only
                    .attrs()
                    .any(|a| a.local_name() == "fldCharType" && a.value() == "separate")
        }
        _ => false,
    }
}

impl Compiler<'_, '_> {
    /// A new revision element with an id, our author and the date. It is detached.
    pub(super) fn fresh_wrapper(
        &mut self,
        part: &Arc<str>,
        local: &str,
    ) -> Result<NodeId, StetError> {
        let id = self.next_id()?;
        let stamp = self
            .stamp()
            .cloned()
            .ok_or_else(|| internal("a tracked wrapper has no stamp"))?;
        let uri = wns(self.tree_ro(part)?);
        let tree = self.tx.part_mut(part)?;
        let el = create_w(tree, &uri, local)?;
        write_revision_attrs(tree, el, &uri, id, &stamp)?;
        Ok(el)
    }

    /// Wraps the sibling `nodes` in one new revision wrapper, put where the first was. A deletion renames the text.
    pub(super) fn wrap_nodes(
        &mut self,
        part: &Arc<str>,
        nodes: &[NodeId],
        rev: Rev,
    ) -> Result<NodeId, StetError> {
        let Some(&first) = nodes.first() else {
            return Err(internal("a group of nodes to wrap is empty"));
        };
        let parent = parent_of(self.tree_ro(part)?, first)
            .ok_or_else(|| internal("a node to wrap has no parent"))?;
        let wrapper = self.fresh_wrapper(part, rev.local())?;
        let tree = self.tx.part_mut(part)?;
        insert(tree, parent, Some(first), wrapper)?;
        for &n in nodes {
            move_to(tree, n, wrapper, None)?;
        }
        if rev == Rev::Del {
            for &n in nodes {
                rename_deleted_deep(tree, n)?;
            }
        } else if rev == Rev::Ins {
            // text that a tool wrote without `xml:space` and that has white space at an end is text, as Word shows it, and the
            // insertion says `preserve` (W2), so that every reader keeps it
            for &n in nodes {
                preserve_inserted_deep(tree, n)?;
            }
        }
        Ok(wrapper)
    }

    /// Wraps every run of the content of `container` (a paragraph, or a hyperlink or a control inside it) in a revision.
    pub(super) fn wrap_content(
        &mut self,
        part: &Arc<str>,
        container: NodeId,
        rev: Rev,
    ) -> Result<(), StetError> {
        let mut work = vec![container];
        while let Some(c) = work.pop() {
            let (groups, deeper) = {
                let tree = self.tree_ro(part)?;
                content_groups(tree, c, rev)
            };
            work.extend(deeper);
            for g in groups {
                self.wrap_nodes(part, &g, rev)?;
            }
        }
        Ok(())
    }

    /// Marks a paragraph: its mark and all its content, or its content only.
    pub(super) fn mark_paragraph(
        &mut self,
        part: &Arc<str>,
        para: NodeId,
        rev: Rev,
        mark: bool,
    ) -> Result<(), StetError> {
        // spell-check marks are not content, and one left outside the wrapper would keep the paragraph from going when its
        // insertion is rejected
        let marks: Vec<NodeId> = {
            let tree = self.tree_ro(part)?;
            let mut out = Vec::new();
            let mut stack = vec![para];
            while let Some(n) = stack.pop() {
                for c in tree.node_ref(n).child_elements() {
                    if is_w(&c, "proofErr") {
                        out.push(c.id());
                    } else if !is_w(&c, "tbl") {
                        stack.push(c.id());
                    }
                }
            }
            out
        };
        for m in marks {
            super::tree_ops::detach(self.tx.part_mut(part)?, m)?;
        }
        if rev == Rev::Ins {
            self.strip_go_back(part, para)?;
        }
        self.wrap_content(part, para, rev)?;
        if mark {
            self.add_mark_revision(part, para, rev.local())?;
        }
        self.touch(&crate::wordml::BlockKey::new(Arc::clone(part), para));
        Ok(())
    }

    /// Word's own `_GoBack` bookmark (where the cursor was) is not content, and in an inserted paragraph it would stay when the
    /// insertion is rejected and keep the paragraph. Both of its ends are removed.
    fn strip_go_back(&mut self, part: &Arc<str>, para: NodeId) -> Result<(), StetError> {
        let doomed: Vec<NodeId> = {
            let tree = self.tree_ro(part)?;
            let mut ids = std::collections::HashSet::new();
            let mut all = Vec::new();
            let mut stack = vec![para];
            while let Some(n) = stack.pop() {
                for c in tree.node_ref(n).child_elements() {
                    if is_w(&c, "bookmarkStart") || is_w(&c, "bookmarkEnd") {
                        all.push(c.id());
                    } else if !is_w(&c, "tbl") {
                        stack.push(c.id());
                    }
                }
            }
            for &b in &all {
                let c = tree.node_ref(b);
                if is_w(&c, "bookmarkStart")
                    && c.attrs()
                        .any(|a| a.local_name() == "name" && a.value() == "_GoBack")
                    && let Some(id) = c
                        .attrs()
                        .find(|a| a.local_name() == "id")
                        .map(|a| a.value().into_owned())
                {
                    ids.insert(id);
                }
            }
            all.into_iter()
                .filter(|&b| {
                    tree.node_ref(b)
                        .attrs()
                        .find(|a| a.local_name() == "id")
                        .is_some_and(|a| ids.contains(a.value().as_ref()))
                })
                .collect()
        };
        for d in doomed {
            super::tree_ops::detach(self.tx.part_mut(part)?, d)?;
        }
        Ok(())
    }

    /// Adds our revision element to the row's `w:trPr`.
    pub(super) fn add_row_rev(
        &mut self,
        part: &Arc<str>,
        row: NodeId,
        rev: Rev,
    ) -> Result<(), StetError> {
        let el = self.fresh_wrapper(part, rev.local())?;
        let tree = self.tx.part_mut(part)?;
        let (trpr, _) = ensure_chain(tree, row, &["trPr"])?;
        place(tree, trpr, el)
    }

    /// Marks a row, the rows nested in it and every paragraph of its cells. `mark` is false for the paragraphs that keep
    /// their mark.
    pub(super) fn mark_row(
        &mut self,
        part: &Arc<str>,
        row: NodeId,
        rev: Rev,
    ) -> Result<(), StetError> {
        let (rows, paragraphs) = blocks_below(self.tree_ro(part)?, row);
        for r in rows {
            self.add_row_rev(part, r, rev)?;
            self.touch(&crate::wordml::BlockKey::new(Arc::clone(part), r));
        }
        for p in paragraphs {
            let sect = find_chain(self.tree_ro(part)?, p, &["pPr", "sectPr"]).is_some();
            self.mark_paragraph(part, p, rev, !(sect && rev == Rev::Ins))?;
        }
        Ok(())
    }
}

/// The rows (the node itself first, if it is one) and the paragraphs below a row or a table, in document order, entering
/// cells and nested tables and not the paragraphs themselves, whose text boxes are not blocks.
pub(super) fn blocks_below(tree: &SpanTree, node: NodeId) -> (Vec<NodeId>, Vec<NodeId>) {
    let (mut rows, mut paragraphs) = (Vec::new(), Vec::new());
    let mut stack = vec![node];
    while let Some(id) = stack.pop() {
        let n = tree.node_ref(id);
        if is_w(&n, "p") {
            paragraphs.push(id);
            continue;
        }
        if is_w(&n, "tr") {
            rows.push(id);
        }
        let mut kids: Vec<NodeId> = n.child_elements().map(|c| c.id()).collect();
        kids.reverse();
        stack.extend(kids);
    }
    (rows, paragraphs)
}

/// The `separate` runs of the paragraph that holds `node` whose field does not begin and end in that paragraph. The resolver
/// takes a field whole only when it lies in one paragraph, so an insertion wraps such a `separate` with the rest, or rejecting
/// it would leave the `separate` behind.
fn spanning_separates(tree: &SpanTree, node: NodeId) -> Vec<NodeId> {
    let mut para = Some(tree.node_ref(node));
    while let Some(p) = para {
        if is_w(&p, "p") {
            break;
        }
        para = p.parent();
    }
    let Some(para) = para else {
        return Vec::new();
    };
    // the separate run of each field that began in this paragraph and has not ended
    let mut open: Vec<Option<NodeId>> = Vec::new();
    let mut out = Vec::new();
    let mut stack = vec![para];
    let mut order: Vec<NodeRef<'_>> = Vec::new();
    while let Some(n) = stack.pop() {
        order.push(n);
        let mut kids: Vec<NodeRef<'_>> = n
            .child_elements()
            .filter(|c| !is_w(c, "txbxContent") && !is_w(c, "p"))
            .collect();
        kids.reverse();
        stack.extend(kids);
    }
    for n in order {
        if !is_w(&n, "fldChar") {
            continue;
        }
        let kind = n
            .attrs()
            .find(|a| a.local_name() == "fldCharType")
            .map(|a| a.value().into_owned())
            .unwrap_or_default();
        let run = n.parent().map(|r| r.id());
        match kind.as_str() {
            "begin" => open.push(None),
            "separate" => match open.last_mut() {
                Some(top) => *top = run,
                None => out.extend(run),
            },
            "end" => {
                open.pop();
            }
            _ => {}
        }
    }
    out.extend(open.into_iter().flatten());
    out
}

/// The groups of nodes to wrap in the content of `container`, and the containers inside it to look through.
fn content_groups(tree: &SpanTree, container: NodeId, rev: Rev) -> (Vec<Vec<NodeId>>, Vec<NodeId>) {
    let spanning = if rev == Rev::Ins {
        spanning_separates(tree, container)
    } else {
        Vec::new()
    };
    let mut groups: Vec<Vec<NodeId>> = Vec::new();
    let mut deeper: Vec<NodeId> = Vec::new();
    let mut current: Vec<NodeId> = Vec::new();
    let mut pending: Vec<NodeId> = Vec::new();
    // markers (a bookmark, a comment range) go into the wrapper with the runs around them, and a paragraph that holds markers
    // alone gets a wrapper of them: left outside, they would keep a paragraph whose insertion is rejected, or whose deletion is
    // accepted
    let close =
        |current: &mut Vec<NodeId>, pending: &mut Vec<NodeId>, groups: &mut Vec<Vec<NodeId>>| {
            // white space after the last piece stays where it is
            pending.clear();
            let holds = current.iter().any(|&id| tree.node_ref(id).is_element());
            if holds {
                groups.push(std::mem::take(current));
            } else {
                current.clear();
            }
        };
    for id in child_ids(tree, container) {
        let n = tree.node_ref(id);
        if !n.is_element() {
            if !current.is_empty() {
                pending.push(id);
            }
            continue;
        }
        if is_w(&n, "pPr") {
            continue;
        }
        if is_w(&n, "r") {
            if rev == Rev::Ins && is_separate_run(&n) && !spanning.contains(&id) {
                close(&mut current, &mut pending, &mut groups);
                continue;
            }
            current.append(&mut pending);
            current.push(id);
        } else if super::carve::is_transparent(&n) {
            current.append(&mut pending);
            current.push(id);
        } else if is_run_holder(&n) {
            close(&mut current, &mut pending, &mut groups);
            deeper.push(id);
        } else if is_w(&n, "sdt") {
            close(&mut current, &mut pending, &mut groups);
            if let Some(content) = n.child_elements().find(|c| is_w(c, "sdtContent")) {
                deeper.push(content.id());
            }
        } else {
            close(&mut current, &mut pending, &mut groups);
        }
    }
    close(&mut current, &mut pending, &mut groups);
    (groups, deeper)
}

/// Splits the nodes of a carved group into the stretches that one insertion may wrap: consecutive runs and the range markers
/// between them. A node that is neither (an equation, a field's `separate` run) ends a stretch and is left out, and a
/// stretch with no run is dropped.
pub(super) fn wrappable(tree: &SpanTree, nodes: &[NodeId]) -> Vec<Vec<NodeId>> {
    let mut out: Vec<Vec<NodeId>> = Vec::new();
    let mut current: Vec<NodeId> = Vec::new();
    let flush = |current: &mut Vec<NodeId>, out: &mut Vec<Vec<NodeId>>| {
        while current
            .last()
            .is_some_and(|&n| !is_w(&tree.node_ref(n), "r"))
        {
            current.pop();
        }
        if current.iter().any(|&n| is_w(&tree.node_ref(n), "r")) {
            out.push(std::mem::take(current));
        } else {
            current.clear();
        }
    };
    for &n in nodes {
        let r = tree.node_ref(n);
        let plain_run = is_w(&r, "r") && !is_separate_run(&r);
        if plain_run || (super::carve::is_transparent(&r) && !current.is_empty()) {
            current.push(n);
        } else {
            flush(&mut current, &mut out);
        }
    }
    flush(&mut current, &mut out);
    out
}

/// Renames the text of a deleted run to what a deletion holds: `w:t` becomes `w:delText` and `w:instrText` becomes
/// `w:delInstrText`, in the run and in what it holds (the base and the text of a ruby, the fallback of an alternate), but not in a text
/// box, whose paragraphs are blocks of their own and which a deleted run carries as it is.
fn rename_deleted_deep(tree: &mut SpanTree, node: NodeId) -> Result<(), StetError> {
    let mut targets: Vec<(NodeId, &'static str)> = Vec::new();
    let mut stack = vec![node];
    while let Some(id) = stack.pop() {
        for c in tree.node_ref(id).child_elements() {
            if super::blocks::is_text_box_content(&c) {
                continue;
            }
            if is_w(&c, "t") {
                targets.push((c.id(), "delText"));
            } else if is_w(&c, "instrText") {
                targets.push((c.id(), "delInstrText"));
            } else {
                stack.push(c.id());
            }
        }
    }
    for (el, name) in targets {
        let renamed = super::tree_ops::rename(tree, el, name)?;
        super::tree_ops::ensure_preserve(tree, renamed)?;
    }
    Ok(())
}

/// Gives `xml:space="preserve"` to the `w:t` of an inserted run that needs it, in the run and in what it holds, but not in a text box.
fn preserve_inserted_deep(tree: &mut SpanTree, node: NodeId) -> Result<(), StetError> {
    let mut targets: Vec<NodeId> = Vec::new();
    let mut stack = vec![node];
    while let Some(id) = stack.pop() {
        for c in tree.node_ref(id).child_elements() {
            if super::blocks::is_text_box_content(&c) {
                continue;
            }
            if is_w(&c, "t") {
                targets.push(c.id());
            } else {
                stack.push(c.id());
            }
        }
    }
    for el in targets {
        super::tree_ops::ensure_preserve(tree, el)?;
    }
    Ok(())
}
