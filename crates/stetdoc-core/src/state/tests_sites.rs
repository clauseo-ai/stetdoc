//! Tests of the sites: what `SiteHandle` reads from the paragraph, the run, the table, the numbering and the
//! document, and that the cascade answers from it as it should.

use super::tests::*;
use super::tests_cascade::*;
use super::*;
use crate::api::view::Mode;
use crate::styles::{HiddenReason, LooksHidden};
use crate::wordml::{Item, Paragraph};
use crate::xml::NodeId;

/// A document with these style definitions and body, and the usual numbering part.
fn doc(styles: &str, body: &str) -> State {
    let mut related = all_parts();
    related[0].2 = styles_xml(styles);
    state_from(
        package_of(&document_xml(body), TRANSITIONAL, &related),
        &Limits::default(),
    )
}

/// The paragraphs of the body that are blocks, in order.
fn paragraph_nodes(s: &State) -> Vec<NodeId> {
    let story = s.parts().story(id(s, MAIN)).unwrap();
    story
        .blocks()
        .iter()
        .filter(|e| e.kind == crate::wordml::BlockKind::Paragraph)
        .map(|e| e.node)
        .collect()
}

/// The items of the text map of a paragraph of the body.
fn items(s: &State, node: NodeId) -> Vec<Item> {
    let view = s.parts();
    let tree = view.tree(id(s, MAIN)).unwrap();
    Paragraph::new(tree, node).unwrap().map().items().to_vec()
}

fn text_items(s: &State, node: NodeId) -> Vec<Item> {
    items(s, node)
        .into_iter()
        .filter(|i| i.kind == crate::wordml::ItemKind::Text)
        .collect()
}

fn run_key(item: &Item) -> RunKey<'_> {
    RunKey {
        item,
        complex_script: false,
    }
}

const WHITE_RUN: &str = "<w:r><w:rPr><w:color w:val=\"FFFFFF\"/></w:rPr><w:t>white</w:t></w:r>";

#[test]
fn a_paragraph_site_has_its_style_its_direct_properties_and_its_mark() {
    let heading = "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:pPr>\
                   <w:outlineLvl w:val=\"0\"/></w:pPr></w:style>";
    let s = doc(
        heading,
        "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/><w:jc w:val=\"center\"/>\
         <w:rPr><w:vanish/></w:rPr></w:pPr><w:r><w:t>x</w:t></w:r></w:p>",
    );
    let view = s.parts();
    let node = paragraph_nodes(&s)[0];
    let cascade = view.cascade().unwrap();
    let mut handle: SiteHandle<'_> = view.sites(id(&s, MAIN), Mode::Final).unwrap();
    let site = handle.site_for(node, None).unwrap();
    assert_eq!(site.style, Some("Heading1"));
    assert_eq!(
        site.ppr.and_then(|p| p.val("jc")).as_deref(),
        Some("center")
    );
    assert!(site.run.is_none() && site.table.is_none() && site.level.is_none());
    assert_eq!(cascade.outline_level(&site), Some(0));
    // With no run, `hidden` is about the paragraph mark.
    assert_eq!(cascade.hidden(&site), Some(HiddenReason::Vanish));
}

#[test]
fn a_run_site_has_the_character_style_the_direct_properties_and_the_script() {
    let strong =
        "<w:style w:type=\"character\" w:styleId=\"Strong\"><w:rPr><w:b/></w:rPr></w:style>";
    let s = doc(
        strong,
        "<w:p><w:r><w:rPr><w:rStyle w:val=\"Strong\"/><w:vanish/></w:rPr><w:t>x</w:t></w:r>\
         <w:r><w:t>y</w:t></w:r></w:p>",
    );
    let view = s.parts();
    let node = paragraph_nodes(&s)[0];
    let texts = text_items(&s, node);
    let cascade = view.cascade().unwrap();
    let mut handle = view.sites(id(&s, MAIN), Mode::Final).unwrap();
    let first = RunKey {
        item: &texts[0],
        complex_script: true,
    };
    let site = handle.site_for(node, Some(first)).unwrap();
    let run = site.run.unwrap();
    assert_eq!(run.style, Some("Strong"));
    assert!(run.complex_script && run.direct.is_some());
    assert_eq!(cascade.hidden(&site), Some(HiddenReason::Vanish));
    assert!(cascade.effective(&site).on("b"));
    // The next run of the same paragraph is its own site, with none of the first one's properties.
    let site = handle.site_for(node, Some(run_key(&texts[1]))).unwrap();
    assert_eq!(site.run.unwrap().style, None);
    assert!(!site.run.unwrap().complex_script);
    assert_eq!(cascade.hidden(&site), None);
    assert!(!cascade.effective(&site).on("b"));
}

