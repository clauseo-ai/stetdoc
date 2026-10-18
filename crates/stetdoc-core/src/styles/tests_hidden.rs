//! Tests for hidden text, text that looks hidden, the paragraph mark and the outline level.
//! Each test says what it proves.

use super::hidden::{HiddenReason, LooksHidden, MarkEffect};
use super::site::{RunSite, Site};
use super::tests_support::*;
use crate::wordml::PropsRef;

fn run<'a>(para: Option<&'a str>, chr: Option<&'a str>, direct: Option<PropsRef<'a>>) -> Site<'a> {
    Site {
        style: para,
        run: Some(RunSite {
            style: chr,
            direct,
            complex_script: false,
        }),
        ..Site::default()
    }
}

fn hidden_styles() -> String {
    format!(
        "{}{}{}{}{}",
        defaults(),
        style("paragraph", "Hidden", None, "<w:rPr><w:vanish/></w:rPr>"),
        style("character", "Hidden1", None, "<w:rPr><w:vanish/></w:rPr>"),
        style(
            "table",
            "HiddenGrid",
            None,
            "<w:tblStylePr w:type=\"firstRow\"><w:rPr><w:vanish/></w:rPr></w:tblStylePr>"
        ),
        style("paragraph", "Web", None, "<w:rPr><w:webHidden/></w:rPr>")
    )
}

#[test]
fn vanish_hides_through_every_level() {
    // Proves vanish from the paragraph style, the character style, the table style and direct
    // formatting, and a direct vanish of 0 showing text that a style hides.
    let c = cascade(&hidden_styles());
    assert_eq!(
        c.hidden(&run(Some("Hidden"), None, None)),
        Some(HiddenReason::Vanish)
    );
    assert_eq!(
        c.hidden(&run(None, Some("Hidden1"), None)),
        Some(HiddenReason::Vanish)
    );
    assert_eq!(c.hidden(&run(None, None, None)), None);
    let t = tree("<w:r><w:rPr><w:vanish/></w:rPr></w:r>");
    assert_eq!(
        c.hidden(&run(None, None, props(&t, "rPr"))),
        Some(HiddenReason::Vanish)
    );
    let off = tree("<w:r><w:rPr><w:vanish w:val=\"0\"/></w:rPr></w:r>");
    assert_eq!(
        c.hidden(&run(Some("Hidden"), None, props(&off, "rPr"))),
        None
    );
    let header = site(
        Some("HiddenGrid"),
        look(true, false, false, false),
        0,
        2,
        0,
        2,
    );
    let body = site(
        Some("HiddenGrid"),
        look(true, false, false, false),
        1,
        2,
        0,
        2,
    );
    let cell = Site {
        table: Some(&header),
        run: Some(RunSite::default()),
        ..Site::default()
    };
    assert_eq!(c.hidden(&cell), Some(HiddenReason::Vanish));
    let body_cell = Site {
        table: Some(&body),
        ..cell
    };
    assert_eq!(c.hidden(&body_cell), None);
}

#[test]
fn a_hidden_character_style_over_a_hidden_paragraph_style_shows_the_text() {
    // Register: U1a.
    // Proves the tdf154370 pair (`Hidden1` over `Hidden`): vanish toggles across kinds, so the text is
    // visible, which is what Word shows and what the reference does not do (register U1a).
    let c = cascade(&hidden_styles());
    assert_eq!(c.hidden(&run(Some("Hidden"), Some("Hidden1"), None)), None);
}

#[test]
fn web_hidden_is_its_own_reason_and_replaces_rather_than_toggles() {
    // Proves webHidden is reported apart from vanish, and is not an exclusive-or.
    let c = cascade(&hidden_styles());
    assert_eq!(
        c.hidden(&run(Some("Web"), None, None)),
        Some(HiddenReason::WebHidden)
    );
    let t = tree("<w:r><w:rPr><w:webHidden/></w:rPr></w:r>");
    assert_eq!(
        c.hidden(&run(Some("Web"), None, props(&t, "rPr"))),
        Some(HiddenReason::WebHidden)
    );
    let both = tree("<w:r><w:rPr><w:vanish/><w:webHidden/></w:rPr></w:r>");
    assert_eq!(
        c.hidden(&run(None, None, props(&both, "rPr"))),
        Some(HiddenReason::Vanish)
    );
}

