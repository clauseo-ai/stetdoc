//! Origins (design `revisions.md`, 2.1, and `check.md` 2.3): every block a call makes says what it was built
//! from, a join says what it was made of, and `Outcome::origins` and `Resolution::origins` hand on blocks of the
//! document as it was before.

use super::index::ChangeIndex;
use super::outcome::{Effect, Origin};
use super::resolve::resolve;
use super::stamp::Track;
use super::tests_support::*;
use super::types::{Action, NewParagraph, Place, PropEdit, Selector, Step};
use crate::wordml::BlockKey;

fn new_paragraph(text: &str) -> NewParagraph {
    NewParagraph {
        text: text.to_owned(),
        props: PropEdit::default(),
        style: None,
    }
}

fn origin_of<'a>(fx: &'a Effect, block: &BlockKey) -> &'a Origin {
    fx.origins
        .iter()
        .find(|o| o.block == *block)
        .unwrap_or_else(|| panic!("no origin for {block:?} in {:?}", fx.origins))
}

#[test]
fn every_paragraph_that_insert_block_makes_is_built_from_the_one_it_was_modelled_on() {
    let body = format!("{}{}{}", p(&r("one")), p(&r("two")), p(&r("three")));
    for (track, place, anchor, from) in [
        // shape Y: after a paragraph with a successor, tracked
        (Track::Tracked(stamp()), Place::After, 0, 0),
        // shape X after the last paragraph: the new one is modelled on the anchor
        (Track::Tracked(stamp()), Place::After, 2, 2),
        // before a paragraph: modelled on it
        (Track::Tracked(stamp()), Place::Before, 1, 1),
        // untracked, after: modelled on the anchor
        (Track::Untracked, Place::After, 1, 1),
    ] {
        let s = with_compiler(&body, track, |c, _, ks| {
            vec![c.insert_block(&ks[anchor], place, &new_paragraph("new"))]
        });
        let fx = s.effect(0);
        assert_eq!(fx.new_blocks.len(), 1, "{}", s.xml);
        let origin = origin_of(fx, &fx.new_blocks[0]);
        let ks = {
            let st = state(&body);
            let tx = st.begin();
            para_keys(&tx)
        };
        assert_eq!(origin.from, [ks[from].clone()], "{place:?} {anchor}");
    }
}

#[test]
fn a_split_declares_the_tail_as_built_from_the_paragraph_it_was_cut_from() {
    let body = p(&r("one two"));
    for track in [Track::Tracked(stamp()), Track::Untracked] {
        let s = with_compiler(&body, track, |c, _, ks| vec![c.split_block(&ks[0], 3)]);
        let fx = s.effect(0);
        let tail = &fx.new_blocks[0];
        let st = state(&body);
        let tx = st.begin();
        assert_eq!(origin_of(fx, tail).from, [para_keys(&tx)[0].clone()]);
    }
}

#[test]
fn a_new_row_declares_each_of_its_paragraphs_as_built_from_the_paragraph_of_the_row_it_copies() {
    let cell = |t: &str| {
        format!(
            "<w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr>{}</w:tc>",
            p(&r(t))
        )
    };
    let body = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl>{}",
        cell("a"),
        cell("b"),
        p(&r("after"))
    );
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, all, _| {
        vec![c.insert_row(&all[1], Place::After, &["x".to_owned(), "y".to_owned()])]
    });
    let fx = s.effect(0);
    // The row and its two paragraphs.
    assert_eq!(fx.new_blocks.len(), 3);
    let st = state(&body);
    let tx = st.begin();
    let ps = para_keys(&tx);
    assert_eq!(origin_of(fx, &fx.new_blocks[1]).from, [ps[0].clone()]);
    assert_eq!(origin_of(fx, &fx.new_blocks[2]).from, [ps[1].clone()]);
}

#[test]
fn a_merge_that_joins_declares_the_survivor_as_made_of_both() {
    // Untracked, the paragraphs are joined at once.
    let body = format!("{}{}", p(&r("one")), p(&r("two")));
    let s = with_compiler(&body, Track::Untracked, |c, _, ks| {
        vec![c.merge_blocks(&ks[0], &ks[1])]
    });
    let fx = s.effect(0);
    let st = state(&body);
    let tx = st.begin();
    let ps = para_keys(&tx);
    assert_eq!(origin_of(fx, &ps[0]).from, [ps[0].clone(), ps[1].clone()]);
}

#[test]
fn a_tracked_merge_joins_nothing_and_declares_what_it_copied() {
    // Tracked, the mark is deleted and the two paragraphs stay. The second takes the first one's properties
    // as a change of its own when they differ, and then it is built from both. When they do not differ
    // nothing moved between them, and nothing is declared.
    let same = format!("{}{}", p(&r("one")), p(&r("two")));
    let s = with_compiler(&same, Track::Tracked(stamp()), |c, _, ks| {
        vec![c.merge_blocks(&ks[0], &ks[1])]
    });
    assert!(s.effect(0).origins.is_empty(), "{:?}", s.effect(0).origins);
    let differ = format!(
        "<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr>{}</w:p>{}",
        r("one"),
        p(&r("two"))
    );
    let s = with_compiler(&differ, Track::Tracked(stamp()), |c, _, ks| {
        vec![c.merge_blocks(&ks[0], &ks[1])]
    });
    let fx = s.effect(0);
    let st = state(&differ);
    let tx = st.begin();
    let ps = para_keys(&tx);
    assert_eq!(origin_of(fx, &ps[1]).from, [ps[1].clone(), ps[0].clone()]);
}

