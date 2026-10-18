//! Tests of `insert_block_in`: a run of insertions, each after the paragraph the one before made, writes exactly what the
//! same insertions write one by one with `insert_block`, which looks the neighbours up in the story every time.

use super::compiler::Compiler;
use super::stamp::Track;
use super::tests_support::*;
use super::types::{InsertRun, NewParagraph, Place};
use crate::wordml::BlockKey;

fn spec(text: &str, style: Option<&str>) -> NewParagraph {
    NewParagraph {
        text: text.to_owned(),
        style: style.map(str::to_owned),
        ..NewParagraph::default()
    }
}

/// Bodies with each kind of neighbour: paragraphs with properties and a pending mark of another author, a table, a
/// content control around the last block, a move that holds whole paragraphs, and a paragraph that ends inside a field.
fn bodies() -> Vec<String> {
    let table = "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"2000\"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>";
    vec![
        format!(
            "<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:r><w:t>One</w:t></w:r></w:p>{}{}",
            para("Two"),
            para("Three")
        ),
        format!("{}{table}{}", para("Before"), para("After")),
        format!("{table}{}", para("Only")),
        format!(
            "{}<w:sdt><w:sdtPr/><w:sdtContent>{}</w:sdtContent></w:sdt>",
            para("Outside"),
            para("Inside")
        ),
        format!(
            "{}<w:moveFrom {}>{}{}</w:moveFrom>{}",
            para("Before"),
            eric(901),
            para("Moved"),
            para("Also moved"),
            para("After")
        ),
        format!(
            "<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr><w:r><w:t>Theirs</w:t></w:r></w:p>{}",
            eric(900),
            para("Next")
        ),
        format!(
            "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText>REF x</w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>a</w:t></w:r></w:p><w:p><w:r><w:t>b</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>{}",
            para("Last")
        ),
    ]
}

/// What a run of `specs` at `anchor` writes, through `insert_block_in` with one run or through `insert_block`: the
/// body, the effects, or the first error.
fn write(body: &str, track: &Track, anchor: usize, place: Place, carried: bool) -> String {
    let s = state(body);
    let mut tx = s.begin();
    let ks = keys(&tx);
    let Some(first) = ks.get(anchor).cloned() else {
        return String::new();
    };
    let specs = [
        spec("first", None),
        spec("", None),
        spec("third \"quoted\"", Some("Heading1")),
        spec("fourth", None),
    ];
    let mut c = Compiler::new(&mut tx, track.clone()).unwrap();
    let mut run = InsertRun::default();
    let mut at: BlockKey = first;
    let mut place = place;
    let mut out = String::new();
    for p in &specs {
        let got = if carried {
            c.insert_block_in(&mut run, &at, place, p)
        } else {
            c.insert_block(&at, place, p)
        };
        match got {
            Ok(effect) => {
                out.push_str(&format!("{effect:?}\n"));
                at = effect.new_blocks.first().cloned().unwrap();
                place = Place::After;
            }
            Err(e) => {
                out.push_str(&format!("error {}: {}\n", e.code().as_str(), e.message()));
                break;
            }
        }
    }
    drop(c);
    out.push_str(&body_xml(&tx));
    out
}

#[test]
fn a_run_of_insertions_writes_what_the_insertions_write_one_by_one() {
    let tracks = [Track::Tracked(stamp()), Track::Untracked];
    let mut compared = 0;
    for body in bodies() {
        let n = keys(&state(&body).begin()).len();
        for track in &tracks {
            for anchor in 0..n {
                for place in [Place::Before, Place::After] {
                    let one_by_one = write(&body, track, anchor, place, false);
                    let carried = write(&body, track, anchor, place, true);
                    assert_eq!(
                        carried, one_by_one,
                        "body {body}, anchor {anchor}, {place:?}, {track:?}"
                    );
                    compared += 1;
                }
            }
        }
    }
    assert!(compared > 40, "{compared}");
}
