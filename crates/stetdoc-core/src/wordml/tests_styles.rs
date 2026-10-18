//! Tests for `StyleDefs`. Each test names what it proves.

use super::names::{W_S, W_T};
use super::styles_defs::{StyleDefs, StyleIssue, StyleKind, TableRegion};
use crate::api::limits::Limits;
use crate::xml::{Bytes, SpanTree};

fn tree(xml: &str) -> SpanTree {
    SpanTree::parse(
        "styles.xml",
        Bytes::from(xml.as_bytes()),
        &Limits::default(),
    )
    .unwrap()
}

fn read_in(w: &str, body: &str) -> StyleDefs {
    StyleDefs::read(&tree(&format!(
        "<w:styles xmlns:w=\"{w}\" xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\">{body}</w:styles>"
    )))
}

fn read(body: &str) -> StyleDefs {
    read_in(W_T, body)
}

fn style(id: &str, based_on: Option<&str>) -> String {
    let b = based_on
        .map(|b| format!("<w:basedOn w:val=\"{b}\"/>"))
        .unwrap_or_default();
    format!(
        "<w:style w:type=\"paragraph\" w:styleId=\"{id}\"><w:name w:val=\"{id}\"/>{b}</w:style>"
    )
}

fn ids<'a>(it: impl Iterator<Item = &'a super::styles_defs::StyleDef>) -> Vec<&'a str> {
    it.map(|s| &*s.id).collect()
}

#[test]
fn a_style_is_read_in_full() {
    // Proves every scalar field of `StyleDef` and the five containers.
    for w in [W_T, W_S] {
        let d = read_in(
            w,
            "<w:style w:type=\"table\" w:styleId=\"T1\" w:default=\"1\" w:customStyle=\"1\">\
             <w:name w:val=\"My Table\"/><w:aliases w:val=\"a1, a2 ,,a3\"/><w:basedOn w:val=\"TableNormal\"/>\
             <w:next w:val=\"Normal\"/><w:link w:val=\"Normal\"/><w:uiPriority w:val=\"39\"/>\
             <w:semiHidden/><w:hidden w:val=\"0\"/><w:qFormat/>\
             <w:pPr><w:spacing w:after=\"0\"/></w:pPr><w:rPr><w:b/></w:rPr>\
             <w:tblPr><w:tblW w:w=\"0\"/></w:tblPr><w:trPr><w:cantSplit/></w:trPr>\
             <w:tcPr><w:vAlign w:val=\"top\"/></w:tcPr></w:style>\
             <w:style w:styleId=\"TableNormal\" w:type=\"table\"/><w:style w:styleId=\"Normal\"/>",
        );
        let s = d.get("T1").unwrap();
        assert_eq!(s.kind, StyleKind::Table);
        assert_eq!(&*s.name, "My Table");
        assert_eq!(s.based_on.as_deref(), Some("TableNormal"));
        assert_eq!(s.next.as_deref(), Some("Normal"));
        assert_eq!(s.link.as_deref(), Some("Normal"));
        assert!(s.default && s.custom && s.semi_hidden && s.q_format);
        assert!(!s.hidden);
        assert_eq!(s.ui_priority, Some(39));
        assert_eq!(
            s.aliases,
            ["a1".into(), "a2".into(), "a3".into()] as [Box<str>; 3]
        );
        assert_eq!(s.paragraph.attr("spacing", "after"), Some("0"));
        assert_eq!(s.run.on_off("b"), Some(true));
        assert_eq!(s.table.attr("tblW", "w"), Some("0"));
        assert_eq!(s.row.on_off("cantSplit"), Some(true));
        assert_eq!(s.cell.val("vAlign"), Some("top"));
        assert_eq!(d.get("Normal").unwrap().kind, StyleKind::Paragraph);
        assert!(d.issues().is_empty(), "{:?}", d.issues());
    }
}

