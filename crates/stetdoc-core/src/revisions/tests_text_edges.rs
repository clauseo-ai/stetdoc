//! Tests of `replace_text` at the edges of what it may touch: a comment reference at the edge of a range, a
//! violation that an element carries through a rename, a run that holds a text box, and an equation that holds
//! a deletion.

use super::tests_support::*;
use crate::wordml::Mode;

// --- a comment reference at the edge of a range

const REF9: &str = "<w:r><w:rPr><w:rStyle w:val=\"CommentReference\"/></w:rPr><w:commentReference w:id=\"9\"/></w:r>";

#[test]
fn a_comment_reference_at_the_edge_of_a_range_is_not_covered_by_a_replace_of_the_whole_paragraph() {
    // The edge rule: a range covers the items that have characters in it, and a comment reference has
    // none, so a replacement over all the text of a paragraph leaves the reference, and the range markers
    // around it, where they are, and the comment still has its anchor. Tracked and untracked alike.
    for tracked in [true, false] {
        let body = p(&format!(
            "{REF9}<w:commentRangeStart w:id=\"9\"/>{}<w:commentRangeEnd w:id=\"9\"/>{REF9}",
            r("hello world")
        ));
        let track = if tracked {
            super::stamp::Track::Tracked(stamp())
        } else {
            super::stamp::Track::Untracked
        };
        let s = with_compiler(&body, track, |c, _, ks| {
            vec![c.replace_text(&ks[0], 0..11, "HELLO")]
        });
        if tracked {
            s.assert_clean();
        } else {
            assert!(s.schema.is_empty(), "{:?}", s.schema);
        }
        assert_eq!(s.fin, ["HELLO"], "tracked: {tracked}");
        assert_eq!(s.xml.matches("w:commentReference").count(), 2, "{}", s.xml);
        assert_eq!(s.xml.matches("w:commentRangeStart").count(), 1, "{}", s.xml);
        assert_eq!(s.xml.matches("w:commentRangeEnd").count(), 1, "{}", s.xml);
        // No reference is inside a deletion, and the markers keep their order around the new text.
        for part in s.xml.split("<w:del ").skip(1) {
            let deleted = part.split("</w:del>").next().unwrap_or_default();
            assert!(!deleted.contains("commentReference"), "{}", s.xml);
        }
        let start = s.xml.find("commentRangeStart").unwrap();
        let end = s.xml.find("commentRangeEnd").unwrap();
        assert!(start < end, "{}", s.xml);
    }
}

#[test]
fn a_violation_that_an_element_carries_through_a_rename_keeps_its_key() {
    // python-docx writes `w:space` where `xml:space` belongs. The violation is in the document already, and a
    // deletion that renames the `w:t` to `w:delText` (and a rejection that renames it back) does not add one.
    let body = "<w:p><w:r><w:t w:space=\"preserve\">kept words and gone words</w:t></w:r></w:p>";
    let s = state(body);
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    let mut c =
        super::compiler::Compiler::new(&mut tx, super::stamp::Track::Tracked(stamp())).unwrap();
    c.replace_text(&ks[0], 10..14, "").unwrap();
    c.finish().unwrap();
    assert!(body_xml(&tx).contains("w:delText"), "{}", body_xml(&tx));
    assert_eq!(schema_added(&s, &tx), Vec::<String>::new());
    // And back: rejecting the deletion renames the text back to `w:t`.
    super::resolve::resolve(
        &mut tx,
        &mut super::index::ChangeIndex::new(),
        &[super::types::Step {
            action: super::types::Action::Reject,
            selector: super::types::Selector::All,
        }],
    )
    .unwrap();
    assert!(!body_xml(&tx).contains("w:delText"), "{}", body_xml(&tx));
    assert_eq!(schema_added(&s, &tx), Vec::<String>::new());
}

#[test]
fn deleting_a_run_that_holds_a_text_box_leaves_the_text_of_the_box_alone_in_either_namespace() {
    // The text of a box is the text of another story: it stays `w:t` inside the deletion, and the rules that
    // read `w:t` under `w:del` do not look into it. A Strict document writes the box as `wne:txbxContent`
    // (strict-smartart), where a Transitional one writes `w:txbxContent`.
    for tag in [
        "w:txbxContent",
        "wne:txbxContent xmlns:wne=\"http://schemas.microsoft.com/office/word/2006/wordml\"",
    ] {
        let name = tag.split(' ').next().unwrap();
        let body = format!(
            "<w:p>{}<w:r><w:drawing><wp:inline xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\"><wp:txbx><{tag}><w:p><w:r><w:t>inner</w:t></w:r></w:p></{name}></wp:txbx></wp:inline></w:drawing></w:r></w:p>",
            r("one ")
        );
        let len = state_texts(&state(&body), Mode::Final)[0].chars().count();
        let s = with_compiler(&body, super::stamp::Track::Tracked(stamp()), |c, _, ks| {
            vec![c.replace_text(&ks[0], 0..len, "")]
        });
        assert!(s.effects[0].is_ok(), "{tag}: {:?}", s.effects[0]);
        assert!(s.outcome.is_ok(), "{tag}: {:?}", s.outcome.as_ref().err());
        assert!(s.xml.contains("<w:t>inner</w:t>"), "{tag}: {}", s.xml);
        assert!(s.xml.contains("<w:delText"), "{tag}: {}", s.xml);
    }
}

