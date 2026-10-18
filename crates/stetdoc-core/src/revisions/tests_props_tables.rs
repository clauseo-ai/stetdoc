//! Tests of `set_props` on tables, rows and cells.

use super::stamp::Track;
use super::tests_support::*;
use super::types::PropTarget;
use crate::api::error::ErrorCode;

const TABLE: &str = "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl>";

#[test]
fn table_row_and_cell_properties_get_their_own_changes() {
    let s = with_compiler(TABLE, Track::Tracked(stamp()), |c, all, _| {
        vec![
            c.set_props(
                &PropTarget::Table {
                    table: all[0].clone(),
                },
                &set_edit(vec![pel("tblStyle", &[("val", "Grid")], vec![])]),
            ),
            c.set_props(
                &PropTarget::Row {
                    row: all[1].clone(),
                },
                &set_edit(vec![pel("cantSplit", &[], vec![])]),
            ),
            c.set_props(
                &PropTarget::Cell {
                    row: all[1].clone(),
                    cell: 0,
                },
                &set_edit(vec![pel("tcW", &[("w", "4000"), ("type", "dxa")], vec![])]),
            ),
        ]
    });
    s.assert_clean();
    assert!(
        s.xml
            .contains("<w:tblPr><w:tblStyle w:val=\"Grid\"/><w:tblPrChange"),
        "{}",
        s.xml
    );
    assert!(
        s.xml.contains("<w:trPr><w:cantSplit/><w:trPrChange"),
        "{}",
        s.xml
    );
    assert!(
        s.xml
            .contains("<w:tcPr><w:tcW w:w=\"4000\" w:type=\"dxa\"/><w:tcPrChange"),
        "{}",
        s.xml
    );
    assert!(
        s.xml
            .contains("<w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr></w:tcPrChange>"),
        "{}",
        s.xml
    );
    assert_eq!(created_ids(s.outcome.as_ref().unwrap()).len(), 3);
}

#[test]
fn a_cell_that_does_not_exist_is_invalid() {
    let s = with_compiler(TABLE, Track::Tracked(stamp()), |c, all, _| {
        vec![c.set_props(
            &PropTarget::Cell {
                row: all[1].clone(),
                cell: 3,
            },
            &bold(),
        )]
    });
    assert_eq!(s.code(0), Some(ErrorCode::InvalidPayload));
}

#[test]
fn the_snapshot_of_a_container_written_out_of_order_is_written_in_order() {
    // docxbox writes w:jc and w:tblInd before w:tblW. The live w:tblPr is put in order where it changes, and so is the
    // snapshot in its w:tblPrChange, which is a new element.
    let t = "<w:tbl><w:tblPr><w:tblStyle w:val=\"Grid\"/><w:jc w:val=\"left\"/><w:tblInd w:w=\"0\" w:type=\"dxa\"/><w:tblW w:w=\"9063\" w:type=\"dxa\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:vMerge/><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl>";
    let s = with_compiler(t, Track::Tracked(stamp()), |c, all, _| {
        vec![
            c.set_props(
                &PropTarget::Table {
                    table: all[0].clone(),
                },
                &set_edit(vec![pel("jc", &[("val", "center")], vec![])]),
            ),
            c.set_props(
                &PropTarget::Cell {
                    row: all[1].clone(),
                    cell: 0,
                },
                &set_edit(vec![pel("noWrap", &[], vec![])]),
            ),
        ]
    });
    s.assert_clean();
    assert!(
        s.xml.contains("<w:tblPr><w:tblStyle w:val=\"Grid\"/><w:tblW w:w=\"9063\" w:type=\"dxa\"/><w:jc w:val=\"left\"/><w:tblInd"),
        "{}",
        s.xml
    );
    assert!(
        s.xml.contains(
            "<w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/><w:vMerge/></w:tcPr></w:tcPrChange>"
        ),
        "{}",
        s.xml
    );
}
