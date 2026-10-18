//! A paragraph: a cheap view over the tree, and the door to its text model.
//!
//! `Paragraph` holds a tree reference and a node. It reads what the paragraph says about itself
//! (style, numbering, section break, the revision on its mark) and builds its [`TextMap`] on demand.
//! A paragraph made by [`Story::paragraph`] knows the fields that are open where it starts; one made
//! by `Paragraph::new` (for text-box content and for tests) starts with none.

use super::control::{Control, Hyperlink, control, hyperlink};
use super::names::{CELL_CONTENT, ROW_CONTENT, int, is_w, leading_child, w_attr, w_child};
use super::props::{Props, PropsRef};
use super::revision::{Revision, branch_of, revision_at};
use super::story::{BlockKind, MarkRevs, Story};
use super::story_scan::paragraph_flags;
use super::text_build::{self, PROPERTIES_LIMIT};
use super::text_fields::{FieldIndex, ParaState};
use super::text_model::{Context, Item, ItemKind, TextMap, WrapperKind};
use crate::api::view::Mode;
use crate::xml::{NodeId, NodeRef, SpanTree};
use std::borrow::Cow;

/// The element `name` among the properties `props` of a row or a cell, where Word keeps them: before
/// the content, as the text model reads them.
fn property_in<'a>(
    holder: &NodeRef<'a>,
    props: &str,
    content: &[&str],
    name: &str,
) -> Option<NodeRef<'a>> {
    leading_child(holder, props, content)?
        .child_elements()
        .take(PROPERTIES_LIMIT)
        .find(|c| is_w(c, name))
}

/// The direct `w:numPr` of a paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumPr {
    /// The `w:numId`, if there is one. Zero means the numbering is removed.
    pub num_id: Option<i64>,
    /// The `w:ilvl`, if there is one.
    pub ilvl: Option<u8>,
}

/// A `w:p`.
#[derive(Clone, Copy)]
pub struct Paragraph<'a> {
    tree: &'a SpanTree,
    node: NodeRef<'a>,
    start: Option<&'a ParaState>,
}

impl<'a> Paragraph<'a> {
    /// The paragraph whose element is `node`, starting with no open field. `None` if the node is not a
    /// `w:p`.
    pub fn new(tree: &'a SpanTree, node: NodeId) -> Option<Paragraph<'a>> {
        let node = tree.node_ref(node);
        is_w(&node, "p").then_some(Paragraph {
            tree,
            node,
            start: None,
        })
    }

    /// The element.
    pub fn element(&self) -> NodeRef<'a> {
        self.node
    }

    /// The element's id.
    pub fn id(&self) -> NodeId {
        self.node.id()
    }

