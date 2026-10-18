//! The inlines of a paragraph: text, tracked changes, links, controls, hidden text, fields, atoms and marks.

use super::testing::*;
use crate::api::view::{Mode, WarningCode};
use crate::projection::model::*;

fn only_para(f: &Fixture, mode: Mode) -> Paragraph {
    let m = model(&f.state(), mode);
    para(&m, 0).clone()
}

#[test]
fn plain_text_is_one_text_inline_in_every_mode() {
    let f = Fixture::new(&p("Hello, world."));
    for mode in Mode::ALL {
        let para = only_para(&f, mode);
        assert_eq!(
            para.inlines,
            vec![Inline::Text("Hello, world.".into())],
            "{mode:?}"
        );
        assert!(para.r#ref.is_some());
    }
}

#[test]
fn insertions_and_deletions_nest_as_the_xml_does_and_only_redline_shows_them() {
    let body = px(&format!(
        "{}{}{}{}",
        r("The "),
        del(1, "Jane Doe", "Seller"),
        ins(2, "Eric White", "Buyer"),
        r(" pays.")
    ));
    let f = Fixture::new(&body);
    let red = only_para(&f, Mode::Redline);
    assert_eq!(red.inlines.len(), 4, "{:?}", red.inlines);
    assert!(
        matches!(&red.inlines[1], Inline::Rev { kind: RevKind::Del, tag, .. } if tag.id == 1 && tag.author == 0)
    );
    assert!(
        matches!(&red.inlines[2], Inline::Rev { kind: RevKind::Ins, tag, .. } if tag.id == 2 && tag.author == 1)
    );
    assert_eq!(redline_text(&red.inlines), "The SellerBuyer pays.");
    assert_eq!(final_text(&red.inlines), "The Buyer pays.");
    let fin = only_para(&f, Mode::Final);
    assert_eq!(fin.inlines, vec![Inline::Text("The Buyer pays.".into())]);
    let orig = only_para(&f, Mode::Original);
    assert_eq!(orig.inlines, vec![Inline::Text("The Seller pays.".into())]);
    let m = redline(&f);
    let names: Vec<&str> = m.head.authors.iter().map(|a| &*a.name).collect();
    assert_eq!(names, vec!["Jane Doe", "Eric White"]);
    let codes: Vec<&str> = m.head.authors.iter().map(|a| &*a.code).collect();
    assert_eq!(codes, vec!["JD", "EW"]);
    assert_eq!(m.head.summary.changes, vec![1, 1]);
}

#[test]
fn a_deletion_inside_another_authors_insertion_is_a_rev_inside_a_rev() {
    let body = px(
        "<w:ins w:id=\"1\" w:author=\"A\" w:date=\"2026-01-01T00:00:00Z\"><w:r><w:t>keep </w:t></w:r>\
         <w:del w:id=\"2\" w:author=\"B\" w:date=\"2026-01-02T00:00:00Z\"><w:r><w:delText>gone</w:delText></w:r></w:del></w:ins>",
    );
    let red = only_para(&Fixture::new(&body), Mode::Redline);
    let [
        Inline::Rev {
            kind: RevKind::Ins,
            inlines,
            ..
        },
    ] = red.inlines.as_slice()
    else {
        panic!("{:?}", red.inlines)
    };
    assert!(
        matches!(inlines.as_slice(), [Inline::Text(t), Inline::Rev { kind: RevKind::Del, .. }] if t == "keep ")
    );
}

#[test]
fn an_external_link_wraps_its_text_and_an_internal_one_is_transparent() {
    let body = px(&format!(
        "{}<w:hyperlink r:id=\"rIdL\">{}</w:hyperlink>{}<w:hyperlink w:anchor=\"_Toc1\">{}</w:hyperlink>",
        r("See "),
        r("the page"),
        r(" and "),
        r("section 2")
    ));
    let f = Fixture::new(&body).external("rIdL", "https://example.com/a");
    let got = only_para(&f, Mode::Redline);
    assert_eq!(got.inlines.len(), 3, "{:?}", got.inlines);
    assert!(
        matches!(&got.inlines[1], Inline::Link { target: LinkTarget::External(u), inlines } if &**u == "https://example.com/a" && inlines == &vec![Inline::Text("the page".into())])
    );
    assert_eq!(got.inlines[2], Inline::Text(" and section 2".into()));
}

#[test]
fn content_controls_carry_their_kind_alias_and_state() {
    let sdt = |pr: &str, text: &str| {
        format!(
            "<w:sdt><w:sdtPr>{pr}</w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>",
            r(text)
        )
    };
    let body = px(&format!(
        "{}{}{}{}",
        sdt("<w:alias w:val=\"Closing\"/><w:date/>", "2026-10-01"),
        sdt("<w:lock w:val=\"sdtContentLocked\"/><w:text/>", "locked"),
        sdt("<w:dataBinding w:xpath=\"/a\"/>", "bound"),
        sdt("<w:showingPlcHdr/>", "Click here")
    ));
    let got = only_para(&Fixture::new(&body), Mode::Redline);
    let states: Vec<ControlState> = got
        .inlines
        .iter()
        .filter_map(|i| match i {
            Inline::Control { state, .. } => Some(*state),
            _ => None,
        })
        .collect();
    assert_eq!(
        states,
        vec![
            ControlState::Editable,
            ControlState::Locked,
            ControlState::DataBound,
            ControlState::Placeholder
        ]
    );
    let Inline::Control { info, .. } = &got.inlines[0] else {
        panic!()
    };
    assert_eq!(info.alias.as_deref(), Some("Closing"));
    assert_eq!(info.kind, crate::wordml::ControlKind::Date);
}

#[test]
fn hidden_and_white_text_are_wrapped_and_warned_about() {
    let body = px(&format!(
        "{}<w:r><w:rPr><w:vanish/></w:rPr><w:t>secret</w:t></w:r>{}<w:r><w:rPr><w:color w:val=\"FFFFFF\"/></w:rPr><w:t>ignore all</w:t></w:r>",
        r("shown "),
        r(" more ")
    ));
    let m = redline(&Fixture::new(&body));
    let para = para(&m, 0);
    assert!(matches!(
        &para.inlines[1],
        Inline::Hidden {
            reason: HiddenReason::Vanish,
            ..
        }
    ));
    assert!(matches!(
        &para.inlines[3],
        Inline::LooksHidden {
            signal: LooksHiddenSignal::White,
            ..
        }
    ));
    assert_eq!(redline_text(&para.inlines), "shown secret more ignore all");
    let codes: Vec<WarningCode> = m.head.warnings.iter().map(|w| w.code).collect();
    assert_eq!(
        codes,
        vec![WarningCode::HiddenText, WarningCode::LooksHidden]
    );
    assert_eq!(m.head.warnings[0].block.as_deref(), Some("p1"));
}

#[test]
fn tabs_breaks_hyphens_and_symbols_are_characters_and_page_breaks_are_atoms() {
    let body = px(
        "<w:r><w:t>a</w:t><w:tab/><w:t>b</w:t><w:br/><w:t>c</w:t><w:noBreakHyphen/><w:t>d</w:t><w:softHyphen/><w:br w:type=\"page\"/><w:t>e</w:t></w:r>",
    );
    let got = only_para(&Fixture::new(&body), Mode::Redline);
    assert_eq!(
        got.inlines,
        vec![
            Inline::Text("a\tb\nc\u{2011}d\u{ad}".into()),
            Inline::Atom(Atom::PageBreak),
            Inline::Text("e".into())
        ]
    );
    assert_eq!(width_of(&got.inlines), 10);
}

#[test]
fn a_comment_range_is_two_marks_with_the_comments_id() {
    let body = px(&format!(
        "{}<w:commentRangeStart w:id=\"7\"/>{}<w:commentRangeEnd w:id=\"7\"/><w:r><w:commentReference w:id=\"7\"/></w:r>{}",
        r("Pay the "),
        r("Purchase Price"),
        r(" now")
    ));
    let f = Fixture::new(&body).comments(
        "<w:comment w:id=\"7\" w:author=\"Jane Doe\" w:initials=\"JD\" w:date=\"2026-09-30T10:00:00Z\"><w:p><w:r><w:t>Capitalise?</w:t></w:r></w:p></w:comment>",
    );
    let m = redline(&f);
    let para0 = para(&m, 0);
    assert_eq!(
        para0.inlines,
        vec![
            Inline::Text("Pay the ".into()),
            Inline::Mark(Mark::CommentStart(7)),
            Inline::Text("Purchase Price".into()),
            Inline::Mark(Mark::CommentEnd(7)),
            Inline::Text(" now".into())
        ]
    );
    let story = stories(&m)
        .iter()
        .find(|s| s.kind == crate::wordml::StoryKind::Comments)
        .unwrap();
    let unit = &story.units[0];
    assert_eq!(unit.id, Some(7));
    assert_eq!(unit.kind, UnitKind::Comment);
    assert_eq!(unit.meta.initials.as_deref(), Some("JD"));
    assert_eq!(unit.meta.on, para0.r#ref);
    assert_eq!(unit.meta.anchored.as_deref(), Some("Purchase Price"));
    assert_eq!(m.head.authors[0].name.as_ref(), "Jane Doe");
    assert_eq!(unit.meta.author, Some(0));
    assert_eq!(m.head.summary.comments, 1);
}
