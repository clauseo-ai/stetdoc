//! The properties of paired tables, rows and cells (decision 0022).
//!
//! Two paired tables compare their `w:tblPr`, two paired rows their `w:trPr`, and the cells of two paired rows, which have the
//! same shape by the pairing (design 4.5), compare their `w:tcPr` position by position. Properties are compared as `fmt` compares
//! a paragraph's, after the ignore list and with style ids as they are written, and a difference is a `PropsPlan`: `build` writes
//! a `w:tblPrChange`, `w:trPrChange` or `w:tcPrChange` that holds the original's properties. The grid (`w:tblGrid`) and the
//! row's table property exceptions (`w:tblPrEx`) are not compared.

use super::fmt::canon;
use super::model::RowInfo;
use super::tables::RowsPairing;
use crate::wordml::{NodeId, Props, Row, SpanTree};

/// Children of `tblPr`, `trPr` and `tcPr` that are revisions, and never a difference or a part of a snapshot.
pub(super) const NOT_TABLE: &[&str] = &[
    "tblPrChange",
    "trPrChange",
    "tcPrChange",
    "ins",
    "del",
    "cellIns",
    "cellDel",
    "cellMerge",
];

/// One table, row or cell whose properties differ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropsPlan {
    /// The revised block that is written to: the table, or the row (for a row and for one of its cells).
    pub block: NodeId,
    /// The original's `w:tbl`, `w:tr` or `w:tc`.
    pub a: NodeId,
    /// The revised `w:tbl`, `w:tr` or `w:tc`.
    pub b: NodeId,
    /// The container: `tblPr`, `trPr` or `tcPr`.
    pub container: &'static str,
}

/// Properties that say which parts of a table style apply, which Word rewrites as it pleases: not a difference, though a
/// snapshot keeps them.
pub(super) const TABLE_NOISE: &[&str] = &["tblLook"];

/// The canonical form of the properties container `name` of `owner`, after the ignore list, without revisions.
fn canon_of(tree: &SpanTree, owner: NodeId, name: &str) -> String {
    tree.node_ref(owner)
        .child_elements()
        .find(|c| c.is_element() && c.local_name() == name)
        .map(|pr| canon(&Props::read(pr).without(NOT_TABLE).without(TABLE_NOISE)))
        .unwrap_or_default()
}

/// The table, row and cell properties that differ between the paired tables `ta` and `tb`, whose rows are `ra` and `rb` and pair
/// as `rows` says.
pub(super) fn table_props(
    trees: (&SpanTree, &SpanTree),
    tables: (NodeId, NodeId),
    rows: (&[RowInfo], &[RowInfo]),
    pairing: &RowsPairing,
) -> Vec<PropsPlan> {
    let (a, b) = trees;
    let mut out = Vec::new();
    let (ta, tb) = tables;
    if canon_of(a, ta, "tblPr") != canon_of(b, tb, "tblPr") {
        out.push(PropsPlan {
            block: tb,
            a: ta,
            b: tb,
            container: "tblPr",
        });
    }
    for rp in &pairing.pairs {
        let (Some(xa), Some(xb)) = (rows.0.get(rp.a), rows.1.get(rp.b)) else {
            continue;
        };
        if canon_of(a, xa.id, "trPr") != canon_of(b, xb.id, "trPr") {
            out.push(PropsPlan {
                block: xb.id,
                a: xa.id,
                b: xb.id,
                container: "trPr",
            });
        }
        let (Some(row_a), Some(row_b)) = (Row::new(a, xa.id), Row::new(b, xb.id)) else {
            continue;
        };
        let cells_a: Vec<NodeId> = row_a.cells().map(|c| c.id()).collect();
        let cells_b: Vec<NodeId> = row_b.cells().map(|c| c.id()).collect();
        if cells_a.len() != cells_b.len() {
            continue;
        }
        for (&ca, &cb) in cells_a.iter().zip(&cells_b) {
            if canon_of(a, ca, "tcPr") != canon_of(b, cb, "tcPr") {
                out.push(PropsPlan {
                    block: xb.id,
                    a: ca,
                    b: cb,
                    container: "tcPr",
                });
            }
        }
    }
    out
}