#[test]
fn the_paragraph_mark_can_be_hidden_or_join_the_next_paragraph() {
    // Proves hidden() with no run reports the mark, mark_effect tells hidden from specVanish, and
    // specVanish on a run does nothing to the run.
    let c = cascade(&hidden_styles());
    let hidden_mark = tree("<w:p><w:pPr><w:rPr><w:vanish/></w:rPr></w:pPr></w:p>");
    fn site_of<'a>(t: &'a crate::wordml::SpanTree, style: Option<&'a str>) -> Site<'a> {
        Site {
            style,
            mark: props(t, "rPr"),
            ..Site::default()
        }
    }
    let s = site_of(&hidden_mark, None);
    assert_eq!(c.hidden(&s), Some(HiddenReason::Vanish));
    assert_eq!(c.mark_effect(&s), MarkEffect::Hidden);
    let joined = tree("<w:p><w:pPr><w:rPr><w:specVanish/></w:rPr></w:pPr></w:p>");
    let s = site_of(&joined, None);
    assert_eq!(c.mark_effect(&s), MarkEffect::JoinsNext);
    assert_eq!(c.hidden(&s), None);
    let plain = tree("<w:p><w:pPr/></w:p>");
    assert_eq!(c.mark_effect(&site_of(&plain, None)), MarkEffect::Normal);
    // A hidden paragraph style hides the mark too.
    assert_eq!(
        c.mark_effect(&site_of(&plain, Some("Hidden"))),
        MarkEffect::Hidden
    );
    // specVanish on a run hides nothing.
    let run_spec = tree("<w:r><w:rPr><w:specVanish/></w:rPr></w:r>");
    assert_eq!(c.hidden(&run(None, None, props(&run_spec, "rPr"))), None);
}

#[test]
fn tiny_and_white_text_look_hidden_but_are_not_hidden() {
    // Proves looks_hidden: 2 pt or less, and white text with nothing coloured behind it, from a style
    // or directly, and that hidden() says nothing about either.
    let styles = format!(
        "{}{}{}",
        defaults(),
        style(
            "paragraph",
            "Tiny",
            None,
            "<w:rPr><w:sz w:val=\"4\"/></w:rPr>"
        ),
        style(
            "character",
            "White",
            None,
            "<w:rPr><w:color w:val=\"FFFFFF\"/></w:rPr>"
        )
    );
    let c = cascade(&styles);
    assert_eq!(
        c.looks_hidden(&run(Some("Tiny"), None, None)),
        Some(LooksHidden::TinyText)
    );
    assert_eq!(c.hidden(&run(Some("Tiny"), None, None)), None);
    let five = tree("<w:r><w:rPr><w:sz w:val=\"5\"/></w:rPr></w:r>");
    assert_eq!(
        c.looks_hidden(&run(Some("Tiny"), None, props(&five, "rPr"))),
        None
    );
    assert_eq!(
        c.looks_hidden(&run(None, Some("White"), None)),
        Some(LooksHidden::WhiteText)
    );
    let direct = tree("<w:r><w:rPr><w:color w:val=\"ffffff\"/></w:rPr></w:r>");
    assert_eq!(
        c.looks_hidden(&run(None, None, props(&direct, "rPr"))),
        Some(LooksHidden::WhiteText)
    );
    let dark = tree("<w:r><w:rPr><w:color w:val=\"000000\"/></w:rPr></w:r>");
    assert_eq!(
        c.looks_hidden(&run(None, Some("White"), props(&dark, "rPr"))),
        None
    );
}

#[test]
fn white_text_on_a_colour_is_not_flagged() {
    // Proves the background check: run shading, highlight, the paragraph's shading, the cell's, the
    // table style's cell shading for the region, and the page background all excuse white text.
    let styles = format!(
        "{}{}",
        defaults(),
        style(
            "table",
            "Header",
            None,
            "<w:tblStylePr w:type=\"firstRow\"><w:tcPr><w:shd w:val=\"clear\" w:fill=\"4472C4\"/></w:tcPr>\
             <w:rPr><w:color w:val=\"FFFFFF\"/></w:rPr></w:tblStylePr>"
        )
    );
    let c = cascade(&styles);
    let white = tree("<w:r><w:rPr><w:color w:val=\"FFFFFF\"/></w:rPr></w:r>");
    let flagged = |s: &Site<'_>| c.looks_hidden(s);
    let base = || run(None, None, props(&white, "rPr"));
    assert_eq!(flagged(&base()), Some(LooksHidden::WhiteText));
    let shaded = tree(
        "<w:r><w:rPr><w:color w:val=\"FFFFFF\"/><w:shd w:val=\"clear\" w:fill=\"000000\"/></w:rPr></w:r>",
    );
    assert_eq!(flagged(&run(None, None, props(&shaded, "rPr"))), None);
    let highlighted =
        tree("<w:r><w:rPr><w:color w:val=\"FFFFFF\"/><w:highlight w:val=\"black\"/></w:rPr></w:r>");
    assert_eq!(flagged(&run(None, None, props(&highlighted, "rPr"))), None);
    let no_fill = tree(
        "<w:r><w:rPr><w:color w:val=\"FFFFFF\"/><w:shd w:val=\"clear\" w:fill=\"auto\"/></w:rPr></w:r>",
    );
    assert_eq!(
        flagged(&run(None, None, props(&no_fill, "rPr"))),
        Some(LooksHidden::WhiteText)
    );
    assert_eq!(
        flagged(&Site {
            cell_fill: Some("1F3864"),
            ..base()
        }),
        None
    );
    assert_eq!(
        flagged(&Site {
            background: Some("000000"),
            ..base()
        }),
        None
    );
    let para = tree("<w:p><w:pPr><w:shd w:val=\"clear\" w:fill=\"203040\"/></w:pPr></w:p>");
    assert_eq!(
        flagged(&Site {
            ppr: props(&para, "pPr"),
            ..base()
        }),
        None
    );
    let header_site = site(Some("Header"), look(true, false, false, false), 0, 3, 0, 2);
    let body_site = site(Some("Header"), look(true, false, false, false), 1, 3, 0, 2);
    let header_cell = Site {
        table: Some(&header_site),
        run: Some(RunSite::default()),
        ..Site::default()
    };
    assert_eq!(flagged(&header_cell), None);
    let body_cell = Site {
        table: Some(&body_site),
        run: Some(RunSite::default()),
        ..Site::default()
    };
    assert_eq!(flagged(&body_cell), None);
}

