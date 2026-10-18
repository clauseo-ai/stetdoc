//! Tests for table styles and themes in the cascade. Each test says what it proves.

use super::effective::FontSlot;
use super::site::{RunSite, Site};
use super::source::Source;
use super::tables::applicable;
use super::tests_support::*;
use super::theme::Rgb;
use crate::wordml::{Props, TableRegion};

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
fn table_regions_apply_in_the_references_order() {
    // Register: U20.
    // Proves which regions match a cell and in what order: banding, then columns, then rows, then
    // corners, and the first row beats the first column where they meet (register U20).
    let r =
        |row, rows, col, cols, l| applicable(&site(None, l, row, rows, col, cols), 1, 1).to_vec();
    use TableRegion::*;
    let both = look(true, false, true, false);
    assert_eq!(r(0, 4, 0, 3, both), vec![FirstCol, FirstRow, NwCell]);
    assert_eq!(r(1, 4, 0, 3, both), vec![Band1Horz, FirstCol]);
    assert_eq!(r(2, 4, 1, 3, both), vec![Band1Vert, Band2Horz]);
    let mut banded_off = look(false, false, false, false);
    banded_off.no_h_band = true;
    banded_off.no_v_band = true;
    assert!(r(1, 4, 1, 3, banded_off).is_empty());
    let last = look(false, true, false, true);
    assert_eq!(r(3, 4, 2, 3, last), vec![LastCol, LastRow, SeCell]);
    // A band size of two: rows 0 and 1 are the first band, rows 2 and 3 the second.
    let bands = |row| {
        applicable(
            &site(None, look(false, false, false, false), row, 6, 0, 1),
            2,
            1,
        )
        .to_vec()
    };
    assert_eq!(bands(0)[1], Band1Horz);
    assert_eq!(bands(1)[1], Band1Horz);
    assert_eq!(bands(2)[1], Band2Horz);
}

#[test]
fn a_table_style_conditional_format_reaches_the_cell_text() {
    // Register: U1a.
    // Proves the table level end to end: the header row is bold, a body row is not, the paragraph
    // style's bold exclusive-ors with the table's (register U1a across all three kinds).
    let styles = format!(
        "{}{}{}",
        defaults(),
        style(
            "table",
            "Grid",
            None,
            "<w:tblStylePr w:type=\"firstRow\"><w:rPr><w:b/></w:rPr></w:tblStylePr>"
        ),
        style("paragraph", "Bold", None, "<w:rPr><w:b/></w:rPr>")
    );
    let c = cascade(&styles);
    let rows = [
        site(Some("Grid"), look(true, false, false, false), 0, 3, 0, 2),
        site(Some("Grid"), look(true, false, false, false), 1, 3, 0, 2),
    ];
    let cell = |row: usize, para: Option<&'static str>| Site {
        style: para,
        table: rows.get(row),
        run: Some(RunSite::default()),
        ..Site::default()
    };
    assert!(c.effective(&cell(0, None)).on("b"));
    assert!(!c.effective(&cell(1, None)).on("b"));
    assert!(!c.effective(&cell(0, Some("Bold"))).on("b"));
    assert!(c.effective(&cell(1, Some("Bold"))).on("b"));
    let e = c.effective(&cell(0, None));
    assert_eq!(
        e.get("b").unwrap().source,
        Source::TableStyle {
            id: "Grid".into(),
            region: Some(TableRegion::FirstRow)
        }
    );
}

const THEME: &str = "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"t\">\
<a:themeElements><a:clrScheme name=\"c\">\
<a:dk1><a:sysClr val=\"windowText\" lastClr=\"000000\"/></a:dk1>\
<a:lt1><a:sysClr val=\"window\" lastClr=\"FFFFFF\"/></a:lt1>\
<a:dk2><a:srgbClr val=\"44546A\"/></a:dk2><a:lt2><a:srgbClr val=\"E7E6E6\"/></a:lt2>\
<a:accent1><a:srgbClr val=\"4472C4\"/></a:accent1><a:accent2><a:srgbClr val=\"ED7D31\"/></a:accent2>\
</a:clrScheme><a:fontScheme name=\"f\">\
<a:majorFont><a:latin typeface=\"Calibri Light\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/>\
<a:font script=\"Jpan\" typeface=\"Yu Gothic Light\"/><a:font script=\"Hans\" typeface=\"DengXian Light\"/></a:majorFont>\
<a:minorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/>\
<a:font script=\"Jpan\" typeface=\"Yu Mincho\"/><a:font script=\"Hans\" typeface=\"DengXian\"/></a:minorFont>\
</a:fontScheme></a:themeElements></a:theme>";

