//! Views over blocks: `Block`, `Table`, `Row` and `Cell`.
//!
//! Each is one `NodeRef` (a tree reference and an id, `Copy`) and holds nothing else, so building one
//! costs nothing and a method reads the tree when it is called. They are for the callers that need to
//! look inside one block; the flat `Story::blocks` list is for enumeration. A table's rows, a row's
//! cells and a cell's blocks are found the way the block scan finds them (`docs/design/wordml.md`
//! 3.2 and 3.6): through `w:sdt`, `w:customXml`, `w:smartTag`, the revision wrappers and the taken
//! branch of an `mc:AlternateContent`, with an explicit stack and no recursion.

use super::mc;
use super::names::{
    CELL_CONTENT, ROW_CONTENT, TABLE_CONTENT, int, is_w, is_word_ns, leading_child, w_attr, w_child,
};
use super::paragraph::Paragraph;
use super::props::PropsRef;
use super::revision::{RevKind, Revision, revision_at};
use super::story::{BlockKind, MarkRevs, Story};
use crate::xml::{Children, NodeId, NodeRef, SpanTree};

/// A block of a story: a paragraph, a table or a row. A cell is not a block.
#[derive(Clone, Copy)]
pub enum Block<'a> {
    /// A `w:p`.
    Paragraph(Paragraph<'a>),
    /// A `w:tbl`.
    Table(Table<'a>),
    /// A `w:tr`.
    Row(Row<'a>),
}

impl<'a> Block<'a> {
    /// The element.
    pub fn element(&self) -> NodeRef<'a> {
        match self {
            Block::Paragraph(p) => p.element(),
            Block::Table(t) => t.element(),
            Block::Row(r) => r.element(),
        }
    }

    /// The element's id.
    pub fn id(&self) -> NodeId {
        self.element().id()
    }

    /// What kind of block it is.
    pub fn kind(&self) -> BlockKind {
        match self {
            Block::Paragraph(_) => BlockKind::Paragraph,
            Block::Table(_) => BlockKind::Table,
            Block::Row(_) => BlockKind::Row,
        }
    }

    /// The paragraphs and tables inside `container`, in document order, through the transparent
    /// wrappers of the block scan. `container` is a `w:tc`, a `w:txbxContent`, a `w:body`, a header
    /// or a note: any element whose children are blocks. The paragraphs start with no open field.
    pub fn within(tree: &'a SpanTree, container: NodeRef<'a>) -> impl Iterator<Item = Block<'a>> {
        Walk::new(container, Want::Blocks).filter_map(move |n| {
            if is_w(&n, "p") {
                Some(Block::Paragraph(Paragraph::from_element(tree, n)))
            } else if is_w(&n, "tbl") {
                Some(Block::Table(Table { tree, node: n }))
            } else {
                None
            }
        })
    }
}

/// What a walk through transparent wrappers is looking for.
#[derive(Clone, Copy)]
enum Want {
    /// `w:tr`, in a table.
    Rows,
    /// `w:tc`, in a row.
    Cells,
    /// `w:p` and `w:tbl`, in a cell or any other container of blocks.
    Blocks,
}

impl Want {
    fn is_wanted(self, n: &NodeRef<'_>) -> bool {
        match self {
            Want::Rows => is_w(n, "tr"),
            Want::Cells => is_w(n, "tc"),
            Want::Blocks => is_w(n, "p") || is_w(n, "tbl"),
        }
    }
}

/// The elements a container holds that are what is wanted, found through the transparent wrappers.
struct Walk<'a> {
    stack: Vec<Children<'a>>,
    want: Want,
}

impl<'a> Walk<'a> {
    fn new(container: NodeRef<'a>, want: Want) -> Walk<'a> {
        Walk {
            stack: vec![container.children()],
            want,
        }
    }
}

impl<'a> Iterator for Walk<'a> {
    type Item = NodeRef<'a>;

    fn next(&mut self) -> Option<NodeRef<'a>> {
        while let Some(top) = self.stack.last_mut() {
            let Some(c) = top.next() else {
                self.stack.pop();
                continue;
            };
            if !c.is_element() {
                continue;
            }
            if self.want.is_wanted(&c) {
                return Some(c);
            }
            if is_word_ns(c.namespace()) {
                match c.local_name() {
                    "sdt" => {
                        if let Some(content) = w_child(&c, "sdtContent") {
                            self.stack.push(content.children());
                        }
                    }
                    "customXml" | "smartTag" | "ins" | "del" | "moveFrom" | "moveTo" => {
                        self.stack.push(c.children());
                    }
                    _ => {}
                }
            } else {
                match mc::disposition(&c) {
                    mc::Mc::Branch(branch) => self.stack.push(branch.children()),
                    mc::Mc::ProcessContent => self.stack.push(c.children()),
                    _ => {}
                }
            }
        }
        None
    }
}