#[test]
fn the_outline_level_comes_from_the_style_unless_the_paragraph_says_otherwise() {
    // Proves outline_level: the style's, the direct one over it, and body text (level 9) as None.
    let styles = format!(
        "{}{}{}",
        defaults(),
        style(
            "paragraph",
            "H1",
            None,
            "<w:pPr><w:outlineLvl w:val=\"0\"/></w:pPr>"
        ),
        style(
            "paragraph",
            "H2",
            Some("H1"),
            "<w:pPr><w:outlineLvl w:val=\"1\"/></w:pPr>"
        )
    );
    let c = cascade(&styles);
    let level = |s: Option<&str>, direct: Option<PropsRef<'_>>| {
        c.outline_level(&Site {
            style: s,
            ppr: direct,
            ..Site::default()
        })
    };
    assert_eq!(level(Some("H1"), None), Some(0));
    assert_eq!(level(Some("H2"), None), Some(1));
    assert_eq!(level(None, None), None);
    let direct = tree("<w:p><w:pPr><w:outlineLvl w:val=\"3\"/></w:pPr></w:p>");
    assert_eq!(level(Some("H1"), props(&direct, "pPr")), Some(3));
    let body = tree("<w:p><w:pPr><w:outlineLvl w:val=\"9\"/></w:pPr></w:p>");
    assert_eq!(level(Some("H1"), props(&body, "pPr")), None);
}

#[test]
fn shading_and_highlight_that_come_from_styles_excuse_white_text() {
    // Proves the safety signal does not cry wolf: a fill or highlight set by the paragraph style's
    // run properties, by its paragraph shading, or by the character style, is behind the text, and a
    // later level that sets the fill back to auto or white takes the excuse away.
    let styles = format!(
        "{}{}{}{}{}{}",
        defaults(),
        style(
            "paragraph",
            "RunFill",
            None,
            "<w:rPr><w:color w:val=\"FFFFFF\"/><w:shd w:val=\"clear\" w:fill=\"000000\"/></w:rPr>"
        ),
        style(
            "paragraph",
            "ParaFill",
            None,
            "<w:pPr><w:shd w:val=\"clear\" w:fill=\"1F3864\"/></w:pPr><w:rPr><w:color w:val=\"FFFFFF\"/></w:rPr>"
        ),
        style(
            "character",
            "Marked",
            None,
            "<w:rPr><w:color w:val=\"FFFFFF\"/><w:highlight w:val=\"darkBlue\"/></w:rPr>"
        ),
        style(
            "character",
            "Cleared",
            None,
            "<w:rPr><w:shd w:val=\"clear\" w:fill=\"auto\"/></w:rPr>"
        ),
        style(
            "character",
            "White",
            None,
            "<w:rPr><w:color w:val=\"FFFFFF\"/></w:rPr>"
        )
    );
    let c = cascade(&styles);
    assert_eq!(c.looks_hidden(&run(Some("RunFill"), None, None)), None);
    assert_eq!(c.looks_hidden(&run(Some("ParaFill"), None, None)), None);
    assert_eq!(c.looks_hidden(&run(None, Some("Marked"), None)), None);
    assert_eq!(
        c.looks_hidden(&run(Some("RunFill"), Some("Cleared"), None)),
        Some(LooksHidden::WhiteText)
    );
    assert_eq!(
        c.looks_hidden(&run(None, Some("White"), None)),
        Some(LooksHidden::WhiteText)
    );
}