#[test]
fn the_origins_of_an_outcome_are_blocks_the_document_had_before() {
    // Cut a paragraph, then join the tail back: the tail is a block that only this compiler knew, and what
    // `check` can read is the paragraph it came from. And a paragraph made from a model that a later call
    // joined into another is the model.
    let body = format!("{}{}", p(&r("one two")), p(&r("three")));
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, _, ks| {
        let cut = c.split_block(&ks[0], 3);
        let tail = cut.as_ref().unwrap().new_blocks[0].clone();
        let join = c.merge_blocks(&ks[0], &tail);
        vec![cut, join]
    });
    let out = s.outcome.as_ref().unwrap();
    let raw: Vec<_> = out.effects.iter().flat_map(|e| e.origins.iter()).collect();
    let st = state(&body);
    let tx = st.begin();
    let ps = para_keys(&tx);
    // As the calls declared them, the join names the tail.
    assert!(
        raw.iter()
            .any(|o| o.from.len() == 2 && o.from[0] == ps[0] && o.from[1] != ps[1]),
        "{raw:?}"
    );
    // As handed on, every `from` is a block of the base, in order, with no repeats.
    let handed = out.origins();
    assert_eq!(handed.len(), raw.len());
    for o in &handed {
        for from in &o.from {
            assert!(
                ps.contains(from),
                "{from:?} is not a block of the base: {handed:?}"
            );
        }
    }
    let join = handed.last().unwrap();
    assert_eq!(join.block, ps[0]);
    assert_eq!(
        join.from,
        [ps[0].clone()],
        "the tail is the paragraph it was cut from"
    );
}

#[test]
fn a_block_made_from_nothing_is_left_out_of_what_is_handed_on() {
    let body = p(&r("one"));
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, _, ks| {
        vec![c.insert_block(&ks[0], Place::After, &new_paragraph("new"))]
    });
    let mut out = s.outcome.clone().unwrap();
    // Say the new block was made from nothing, and let a later call name it.
    let new = out.effects[0].new_blocks[0].clone();
    out.effects[0].origins.clear();
    out.effects[0].origins.push(Origin {
        block: new.clone(),
        from: Vec::new(),
    });
    let st = state(&body);
    let tx = st.begin();
    let ps = para_keys(&tx);
    out.effects.push(Effect {
        origins: vec![Origin {
            block: ps[0].clone(),
            from: vec![ps[0].clone(), new],
        }],
        ..Effect::default()
    });
    let handed = out.origins();
    assert_eq!(handed[1].from, [ps[0].clone()], "{handed:?}");
}

#[test]
fn a_resolution_declares_the_paragraph_a_join_made_of_both() {
    // Accepting the deletion of the first paragraph's mark joins it with the next.
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:del w:id=\"1\" {AT}/></w:rPr></w:pPr>{}</w:p>{}",
        r("one"),
        p(&r("two"))
    );
    let st = state(&body);
    let mut tx = st.begin();
    let ps = para_keys(&tx);
    let res = resolve(
        &mut tx,
        &mut ChangeIndex::new(),
        &[Step {
            action: Action::Accept,
            selector: Selector::All,
        }],
    )
    .unwrap();
    let origins = res[0].origins();
    assert_eq!(origins.len(), 1, "{:?}", res[0].joined);
    assert_eq!(
        origins[0].block, ps[0],
        "the first paragraph's element survives"
    );
    assert_eq!(origins[0].from, [ps[0].clone(), ps[1].clone()]);
}

#[test]
fn an_outcome_lists_what_its_calls_removed_folded_and_swallowed_as_ids() {
    use super::outcome::{Folded, RemovedKind};
    use super::types::PropTarget;
    // Own text typed and deleted again: removed outright, with the id of the insertion it was in.
    let s = with_compiler(&p(&r("hello")), Track::Tracked(stamp()), |c, _, ks| {
        vec![
            c.replace_text(&ks[0], 2..2, "new"),
            c.replace_text(&ks[0], 2..5, ""),
        ]
    });
    let out = s.outcome.as_ref().unwrap();
    let removed = out.removed_own();
    assert_eq!(removed.len(), 1, "{removed:?}");
    assert_eq!(removed[0].kind, RemovedKind::Text);
    assert!(removed[0].id.is_some());
    // An edit that brings the properties back to the snapshot of the author's own earlier change takes the
    // change away, and the outcome says which change it was.
    let body = p(&format!(
        "<w:r><w:rPr><w:b/><w:rPrChange w:id=\"300\" {AT}><w:rPr/></w:rPrChange></w:rPr><w:t>mine</w:t></w:r>"
    ));
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, _, ks| {
        let unbold = super::types::PropEdit {
            set: Vec::new(),
            clear: vec![(
                "http://schemas.openxmlformats.org/wordprocessingml/2006/main".to_owned(),
                "b".to_owned(),
            )],
        };
        vec![c.set_props(
            &PropTarget::Run {
                block: ks[0].clone(),
                range: 0..4,
            },
            &unbold,
        )]
    });
    let out = s.outcome.as_ref().unwrap();
    assert_eq!(
        out.folded(),
        [Folded {
            id: 300,
            cancelled: true
        }]
    );
    // Swallowed ids are listed once each, in order, over every call.
    let mut synthetic = super::outcome::Outcome::default();
    for ids in [vec![5, 7], vec![7, 9]] {
        synthetic.effects.push(Effect {
            swallowed: ids,
            ..Effect::default()
        });
    }
    assert_eq!(synthetic.swallowed(), [5, 7, 9]);
}
