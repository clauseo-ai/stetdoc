//! Tests for the cascade: order, merge rules, toggles, tables, numbering and themes.
//! Each test says what it proves.

use super::effective::FontSlot;
use super::site::{LevelSite, RunSite, Site};
use super::source::Source;
use super::tests_support::*;
use crate::wordml::Props;

fn run_site<'a>(
    para: Option<&'a str>,
    run_style: Option<&'a str>,
    direct: Option<crate::wordml::PropsRef<'a>>,
) -> Site<'a> {
    Site {
        style: para,
        run: Some(RunSite {
            style: run_style,
            direct,
            ..RunSite::default()
        }),
        ..Site::default()
    }
}

#[test]
fn each_level_overrides_the_one_below_and_names_itself() {
    // Proves the order: defaults, paragraph style, character style, direct, and the source of each.
    let styles = format!(
        "<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val=\"20\"/></w:rPr></w:rPrDefault></w:docDefaults>{}{}{}",
        defaults(),
        style(
            "paragraph",
            "P",
            None,
            "<w:rPr><w:sz w:val=\"22\"/></w:rPr>"
        ),
        style(
            "character",
            "C",
            None,
            "<w:rPr><w:sz w:val=\"24\"/></w:rPr>"
        )
    );
    let c = cascade(&styles);
    let t = tree("<w:r><w:rPr><w:sz w:val=\"26\"/></w:rPr></w:r>");
    let direct = props(&t, "rPr");
    let size = |site: &Site<'_>| {
        let e = c.effective(site);
        let entry = e.get("sz").unwrap();
        (entry.prop.val().unwrap().to_string(), entry.source.clone())
    };
    assert_eq!(
        size(&run_site(None, None, None)),
        ("20".into(), Source::Default)
    );
    assert_eq!(
        size(&run_site(Some("P"), None, None)),
        ("22".into(), Source::ParagraphStyle { id: "P".into() })
    );
    assert_eq!(
        size(&run_site(Some("P"), Some("C"), None)),
        ("24".into(), Source::CharacterStyle { id: "C".into() })
    );
    assert_eq!(
        size(&run_site(Some("P"), Some("C"), direct)),
        ("26".into(), Source::Direct)
    );
}

#[test]
fn a_style_inherits_along_based_on_and_the_nearest_wins() {
    // Proves a derived style overrides its parent and still inherits what it does not set.
    let styles = format!(
        "{}{}{}",
        defaults(),
        style(
            "paragraph",
            "Base",
            None,
            "<w:rPr><w:sz w:val=\"20\"/><w:color w:val=\"FF0000\"/></w:rPr>"
        ),
        style(
            "paragraph",
            "Kid",
            Some("Base"),
            "<w:rPr><w:sz w:val=\"30\"/></w:rPr>"
        )
    );
    let c = cascade(&styles);
    let e = c.effective(&run_site(Some("Kid"), None, None));
    assert_eq!(e.val("sz"), Some("30"));
    assert_eq!(
        e.get("sz").unwrap().source,
        Source::ParagraphStyle { id: "Kid".into() }
    );
    assert_eq!(e.val("color"), Some("FF0000"));
    assert_eq!(
        e.get("color").unwrap().source,
        Source::ParagraphStyle { id: "Base".into() }
    );
}

/// A value written for one level of the toggle truth table.
#[derive(Clone, Copy, Debug)]
enum V {
    Absent,
    False,
    True,
}

fn b(v: V) -> String {
    match v {
        V::Absent => String::new(),
        V::False => "<w:b w:val=\"0\"/>".into(),
        V::True => "<w:b/>".into(),
    }
}