    /// A paragraph over the element `node` of `tree`, which the caller has found to be a `w:p`,
    /// starting with no open field. The block views build the paragraphs of a cell here.
    pub(super) fn from_element(tree: &'a SpanTree, node: NodeRef<'a>) -> Paragraph<'a> {
        Paragraph {
            tree,
            node,
            start: None,
        }
    }

    /// The paragraph's direct properties, `w:pPr`, read in place. `None` if it has none.
    pub fn ppr(&self) -> Option<PropsRef<'a>> {
        self.ppr_node().map(PropsRef::new)
    }

    fn ppr_node(&self) -> Option<NodeRef<'a>> {
        self.node.child_elements().next().filter(|e| is_w(e, "pPr"))
    }

    /// The id of the paragraph style, `w:pPr/w:pStyle/@w:val`.
    pub fn style_id(&self) -> Option<Cow<'a, str>> {
        super::num_facts::style_in(&self.ppr_node()?)
    }

    /// The numbering the paragraph sets itself. A style's numbering is `numbering`'s business.
    pub fn num_pr(&self) -> Option<NumPr> {
        super::num_facts::num_pr_in(&self.ppr_node()?)
    }

    /// The paragraph's own formatting: the children of its `w:pPr` without the properties of the
    /// paragraph mark (`w:rPr`), a section break (`w:sectPr`) and the old properties of a tracked
    /// change (`w:pPrChange`). Two paragraphs with equal base properties are formatted alike, so a
    /// merge or an insertion can tell whether they differ. A paragraph with no `w:pPr` has none.
    pub fn base_props(&self) -> Props {
        self.ppr_node()
            .map_or_else(Props::new, Props::read)
            .without(&["rPr", "sectPr", "pPrChange"])
    }

    /// The properties of the paragraph mark, `w:pPr/w:rPr`.
    pub fn mark_props(&self) -> Option<PropsRef<'a>> {
        self.ppr_node()
            .and_then(|ppr| w_child(&ppr, "rPr"))
            .map(PropsRef::new)
    }

    /// The run of an item of this paragraph's map: the `w:r` that holds it.
    fn run_of(&self, item: &Item) -> Option<NodeRef<'a>> {
        item.run.map(|id| self.tree.node_ref(id))
    }

    /// The character properties, `w:rPr`, of the run that holds an item of this paragraph's map.
    /// `None` for an item with no run, such as a mark made from a simple field, and for a run with no
    /// properties.
    pub fn run_props(&self, item: &Item) -> Option<PropsRef<'a>> {
        self.run_of(item)
            .and_then(|r| w_child(&r, "rPr"))
            .map(PropsRef::new)
    }

    /// The character style of the run that holds an item, `w:rPr/w:rStyle/@w:val`.
    pub fn run_style(&self, item: &Item) -> Option<Cow<'a, str>> {
        self.run_props(item).and_then(|p| p.val("rStyle"))
    }

    /// The `w:sectPr` of a section break at this paragraph.
    pub fn section_break(&self) -> Option<NodeRef<'a>> {
        w_child(&self.ppr_node()?, "sectPr")
    }

    /// The tracked changes on the paragraph mark, all of them: a mark that one author inserted and
    /// another deleted holds both. They are the same bits the story scan keeps in
    /// [`BlockEntry::flags`](super::story::BlockEntry::flags).
    pub fn mark_revs(&self) -> MarkRevs {
        MarkRevs::from_flags(paragraph_flags(&self.node))
    }

    /// True if, in `mode`, this paragraph's mark is gone and its text joins the next paragraph: the
    /// mark is deleted or moved away in `Final`, inserted or moved in in `Original`, and a mark that
    /// is both gone in both. Whether the next block is a paragraph that can take it is the caller's to
    /// check (`Story::join_run`).
    pub fn joins_next(&self, mode: Mode) -> bool {
        self.mark_revs().gone_in(mode)
    }

    /// The `w:id` of the marker an item stands for: a comment range start, end or reference, a note
    /// reference, a bookmark or a move range. For the automatic number inside a note
    /// (`ItemKind::NoteMark`), which has no id of its own, it is the id of the note it is in. `None` for
    /// any other item, and where the id is not a whole number.
    pub fn mark_id(&self, item: &Item) -> Option<i64> {
        let node = self.tree.node_ref(item.node);
        match item.kind {
            ItemKind::CommentStart
            | ItemKind::CommentEnd
            | ItemKind::CommentRef
            | ItemKind::NoteRef
            | ItemKind::BookmarkStart
            | ItemKind::BookmarkEnd
            | ItemKind::MoveRangeStart
            | ItemKind::MoveRangeEnd => w_attr(&node, "id").and_then(|v| int(&v)),
            ItemKind::NoteMark => {
                let mut cur = node.parent();
                while let Some(a) = cur {
                    if is_w(&a, "footnote") || is_w(&a, "endnote") {
                        return w_attr(&a, "id").and_then(|v| int(&v));
                    }
                    cur = a.parent();
                }
                None
            }
            _ => None,
        }
    }

    /// The revision a wrapper of this paragraph's map is: a `w:ins`, `w:del`, `w:moveFrom` or
    /// `w:moveTo` around content, or the revision in the properties of an inserted or deleted row or
    /// cell. A row or a cell marked as both has two wrappers, and each gives its own revision. `None`
    /// for any other wrapper.
    pub fn revision_of(&self, ctx: &Context) -> Option<Revision> {
        let node = self.tree.node_ref(ctx.node);
        let element = match ctx.kind {
            WrapperKind::Ins | WrapperKind::Del | WrapperKind::MoveFrom | WrapperKind::MoveTo => {
                node
            }
            WrapperKind::RowIns => property_in(&node, "trPr", &ROW_CONTENT, "ins")?,
            WrapperKind::RowDel => property_in(&node, "trPr", &ROW_CONTENT, "del")?,
            WrapperKind::CellIns => property_in(&node, "tcPr", &CELL_CONTENT, "cellIns")?,
            WrapperKind::CellDel => property_in(&node, "tcPr", &CELL_CONTENT, "cellDel")?,
            _ => return None,
        };
        let mut revision = revision_at(&element)?;
        revision.branch = branch_of(&element);
        Some(revision)
    }

    /// The properties of the content control a wrapper of this paragraph's map is. `None` for any
    /// other wrapper.
    pub fn control_of(&self, ctx: &Context) -> Option<Control> {
        (ctx.kind == WrapperKind::Control).then(|| control(self.tree.node_ref(ctx.node)))
    }

    /// The target of the hyperlink a wrapper of this paragraph's map is. `None` for any other
    /// wrapper.
    pub fn hyperlink_of(&self, ctx: &Context) -> Option<Hyperlink> {
        (ctx.kind == WrapperKind::Hyperlink).then(|| hyperlink(self.tree.node_ref(ctx.node)))
    }

    /// The text model of the paragraph: its items, the text of every mode, and the map to XML.
    pub fn map(&self) -> TextMap {
        let (carry, base) = match self.start {
            Some(s) => (s.start.as_slice(), s.base),
            None => (&[][..], 0),
        };
        text_build::build(&self.node, carry, base)
    }

    /// The paragraph's text in a mode.
    pub fn text(&self, mode: Mode) -> String {
        self.map().text(mode).into_owned()
    }
}