#[test]
fn the_four_kinds_and_a_style_without_a_type() {
    // Proves the `w:type` values, and that a missing type means a paragraph style.
    let d = read(
        "<w:style w:type=\"character\" w:styleId=\"C\"/><w:style w:type=\"numbering\" w:styleId=\"N\"/>\
         <w:style w:type=\"table\" w:styleId=\"T\"/><w:style w:styleId=\"P\"/>",
    );
    assert_eq!(d.get("C").unwrap().kind, StyleKind::Character);
    assert_eq!(d.get("N").unwrap().kind, StyleKind::Numbering);
    assert_eq!(d.get("T").unwrap().kind, StyleKind::Table);
    assert_eq!(d.get("P").unwrap().kind, StyleKind::Paragraph);
    assert_eq!(d.styles().len(), 4);
}

#[test]
fn an_unknown_type_is_a_paragraph_style_with_an_issue() {
    // Proves an unrecognised `w:type` is reported and read as a paragraph style.
    let d = read("<w:style w:type=\"frame\" w:styleId=\"F\"/>");
    assert_eq!(d.get("F").unwrap().kind, StyleKind::Paragraph);
    assert_eq!(
        d.issues(),
        [StyleIssue::UnknownType {
            id: "F".into(),
            value: "frame".into()
        }]
    );
}

#[test]
fn doc_defaults_hold_run_and_paragraph_properties() {
    // Proves `docDefaults` reads `rPrDefault` and `pPrDefault`, and that its absence gives empty ones.
    let d = read(
        "<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val=\"22\"/></w:rPr></w:rPrDefault>\
         <w:pPrDefault><w:pPr><w:spacing w:after=\"160\"/></w:pPr></w:pPrDefault></w:docDefaults>",
    );
    assert_eq!(d.doc_defaults.run.val("sz"), Some("22"));
    assert_eq!(
        d.doc_defaults.paragraph.attr("spacing", "after"),
        Some("160")
    );
    let none = read("");
    assert!(none.doc_defaults.run.is_empty() && none.doc_defaults.paragraph.is_empty());
}

#[test]
fn the_chain_follows_based_on_in_order() {
    // Proves `chain` yields the style then its parents.
    let d = read(&format!(
        "{}{}{}",
        style("C", Some("B")),
        style("B", Some("A")),
        style("A", None)
    ));
    assert_eq!(ids(d.chain("C")), ["C", "B", "A"]);
    assert_eq!(ids(d.chain("A")), ["A"]);
    assert!(d.issues().is_empty());
}

#[test]
fn a_dangling_based_on_ends_the_chain_and_is_reported() {
    // Proves `chain` stops at an id with no style, and the issue names both ends.
    let d = read(&style("B", Some("Gone")));
    assert_eq!(ids(d.chain("B")), ["B"]);
    assert_eq!(ids(d.chain("Nope")), Vec::<&str>::new());
    assert_eq!(
        d.issues(),
        [StyleIssue::BasedOnMissing {
            id: "B".into(),
            target: "Gone".into()
        }]
    );
}

#[test]
fn dangling_next_and_link_are_reported() {
    // Proves `next` and `link` naming no style are issues.
    let d = read("<w:style w:styleId=\"A\"><w:next w:val=\"X\"/><w:link w:val=\"Y\"/></w:style>");
    assert_eq!(
        d.issues(),
        [
            StyleIssue::NextMissing {
                id: "A".into(),
                target: "X".into()
            },
            StyleIssue::LinkMissing {
                id: "A".into(),
                target: "Y".into()
            }
        ]
    );
}

#[test]
fn a_based_on_cycle_ends_the_chain_and_is_reported_once() {
    // Proves cycle safety: the chain terminates, and a two-style cycle is one issue.
    let d = read(&format!(
        "{}{}",
        style("A", Some("B")),
        style("B", Some("A"))
    ));
    assert_eq!(ids(d.chain("A")), ["A", "B"]);
    assert_eq!(d.issues(), [StyleIssue::BasedOnCycle { id: "A".into() }]);
}

#[test]
fn a_style_based_on_itself_is_a_cycle() {
    // Proves the shortest cycle.
    let d = read(&style("A", Some("A")));
    assert_eq!(ids(d.chain("A")), ["A"]);
    assert_eq!(d.issues().len(), 1);
}

#[test]
fn a_style_leading_into_a_cycle_is_not_a_second_issue() {
    // Proves a tail into a cycle is not reported as another cycle.
    let d = read(&format!(
        "{}{}{}",
        style("T", Some("A")),
        style("A", Some("B")),
        style("B", Some("A"))
    ));
    assert_eq!(d.issues().len(), 1);
    assert_eq!(ids(d.chain("T")), ["T", "A", "B"]);
}