#[test]
fn the_size_of_complex_script_text_is_szcs() {
    // Proves tiny text is judged by sz for ordinary text and szCs for complex script text, so a run
    // whose Arabic is full size is not flagged for the 2 pt size of its Latin letters, and the other
    // way round.
    let c = cascade(&defaults());
    fn site(t: &crate::wordml::SpanTree, complex_script: bool) -> Site<'_> {
        Site {
            run: Some(RunSite {
                direct: props(t, "rPr"),
                complex_script,
                ..RunSite::default()
            }),
            ..Site::default()
        }
    }
    let latin_tiny = tree("<w:r><w:rPr><w:sz w:val=\"4\"/><w:szCs w:val=\"22\"/></w:rPr></w:r>");
    assert_eq!(
        c.looks_hidden(&site(&latin_tiny, false)),
        Some(LooksHidden::TinyText)
    );
    assert_eq!(c.looks_hidden(&site(&latin_tiny, true)), None);
    let complex_tiny = tree("<w:r><w:rPr><w:sz w:val=\"22\"/><w:szCs w:val=\"4\"/></w:rPr></w:r>");
    assert_eq!(c.looks_hidden(&site(&complex_tiny, false)), None);
    assert_eq!(
        c.looks_hidden(&site(&complex_tiny, true)),
        Some(LooksHidden::TinyText)
    );
}

#[test]
fn a_colour_with_every_channel_at_or_above_f8_is_white_text() {
    // Proves the near-white rule of the design (3.6): text whose every channel is 0xF8 or more cannot be
    // told from white, and one channel below the line is a visible colour.
    let c = cascade(&defaults());
    let flagged = |colour: &str| {
        let t = tree(&format!(
            "<w:r><w:rPr><w:color w:val=\"{colour}\"/></w:rPr></w:r>"
        ));
        c.looks_hidden(&run(None, None, props(&t, "rPr")))
    };
    for colour in ["FFFFFF", "F8F8F8", "FAFBFC", "FEFEFE", "F8FFFF", "fcfcfc"] {
        assert_eq!(flagged(colour), Some(LooksHidden::WhiteText), "{colour}");
    }
    for colour in ["F7F8F8", "F8F7F8", "F8F8F7", "EEEEEE", "000000", "auto"] {
        assert_eq!(flagged(colour), None, "{colour}");
    }
}

#[test]
fn a_near_white_background_does_not_make_white_text_visible() {
    // The same line for what is behind the text: a fill at or above 0xF8 in every channel is a white
    // background, and white text on it is flagged. A fill below the line is a colour.
    let c = cascade(&defaults());
    let white = tree("<w:r><w:rPr><w:color w:val=\"FFFFFF\"/></w:rPr></w:r>");
    let on = |fill: &'static str| {
        c.looks_hidden(&Site {
            cell_fill: Some(fill),
            ..run(None, None, props(&white, "rPr"))
        })
    };
    assert_eq!(on("F8F8F8"), Some(LooksHidden::WhiteText));
    assert_eq!(on("FDFDFD"), Some(LooksHidden::WhiteText));
    assert_eq!(on("F7F7F7"), None);
    assert_eq!(on("1F3864"), None);
    let shaded = tree(
        "<w:r><w:rPr><w:color w:val=\"FFFFFF\"/><w:shd w:val=\"clear\" w:fill=\"F9F9F9\"/></w:rPr></w:r>",
    );
    assert_eq!(
        c.looks_hidden(&run(None, None, props(&shaded, "rPr"))),
        Some(LooksHidden::WhiteText)
    );
}

#[test]
fn a_near_white_theme_colour_is_white_text_whether_the_style_or_the_run_names_it() {
    let theme = "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
        <a:themeElements><a:clrScheme name=\"c\">\
        <a:dk1><a:srgbClr val=\"000000\"/></a:dk1><a:lt1><a:srgbClr val=\"FAFAFA\"/></a:lt1>\
        <a:accent1><a:srgbClr val=\"4472C4\"/></a:accent1></a:clrScheme></a:themeElements></a:theme>";
    let styles = format!(
        "{}{}",
        defaults(),
        style(
            "character",
            "Paper",
            None,
            "<w:rPr><w:color w:val=\"000000\" w:themeColor=\"background1\"/></w:rPr>"
        )
    );
    let c = cascade_with(&styles, Some(theme), None);
    assert_eq!(
        c.looks_hidden(&run(None, Some("Paper"), None)),
        Some(LooksHidden::WhiteText)
    );
    let direct =
        tree("<w:r><w:rPr><w:color w:val=\"000000\" w:themeColor=\"background1\"/></w:rPr></w:r>");
    assert_eq!(
        c.looks_hidden(&run(None, None, props(&direct, "rPr"))),
        Some(LooksHidden::WhiteText)
    );
    let dark = tree("<w:r><w:rPr><w:color w:val=\"000000\" w:themeColor=\"text1\"/></w:rPr></w:r>");
    assert_eq!(c.looks_hidden(&run(None, None, props(&dark, "rPr"))), None);
}
