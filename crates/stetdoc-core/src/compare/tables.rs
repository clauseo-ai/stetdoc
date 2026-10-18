//! Pairing the blocks of two containers, and, through paired tables, their rows and cells (design `compare.md`, 4.5).
//!
//! A container is aligned by `align`. A pair of tables is entered: their rows are aligned as blocks (a row's key is its
//! cells' texts joined by U+001F), and two paired rows are compared cell by cell **by position**, only when they have the
//! same number of cells with the same `gridSpan` and `vMerge`. Column operations are not in this release, so a row whose
//! cells changed shape is not paired: it is a deleted row and an inserted row, and a count says how many. The blocks of two
//! paired cells are a container again, so a table in a cell is handled by the same code.

use super::align::{How, Pair, align};
use super::model::{Body, Node, StoryModel};

/// Two paired blocks of a container pairing.
#[derive(Clone, Debug)]
pub(super) struct ItemPair {
    /// The index in the original container.
    pub a: usize,
    /// The index in the revised container.
    pub b: usize,
    /// How the pair was found.
    pub how: How,
    /// For two tables: the pairing of their rows.
    pub rows: Option<RowsPairing>,
}

/// The rows of two paired tables.
#[derive(Clone, Debug, Default)]
pub(super) struct RowsPairing {
    /// The paired rows that compare cell by cell, in order.
    pub pairs: Vec<RowPair>,
}

/// Two paired rows.
#[derive(Clone, Debug)]
pub(super) struct RowPair {
    /// The index of the row in the original table.
    pub a: usize,
    /// The index of the row in the revised table.
    pub b: usize,
    /// How the pair was found.
    pub how: How,
    /// The pairing of each cell's blocks, as indexes into the story pairing's containers, cell by cell.
    pub cells: Vec<usize>,
}

/// The pairing of one container of the original with one of the revised document.
#[derive(Clone, Debug)]
pub(super) struct ContainerPairing {
    /// The container in the original's model.
    pub ca: usize,
    /// The container in the revised model.
    pub cb: usize,
    /// The paired blocks, ascending in both lists.
    pub pairs: Vec<ItemPair>,
}

/// The pairing of two stories (or of one note with another): every container pairing reached from the roots.
#[derive(Clone, Debug, Default)]
pub(super) struct StoryPairing {
    /// The container pairings, the root first.
    pub containers: Vec<ContainerPairing>,
    /// How many rows had the same text on both sides but a different shape of cells, and were left unpaired.
    pub table_replaced: usize,
}

/// Pairs the container `ca` of `a` with the container `cb` of `b`, and everything inside the paired tables. Returns the
/// index of the new container pairing.
pub(super) fn pair_containers(
    a: &StoryModel,
    ca: usize,
    b: &StoryModel,
    cb: usize,
    out: &mut StoryPairing,
) -> usize {
    let at = out.containers.len();
    out.containers.push(ContainerPairing {
        ca,
        cb,
        pairs: Vec::new(),
    });
    let blocks_a: Vec<_> = a
        .containers
        .get(ca)
        .map(|c| c.blocks.iter().map(|n| n.block.clone()).collect())
        .unwrap_or_default();
    let blocks_b: Vec<_> = b
        .containers
        .get(cb)
        .map(|c| c.blocks.iter().map(|n| n.block.clone()).collect())
        .unwrap_or_default();
    let mut aligned = align(&blocks_a, &blocks_b).pairs;
    let nodes = |m: &StoryModel, c: usize| {
        m.containers
            .get(c)
            .map(|c| c.blocks.clone())
            .unwrap_or_default()
    };
    let (na, nb) = (nodes(a, ca), nodes(b, cb));
    join_replaced_tables(&na, &nb, &mut aligned);
    pair_the_last_paragraphs(&na, &nb, &mut aligned);
    let mut pairs = Vec::with_capacity(aligned.len());
    for p in aligned {
        let rows = match (
            a.containers.get(ca).and_then(|c| c.blocks.get(p.a)),
            b.containers.get(cb).and_then(|c| c.blocks.get(p.b)),
        ) {
            (Some(x), Some(y)) => match (&x.body, &y.body) {
                (Body::Table(ra), Body::Table(rb)) => Some(pair_rows(a, ra, b, rb, out)),
                _ => None,
            },
            _ => None,
        };
        pairs.push(ItemPair {
            a: p.a,
            b: p.b,
            how: p.how,
            rows,
        });
    }
    if let Some(c) = out.containers.get_mut(at) {
        c.pairs = pairs;
    }
    at
}

