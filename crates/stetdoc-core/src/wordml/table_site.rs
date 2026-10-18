//! The table context of a paragraph: `Story::table_site`.
//!
//! `styles` and `numbering` may not read XML, and a table style applies to a paragraph by where its
//! cell sits: the first row, the last column, a band. `table_site` reads that position for one
//! paragraph, from the innermost table around it. Word does not inherit a table style into a table
//! nested in one of its cells, so the outer tables are not looked at (`docs/design/wordml.md` 3.6).

use super::block::{Cell, Row, Table, VMerge};
use super::names::{int, is_w, on_off, w_attr, w_child};
use super::props::PropsRef;
use super::story::{BlockKind, Story};
use crate::xml::{NodeId, NodeRef, SpanTree};
use std::collections::HashMap;

/// The `w:tblLook` flags of a table: which conditional parts of its style apply.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TblLook {
    /// The first row has its own formatting.
    pub first_row: bool,
    /// The last row has its own formatting.
    pub last_row: bool,
    /// The first column has its own formatting.
    pub first_column: bool,
    /// The last column has its own formatting.
    pub last_column: bool,
    /// Horizontal banding is off.
    pub no_h_band: bool,
    /// Vertical banding is off.
    pub no_v_band: bool,
}

const FIRST_ROW: u32 = 0x0020;
const LAST_ROW: u32 = 0x0040;
const FIRST_COLUMN: u32 = 0x0080;
const LAST_COLUMN: u32 = 0x0100;
const NO_H_BAND: u32 = 0x0200;
const NO_V_BAND: u32 = 0x0400;

impl TblLook {
    /// Reads a `w:tblLook`. If any of its six attributes is written, they are used, each read as an
    /// on/off value and an absent one taken as off. Otherwise `w:val` is read as hexadecimal flags,
    /// and a missing or invalid one gives all off.
    pub fn read(look: &NodeRef<'_>) -> TblLook {
        let flag = |name: &str| w_attr(look, name);
        let names = [
            "firstRow",
            "lastRow",
            "firstColumn",
            "lastColumn",
            "noHBand",
            "noVBand",
        ];
        if names.iter().any(|n| flag(n).is_some()) {
            let on = |n: &str| flag(n).is_some_and(|v| on_off(Some(&v)));
            return TblLook {
                first_row: on("firstRow"),
                last_row: on("lastRow"),
                first_column: on("firstColumn"),
                last_column: on("lastColumn"),
                no_h_band: on("noHBand"),
                no_v_band: on("noVBand"),
            };
        }
        let bits = w_attr(look, "val")
            .and_then(|v| u32::from_str_radix(v.trim(), 16).ok())
            .unwrap_or(0);
        let has = |bit: u32| bits & bit != 0;
        TblLook {
            first_row: has(FIRST_ROW),
            last_row: has(LAST_ROW),
            first_column: has(FIRST_COLUMN),
            last_column: has(LAST_COLUMN),
            no_h_band: has(NO_H_BAND),
            no_v_band: has(NO_V_BAND),
        }
    }
}

/// Where a paragraph sits in the innermost table around it, and what that table says about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableSite {
    /// The table style, `w:tblPr/w:tblStyle`.
    pub style_id: Option<Box<str>>,
    /// The `w:tblLook` flags. All off if the table has none.
    pub look: TblLook,
    /// The index of the paragraph's row among the table's rows, from 0, counting every row.
    pub row: usize,
    /// How many rows the table has.
    pub row_count: usize,
    /// The grid column where the paragraph's cell starts: `w:gridBefore` of the row plus the spans
    /// of the cells before it.
    pub column: usize,
    /// The number of grid columns: the `w:gridCol` count, or, with no grid, the widest row.
    pub column_count: usize,
    /// The cell's `w:gridSpan`.
    pub span: u32,
    /// The cell's vertical merge state.
    pub v_merge: VMerge,
    /// The `w:val` of the row's `w:trPr/w:cnfStyle`, as written.
    pub row_cnf: Option<Box<str>>,
    /// The `w:val` of the cell's `w:tcPr/w:cnfStyle`, as written.
    pub cell_cnf: Option<Box<str>>,
}

/// A whole number of grid columns written in a property (`gridBefore`, `gridAfter`), zero if it is
/// missing, negative or not a number.
fn grid_count(props: Option<PropsRef<'_>>, name: &str) -> usize {
    props
        .and_then(|p| p.val(name))
        .and_then(|v| int(&v))
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0)
}

/// The `w:val` of the `w:cnfStyle` in a property container.
fn cnf(props: Option<PropsRef<'_>>) -> Option<Box<str>> {
    let e = props?.get("cnfStyle")?;
    w_attr(&e, "val").map(|v| Box::from(v.as_ref()))
}

/// The columns a row takes in the grid: the cells before the first, the spans, and the cells after
/// the last.
fn row_width(row: &Row<'_>) -> usize {
    let props = row.props();
    let spans = row.cells().fold(0usize, |sum, c| {
        sum.saturating_add(usize::try_from(c.grid_span()).unwrap_or(usize::MAX))
    });
    grid_count(props, "gridBefore")
        .saturating_add(spans)
        .saturating_add(grid_count(props, "gridAfter"))
}

/// The nearest `w:tc` above `node` that is below `limit`, found by walking up.
fn cell_above<'a>(tree: &'a SpanTree, node: NodeRef<'a>, limit: NodeRef<'a>) -> Option<Cell<'a>> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.id() == limit.id() {
            return None;
        }
        if is_w(&n, "tc") {
            return Cell::new(tree, n.id());
        }
        cur = n.parent();
    }
    None
}