#[test]
fn theme_fonts_and_colours_resolve_and_fall_back_to_the_cached_value() {
    // Proves theme fonts (with the per-script font of themeFontLang for an empty typeface), plain and
    // tinted colours, the colour map, and the cached value when the theme part is absent.
    let styles = format!(
        "{}{}",
        defaults(),
        style(
            "paragraph",
            "P",
            None,
            "<w:rPr><w:rFonts w:asciiTheme=\"majorHAnsi\" w:ascii=\"Arial\" w:hAnsiTheme=\"minorHAnsi\" w:eastAsiaTheme=\"minorEastAsia\"/>\
             <w:color w:val=\"4472C4\" w:themeColor=\"accent1\"/></w:rPr>"
        )
    );
    let settings = "<w:clrSchemeMapping w:bg1=\"light2\" w:t1=\"dark1\"/>\
                    <w:themeFontLang w:val=\"en-US\" w:eastAsia=\"ja-JP\"/>";
    let c = cascade_with(&styles, Some(THEME), Some(settings));
    let e = c.effective(&run_site(Some("P"), None, None));
    assert_eq!(e.font(FontSlot::Ascii), Some("Calibri Light".into()));
    assert_eq!(e.font(FontSlot::HAnsi), Some("Calibri".into()));
    assert_eq!(e.font(FontSlot::EastAsia), Some("Yu Mincho".into()));
    assert_eq!(e.color(), Some(Rgb::parse("4472C4").unwrap()));
    // Without a theme: the explicit font and the cached colour value.
    let plain = cascade(&styles);
    let e = plain.effective(&run_site(Some("P"), None, None));
    assert_eq!(e.font(FontSlot::Ascii), Some("Arial".into()));
    assert_eq!(e.font(FontSlot::EastAsia), None);
    assert_eq!(e.color(), Some(Rgb::parse("4472C4").unwrap()));
}

#[test]
fn theme_colours_follow_the_colour_map_and_scale_luminance_for_tint_and_shade() {
    // Proves background1 follows w:clrSchemeMapping, text1 is dark1 by default, and a shade darkens
    // while a tint lightens, in HSL luminance.
    use super::theme::Theme;
    let theme = parse_theme(THEME);
    let mapped = Theme::read(
        Some(&theme),
        Some(&crate::wordml::testing::part(
            "word/settings.xml",
            "settings",
            "<w:clrSchemeMapping w:bg1=\"light2\"/>",
        )),
    );
    assert_eq!(
        mapped.color("background1", None, None).unwrap().hex(),
        "E7E6E6"
    );
    let plain = Theme::read(Some(&theme), None);
    assert_eq!(
        plain.color("background1", None, None).unwrap().hex(),
        "FFFFFF"
    );
    assert_eq!(plain.color("text1", None, None).unwrap().hex(), "000000");
    assert_eq!(plain.color("hyperlink", None, None), None);
    let accent = plain.color("accent1", None, None).unwrap();
    let shaded = plain.color("accent1", None, Some(0xBF)).unwrap();
    let tinted = plain.color("accent1", Some(0x66), None).unwrap();
    let luminance = |c: Rgb| u32::from(c.r) + u32::from(c.g) + u32::from(c.b);
    assert!(luminance(shaded) < luminance(accent));
    assert!(luminance(tinted) > luminance(accent));
    // The same hue family: red stays the weakest channel in a shade of a blue.
    assert!(shaded.b > shaded.r);
    // A full tint is white, a zero shade is black.
    assert_eq!(
        plain.color("accent1", Some(0), None).unwrap().hex(),
        "FFFFFF"
    );
    assert_eq!(
        plain.color("accent1", None, Some(0)).unwrap().hex(),
        "000000"
    );
}

fn parse_theme(xml: &str) -> crate::wordml::SpanTree {
    crate::wordml::testing::parse("word/theme/theme1.xml", xml)
}

#[test]
fn an_empty_paragraph_with_no_styles_has_no_properties() {
    // Proves the cascade answers for a document with no style definitions at all.
    let c = cascade("");
    assert_eq!(c.effective(&Site::default()).iter().count(), 0);
    assert!(Props::new().is_empty());
}