#[test]
fn the_handle_moves_between_paragraphs_and_reads_each_one() {
    let styles = "<w:style w:type=\"paragraph\" w:styleId=\"A\"/><w:style w:type=\"paragraph\" w:styleId=\"B\"/>";
    let s = doc(
        styles,
        "<w:p><w:pPr><w:pStyle w:val=\"A\"/></w:pPr></w:p><w:p><w:pPr><w:pStyle w:val=\"B\"/></w:pPr></w:p><w:p/>",
    );
    let view = s.parts();
    let nodes = paragraph_nodes(&s);
    let mut handle = view.sites(id(&s, MAIN), Mode::Final).unwrap();
    let mut seen = Vec::new();
    for node in nodes.iter().chain(nodes.iter().rev()) {
        seen.push(
            handle
                .site_for(*node, None)
                .unwrap()
                .style
                .map(str::to_owned),
        );
    }
    let some = |s: &str| Some(s.to_owned());
    assert_eq!(
        seen,
        [some("A"), some("B"), None, None, some("B"), some("A")]
    );
    // `view.site` positions a handle, and `site` lends where it was moved to.
    let one = view
        .site(id(&s, MAIN), nodes[1], None, Mode::Final)
        .unwrap();
    assert_eq!(one.site().style, Some("B"));
}

#[test]
fn a_node_that_is_not_a_paragraph_is_a_caller_bug() {
    let s = doc("", "<w:p><w:r><w:t>x</w:t></w:r></w:p>");
    let view = s.parts();
    let tree = view.tree(id(&s, MAIN)).unwrap();
    let text = *text_nodes(tree).first().unwrap();
    let mut handle = view.sites(id(&s, MAIN), Mode::Final).unwrap();
    let err = handle.site_for(text, None).err().unwrap();
    assert_eq!(err.code(), crate::api::error::ErrorCode::Internal);
}

#[test]
fn in_original_a_tracked_formatting_change_is_read_as_it_was() {
    let styles = "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"/><w:style w:type=\"paragraph\" w:styleId=\"Body\"/>";
    let s = doc(
        styles,
        "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/><w:pPrChange w:id=\"1\" w:author=\"a\">\
         <w:pPr><w:pStyle w:val=\"Body\"/></w:pPr></w:pPrChange></w:pPr>\
         <w:r><w:rPr><w:vanish/><w:rPrChange w:id=\"2\" w:author=\"a\"><w:rPr/></w:rPrChange></w:rPr>\
         <w:t>x</w:t></w:r></w:p>",
    );
    let view = s.parts();
    let node = paragraph_nodes(&s)[0];
    let texts = text_items(&s, node);
    let cascade = view.cascade().unwrap();
    let probe = |mode| {
        let mut handle = view.sites(id(&s, MAIN), mode).unwrap();
        let style = handle
            .site_for(node, None)
            .unwrap()
            .style
            .map(str::to_owned);
        let site = handle.site_for(node, Some(run_key(&texts[0]))).unwrap();
        (style, cascade.hidden(&site))
    };
    let vanish = Some(HiddenReason::Vanish);
    assert_eq!(probe(Mode::Final), (Some("Heading1".into()), vanish));
    assert_eq!(probe(Mode::Redline), (Some("Heading1".into()), vanish));
    assert_eq!(probe(Mode::Original), (Some("Body".into()), None));
}

const GRID: &str = "<w:style w:type=\"table\" w:styleId=\"Grid\"><w:tblStylePr w:type=\"firstRow\">\
    <w:rPr><w:b/></w:rPr></w:tblStylePr></w:style>";