#[test]
fn a_chain_stops_after_64_styles() {
    // Proves the bound: a 100-style chain yields 64.
    let body: String = (0..100)
        .map(|i| style(&format!("S{i}"), Some(&format!("S{}", i + 1))))
        .collect();
    let d = read(&body);
    assert_eq!(d.chain("S0").count(), 64);
}

#[test]
fn duplicate_ids_keep_all_and_the_first_wins() {
    // Proves `get` returns the first definition, `styles` keeps both, and the issue is reported.
    let d = read(
        "<w:style w:styleId=\"A\"><w:name w:val=\"first\"/></w:style>\
         <w:style w:styleId=\"A\"><w:name w:val=\"second\"/></w:style>",
    );
    assert_eq!(&*d.get("A").unwrap().name, "first");
    assert_eq!(d.styles().len(), 2);
    assert_eq!(d.issues(), [StyleIssue::DuplicateId { id: "A".into() }]);
    assert!(d.by_name("second").is_none());
    assert_eq!(&*d.by_name("first").unwrap().id, "A");
}

#[test]
fn a_style_without_an_id_is_skipped_and_reported() {
    // Proves the missing-id issue, including an empty id.
    let d = read(
        "<w:style><w:name w:val=\"orphan\"/></w:style><w:style w:styleId=\"\"/><w:style w:styleId=\"K\"/>",
    );
    assert_eq!(d.styles().len(), 1);
    assert_eq!(
        d.issues(),
        [
            StyleIssue::MissingId {
                name: "orphan".into()
            },
            StyleIssue::MissingId { name: "".into() }
        ]
    );
}

#[test]
fn by_name_ignores_case_and_checks_aliases() {
    // Proves `by_name`: lower-case built-in names, aliases, and name before alias.
    let d = read(
        "<w:style w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:aliases w:val=\"Title 1,Kopf\"/></w:style>\
         <w:style w:styleId=\"Other\"><w:name w:val=\"Kopf\"/></w:style>",
    );
    assert_eq!(&*d.by_name("Heading 1").unwrap().id, "Heading1");
    assert_eq!(&*d.by_name("HEADING 1").unwrap().id, "Heading1");
    assert_eq!(&*d.by_name("title 1").unwrap().id, "Heading1");
    assert_eq!(&*d.by_name("kopf").unwrap().id, "Other");
    assert!(d.by_name("Heading1").is_none());
}

#[test]
fn default_for_finds_the_default_of_each_kind() {
    // Proves `default_for` per kind, the first one winning, and `None` when there is none.
    let d = read(
        "<w:style w:styleId=\"N\" w:default=\"1\"/><w:style w:styleId=\"N2\" w:default=\"1\"/>\
         <w:style w:type=\"character\" w:styleId=\"DC\" w:default=\"true\"/>\
         <w:style w:type=\"table\" w:styleId=\"TN\" w:default=\"0\"/>",
    );
    assert_eq!(&*d.default_for(StyleKind::Paragraph).unwrap().id, "N");
    assert_eq!(&*d.default_for(StyleKind::Character).unwrap().id, "DC");
    assert!(d.default_for(StyleKind::Table).is_none());
    assert!(d.default_for(StyleKind::Numbering).is_none());
}