/// A `w:tbl`.
#[derive(Clone, Copy)]
pub struct Table<'a> {
    tree: &'a SpanTree,
    node: NodeRef<'a>,
}

impl<'a> Table<'a> {
    /// The table whose element is `node`. `None` if the node is not a `w:tbl`.
    pub fn new(tree: &'a SpanTree, node: NodeId) -> Option<Table<'a>> {
        let node = tree.node_ref(node);
        is_w(&node, "tbl").then_some(Table { tree, node })
    }

    /// The element.
    pub fn element(&self) -> NodeRef<'a> {
        self.node
    }

    /// The element's id.
    pub fn id(&self) -> NodeId {
        self.node.id()
    }

    /// The rows: the `w:tr` elements in the table, through transparent wrappers.
    pub fn rows(&self) -> impl Iterator<Item = Row<'a>> + use<'a> {
        let tree = self.tree;
        Walk::new(self.node, Want::Rows).map(move |node| Row { tree, node })
    }

    /// The table properties, `w:tblPr`, read in place. `None` if the table has none.
    pub fn props(&self) -> Option<PropsRef<'a>> {
        leading_child(&self.node, "tblPr", &TABLE_CONTENT).map(PropsRef::new)
    }

    /// The `w:tblGrid` element, if there is one.
    pub fn grid_element(&self) -> Option<NodeRef<'a>> {
        leading_child(&self.node, "tblGrid", &TABLE_CONTENT)
    }

    /// The grid: the width in twentieths of a point of each `w:gridCol`, in order. A column with a
    /// missing or invalid `w:w` is `None`, so the number of columns is the count of the items.
    pub fn grid(&self) -> impl Iterator<Item = Option<i64>> + use<'a> {
        self.grid_element()
            .into_iter()
            .flat_map(|g| g.children())
            .filter(|c| is_w(c, "gridCol"))
            .map(|c| w_attr(&c, "w").and_then(|v| int(&v)))
    }
}

/// A `w:tr`.
#[derive(Clone, Copy)]
pub struct Row<'a> {
    tree: &'a SpanTree,
    node: NodeRef<'a>,
}

impl<'a> Row<'a> {
    /// The row whose element is `node`. `None` if the node is not a `w:tr`.
    pub fn new(tree: &'a SpanTree, node: NodeId) -> Option<Row<'a>> {
        let node = tree.node_ref(node);
        is_w(&node, "tr").then_some(Row { tree, node })
    }

    /// The element.
    pub fn element(&self) -> NodeRef<'a> {
        self.node
    }

    /// The element's id.
    pub fn id(&self) -> NodeId {
        self.node.id()
    }

    /// The cells: the `w:tc` elements in the row, through transparent wrappers.
    pub fn cells(&self) -> impl Iterator<Item = Cell<'a>> + use<'a> {
        let tree = self.tree;
        Walk::new(self.node, Want::Cells).map(move |node| Cell { tree, node })
    }

    /// The row properties, `w:trPr`, read in place. `None` if the row has none.
    pub fn props(&self) -> Option<PropsRef<'a>> {
        leading_child(&self.node, "trPr", &ROW_CONTENT).map(PropsRef::new)
    }

    /// The table property exceptions of the row, `w:tblPrEx`, read in place.
    pub fn table_prop_ex(&self) -> Option<PropsRef<'a>> {
        leading_child(&self.node, "tblPrEx", &ROW_CONTENT).map(PropsRef::new)
    }

    /// The revisions that are the row itself, in order: a tracked row insertion, `w:trPr/w:ins`, and a
    /// deletion, `w:trPr/w:del`. A row can hold both: one author inserted it and another deleted it, and
    /// it is gone in both modes. Empty for any other row.
    pub fn revisions(&self) -> impl Iterator<Item = Revision> + use<'a> {
        self.props()
            .into_iter()
            .flat_map(PropsRef::iter)
            .filter_map(|c| revision_at(&c))
            .filter(|r| matches!(r.kind, RevKind::RowIns | RevKind::RowDel))
    }

    /// The revisions of [`Row::revisions`] as [`MarkRevs`], the way a paragraph mark carries them:
    /// `inserted` for a `RowIns`, `deleted` for a `RowDel`. Read it through [`MarkRevs::gone_in`].
    pub fn revs(&self) -> MarkRevs {
        let mut revs = MarkRevs::default();
        for r in self.revisions() {
            match r.kind {
                RevKind::RowIns => revs.inserted = true,
                RevKind::RowDel => revs.deleted = true,
                _ => {}
            }
        }
        revs
    }
}