/// What a table says about itself and its rows, read once and shared by the paragraphs in it.
struct TableFacts {
    style_id: Option<Box<str>>,
    look: TblLook,
    /// The index of each row among the table's rows.
    rows: HashMap<NodeId, usize>,
    row_count: usize,
    column_count: usize,
}

impl TableFacts {
    fn read(table: &Table<'_>) -> TableFacts {
        let tbl_pr = table.props();
        let style_id = tbl_pr
            .and_then(|p| p.val("tblStyle"))
            .map(|v| Box::from(v.as_ref()));
        let look = tbl_pr
            .and_then(|p| p.get("tblLook"))
            .map(|l| TblLook::read(&l))
            .unwrap_or_default();
        let grid = table.grid().count();
        let mut rows = HashMap::new();
        let mut widest = 0usize;
        for (index, r) in table.rows().enumerate() {
            rows.insert(r.id(), index);
            if grid == 0 {
                widest = widest.max(row_width(&r));
            }
        }
        TableFacts {
            style_id,
            look,
            row_count: rows.len(),
            rows,
            column_count: if grid > 0 { grid } else { widest },
        }
    }
}

/// What a row says about itself and where each of its cells starts, read once and shared by the
/// paragraphs in it, so a row of many cells is read once and not once for each cell.
struct RowFacts {
    /// The grid column at which each cell of the row starts.
    start: HashMap<NodeId, usize>,
    cnf: Option<Box<str>>,
}

impl RowFacts {
    fn read(row: &Row<'_>) -> RowFacts {
        let props = row.props();
        let mut column = grid_count(props, "gridBefore");
        let mut start = HashMap::new();
        for c in row.cells() {
            start.insert(c.id(), column);
            column = column.saturating_add(usize::try_from(c.grid_span()).unwrap_or(usize::MAX));
        }
        RowFacts {
            start,
            cnf: cnf(props),
        }
    }
}

impl Story {
    /// The table context of the paragraph at entry `i`: the innermost table around it, the row and
    /// the cell. `None` if entry `i` is not a paragraph or is not in a cell of a table. The outer
    /// tables of a nested table are not read, because a table style is not inherited into one. Each
    /// call reads the rows of the table and the cells of the row, so a caller that wants every
    /// paragraph uses [`Story::table_sites`].
    pub fn table_site(&self, tree: &SpanTree, i: usize) -> Option<TableSite> {
        let table_entry = self.table_entry_of(i)?;
        let facts = TableFacts::read(&Table::new(tree, self.entries.get(table_entry)?.node)?);
        let row_entry = self
            .entries
            .get(usize::try_from(self.entries.get(i)?.parent?).ok()?)?;
        let row = RowFacts::read(&Row::new(tree, row_entry.node)?);
        self.site_in(tree, i, &facts, &row)
    }

    /// The table context of every entry, in entry order: [`Story::table_site`] of each, with the work
    /// on each table and each row done once however many paragraphs it holds.
    pub fn table_sites(&self, tree: &SpanTree) -> Vec<Option<TableSite>> {
        let mut tables: HashMap<usize, Option<TableFacts>> = HashMap::new();
        let mut rows: HashMap<usize, Option<RowFacts>> = HashMap::new();
        (0..self.entries.len())
            .map(|i| {
                let t = self.table_entry_of(i)?;
                let facts = tables.entry(t).or_insert_with(|| {
                    let node = self.entries.get(t)?.node;
                    Table::new(tree, node).map(|table| TableFacts::read(&table))
                });
                let r = usize::try_from(self.entries.get(i)?.parent?).ok()?;
                let row = rows.entry(r).or_insert_with(|| {
                    let node = self.entries.get(r)?.node;
                    Row::new(tree, node).map(|row| RowFacts::read(&row))
                });
                self.site_in(tree, i, facts.as_ref()?, row.as_ref()?)
            })
            .collect()
    }

    /// The entry of the table that holds the paragraph at entry `i` through a row, if it is in one.
    fn table_entry_of(&self, i: usize) -> Option<usize> {
        let entry = self
            .entries
            .get(i)
            .filter(|e| e.kind == BlockKind::Paragraph)?;
        let row_entry = self
            .entries
            .get(usize::try_from(entry.parent?).ok()?)
            .filter(|e| e.kind == BlockKind::Row)?;
        let table = usize::try_from(row_entry.parent?).ok()?;
        self.entries
            .get(table)
            .filter(|e| e.kind == BlockKind::Table)
            .map(|_| table)
    }

    /// The site of the paragraph at entry `i`, given what its table and its row say about
    /// themselves.
    fn site_in(
        &self,
        tree: &SpanTree,
        i: usize,
        facts: &TableFacts,
        row: &RowFacts,
    ) -> Option<TableSite> {
        let entry = self.entries.get(i)?;
        let row_entry = self.entries.get(usize::try_from(entry.parent?).ok()?)?;
        let row_node = tree.node_ref(row_entry.node);
        let cell = cell_above(tree, tree.node_ref(entry.node), row_node)?;
        Some(TableSite {
            style_id: facts.style_id.clone(),
            look: facts.look,
            row: *facts.rows.get(&row_node.id())?,
            row_count: facts.row_count,
            column: *row.start.get(&cell.id())?,
            column_count: facts.column_count,
            span: cell.grid_span(),
            v_merge: cell.v_merge(),
            row_cnf: row.cnf.clone(),
            cell_cnf: cnf(cell.props()),
        })
    }
}