/// Pairs the last deleted block of each gap with the first inserted one when both are tables: the redline puts the deletions of a
/// gap right before its insertions (`plan::gap_entries`), and two tables with nothing between them are a shape that Word does not
/// keep apart, so a table replaced by another is written as one table of deleted rows and then inserted rows (design `compare.md`
/// 4.5), whatever the two grids are: `build` fits the rows to one grid.
fn join_replaced_tables(a: &[Node], b: &[Node], pairs: &mut Vec<Pair>) {
    let mut extra = Vec::new();
    let (mut a0, mut b0) = (0usize, 0usize);
    let ends = pairs
        .iter()
        .map(|p| (p.a, p.b))
        .chain(std::iter::once((a.len(), b.len())));
    for (a1, b1) in ends {
        if a1 > a0 && b1 > b0 {
            let (ta, tb) = (a1 - 1, b0);
            if let (Some(x), Some(y)) = (a.get(ta), b.get(tb))
                && matches!((&x.body, &y.body), (Body::Table(_), Body::Table(_)))
            {
                extra.push(Pair {
                    a: ta,
                    b: tb,
                    how: How::Similar,
                });
            }
        }
        (a0, b0) = (a1.saturating_add(1), b1.saturating_add(1));
    }
    if !extra.is_empty() {
        pairs.extend(extra);
        pairs.sort_by_key(|p| (p.a, p.b));
    }
}

/// Pairs the last paragraph of the original container with the last of the revised one when neither has a partner and both are
/// in the gap after the last pair: written as a deleted run followed by an inserted run, the last inserted paragraph would end the
/// container with an inserted mark, which no rejection removes in Word, so the two share the container's last mark as
/// one paragraph whose text is replaced, as Word writes new text typed over a cell's.
fn pair_the_last_paragraphs(a: &[Node], b: &[Node], pairs: &mut Vec<Pair>) {
    let (Some(la), Some(lb)) = (a.len().checked_sub(1), b.len().checked_sub(1)) else {
        return;
    };
    if pairs.last().is_some_and(|p| p.a >= la || p.b >= lb) {
        return;
    }
    // Two paragraphs with no section break and no atom (an equation, a picture), which a tracked change of text may not carry.
    let plain = |n: Option<&Node>| {
        n.is_some_and(|n| {
            matches!(&n.body, Body::Para(p) if !p.section && !p.alternate && !p.text.chars().any(super::text::is_atom))
        })
    };
    if plain(a.get(la)) && plain(b.get(lb)) {
        pairs.push(Pair {
            a: la,
            b: lb,
            how: How::Similar,
        });
    }
}

/// Pairs the rows of two paired tables and, for rows of one shape, their cells.
fn pair_rows(
    a: &StoryModel,
    rows_a: &[super::model::RowInfo],
    b: &StoryModel,
    rows_b: &[super::model::RowInfo],
    out: &mut StoryPairing,
) -> RowsPairing {
    let blocks_a: Vec<_> = rows_a.iter().map(|r| r.block.clone()).collect();
    let blocks_b: Vec<_> = rows_b.iter().map(|r| r.block.clone()).collect();
    let aligned = align(&blocks_a, &blocks_b);
    let mut pairs = Vec::new();
    for p in aligned.pairs {
        let (Some(ra), Some(rb)) = (rows_a.get(p.a), rows_b.get(p.b)) else {
            continue;
        };
        if ra.shape != rb.shape {
            out.table_replaced += 1;
            continue;
        }
        let cells = ra
            .cells
            .iter()
            .zip(&rb.cells)
            .map(|(&x, &y)| pair_containers(a, x, b, y, out))
            .collect();
        pairs.push(RowPair {
            a: p.a,
            b: p.b,
            how: p.how,
            cells,
        });
    }
    RowsPairing { pairs }
}