impl Story {
    /// The paragraph at entry `i`, with the fields that are open where it starts. The first call runs
    /// the field pass, once for the story. `None` if entry `i` is not a paragraph, or if `tree` was
    /// edited since the story was scanned ([`Story::is_current`]).
    pub fn paragraph<'a>(&'a self, tree: &'a SpanTree, i: usize) -> Option<Paragraph<'a>> {
        if !self.is_current(tree) {
            return None;
        }
        let entry = self
            .entries
            .get(i)
            .filter(|e| e.kind == BlockKind::Paragraph)?;
        let node = tree.node_ref(entry.node);
        Some(Paragraph {
            tree,
            node,
            start: self.field_index(tree).paras.get(&entry.node),
        })
    }

    /// The fields of the story, in document order of their start. The first call runs the field pass,
    /// once for the story. Empty if `tree` was edited since the story was scanned
    /// ([`Story::is_current`]).
    pub fn fields(&self, tree: &SpanTree) -> &[super::text_fields::FieldInfo] {
        if !self.is_current(tree) {
            return &[];
        }
        &self.field_index(tree).fields
    }

    /// Fields that were still open where their note, comment or story ended. The first call runs the
    /// field pass, once for the story. Empty if `tree` was edited since the story was scanned
    /// ([`Story::is_current`]).
    pub fn field_anomalies(&self, tree: &SpanTree) -> &[super::anomaly::Anomaly] {
        if !self.is_current(tree) {
            return &[];
        }
        &self.field_index(tree).anomalies
    }

    /// The field index, built on first use and kept: valid for the tree the story was scanned from.
    pub(super) fn field_index(&self, tree: &SpanTree) -> &FieldIndex {
        self.fields
            .get_or_init(|| std::sync::Arc::new(FieldIndex::build(self, tree, None)))
    }
}