// --- an equation that holds a deletion

/// RP013-Deleted-Math-Control-Char: Word deleted one math run with a `w:del` inside its `m:r`.
fn equation_with_a_deletion() -> String {
    let rf = "<w:rPr><w:rFonts w:ascii=\"Cambria Math\" w:hAnsi=\"Cambria Math\"/></w:rPr>";
    format!(
        "<w:p><m:oMathPara><m:oMath><m:r>{rf}<m:t>A=</m:t></m:r><m:r><w:del w:id=\"0\" w:author=\"Eric White\" w:date=\"2017-03-24T22:53:00Z\">{rf}<m:t>2</m:t></w:del></m:r><w:bookmarkStart w:id=\"1\" w:name=\"_GoBack\"/><w:bookmarkEnd w:id=\"1\"/><m:r>{rf}<m:t>x</m:t></m:r></m:oMath></m:oMathPara>{}</w:p>",
        r(" after")
    )
}

#[test]
fn a_tracked_deletion_of_an_equation_that_holds_a_deletion_is_refused_and_writes_no_nested_deletion()
 {
    use crate::api::error::ErrorCode;
    // Replacing the whole paragraph would put the equation in a `w:del`, around the deletion inside it.
    let s = tracked_edits(&equation_with_a_deletion(), &[(0, 0..7, "new")]);
    assert_eq!(s.code(0), Some(ErrorCode::ReadonlyRegion), "{}", s.xml);
    assert!(s.outcome.is_ok(), "{:?}", s.outcome);
    assert!(!s.xml.contains("Clauseo"), "{}", s.xml);
    // Text beside the equation is still edited, and untracked the equation is removed.
    let s = tracked_edits(&equation_with_a_deletion(), &[(0, 2..7, "fter")]);
    s.assert_clean();
    let s = untracked_edits(&equation_with_a_deletion(), &[(0, 0..7, "new")]);
    assert!(s.effects[0].is_ok(), "{:?}", s.effects);
    assert!(s.outcome.is_ok(), "{:?}", s.outcome);
    assert_eq!(s.fin, ["new"], "{}", s.xml);
}

// --- a w:t that holds several text nodes and CDATA sections

#[test]
fn a_cut_in_a_text_element_with_several_text_nodes_keeps_the_order_and_reject_restores_it() {
    // `a]]&gt;b<![CDATA[c]]>d` is one w:t holding three nodes: a cut must move every node after it, and a cut at the
    // edge between two nodes must cut the w:t, not the run after it.
    let body = "<w:p><w:r><w:t>a]]&gt;b<![CDATA[c]]>d</w:t></w:r></w:p>";
    let text = "a]]>bcd";
    let n = text.chars().count();
    for at in 1..n {
        let s = with_compiler(body, super::stamp::Track::Tracked(stamp()), |c, _, ks| {
            vec![c.split_block(&ks[0], at)]
        });
        s.assert_clean();
        let (l, r): (String, String) = (
            text.chars().take(at).collect(),
            text.chars().skip(at).collect(),
        );
        assert_eq!(s.fin, [l, r], "split at {at}: {}", s.xml);
        let j = super::tests_resolve::run(&s.xml, &[super::tests_resolve::reject_all()]);
        assert_eq!(j.fin.concat(), text, "reject of a split at {at}: {}", j.xml);
    }
    for start in 0..n {
        for end in start..=n {
            let s = with_compiler(body, super::stamp::Track::Tracked(stamp()), |c, _, ks| {
                vec![c.replace_text(&ks[0], start..end, "x")]
            });
            // Only a cut inside the CDATA section is refused.
            if let Err(e) = &s.effects[0] {
                assert_eq!(
                    e.code(),
                    crate::api::error::ErrorCode::Unsupported,
                    "replace {start}..{end}: {e}"
                );
                continue;
            }
            s.assert_clean();
            let want: String = text
                .chars()
                .take(start)
                .chain("x".chars())
                .chain(text.chars().skip(end))
                .collect();
            assert_eq!(s.fin, [want], "replace {start}..{end}: {}", s.xml);
            assert_eq!(s.orig, [text], "replace {start}..{end}: {}", s.xml);
            let j = super::tests_resolve::run(&s.xml, &[super::tests_resolve::reject_all()]);
            assert_eq!(
                j.fin.concat(),
                text,
                "reject of a replace at {start}..{end}: {}",
                j.xml
            );
        }
    }
}

// --- a defect of the input that a new revision wraps

#[test]
fn an_empty_rpr_of_the_input_that_a_new_revision_wraps_is_carried() {
    // Word leaves an empty `w:rPr` on runs. A tracked replacement wraps the run in a deletion and copies its
    // properties onto the inserted run; `finish` finds the empty `w:rPr` under a created revision and carries it,
    // because the same unit had it before, without reading the rest of the part (revisions_compile_50).
    let body = format!(
        "<w:p><w:r><w:rPr/><w:t>hello world</w:t></w:r></w:p>{}",
        p(&r("another paragraph"))
    );
    for edit in [(0, 0..5, "HELLO"), (0, 6..11, ""), (1, 0..7, "a")] {
        let s = tracked_edits(&body, &[edit]);
        s.assert_clean();
    }
}
