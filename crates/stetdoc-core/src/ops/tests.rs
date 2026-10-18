//! The parts of `ops` that need no document: the mapping of the plan, and the labels of operations.

use super::*;
use crate::api::files::{NewParagraph, Origin, Place, Planned, PlannedOp};
use crate::api::op::{RowAction, RowOp};

fn origin(line: u32) -> Origin {
    Origin {
        file: "document.md".to_string(),
        line,
        block: None,
        also: None,
    }
}

#[test]
fn a_plan_maps_to_operations_one_to_one_and_in_order() {
    let planned = Planned {
        ops: vec![
            PlannedOp::Replace {
                block: "p4".into(),
                start: 3,
                end: 9,
                with: "must".into(),
            },
            PlannedOp::Insert {
                anchor: "p4".into(),
                place: Place::After,
                paragraphs: vec![NewParagraph {
                    text: "new".into(),
                    style: Some("Heading2".into()),
                }],
            },
            PlannedOp::Delete { block: "t2".into() },
            PlannedOp::RowDelete { row: "r7".into() },
            PlannedOp::Split {
                block: "p5".into(),
                at: 12,
            },
            PlannedOp::Merge {
                first: "p6".into(),
                second: "p7".into(),
            },
        ],
        origins: (1..=6).map(origin).collect(),
        notes: Vec::new(),
        baseline: Vec::new(),
    };
    let batch = planned::batch("3f9a0c11d2b4e577:7:51ac09f2e8b34d11", &planned);
    assert_eq!(batch.base, "3f9a0c11d2b4e577:7:51ac09f2e8b34d11");
    let names: Vec<&str> = batch.ops.iter().map(Op::name).collect();
    assert_eq!(
        names,
        ["replace", "insert", "delete", "row", "split", "merge"]
    );
    let Op::Replace(replace) = &batch.ops[0] else {
        panic!("not a replace")
    };
    assert_eq!(
        (replace.start, replace.end, replace.find.as_deref()),
        (Some(3), Some(9), None)
    );
    let Op::Insert(insert) = &batch.ops[1] else {
        panic!("not an insert")
    };
    assert_eq!(insert.paragraphs[0].style.as_deref(), Some("Heading2"));
    let Op::Row(RowOp { action, r#ref, .. }) = &batch.ops[3] else {
        panic!("not a row")
    };
    assert_eq!((*action, r#ref.as_str()), (RowAction::Delete, "r7"));
    let Op::Merge(merge) = &batch.ops[5] else {
        panic!("not a merge")
    };
    assert_eq!(merge.second.as_deref(), Some("p7"));
    // The result is a batch that validates, which is what `Edit::files` then applies.
    validate::batch(&batch, &crate::api::limits::Limits::default()).unwrap();
}

#[test]
fn an_operation_is_labelled_by_what_it_names() {
    let batch = Batch::parse(
        r#"{"base":"3f9a0c11d2b4e577:7:51ac09f2e8b34d11","ops":[
            {"op":"replace","ref":"p57","with":"x"},
            {"op":"accept","ids":[7,8]},
            {"op":"reject","author":"JD"},
            {"op":"accept","all":true}]}"#,
        &crate::api::limits::Limits::default(),
    )
    .unwrap();
    let labels: Vec<String> = batch.ops.iter().map(label).collect();
    assert_eq!(
        labels,
        ["replace p57", "accept #7 #8", "reject JD", "accept all"]
    );
}
