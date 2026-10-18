//! Tests of `insert_block` next to a block inside a revision wrapper that holds whole blocks (`w:moveFrom`, `w:moveTo`,
//! `w:ins`, `w:del` at body level): the new paragraph goes outside the wrapper next to its first or last block, and
//! anywhere else inside it is `readonly_region`.

use super::stamp::Track;
use super::tests_support::*;
use super::types::{NewParagraph, Place};
use crate::api::error::ErrorCode;
use crate::wordml::Mode;

fn new_para(text: &str) -> NewParagraph {
    NewParagraph {
        text: text.to_owned(),
        ..NewParagraph::default()
    }
}

/// `A`, then `B` and `C` inside a body-level wrapper, then `D`.
fn wrapped(wrapper: &str) -> String {
    format!(
        "{}<w:{wrapper} w:id=\"901\" w:author=\"Eric\" w:date=\"2020-01-02T03:04:05Z\">{}{}</w:{wrapper}>{}",
        para("A"),
        para("B"),
        para("C"),
        para("D")
    )
}

/// The body after inserting `New` at block `anchor` (0 is `A`, 1 `B`, 2 `C`, 3 `D`), or the error code.
fn insert_at(body: &str, track: Track, anchor: usize, place: Place) -> Result<String, ErrorCode> {
    let s = state(body);
    let mut tx = s.begin();
    let ks = keys(&tx);
    let mut c = super::compiler::Compiler::new(&mut tx, track).unwrap();
    let got = c.insert_block(&ks[anchor], place, &new_para("New"));
    let fx = match got {
        Ok(fx) => fx,
        Err(e) => return Err(e.code()),
    };
    drop(c);
    let made = fx.new_blocks[0].node();
    let view = tx.parts();
    let tree = view.tree(view.id(MAIN).unwrap()).unwrap();
    let parent = tree.node_ref(made).parent().unwrap();
    assert_eq!(
        parent.local_name(),
        "body",
        "the new paragraph is outside the wrapper: {}",
        body_xml(&tx)
    );
    assert!(
        texts(&tx, Mode::Final).iter().any(|t| t == "New"),
        "the new paragraph reads New in Final: {:?}",
        texts(&tx, Mode::Final)
    );
    Ok(body_xml(&tx))
}

#[test]
fn a_paragraph_next_to_the_edge_of_a_wrapper_goes_outside_it() {
    for wrapper in ["moveFrom", "moveTo", "ins", "del"] {
        let body = wrapped(wrapper);
        for track in [Track::Tracked(stamp()), Track::Untracked] {
            // after the last block: between the wrapper and D
            let xml = insert_at(&body, track.clone(), 2, Place::After)
                .unwrap_or_else(|e| panic!("{wrapper} after C: {e:?}"));
            let close = format!("</w:{wrapper}>");
            let (wrapped, rest) = xml.split_once(&close).unwrap();
            assert!(
                !wrapped.contains(">New<") && rest.find(">New<") < rest.find(">D<"),
                "{wrapper}: {xml}"
            );
            // before the first block: between A and the wrapper
            let xml = insert_at(&body, track.clone(), 1, Place::Before)
                .unwrap_or_else(|e| panic!("{wrapper} before B: {e:?}"));
            let open = format!("<w:{wrapper} w:id=\"901\"");
            let (before, _) = xml.split_once(&open).unwrap();
            assert!(
                before.find(">A<") < before.find(">New<"),
                "{wrapper}: {xml}"
            );
        }
    }
}

#[test]
fn a_paragraph_between_two_blocks_of_a_wrapper_is_refused() {
    for wrapper in ["moveFrom", "moveTo", "ins", "del"] {
        let body = wrapped(wrapper);
        for track in [Track::Tracked(stamp()), Track::Untracked] {
            assert_eq!(
                insert_at(&body, track.clone(), 1, Place::After),
                Err(ErrorCode::ReadonlyRegion),
                "{wrapper} after B"
            );
            assert_eq!(
                insert_at(&body, track.clone(), 2, Place::Before),
                Err(ErrorCode::ReadonlyRegion),
                "{wrapper} before C"
            );
        }
    }
}

#[test]
fn a_range_marker_beside_the_last_block_does_not_hold_it_inside() {
    let body = format!(
        "{}<w:moveFrom w:id=\"901\" w:author=\"Eric\" w:date=\"2020-01-02T03:04:05Z\">{}<w:bookmarkEnd w:id=\"5\"/></w:moveFrom>{}",
        para("A"),
        para("B"),
        para("D")
    );
    // B is both the first and the last block, so either side goes out
    insert_at(&body, Track::Tracked(stamp()), 1, Place::After).unwrap();
    insert_at(&body, Track::Tracked(stamp()), 1, Place::Before).unwrap();
}
