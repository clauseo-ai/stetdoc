//! Random formatting of ranges: the text in every mode never moves, the formatting of each character is
//! what a per-character model says, every revision passes the Word rules, and the schema has nothing
//! new to say.

use super::compiler::Compiler;
use super::stamp::Track;
use super::tests_diff::Rng;
use super::tests_support::*;
use super::tests_text_rand::fixtures;
use super::types::PropTarget;
use crate::api::error::ErrorCode;
use crate::state::Tx;
use crate::wordml::{Item, ItemKind, Mode, Paragraph};

/// The characters of paragraph `i` in `Final`, each with whether its run is bold and italic.
fn formatting(tx: &Tx<'_>, i: usize) -> Vec<(char, bool, bool)> {
    let view = tx.parts();
    let id = view.id(MAIN).unwrap();
    let story = view.story(id).unwrap();
    let tree = view.tree(id).unwrap();
    let block = story
        .blocks()
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == crate::wordml::BlockKind::Paragraph)
        .nth(i)
        .unwrap()
        .0;
    let para = Paragraph::new(tree, story.key(&story.blocks()[block]).node()).unwrap();
    let map = para.map();
    let flag = |item: &Item, name: &str| -> bool {
        let Some(run) = item.run else { return false };
        let r = tree.node_ref(run);
        r.child_elements()
            .find(|c| c.local_name() == "rPr")
            .is_some_and(|rpr| {
                rpr.child_elements().any(|c| {
                    c.local_name() == name
                        && c.attrs()
                            .find(|a| a.local_name() == "val")
                            .is_none_or(|a| !matches!(&*a.value(), "0" | "false" | "off"))
                })
            })
    };
    let mut out = Vec::new();
    for item in map.items() {
        if item.len == 0 || !item.vis.in_mode(Mode::Final) {
            continue;
        }
        let chars: Vec<char> = if item.kind == ItemKind::Text {
            tree.node_ref(item.node)
                .text()
                .chars()
                .skip(item.from as usize)
                .take(item.len as usize)
                .collect()
        } else {
            item.kind.sentinel().into_iter().collect()
        };
        let (b, it) = (flag(item, "b"), flag(item, "i"));
        out.extend(chars.into_iter().map(|c| (c, b, it)));
    }
    out
}

fn batch(body: &str, seed: u64, by: Option<&str>) {
    let s = state(body);
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    let n = ks.len();
    let mut rng = Rng(seed | 1);
    let before_texts: Vec<Vec<String>> = Mode::ALL.iter().map(|&m| texts(&tx, m)).collect();
    let mut model: Vec<Vec<(char, bool, bool)>> = (0..n).map(|i| formatting(&tx, i)).collect();
    let track = Track::Tracked(match by {
        Some(a) => stamp_of(a),
        None => stamp(),
    });
    let mut log = Vec::new();
    let out = {
        let mut c = Compiler::new(&mut tx, track).unwrap();
        for _ in 0..1 + rng.below(4) {
            let i = rng.below(n);
            let len = model[i].len();
            let (a, b) = (rng.below(len + 1), rng.below(len + 1));
            let (a, b) = if a <= b { (a, b) } else { (b, a) };
            let on = rng.below(3) != 0;
            let which = rng.below(2) == 0;
            let name = if which { "b" } else { "i" };
            let edit = if on {
                set_edit(vec![pel(name, &[], vec![])])
            } else {
                clear_edit(&[name])
            };
            log.push(format!(
                "p{i} {a}..{b} {} {name}",
                if on { "set" } else { "clear" }
            ));
            match c.set_props(
                &PropTarget::Run {
                    block: ks[i].clone(),
                    range: a..b,
                },
                &edit,
            ) {
                Ok(_) => {
                    // An equation has no run of its own, so there is nothing of it to format.
                    for ch in model[i][a..b].iter_mut().filter(|c| c.0 != '\u{4}') {
                        if which {
                            ch.1 = on;
                        } else {
                            ch.2 = on;
                        }
                    }
                }
                Err(e)
                    if matches!(e.code(), ErrorCode::ReadonlyRegion | ErrorCode::Unsupported) =>
                {
                    return;
                }
                Err(e) => panic!("seed {seed}: {e}\n{log:?}\n{body}"),
            }
        }
        c.finish()
    };
    out.unwrap_or_else(|e| panic!("seed {seed}: finish: {e}\n{log:?}\n{}", body_xml(&tx)));
    for (k, &m) in Mode::ALL.iter().enumerate() {
        assert_eq!(
            texts(&tx, m),
            before_texts[k],
            "seed {seed}: {m:?} text moved\n{log:?}\n{}",
            body_xml(&tx)
        );
    }
    for (i, want) in model.iter().enumerate() {
        assert_eq!(
            &formatting(&tx, i),
            want,
            "seed {seed}: formatting of p{i}\n{log:?}\n{}",
            body_xml(&tx)
        );
    }
    let added = schema_added(&s, &tx);
    assert!(
        added.is_empty(),
        "seed {seed}: {added:?}\n{log:?}\n{}",
        body_xml(&tx)
    );
    tx.commit()
        .unwrap_or_else(|e| panic!("seed {seed}: commit: {e}"));
}

#[test]
fn random_formatting_matches_the_per_character_model_and_never_moves_the_text() {
    for (n, body) in fixtures().iter().enumerate() {
        for k in 0..80u64 {
            batch(
                body,
                0x2545_F491_4F6C_DD1Du64.wrapping_mul(1 + n as u64 * 1000 + k),
                None,
            );
        }
    }
}

#[test]
fn a_second_author_formatting_the_first_authors_output_keeps_the_model() {
    for (n, body) in fixtures().iter().enumerate() {
        for k in 0..30u64 {
            batch(
                body,
                0x6C62_272E_07BB_0142u64.wrapping_mul(1 + n as u64 * 100 + k),
                Some("Zed"),
            );
        }
    }
}
