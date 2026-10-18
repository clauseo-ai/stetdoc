//! Tests of what a new paragraph copies from the paragraph it is modelled on: not the hidden formatting of
//! the mark, and `w14` ids only in the Word 2010 namespace.

use super::stamp::Track;
use super::tests_support::*;
use super::types::{NewParagraph, Place};

fn new_para(text: &str) -> NewParagraph {
    NewParagraph {
        text: text.to_owned(),
        ..NewParagraph::default()
    }
}

#[test]
fn a_line_added_after_a_paragraph_with_a_hidden_mark_is_not_hidden() {
    // The mark is hidden (Word hides the paragraph mark to join a heading to the next paragraph), its text is
    // not. The new run copies the mark's formatting, but neither `w:vanish` nor `w:specVanish`.
    for hidden in [
        "<w:vanish/>",
        "<w:specVanish/>",
        "<w:vanish/><w:specVanish/>",
    ] {
        let body = format!(
            "<w:p><w:pPr><w:rPr><w:b/>{hidden}</w:rPr></w:pPr>{}</w:p><w:p>{}</w:p>",
            r("one"),
            r("two")
        );
        for (anchor, place) in [(0, Place::After), (1, Place::After), (0, Place::Before)] {
            for track in [Track::Tracked(stamp()), Track::Untracked] {
                let s = with_compiler(&body, track.clone(), |c, _, ks| {
                    vec![c.insert_block(&ks[anchor], place, &new_para("New"))]
                });
                assert!(s.effects[0].is_ok(), "{:?}", s.effects);
                assert!(s.outcome.is_ok(), "{:?}", s.outcome);
                assert!(s.schema.is_empty(), "{:?}", s.schema);
                let at = s.xml.find(">New<").unwrap();
                let run = &s.xml[s.xml[..at].rfind("<w:r>").unwrap()..at];
                assert!(
                    !run.contains("anish"),
                    "{hidden} {anchor}: {run}\n{}",
                    s.xml
                );
                if anchor == 0 {
                    assert!(run.contains("<w:b/>"), "{hidden}: {run}");
                }
                // The marks keep what they had.
                assert_eq!(
                    s.xml.matches(hidden).count(),
                    1 + usize::from(anchor == 0),
                    "{}",
                    s.xml
                );
            }
        }
    }
}

#[test]
fn a_hidden_mark_with_nothing_else_gives_a_run_with_no_properties() {
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:vanish/></w:rPr></w:pPr>{}</w:p><w:p>{}</w:p>",
        r("one"),
        r("two")
    );
    let s = with_compiler(&body, Track::Untracked, |c, _, ks| {
        vec![c.insert_block(&ks[0], Place::After, &new_para("New"))]
    });
    assert!(s.effects[0].is_ok(), "{:?}", s.effects);
    assert!(s.xml.contains("<w:r><w:t>New</w:t></w:r>"), "{}", s.xml);
}

#[test]
fn text_put_into_an_empty_paragraph_with_a_hidden_mark_is_not_hidden() {
    // With no text to copy from, `replace_text` takes the mark's formatting, as `insert_block` does, without
    // `w:vanish` or `w:specVanish`.
    for hidden in [
        "<w:vanish/>",
        "<w:specVanish/>",
        "<w:vanish/><w:specVanish/>",
    ] {
        let body = format!(
            "<w:p><w:pPr><w:rPr><w:b/>{hidden}</w:rPr></w:pPr></w:p><w:p>{}</w:p>",
            r("two")
        );
        for track in [Track::Tracked(stamp()), Track::Untracked] {
            let s = with_compiler(&body, track.clone(), |c, _, ks| {
                vec![c.replace_text(&ks[0], 0..0, "New")]
            });
            assert!(s.effects[0].is_ok(), "{:?}", s.effects);
            assert!(s.outcome.is_ok(), "{:?}", s.outcome);
            assert!(s.schema.is_empty(), "{:?}", s.schema);
            let at = s.xml.find(">New<").unwrap();
            let run = &s.xml[s.xml[..at].rfind("<w:r>").unwrap()..at];
            assert!(!run.contains("anish"), "{hidden}: {run}\n{}", s.xml);
            assert!(run.contains("<w:b/>"), "{hidden}: {run}");
            assert_eq!(s.xml.matches(hidden).count(), 1, "{}", s.xml);
        }
    }
}