#[test]
fn bold_follows_the_toggle_rule_over_every_combination() {
    // Register: U1a, U2.
    // Proves ISO 29500 17.7.3 as the design states it: the document default is the start, the paragraph
    // style and the character style exclusive-or into it (false is no toggle), and direct formatting
    // is absolute. 81 combinations, checked against the rule written out independently.
    let all = [V::Absent, V::False, V::True];
    for default in all {
        for para in all {
            for chr in all {
                for direct in all {
                    let styles = format!(
                        "<w:docDefaults><w:rPrDefault><w:rPr>{}</w:rPr></w:rPrDefault></w:docDefaults>{}{}{}",
                        b(default),
                        defaults(),
                        style(
                            "paragraph",
                            "P",
                            None,
                            &format!("<w:rPr>{}</w:rPr>", b(para))
                        ),
                        style(
                            "character",
                            "C",
                            None,
                            &format!("<w:rPr>{}</w:rPr>", b(chr))
                        )
                    );
                    let c = cascade(&styles);
                    let t = tree(&format!("<w:r><w:rPr>{}</w:rPr></w:r>", b(direct)));
                    let got = c
                        .effective(&run_site(Some("P"), Some("C"), props(&t, "rPr")))
                        .on("b");
                    let on = |v: V| matches!(v, V::True);
                    let want = match direct {
                        V::Absent => on(default) ^ on(para) ^ on(chr),
                        d => on(d),
                    };
                    assert_eq!(got, want, "{default:?} {para:?} {chr:?} {direct:?}");
                }
            }
        }
    }
}

#[test]
fn a_bold_character_style_in_a_bold_paragraph_style_is_not_bold() {
    // Register: U1a.
    // Proves the case that separates this design from the reference (register U1a), and the source
    // that explains it.
    let styles = format!(
        "{}{}{}",
        defaults(),
        style("paragraph", "Heading", None, "<w:rPr><w:b/></w:rPr>"),
        style("character", "Strong", None, "<w:rPr><w:b/></w:rPr>")
    );
    let c = cascade(&styles);
    let e = c.effective(&run_site(Some("Heading"), Some("Strong"), None));
    assert!(!e.on("b"));
    let Source::Toggle(parts) = &e.get("b").unwrap().source else {
        panic!("a toggle set at two levels has a Toggle source");
    };
    assert_eq!(parts.len(), 2);
    assert!(parts.iter().all(|(_, on)| *on));
}

#[test]
fn inside_one_based_on_chain_the_nearest_toggle_wins() {
    // Register: U1b.
    // Proves register U1b: Heading based on Normal, both bold, is bold (no exclusive-or in a chain).
    let styles = format!(
        "{}{}{}",
        defaults(),
        style("paragraph", "Normal2", None, "<w:rPr><w:b/></w:rPr>"),
        style(
            "paragraph",
            "Heading",
            Some("Normal2"),
            "<w:rPr><w:b/></w:rPr>"
        )
    );
    let c = cascade(&styles);
    assert!(c.effective(&run_site(Some("Heading"), None, None)).on("b"));
    let styles = format!(
        "{}{}{}",
        defaults(),
        style("paragraph", "Normal2", None, "<w:rPr><w:b/></w:rPr>"),
        style(
            "paragraph",
            "Plain",
            Some("Normal2"),
            "<w:rPr><w:b w:val=\"0\"/></w:rPr>"
        )
    );
    let c = cascade(&styles);
    assert!(!c.effective(&run_site(Some("Plain"), None, None)).on("b"));
}

#[test]
fn only_the_twelve_toggles_exclusive_or() {
    // Proves each of the twelve exclusive-ors, and that `dstrike`, `webHidden` and `color` replace.
    let toggles = [
        "b",
        "bCs",
        "i",
        "iCs",
        "caps",
        "smallCaps",
        "strike",
        "emboss",
        "imprint",
        "outline",
        "shadow",
        "vanish",
    ];
    for name in toggles {
        let styles = format!(
            "{}{}{}",
            defaults(),
            style(
                "paragraph",
                "P",
                None,
                &format!("<w:rPr><w:{name}/></w:rPr>")
            ),
            style(
                "character",
                "C",
                None,
                &format!("<w:rPr><w:{name}/></w:rPr>")
            )
        );
        let c = cascade(&styles);
        assert!(
            !c.effective(&run_site(Some("P"), Some("C"), None)).on(name),
            "{name}"
        );
    }
    for name in ["dstrike", "webHidden", "specVanish", "noProof"] {
        let styles = format!(
            "{}{}{}",
            defaults(),
            style(
                "paragraph",
                "P",
                None,
                &format!("<w:rPr><w:{name}/></w:rPr>")
            ),
            style(
                "character",
                "C",
                None,
                &format!("<w:rPr><w:{name}/></w:rPr>")
            )
        );
        let c = cascade(&styles);
        assert!(
            c.effective(&run_site(Some("P"), Some("C"), None)).on(name),
            "{name}"
        );
    }
}

