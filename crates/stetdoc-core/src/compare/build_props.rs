//! The entry that writes a table's, a row's or a cell's old properties (decision 0022).

use super::build::Builder;
use super::import::Cx;
use super::plan::StoryPlan;
use super::plan_props::PropsPlan;

impl Builder<'_, '_, '_> {
    /// A paired table, row or cell whose properties differ: a `w:tblPrChange`, `w:trPrChange` or `w:tcPrChange` with the
    /// original's properties. One that cannot be written leaves the revised properties and is listed as not redlined.
    pub(super) fn props(&mut self, sp: &StoryPlan, pp: &PropsPlan) {
        let cx = Cx {
            src: &sp.part_a,
            dst: &sp.part_b,
        };
        let block = Self::key(&sp.part_b, pp.block);
        let what = match pp.container {
            "tblPr" => "the table's properties",
            "trPr" => "the row's properties",
            _ => "a cell's properties",
        };
        let items =
            match self
                .imp
                .container_snapshot(&mut self.c.import_writer(), cx, pp.a, pp.container)
            {
                Ok(items) => items,
                Err(e) => {
                    self.exclude(
                        vec![block],
                        Vec::new(),
                        "unredlined",
                        format!("{what} changed, and the original's could not be read: {e}"),
                    );
                    return;
                }
            };
        match self.c.set_container_snapshot(&block, pp.b, &items) {
            Ok(fx) => {
                self.record(&fx);
                self.rc.counts.props += 1;
            }
            Err(e) => self.exclude(
                vec![block],
                Vec::new(),
                "unredlined",
                format!("{what} changed, and the change could not be written: {e}"),
            ),
        }
    }
}