fn cell(fill: Option<&str>, run: &str) -> String {
    let pr = fill.map_or(String::new(), |f| {
        format!("<w:tcPr><w:shd w:val=\"clear\" w:fill=\"{f}\"/></w:tcPr>")
    });
    format!("<w:tc>{pr}<w:p>{run}</w:p></w:tc>")
}

#[test]
fn a_cell_gives_the_table_site_and_the_shading_behind_the_text() {
    let table = format!(
        "<w:tbl><w:tblPr><w:tblStyle w:val=\"Grid\"/><w:tblLook w:val=\"0020\"/></w:tblPr>\
         <w:tblGrid><w:gridCol w:w=\"100\"/><w:gridCol w:w=\"100\"/></w:tblGrid>\
         <w:tr>{}{}</w:tr><w:tr>{}{}</w:tr></w:tbl><w:p>{WHITE_RUN}</w:p>",
        cell(Some("1F3864"), WHITE_RUN),
        cell(None, WHITE_RUN),
        cell(None, "<w:r><w:t>b</w:t></w:r>"),
        cell(None, "<w:r><w:t>c</w:t></w:r>"),
    );
    let s = doc(GRID, &table);
    let view = s.parts();
    let nodes = paragraph_nodes(&s);
    assert_eq!(nodes.len(), 5);
    let cascade = view.cascade().unwrap();
    let mut handle = view.sites(id(&s, MAIN), Mode::Final).unwrap();
    let mut look = |i: usize| {
        let texts = text_items(&s, nodes[i]);
        let site = handle.site_for(nodes[i], Some(run_key(&texts[0]))).unwrap();
        let at = site
            .table
            .map(|t| (t.row, t.column, t.row_count, t.column_count));
        (
            at,
            site.cell_fill.map(str::to_owned),
            cascade.looks_hidden(&site),
            cascade.effective(&site).on("b"),
        )
    };
    // White text on a dark cell is seen, and the first row of the table style is bold.
    assert_eq!(
        look(0),
        (Some((0, 0, 2, 2)), Some("1F3864".into()), None, true)
    );
    // The same text in a cell with no shading is white on white.
    assert_eq!(
        look(1),
        (Some((0, 1, 2, 2)), None, Some(LooksHidden::WhiteText), true)
    );
    assert_eq!(look(2), (Some((1, 0, 2, 2)), None, None, false));
    // After the table: no table, and the cell around it is not read from the table before.
    assert_eq!(look(4), (None, None, Some(LooksHidden::WhiteText), false));
}

#[test]
fn the_page_colour_is_a_background_for_white_text() {
    let body = format!("<w:p>{WHITE_RUN}</w:p>");
    let look = |background: &str| {
        let document =
            format!("<w:document xmlns:w=\"{W}\">{background}<w:body>{body}</w:body></w:document>");
        let s = state_from(
            package_of(&document, TRANSITIONAL, &all_parts()),
            &Limits::default(),
        );
        let view = s.parts();
        let node = paragraph_nodes(&s)[0];
        let texts = text_items(&s, node);
        let mut handle = view.sites(id(&s, MAIN), Mode::Final).unwrap();
        let site = handle.site_for(node, Some(run_key(&texts[0]))).unwrap();
        view.cascade().unwrap().looks_hidden(&site)
    };
    assert_eq!(look(""), Some(LooksHidden::WhiteText));
    assert_eq!(look("<w:background w:color=\"FFFF00\"/>"), None);
}

