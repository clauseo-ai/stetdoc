//! Tests of `set_props` and the mark primitives on containers that hold a property twice, which a tool wrote
//! (tdf104649): the duplicate was there before the edit, and no edit is refused for it.

use super::stamp::Track;
use super::tests_support::*;
use super::types::{PropEdit, PropTarget};

fn run_xml(rpr: &str, text: &str) -> String {
    format!("<w:r>{rpr}<w:t xml:space=\"preserve\">{text}</w:t></w:r>")
}

fn format_runs(body: &str, track: Track, range: std::ops::Range<usize>, edit: &PropEdit) -> Snap {
    with_compiler(body, track, |c, _, ks| {
        vec![c.set_props(
            &PropTarget::Run {
                block: ks[0].clone(),
                range,
            },
            edit,
        )]
    })
}

#[test]
fn a_property_container_with_a_repeated_child_still_takes_an_edit() {
    // tdf104649: a tool wrote `w:sz` twice in a run's properties, 46 of 6,378 documents in the corpus do. The
    // duplicate is the document's problem and was there before the edit, and an edit is not refused for it:
    // not one that adds another property, and not one that sets the property that is there twice.
    let rpr = "<w:rPr><w:color w:val=\"465BFF\"/><w:sz w:val=\"30\"/><w:sz w:val=\"30\"/><w:szCs w:val=\"30\"/></w:rPr>";
    let size = PropEdit {
        set: vec![pel_prop("sz", "40")],
        clear: Vec::new(),
    };
    for track in [Track::Tracked(stamp()), Track::Untracked] {
        for edit in [bold(), size.clone()] {
            let s = format_runs(&p(&run_xml(rpr, "hello")), track.clone(), 0..5, &edit);
            assert!(s.effects[0].is_ok(), "{:?}: {:?}", edit, s.effects[0]);
            assert!(s.schema.is_empty(), "{:?}", s.schema);
        }
    }
}

fn pel_prop(local: &str, val: &str) -> super::types::PropElement {
    super::types::PropElement {
        ns: "http://schemas.openxmlformats.org/wordprocessingml/2006/main".to_owned(),
        local: local.to_owned(),
        attrs: vec![(
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main".to_owned(),
            "val".to_owned(),
            val.to_owned(),
        )],
        children: Vec::new(),
    }
}

#[test]
fn a_paragraph_mark_with_a_repeated_property_takes_a_deleted_or_an_inserted_mark() {
    use super::types::{NewParagraph, Place};
    let mark = "<w:pPr><w:rPr><w:sz w:val=\"30\"/><w:sz w:val=\"30\"/></w:rPr></w:pPr>";
    let body = format!("<w:p>{mark}{}</w:p><w:p>{}</w:p>", r("one"), r("two"));
    for call in 0..4 {
        let s = with_compiler(&body, Track::Tracked(stamp()), |c, _, ks| {
            vec![match call {
                0 => c.delete_block(&ks[0]),
                1 => c.insert_block(&ks[0], Place::After, &NewParagraph::default()),
                2 => c.split_block(&ks[0], 1),
                _ => c.merge_blocks(&ks[0], &ks[1]),
            }]
        });
        assert!(
            s.effects[0].is_ok(),
            "call {call}: {:?}\n{}",
            s.effects[0],
            s.xml
        );
        assert!(s.schema.is_empty(), "call {call}: {:?}", s.schema);
    }
}
#[test]
fn a_tracked_format_on_a_run_with_a_repeated_property_can_be_rejected_after_commit() {
    // tdf104649: the run's properties hold `w:sz` twice. A tracked bold on part of the run snapshots them,
    // twice, and rejecting the change after a commit puts both back, the second right after the first.
    use super::index::ChangeIndex;
    use super::resolve::resolve;
    use super::tests_resolve::reject_all;
    use super::types::PropTarget;
    let rpr = "<w:rPr><w:color w:val=\"465BFF\"/><w:sz w:val=\"30\"/><w:sz w:val=\"30\"/><w:szCs w:val=\"30\"/></w:rPr>";
    let s = state(&p(&run_xml(rpr, "hello world")));
    let before = body_xml_of(&s);
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    {
        let mut c = tracked(&mut tx);
        c.set_props(
            &PropTarget::Run {
                block: ks[0].clone(),
                range: 0..5,
            },
            &bold(),
        )
        .unwrap();
        c.finish().unwrap();
    }
    let (s2, _) = tx.commit().unwrap();
    let mut tx = s2.begin();
    let mut index = ChangeIndex::new();
    resolve(&mut tx, &mut index, &[reject_all()])
        .unwrap_or_else(|e| panic!("reject failed: {e}\n{}", body_xml(&tx)));
    let added = schema_added(&s2, &tx);
    assert!(added.is_empty(), "{added:?}");
    let xml = body_xml(&tx);
    assert_eq!(xml.matches(rpr).count(), 2, "{xml}");
    assert!(before.contains(rpr));
    assert!(
        !xml.contains("<w:b/>") && !xml.contains("rPrChange"),
        "{xml}"
    );
    tx.commit().unwrap();
}

#[test]
fn rejecting_a_format_that_removed_a_repeated_property_adds_no_violation() {
    // tdf168070: `w:sz` twice, and a tracked bold and size over the whole run, so the live properties hold it once and
    // only the snapshot in `w:rPrChange` holds it twice. Rejecting puts the twins back. That is no new problem: the
    // snapshot had it, and the schema sees it there, as it sees it in the run (it once saw no order in a snapshot at all).
    use super::index::ChangeIndex;
    use super::resolve::resolve;
    use super::tests_resolve::reject_all;
    let rpr = "<w:rPr><w:rFonts w:ascii=\"Sakkal Majalla\" w:hAnsi=\"Sakkal Majalla\"/><w:sz w:val=\"24\"/><w:sz w:val=\"24\"/><w:szCs w:val=\"24\"/><w:rtl w:val=\"true\"/></w:rPr>";
    let edit = PropEdit {
        set: vec![pel_prop("b", "true"), pel_prop("sz", "24")],
        clear: Vec::new(),
    };
    let s = state(&p(&run_xml(rpr, "hello")));
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    {
        let mut c = tracked(&mut tx);
        c.set_props(
            &PropTarget::Run {
                block: ks[0].clone(),
                range: 0..5,
            },
            &edit,
        )
        .unwrap();
        c.finish().unwrap();
    }
    let (s2, _) = tx.commit().unwrap();
    let edited = body_xml_of(&s2);
    assert_eq!(edited.matches("<w:sz ").count(), 3, "{edited}");
    let mut tx = s2.begin();
    let mut index = ChangeIndex::new();
    resolve(&mut tx, &mut index, &[reject_all()])
        .unwrap_or_else(|e| panic!("reject failed: {e}\n{}", body_xml(&tx)));
    let added = schema_added(&s2, &tx);
    assert!(added.is_empty(), "{added:?}\n{}", body_xml(&tx));
    let xml = body_xml(&tx);
    assert!(xml.contains(rpr), "{xml}");
    tx.commit().unwrap();
}
