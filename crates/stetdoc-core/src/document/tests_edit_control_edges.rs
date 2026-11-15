//! An insertion at the start or the end of a read-only inline content control (locked against editing, bound to data or
//! showing its placeholder) lands outside the control, before or after it, tracked and then accepted, and untracked, as beside
//! an unlocked control (issue #26). An insertion strictly inside such a control, and an edit of its text, stay
//! `readonly_region`. This is the edge rule fields and simple fields follow (`Compiler::check_gap`).

use super::tests_edit_field_ranges::{every_way, refused_both_ways, wr};

/// The properties of each kind of control, after its id: unlocked, then the four read-only kinds.
const KINDS: [(&str, &str); 5] = [
    ("unlocked", ""),
    ("placeholder", "<w:showingPlcHdr/>"),
    (
        "data-bound",
        r#"<w:dataBinding w:xpath="/root/value" w:storeItemID="{00000000-0000-0000-0000-000000000001}"/>"#,
    ),
    ("contentLocked", r#"<w:lock w:val="contentLocked"/>"#),
    ("sdtContentLocked", r#"<w:lock w:val="sdtContentLocked"/>"#),
];

/// `Lead [Value] tail`, `Value` (offsets 5 to 10) an inline content control with the properties `props`.
fn fixture(props: &str) -> String {
    // The schema puts `w:id` before `w:lock`, `w:showingPlcHdr` and `w:dataBinding`.
    format!(
        r#"<w:p>{}<w:sdt><w:sdtPr><w:id w:val="5"/>{props}</w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>{}</w:p>"#,
        wr("Lead "),
        wr("Value"),
        wr(" tail")
    )
}

fn insert(at: usize) -> String {
    format!(r#"{{"op":"replace","ref":"p1","start":{at},"end":{at},"with":"X"}}"#)
}

#[test]
fn an_insertion_at_a_read_only_controls_start_or_end_lands_outside_it() {
    for (_, props) in KINDS {
        let body = fixture(props);
        for (at, want) in [
            (5, "Lead X[Value] tail"),
            (10, "Lead [Value]X tail"),
            (11, "Lead [Value] Xtail"),
        ] {
            every_way(&body, &insert(at), want);
        }
        // A replacement that starts right after the control, or ends right before it.
        every_way(
            &body,
            r#"{"op":"replace","ref":"p1","start":10,"end":15,"with":" end"}"#,
            "Lead [Value] end",
        );
        every_way(
            &body,
            r#"{"op":"replace","ref":"p1","start":0,"end":5,"with":"First "}"#,
            "First [Value] tail",
        );
    }
}

#[test]
fn an_edit_inside_a_read_only_control_stays_refused() {
    for (_, props) in KINDS.iter().skip(1) {
        let body = fixture(props);
        refused_both_ways(&body, &insert(7));
        refused_both_ways(
            &body,
            r#"{"op":"replace","ref":"p1","start":5,"end":10,"with":"Q"}"#,
        );
        refused_both_ways(
            &body,
            r#"{"op":"replace","ref":"p1","start":4,"end":11,"with":"Q"}"#,
        );
    }
}