#[test]
fn fonts_merge_by_slot_and_a_theme_attribute_replaces_its_explicit_twin() {
    // Register: U14.
    // Proves rFonts merges attribute by attribute, and ascii and asciiTheme are one slot.
    let styles = format!(
        "{}{}{}",
        defaults(),
        style(
            "paragraph",
            "P",
            None,
            "<w:rPr><w:rFonts w:ascii=\"Arial\" w:hAnsi=\"Arial\" w:cs=\"Tahoma\"/></w:rPr>"
        ),
        style(
            "character",
            "C",
            None,
            "<w:rPr><w:rFonts w:asciiTheme=\"minorHAnsi\" w:eastAsia=\"MS Mincho\"/></w:rPr>"
        )
    );
    let c = cascade(&styles);
    let e = c.effective(&run_site(Some("P"), Some("C"), None));
    let f = e.get("rFonts").unwrap();
    assert_eq!(f.prop.attr("ascii"), None);
    assert_eq!(f.prop.attr("asciiTheme"), Some("minorHAnsi"));
    assert_eq!(f.prop.attr("hAnsi"), Some("Arial"));
    assert_eq!(f.prop.attr("cs"), Some("Tahoma"));
    assert_eq!(f.prop.attr("eastAsia"), Some("MS Mincho"));
    assert!(
        f.attr_sources
            .iter()
            .any(|(n, s)| &**n == "cs" && *s == Source::ParagraphStyle { id: "P".into() })
    );
    assert!(
        f.attr_sources
            .iter()
            .any(|(n, s)| &**n == "eastAsia" && *s == Source::CharacterStyle { id: "C".into() })
    );
    assert_eq!(e.font(FontSlot::Complex), Some("Tahoma".into()));
}

fn para_site<'a>(style: Option<&'a str>, ppr: Option<crate::wordml::PropsRef<'a>>) -> Site<'a> {
    Site {
        style,
        ppr,
        ..Site::default()
    }
}

#[test]
fn indents_merge_by_attribute_and_exclusive_pairs_replace_each_other() {
    // Proves ind merges per attribute, and a hanging indent replaces a first-line indent.
    let styles = format!(
        "{}{}",
        defaults(),
        style(
            "paragraph",
            "P",
            None,
            "<w:pPr><w:ind w:left=\"720\" w:firstLine=\"360\"/><w:spacing w:before=\"100\" w:after=\"200\"/></w:pPr>"
        )
    );
    let c = cascade(&styles);
    let t = tree("<w:p><w:pPr><w:ind w:hanging=\"180\"/><w:spacing w:after=\"50\"/></w:pPr></w:p>");
    let e = c.effective(&para_site(Some("P"), props(&t, "pPr")));
    let ind = e.get("ind").unwrap();
    assert_eq!(ind.prop.attr("left"), Some("720"));
    assert_eq!(ind.prop.attr("hanging"), Some("180"));
    assert_eq!(ind.prop.attr("firstLine"), None);
    let sp = e.get("spacing").unwrap();
    assert_eq!(sp.prop.attr("before"), Some("100"));
    assert_eq!(sp.prop.attr("after"), Some("50"));
}