/// The vertical merge state of a cell, `w:tcPr/w:vMerge`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VMerge {
    /// No `w:vMerge`: the cell is not part of a vertical merge.
    None,
    /// `w:vMerge w:val="restart"`: the cell starts a merged region.
    Restart,
    /// `w:vMerge` with no value or `w:val="continue"`: the cell continues the region above it.
    Continue,
}

/// A `w:tc`.
#[derive(Clone, Copy)]
pub struct Cell<'a> {
    tree: &'a SpanTree,
    node: NodeRef<'a>,
}

impl<'a> Cell<'a> {
    /// The cell whose element is `node`. `None` if the node is not a `w:tc`.
    pub fn new(tree: &'a SpanTree, node: NodeId) -> Option<Cell<'a>> {
        let node = tree.node_ref(node);
        is_w(&node, "tc").then_some(Cell { tree, node })
    }

    /// The element.
    pub fn element(&self) -> NodeRef<'a> {
        self.node
    }

    /// The element's id.
    pub fn id(&self) -> NodeId {
        self.node.id()
    }

    /// The paragraphs and tables of the cell, through transparent wrappers. The paragraphs start with
    /// no open field: a paragraph that needs the fields open where it starts comes from
    /// [`Story::paragraph`].
    pub fn blocks(&self) -> impl Iterator<Item = Block<'a>> + use<'a> {
        Block::within(self.tree, self.node)
    }

    /// The cell properties, `w:tcPr`, read in place. `None` if the cell has none.
    pub fn props(&self) -> Option<PropsRef<'a>> {
        leading_child(&self.node, "tcPr", &CELL_CONTENT).map(PropsRef::new)
    }

    /// How many grid columns the cell spans, `w:gridSpan`. One if it is missing or not a whole
    /// number of at least one.
    pub fn grid_span(&self) -> u32 {
        self.props()
            .and_then(|p| p.val("gridSpan"))
            .and_then(|v| int(&v))
            .and_then(|n| u32::try_from(n).ok())
            .filter(|&n| n >= 1)
            .unwrap_or(1)
    }

    /// The vertical merge state, `w:vMerge`.
    pub fn v_merge(&self) -> VMerge {
        let Some(m) = self.props().and_then(|p| p.get("vMerge")) else {
            return VMerge::None;
        };
        match w_attr(&m, "val").as_deref() {
            Some("restart") => VMerge::Restart,
            _ => VMerge::Continue,
        }
    }

    /// The revisions that are the cell itself: `w:cellIns`, `w:cellDel` and `w:cellMerge` in its
    /// `w:tcPr`, in order. A change of the cell's properties (`w:tcPrChange`) is not among them.
    pub fn revisions(&self) -> impl Iterator<Item = Revision> + use<'a> {
        self.props()
            .into_iter()
            .flat_map(PropsRef::iter)
            .filter_map(|c| revision_at(&c))
            .filter(|r| {
                matches!(
                    r.kind,
                    RevKind::CellIns | RevKind::CellDel | RevKind::CellMerge
                )
            })
    }
}

impl Story {
    /// The block at entry `i` as a view: a paragraph (with the fields that are open where it starts,
    /// as [`Story::paragraph`] gives it), a table or a row. `None` if there is no such entry, or if
    /// `tree` was edited since the story was scanned ([`Story::is_current`]).
    pub fn block<'a>(&'a self, tree: &'a SpanTree, i: usize) -> Option<Block<'a>> {
        if !self.is_current(tree) {
            return None;
        }
        let entry = self.entries.get(i)?;
        match entry.kind {
            BlockKind::Paragraph => self.paragraph(tree, i).map(Block::Paragraph),
            BlockKind::Table => Some(Block::Table(Table {
                tree,
                node: tree.node_ref(entry.node),
            })),
            BlockKind::Row => Some(Block::Row(Row {
                tree,
                node: tree.node_ref(entry.node),
            })),
        }
    }
}