#[test]
fn conditional_table_formats_are_read_with_their_region() {
    // Proves every `w:tblStylePr` region, its five containers, and that an unknown region is reported.
    let regions = [
        ("wholeTable", TableRegion::WholeTable),
        ("firstRow", TableRegion::FirstRow),
        ("lastRow", TableRegion::LastRow),
        ("firstCol", TableRegion::FirstCol),
        ("lastCol", TableRegion::LastCol),
        ("band1Vert", TableRegion::Band1Vert),
        ("band2Vert", TableRegion::Band2Vert),
        ("band1Horz", TableRegion::Band1Horz),
        ("band2Horz", TableRegion::Band2Horz),
        ("neCell", TableRegion::NeCell),
        ("nwCell", TableRegion::NwCell),
        ("seCell", TableRegion::SeCell),
        ("swCell", TableRegion::SwCell),
    ];
    let blocks: String = regions
        .iter()
        .map(|(n, _)| format!("<w:tblStylePr w:type=\"{n}\"><w:rPr><w:b/></w:rPr></w:tblStylePr>"))
        .collect();
    let d = read(&format!(
        "<w:style w:type=\"table\" w:styleId=\"T\">{blocks}\
         <w:tblStylePr w:type=\"firstRow\"><w:pPr><w:jc w:val=\"center\"/></w:pPr>\
         <w:tblPr><w:x/></w:tblPr><w:trPr><w:tblHeader/></w:trPr>\
         <w:tcPr><w:shd w:fill=\"EEE\"/></w:tcPr></w:tblStylePr>\
         <w:tblStylePr w:type=\"bogus\"/><w:tblStylePr/></w:style>"
    ));
    let s = d.get("T").unwrap();
    assert_eq!(s.conditional.len(), 14);
    for (got, (_, want)) in s.conditional.iter().zip(regions) {
        assert_eq!(got.0, want);
        assert_eq!(got.1.run.on_off("b"), Some(true));
    }
    let (r, p) = s.conditional.last().unwrap();
    assert_eq!(*r, TableRegion::FirstRow);
    assert_eq!(p.paragraph.val("jc"), Some("center"));
    assert!(p.table.get("x").is_some());
    assert_eq!(p.row.on_off("tblHeader"), Some(true));
    assert_eq!(p.cell.attr("shd", "fill"), Some("EEE"));
    assert_eq!(d.issues().len(), 2);
    assert!(
        matches!(&d.issues()[0], StyleIssue::UnknownRegion { value, .. } if &**value == "bogus")
    );
}

#[test]
fn a_run_property_of_the_extension_namespace_stays_apart_in_a_style() {
    // Proves the namespaced reading reaches styles (`TextEffects_InStyle` has `w14:shadow` in a style).
    let d = read("<w:style w:styleId=\"E\"><w:rPr><w14:shadow/></w:rPr></w:style>");
    assert_eq!(d.get("E").unwrap().run.on_off("shadow"), None);
    assert_eq!(d.get("E").unwrap().run.iter().count(), 1);
}

#[test]
fn latent_styles_are_skipped() {
    // Proves `w:latentStyles` and its exceptions produce no styles.
    let d = read(
        "<w:latentStyles w:count=\"2\"><w:lsdException w:name=\"Normal\"/>\
         <w:lsdException w:name=\"heading 1\"/></w:latentStyles><w:style w:styleId=\"A\"/>",
    );
    assert_eq!(d.styles().len(), 1);
    assert!(d.by_name("Normal").is_none());
}

#[test]
fn a_root_that_is_not_styles_gives_empty_definitions_and_an_issue() {
    // Proves reading is total for the wrong root, including a `w:styles` in a foreign namespace.
    let d = StyleDefs::read(&tree(&format!(
        "<w:numbering xmlns:w=\"{W_T}\"><w:style w:styleId=\"A\"/></w:numbering>"
    )));
    assert!(d.styles().is_empty());
    assert!(d.get("A").is_none());
    assert_eq!(
        d.issues(),
        [StyleIssue::NotStyles {
            root: "numbering".into()
        }]
    );
    let foreign = StyleDefs::read(&tree("<w:styles xmlns:w=\"urn:other\"/>"));
    assert_eq!(foreign.issues().len(), 1);
}

#[test]
fn an_empty_styles_part_reads_as_empty_without_issues() {
    // Proves the empty part, in both dialects.
    for w in [W_T, W_S] {
        let d = read_in(w, "");
        assert!(d.styles().is_empty() && d.issues().is_empty());
        assert!(d.chain("A").next().is_none());
    }
}

#[test]
fn a_ui_priority_out_of_range_is_ignored() {
    // Proves a number that does not fit is `None`, not a panic or a wrapped value.
    let d = read(
        "<w:style w:styleId=\"A\"><w:uiPriority w:val=\"99999999999\"/></w:style>\
         <w:style w:styleId=\"B\"><w:uiPriority w:val=\"x\"/></w:style>",
    );
    assert_eq!(d.get("A").unwrap().ui_priority, None);
    assert_eq!(d.get("B").unwrap().ui_priority, None);
}