#[test]
fn the_numbering_level_is_read_for_a_paragraph_and_the_numbers_only_when_it_is_asked_for() {
    let s = doc("", &numbered(2));
    let view = s.parts();
    let node = paragraph_nodes(&s)[1];
    let texts = text_items(&s, node);
    let cascade = view.cascade().unwrap();
    let mut handle = view.sites(id(&s, MAIN), Mode::Final).unwrap();
    // A run never reads the level, so asking about runs builds no numbers.
    assert!(
        handle
            .site_for(node, Some(run_key(&texts[0])))
            .unwrap()
            .level
            .is_none()
    );
    assert!(handle.numbers().is_none());
    let site = handle.site_for(node, None).unwrap();
    let level = site.level.unwrap();
    assert_eq!((level.num_id, level.ilvl, level.from_style), (1, 0, false));
    assert_eq!(level.props.attr("ind", "left"), Some("720"));
    // The level's indent is in the paragraph's effective properties.
    let effective = cascade.effective(&site);
    assert_eq!(
        effective.get("ind").and_then(|e| e.prop.attr("left")),
        Some("720")
    );
    assert!(handle.numbers().is_some());
}

#[test]
fn a_paragraph_that_removes_its_numbering_says_so() {
    let removed =
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"0\"/></w:numPr></w:pPr></w:p>";
    let s = doc("", &format!("{removed}{}", numbered(1)));
    let view = s.parts();
    let nodes = paragraph_nodes(&s);
    let mut handle = view.sites(id(&s, MAIN), Mode::Final).unwrap();
    assert!(handle.site_for(nodes[0], None).unwrap().numbering_removed);
    assert!(!handle.site_for(nodes[1], None).unwrap().numbering_removed);
}

#[test]
fn a_site_over_a_transaction_sees_the_styles_it_staged() {
    let s = doc(
        "<w:style w:type=\"paragraph\" w:styleId=\"Quiet\"/>",
        "<w:p><w:pPr><w:pStyle w:val=\"Quiet\"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>",
    );
    let main = id(&s, MAIN);
    let node = paragraph_nodes(&s)[0];
    let texts = text_items(&s, node);
    let hidden_in = |view: PartsView<'_>| {
        let mut handle = view.sites(main, Mode::Final).unwrap();
        let site = handle.site_for(node, Some(run_key(&texts[0]))).unwrap();
        view.cascade().unwrap().hidden(&site)
    };
    assert_eq!(hidden_in(s.parts()), None);
    let mut tx = s.begin();
    // Replace the style: the staged tree gets the vanish by adding it to the style's rPr.
    let tree = tx.part_mut(STYLES).unwrap();
    let quiet = tree
        .root()
        .child_elements()
        .find(|e| e.attr(W, "styleId").as_deref() == Some("Quiet"))
        .map(|e| e.id())
        .unwrap();
    let mut edit = tree.edit();
    let rpr = edit.create_element(W, "rPr").unwrap();
    let vanish = edit.create_element(W, "vanish").unwrap();
    edit.append_child(rpr, vanish).unwrap();
    edit.append_child(quiet, rpr).unwrap();
    assert_eq!(hidden_in(tx.parts()), Some(HiddenReason::Vanish));
    assert_eq!(
        hidden_in(s.parts()),
        None,
        "the committed state is as it was"
    );
}

#[test]
fn the_tables_of_a_story_are_read_once_and_only_when_a_paragraph_is_in_one() {
    // A story with no table never pays for the pass over its tables. With a table, the first paragraph in it
    // pays once for the story, and a paragraph outside the table still has none.
    let plain = doc("", &numbered(3));
    let view = plain.parts();
    let mut handle = view.sites(id(&plain, MAIN), Mode::Final).unwrap();
    for node in paragraph_nodes(&plain) {
        assert!(handle.site_for(node, None).unwrap().table.is_none());
    }
    assert!(!handle.tables_built());
    let body = format!(
        "<w:p/><w:tbl><w:tblPr><w:tblStyle w:val=\"Grid\"/></w:tblPr><w:tr>{}</w:tr></w:tbl>",
        cell(None, "<w:r><w:t>a</w:t></w:r>")
    );
    let s = doc(GRID, &body);
    let view = s.parts();
    let nodes = paragraph_nodes(&s);
    let mut handle = view.sites(id(&s, MAIN), Mode::Final).unwrap();
    assert!(handle.site_for(nodes[0], None).unwrap().table.is_none());
    assert!(
        !handle.tables_built(),
        "a paragraph outside a table asks nothing of the tables"
    );
    assert!(handle.site_for(nodes[1], None).unwrap().table.is_some());
    assert!(handle.tables_built());
}
