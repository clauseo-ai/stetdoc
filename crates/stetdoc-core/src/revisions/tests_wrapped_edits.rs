//! Tests of a text or property edit of a paragraph inside a revision wrapper that holds whole blocks: refused inside a
//! `w:moveFrom` or a `w:del`, whose paragraphs are gone in `Final`, and allowed inside a `w:moveTo` or a `w:ins`.

use super::compiler::Compiler;
use super::stamp::Track;
use super::tests_support::*;
use super::types::{PropEdit, PropElement, PropTarget};
use crate::api::error::ErrorCode;
use crate::wordml::Mode;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// `A`, then one paragraph in each wrapper: `B` moved away, `C` moved here, `D` deleted, `E` inserted, by Eric or, with
/// `own`, by the author of the edits (an untracked edit of another author's insertion is refused for that reason).
fn body(own: bool) -> String {
    let wrap = |name: &str, id: u32, text: &str| {
        let attrs = if own {
            format!("w:id=\"{id}\" {AT}")
        } else {
            eric(id)
        };
        format!("<w:{name} {attrs}>{}</w:{name}>", para(text))
    };
    format!(
        "{}{}{}{}{}",
        para("A"),
        wrap("moveFrom", 901, "Bee"),
        wrap("moveTo", 902, "Cee"),
        wrap("del", 903, "Dee"),
        wrap("ins", 904, "Eee")
    )
}

fn centred() -> PropEdit {
    PropEdit {
        set: vec![PropElement {
            ns: W.to_owned(),
            local: "jc".to_owned(),
            attrs: vec![(W.to_owned(), "val".to_owned(), "center".to_owned())],
            children: Vec::new(),
        }],
        clear: Vec::new(),
    }
}

/// The error code of each edit of block `i` (1 is `B`, up to 4 for `E`), or `None` when it applies; and after a text edit
/// that applies, the Final texts.
fn edits(track: &Track, i: usize, own: bool) -> Vec<(String, Option<ErrorCode>)> {
    let s = state(&body(own));
    let mut out = Vec::new();
    for what in ["replace", "split", "paragraph props", "run props"] {
        let mut tx = s.begin();
        let k = keys(&tx)[i].clone();
        let mut c = Compiler::new(&mut tx, track.clone()).unwrap();
        let got = match what {
            "replace" => c.replace_text(&k, 1..3, "ay"),
            "split" => c.split_block(&k, 1),
            "paragraph props" => c.set_props(&PropTarget::Paragraph { block: k }, &centred()),
            _ => c.set_props(
                &PropTarget::Run {
                    block: k,
                    range: 0..2,
                },
                &PropEdit {
                    set: vec![PropElement {
                        ns: W.to_owned(),
                        local: "b".to_owned(),
                        attrs: Vec::new(),
                        children: Vec::new(),
                    }],
                    clear: Vec::new(),
                },
            ),
        };
        drop(c);
        if what == "replace" && got.is_ok() {
            let texts = texts(&tx, Mode::Final);
            assert!(texts.iter().any(|t| t.ends_with("ay")), "{texts:?}");
        }
        out.push((what.to_owned(), got.err().map(|e| e.code())));
    }
    out
}

#[test]
fn a_paragraph_moved_away_or_deleted_whole_is_not_edited() {
    for track in [
        Track::Tracked(stamp()),
        Track::Untracked,
        Track::UntrackedBy("Clauseo".to_owned()),
    ] {
        for i in [1, 3] {
            for (what, code) in edits(&track, i, false)
                .into_iter()
                .chain(edits(&track, i, true))
            {
                assert_eq!(
                    code,
                    Some(ErrorCode::ReadonlyRegion),
                    "{what} of block {i}, {track:?}"
                );
            }
        }
    }
}

#[test]
fn a_paragraph_moved_here_or_inserted_whole_is_edited() {
    // Tracked, whoever wrote the wrapper. Untracked, only the author's own insertion, by that author: an untracked edit
    // inside any pending move, or inside another author's change, is refused for that reason, wherever it is.
    let cases = [
        (Track::Tracked(stamp()), 2, false),
        (Track::Tracked(stamp()), 2, true),
        (Track::Tracked(stamp()), 4, false),
        (Track::Tracked(stamp()), 4, true),
        (Track::UntrackedBy("Clauseo".to_owned()), 4, true),
    ];
    for (track, i, own) in cases {
        for (what, code) in edits(&track, i, own) {
            assert_eq!(code, None, "{what} of block {i}, {track:?}, own {own}");
        }
    }
}
