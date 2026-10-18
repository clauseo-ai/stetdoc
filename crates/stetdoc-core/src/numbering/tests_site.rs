//! `level_site`: the level a list item sits on, with the paragraph properties the cascade reads from it.

use super::counters::StoryNumbers;
use super::site::level_site;
use super::tests_support::*;
use crate::api::view::Mode;
use crate::styles::Cascade;
use crate::wordml::testing::{document, part};
use crate::wordml::{NumberingDefs, Story, StoryKind, StyleDefs};
use std::sync::Arc;

/// The left indent that the level of the nth paragraph's list item gives, with the `numId`, the level
/// the site names and whether it came from a style. `None` when there is no list item or no level.
fn indent(
    numbering: &str,
    styles: &str,
    body: &str,
    nth: usize,
) -> Option<(Option<String>, i64, u8, bool)> {
    let defs = NumberingDefs::read(&part("word/numbering.xml", "numbering", numbering));
    let style_tree = part("word/styles.xml", "styles", styles);
    let cascade = Cascade::build(&StyleDefs::read(&style_tree), None, None);
    let tree = document(body);
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).ok()?;
    let facts = story.num_facts(&tree);
    let numbers = StoryNumbers::build(&facts, &defs, &cascade, Mode::Final);
    let item = numbers.item(facts.get(nth)?.node)?;
    let site = level_site(&defs, &cascade, item)?;
    Some((
        site.props.attr("ind", "left").map(str::to_string),
        site.num_id,
        site.ilvl,
        site.from_style,
    ))
}

fn ind(left: u32) -> String {
    format!("<w:pPr><w:ind w:left=\"{left}\"/></w:pPr>")
}

#[test]
fn the_site_has_the_levels_own_paragraph_properties() {
    // The level of a direct numPr: its own pPr, the list, and the level the paragraph is on.
    let numbering = format!(
        "{}{}",
        abs(
            1,
            &format!(
                "{}{}",
                lvl(0, "decimal", "%1.", &ind(360)),
                lvl(1, "decimal", "%1.%2", &ind(720))
            )
        ),
        num(7, 1, "")
    );
    let got = indent(&numbering, "", &body(&[(7, 0), (7, 1)]), 1);
    assert_eq!(got, Some((Some("720".into()), 7, 1, false)));
}

#[test]
fn a_replacement_level_in_the_num_wins() {
    // A w:lvlOverride with a w:lvl replaces the level, and its pPr is the one the cascade reads.
    let replacement = format!(
        "<w:lvlOverride w:ilvl=\"0\">{}</w:lvlOverride>",
        lvl(0, "decimal", "%1)", &ind(1080))
    );
    let numbering = format!(
        "{}{}",
        abs(1, &lvl(0, "decimal", "%1.", &ind(360))),
        num(7, 1, &replacement)
    );
    let got = indent(&numbering, "", &body(&[(7, 0)]), 0);
    assert_eq!(got.and_then(|g| g.0), Some("1080".into()));
}

#[test]
fn a_level_the_list_lacks_takes_the_nearest_lower_level_and_keeps_the_paragraphs_level() {
    // The same fallback the counters use: a paragraph on level 5 of a list that defines two levels is
    // numbered as level 1, and its indent is level 1's, while the site still says level 5.
    let numbering = format!(
        "{}{}",
        abs(
            1,
            &format!(
                "{}{}",
                lvl(0, "decimal", "%1.", &ind(360)),
                lvl(1, "decimal", "%1.%2", &ind(720))
            )
        ),
        num(7, 1, "")
    );
    let got = indent(&numbering, "", &body(&[(7, 5)]), 0);
    assert_eq!(got, Some((Some("720".into()), 7, 5, false)));
}

#[test]
fn a_numbering_from_a_paragraph_style_says_so() {
    let numbering = format!(
        "{}{}",
        abs(1, &lvl(0, "decimal", "%1.", &ind(540))),
        num(3, 1, "")
    );
    let styles = style_num("Item", None, "<w:numId w:val=\"3\"/>");
    let got = indent(&numbering, &styles, &ps("Item"), 0);
    assert_eq!(got, Some((Some("540".into()), 3, 0, true)));
}

#[test]
fn a_style_linked_list_reads_the_level_of_the_list_it_links_to() {
    // numStyleLink: the w:num's own definition has no levels, and the style it names gives the list.
    let numbering = format!(
        "{}{}{}{}",
        abs(1, "<w:numStyleLink w:val=\"Legal\"/>"),
        abs(
            9,
            &format!(
                "<w:styleLink w:val=\"Legal\"/>{}",
                lvl(0, "decimal", "%1.", &ind(900))
            )
        ),
        num(9, 9, ""),
        num(1, 1, "")
    );
    let styles = "<w:style w:type=\"numbering\" w:styleId=\"Legal\"><w:pPr><w:numPr>\
                  <w:numId w:val=\"9\"/></w:numPr></w:pPr></w:style>";
    let got = indent(&numbering, styles, &body(&[(1, 0)]), 0);
    assert_eq!(got, Some((Some("900".into()), 1, 0, false)));
}