#[test]
fn text_modelled_on_the_mark_of_an_equation_paragraph_does_not_take_its_math_property() {
    // tdf171433: an empty paragraph after an equation has `w:oMath` in its mark, which has no place in a run's
    // properties. New text in it, and a paragraph added after it, copy the rest of the mark and not that.
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:sz w:val=\"23\"/><w:oMath/></w:rPr></w:pPr></w:p><w:p>{}</w:p>",
        r("two")
    );
    for call in 0..2 {
        for track in [Track::Tracked(stamp()), Track::Untracked] {
            let s = with_compiler(&body, track.clone(), |c, _, ks| {
                vec![match call {
                    0 => c.replace_text(&ks[0], 0..0, "New"),
                    _ => c.insert_block(&ks[0], Place::After, &new_para("New")),
                }]
            });
            assert!(s.effects[0].is_ok(), "{:?}", s.effects);
            assert!(s.outcome.is_ok(), "{:?}", s.outcome);
            assert!(
                s.schema.is_empty(),
                "call {call}: {:?}\n{}",
                s.schema,
                s.xml
            );
            let at = s.xml.find(">New<").unwrap();
            let run = &s.xml[s.xml[..at].rfind("<w:r>").unwrap()..at];
            assert!(!run.contains("oMath"), "call {call}: {run}");
            assert!(run.contains("<w:sz w:val=\"23\"/>"), "call {call}: {run}");
            assert!(s.xml.contains("<w:oMath/></w:rPr></w:pPr>"), "{}", s.xml);
        }
    }
}

const BETA: &str = "http://schemas.microsoft.com/office/word/2007/5/30/wordml";

#[test]
fn a_model_with_a_beta_para_id_gives_the_new_paragraph_no_w14_ids() {
    // complex1_NOR: the paragraphs carry `paraId` in the beta namespace of Word 2007, where `textId` does not
    // exist. A new paragraph gets neither id, in that namespace or in Word 2010's.
    let body = format!(
        "<w:p xmlns:w14b=\"{BETA}\" w14b:paraId=\"71E7E559\">{}</w:p><w:p xmlns:w14b=\"{BETA}\" w14b:paraId=\"0C4246F0\">{}</w:p>",
        r("one"),
        r("two")
    );
    for (anchor, place) in [(0, Place::After), (1, Place::After)] {
        let s = with_compiler(&body, Track::Tracked(stamp()), |c, _, ks| {
            vec![
                c.insert_block(&ks[anchor], place, &new_para("New")),
                c.split_block(&ks[0], 1),
            ]
        });
        assert!(s.effects.iter().all(Result::is_ok), "{:?}", s.effects);
        assert!(s.outcome.is_ok(), "{:?}", s.outcome);
        assert!(s.schema.is_empty(), "{:?}", s.schema);
        assert!(!s.xml.contains("textId"), "{}", s.xml);
        assert_eq!(s.xml.matches("paraId").count(), 2, "{}", s.xml);
    }
}

#[test]
fn a_model_with_a_word_2010_para_id_gives_the_new_paragraph_both_ids() {
    let body = format!(
        "<w:p w14:paraId=\"11111111\" w14:textId=\"77777777\">{}</w:p><w:p w14:paraId=\"22222222\" w14:textId=\"77777777\">{}</w:p>",
        r("one"),
        r("two")
    );
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, _, ks| {
        vec![c.insert_block(&ks[1], Place::After, &new_para("New"))]
    });
    assert!(s.effects[0].is_ok(), "{:?}", s.effects);
    assert_eq!(s.xml.matches("w14:paraId=").count(), 3, "{}", s.xml);
    assert_eq!(
        s.xml.matches("w14:textId=\"77777777\"").count(),
        3,
        "{}",
        s.xml
    );
}
