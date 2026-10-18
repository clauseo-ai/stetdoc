//! Which conditional formats of a table style apply to a cell.
//!
//! The regions come from `w:tblLook` and the cell's position, not from `w:cnfStyle`, which ISO 29500
//! calls an optimization. The order is the reference's (banded columns, banded rows, first and last
//! column, first and last row, the four corners), which differs from the standard's listed order where
//! the first row meets the first column: register U20 in the design.

use crate::wordml::{TableRegion, TableSite};

/// The regions that apply to a cell: a fixed array, so asking for them allocates nothing.
#[derive(Clone, Copy, Debug)]
pub(super) struct Regions {
    items: [TableRegion; 12],
    len: usize,
}

impl Regions {
    fn new() -> Regions {
        Regions {
            items: [TableRegion::WholeTable; 12],
            len: 0,
        }
    }

    fn push(&mut self, r: TableRegion) {
        if let Some(slot) = self.items.get_mut(self.len) {
            *slot = r;
            self.len += 1;
        }
    }

    /// The regions, lowest priority first.
    pub fn iter(self) -> impl Iterator<Item = TableRegion> {
        self.items.into_iter().take(self.len)
    }

    /// The regions as a list, for tests.
    pub fn to_vec(self) -> Vec<TableRegion> {
        self.iter().collect()
    }
}

/// The conditional regions that apply to the cell, lowest priority first. The whole-table
/// properties of the style come before all of them and are not listed.
pub(super) fn applicable(site: &TableSite, row_band: usize, col_band: usize) -> Regions {
    let look = site.look;
    let last_row = site.row_count.saturating_sub(1);
    let last_col_start = |s: &TableSite| s.column + s.span.max(1) as usize >= s.column_count;
    let first_row = look.first_row && site.row == 0;
    let last_row_on = look.last_row && site.row == last_row && site.row_count > 1;
    let first_col = look.first_column && site.column == 0;
    let last_col = look.last_column && last_col_start(site) && site.column_count > 1;
    let mut out = Regions::new();
    if !look.no_v_band {
        let skip = usize::from(look.first_column);
        let tail_off = look.last_column && last_col_start(site);
        if site.column >= skip && !tail_off {
            let index = (site.column - skip) / col_band.max(1);
            out.push(if index.is_multiple_of(2) {
                TableRegion::Band1Vert
            } else {
                TableRegion::Band2Vert
            });
        }
    }
    if !look.no_h_band {
        let skip = usize::from(look.first_row);
        let tail_off = look.last_row && site.row == last_row && site.row_count > 1;
        if site.row >= skip && !tail_off {
            let index = (site.row - skip) / row_band.max(1);
            out.push(if index.is_multiple_of(2) {
                TableRegion::Band1Horz
            } else {
                TableRegion::Band2Horz
            });
        }
    }
    if first_col {
        out.push(TableRegion::FirstCol);
    }
    if last_col {
        out.push(TableRegion::LastCol);
    }
    if first_row {
        out.push(TableRegion::FirstRow);
    }
    if last_row_on {
        out.push(TableRegion::LastRow);
    }
    if first_row && last_col {
        out.push(TableRegion::NeCell);
    }
    if first_row && first_col {
        out.push(TableRegion::NwCell);
    }
    if last_row_on && last_col {
        out.push(TableRegion::SeCell);
    }
    if last_row_on && first_col {
        out.push(TableRegion::SwCell);
    }
    out
}