#[test]
fn borders_and_tabs_merge_by_child() {
    // Proves pBdr merges by side, and tabs by position with a clear removing a lower stop.
    let styles = format!(
        "{}{}{}",
        defaults(),
        style(
            "paragraph",
            "Base",
            None,
            "<w:pPr><w:pBdr><w:top w:val=\"single\"/><w:bottom w:val=\"single\"/></w:pBdr>\
             <w:tabs><w:tab w:val=\"left\" w:pos=\"720\"/><w:tab w:val=\"left\" w:pos=\"1440\"/></w:tabs></w:pPr>"
        ),
        style(
            "paragraph",
            "Kid",
            Some("Base"),
            "<w:pPr><w:pBdr><w:top w:val=\"double\"/></w:pBdr>\
             <w:tabs><w:tab w:val=\"clear\" w:pos=\"720\"/><w:tab w:val=\"right\" w:pos=\"2880\"/></w:tabs></w:pPr>"
        )
    );
    let c = cascade(&styles);
    let e = c.effective(&para_site(Some("Kid"), None));
    let bdr = e.get("pBdr").unwrap();
    let side = |n: &str| {
        bdr.prop
            .children
            .iter()
            .find(|c| &*c.name == n)
            .and_then(|c| c.val())
            .map(str::to_string)
    };
    assert_eq!(side("top"), Some("double".into()));
    assert_eq!(side("bottom"), Some("single".into()));
    let tabs: Vec<_> = e
        .get("tabs")
        .unwrap()
        .prop
        .children
        .iter()
        .filter_map(|t| t.attr("pos").map(str::to_string))
        .collect();
    assert_eq!(tabs, vec!["1440", "2880"]);
}

#[test]
fn numbering_indent_sits_above_the_style_for_direct_numbering_and_below_for_style_numbering() {
    // Register: U17.
    // Proves register U17: a direct numPr's level indent beats the style's, and a style's own indent
    // beats the level's when the numbering comes from the style.
    let styles = format!(
        "{}{}",
        defaults(),
        style(
            "paragraph",
            "P",
            None,
            "<w:pPr><w:ind w:left=\"100\"/></w:pPr>"
        )
    );
    let c = cascade(&styles);
    let level = {
        let t = tree("<w:pPr><w:ind w:left=\"900\"/></w:pPr>");
        crate::wordml::Props::read(first(t.root(), "pPr").unwrap())
    };
    let site = |from_style: bool| Site {
        style: Some("P"),
        level: Some(LevelSite {
            props: &level,
            num_id: 3,
            ilvl: 1,
            from_style,
        }),
        ..Site::default()
    };
    let direct = c.effective(&site(false));
    assert_eq!(direct.get("ind").unwrap().prop.attr("left"), Some("900"));
    assert_eq!(
        direct.get("ind").unwrap().source,
        Source::Numbering { num_id: 3, ilvl: 1 }
    );
    let from_style = c.effective(&site(true));
    assert_eq!(
        from_style.get("ind").unwrap().prop.attr("left"),
        Some("100")
    );
}

#[test]
fn numbering_removed_drops_the_style_indent_but_not_the_direct_one() {
    // Register: U19.
    // Proves register U19: a paragraph whose own numPr is numId 0 loses the indent from its style.
    let styles = format!(
        "{}{}",
        defaults(),
        style(
            "paragraph",
            "P",
            None,
            "<w:pPr><w:ind w:left=\"720\"/><w:jc w:val=\"center\"/></w:pPr>"
        )
    );
    let c = cascade(&styles);
    let removed = Site {
        style: Some("P"),
        numbering_removed: true,
        ..Site::default()
    };
    let e = c.effective(&removed);
    assert!(e.get("ind").is_none());
    assert_eq!(e.val("jc"), Some("center"));
    let t = tree("<w:p><w:pPr><w:ind w:left=\"360\"/></w:pPr></w:p>");
    let with_direct = Site {
        ppr: props(&t, "pPr"),
        ..removed
    };
    assert_eq!(
        c.effective(&with_direct)
            .get("ind")
            .unwrap()
            .prop
            .attr("left"),
        Some("360")
    );
}

#[test]
fn only_the_indent_of_a_numbering_level_reaches_the_paragraph() {
    // Register: U7.
    // Proves register U7: a level's tabs and other paragraph properties are not part of the cascade.
    let c = cascade(&defaults());
    let t = tree(
        "<w:pPr><w:ind w:left=\"900\"/><w:tabs><w:tab w:val=\"left\" w:pos=\"900\"/></w:tabs><w:jc w:val=\"right\"/></w:pPr>",
    );
    let level = Props::read(first(t.root(), "pPr").unwrap());
    let e = c.effective(&Site {
        level: Some(LevelSite {
            props: &level,
            num_id: 1,
            ilvl: 0,
            from_style: false,
        }),
        ..Site::default()
    });
    assert!(e.get("ind").is_some());
    assert!(e.get("tabs").is_none());
    assert!(e.get("jc").is_none());
}
