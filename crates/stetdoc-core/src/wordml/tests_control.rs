//! Tests of `control`, `hyperlink` and `enclosing_controls`: every control kind, the properties of
//! `w:sdtPr`, the hyperlink target, and the walk up to the story root. Each test says what it proves.

use super::names::{R_S, W_S};
use super::testing::{document, parse};
use super::*;
use crate::xml::{NodeRef, SpanTree};

/// The first element below the root with this local name, found with an explicit stack.
fn first<'a>(tree: &'a SpanTree, local: &str) -> NodeRef<'a> {
    let mut stack = vec![tree.root().children()];
    while let Some(top) = stack.last_mut() {
        let Some(c) = top.next() else {
            stack.pop();
            continue;
        };
        if c.is_element() && c.local_name() == local {
            return c;
        }
        stack.push(c.children());
    }
    panic!("no element {local}");
}

/// The control of a body-level `w:sdt` whose `w:sdtPr` holds `pr`.
fn read(pr: &str) -> Control {
    let tree = document(&format!(
        "<w:sdt><w:sdtPr>{pr}</w:sdtPr><w:sdtContent><w:p/></w:sdtContent></w:sdt>"
    ));
    control(first(&tree, "sdt"))
}

#[test]
fn every_control_kind_is_read_from_its_type_element() {
    // Proves: each type element of `w:sdtPr` gives its kind, among them the `w14` check box and the
    // `w15` repeating section, and the properties around it do not change the kind.
    let cases = [
        ("<w:text/>", ControlKind::PlainText),
        ("<w:text w:multiLine=\"1\"/>", ControlKind::PlainText),
        ("<w:richText/>", ControlKind::RichText),
        (
            "<w:date w:fullDate=\"2024-01-01T00:00:00Z\"><w:dateFormat w:val=\"d\"/></w:date>",
            ControlKind::Date,
        ),
        (
            "<w:dropDownList><w:listItem w:value=\"a\"/></w:dropDownList>",
            ControlKind::DropDown,
        ),
        ("<w:comboBox/>", ControlKind::ComboBox),
        (
            "<w14:checkbox><w14:checked w14:val=\"1\"/></w14:checkbox>",
            ControlKind::Checkbox,
        ),
        ("<w:picture/>", ControlKind::Picture),
        ("<w:citation/>", ControlKind::Citation),
        ("<w:bibliography/>", ControlKind::Bibliography),
        (
            "<w:docPartObj><w:docPartGallery w:val=\"Cover Pages\"/></w:docPartObj>",
            ControlKind::BuildingBlock,
        ),
        ("<w:docPartList/>", ControlKind::BuildingBlock),
        ("<w:group/>", ControlKind::Group),
        ("<w15:repeatingSection/>", ControlKind::RepeatingSection),
        ("<w:equation/>", ControlKind::Equation),
    ];
    for (pr, kind) in cases {
        assert_eq!(read(pr).kind, kind, "{pr}");
        assert_eq!(
            read(&format!(
                "<w:alias w:val=\"A\"/><w:id w:val=\"5\"/>{pr}<w:tag w:val=\"t\"/>"
            ))
            .kind,
            kind,
            "{pr} among other properties"
        );
    }
}

#[test]
fn a_control_with_no_type_is_rich_text_and_a_non_control_is_unknown() {
    // Proves: an `sdt` with an empty `w:sdtPr`, with only other properties, or with no `w:sdtPr` is
    // rich text, as Word reads it; the same names in another namespace are not types; and a node that
    // is not a `w:sdt` is `Unknown` with every field empty.
    assert_eq!(read("").kind, ControlKind::RichText);
    assert_eq!(read("<w:alias w:val=\"x\"/>").kind, ControlKind::RichText);
    assert_eq!(read("<w14:text/><w15:group/>").kind, ControlKind::RichText);
    let tree = document("<w:sdt><w:sdtContent><w:p/></w:sdtContent></w:sdt>");
    assert_eq!(control(first(&tree, "sdt")).kind, ControlKind::RichText);
    let p = control(first(&tree, "p"));
    assert_eq!(p.kind, ControlKind::Unknown);
    assert_eq!((p.alias, p.tag, p.id), (None, None, None));
    assert_eq!(p.lock, Lock::Unlocked);
    assert!(!p.showing_placeholder && !p.data_bound);
}

#[test]
fn the_first_type_element_decides() {
    // Proves: when `w:sdtPr` holds two type elements (invalid, but it happens), the first one wins.
    assert_eq!(read("<w:comboBox/><w:text/>").kind, ControlKind::ComboBox);
}

#[test]
fn alias_tag_and_id_are_read() {
    // Proves: `w:alias` and `w:tag` give their values (with entities decoded), `w:id` a whole number
    // or `None`, and the first of a repeated property counts.
    let c = read(
        "<w:alias w:val=\"Client &amp; Co\"/><w:tag w:val=\"client\"/><w:id w:val=\"-1234\"/>",
    );
    assert_eq!(c.alias.as_deref(), Some("Client & Co"));
    assert_eq!(c.tag.as_deref(), Some("client"));
    assert_eq!(c.id, Some(-1234));
    let c = read("<w:alias w:val=\"one\"/><w:alias w:val=\"two\"/><w:id w:val=\"x\"/><w:tag/>");
    assert_eq!(c.alias.as_deref(), Some("one"));
    assert_eq!(c.id, None);
    assert_eq!(c.tag, None);
}

#[test]
fn every_lock_value_is_read() {
    // Proves: `w:lock` gives each of its four values, a missing `w:lock` or an unknown value is
    // unlocked, and the two questions a caller asks, whether the control can be deleted and whether
    // the content can be edited, are answered for each.
    let lock = |pr: &str| read(pr).lock;
    assert_eq!(lock("<w:lock w:val=\"sdtLocked\"/>"), Lock::SdtLocked);
    assert_eq!(
        lock("<w:lock w:val=\"contentLocked\"/>"),
        Lock::ContentLocked
    );
    assert_eq!(
        lock("<w:lock w:val=\"sdtContentLocked\"/>"),
        Lock::SdtContentLocked
    );
    assert_eq!(lock("<w:lock w:val=\"unlocked\"/>"), Lock::Unlocked);
    assert_eq!(lock("<w:lock w:val=\"padlock\"/>"), Lock::Unlocked);
    assert_eq!(lock("<w:lock/>"), Lock::Unlocked);
    assert_eq!(lock(""), Lock::Unlocked);
    let asks = |l: Lock| (l.locks_control(), l.locks_content());
    assert_eq!(asks(Lock::Unlocked), (false, false));
    assert_eq!(asks(Lock::SdtLocked), (true, false));
    assert_eq!(asks(Lock::ContentLocked), (false, true));
    assert_eq!(asks(Lock::SdtContentLocked), (true, true));
}

#[test]
fn a_placeholder_and_a_data_binding_are_flags() {
    // Proves: `w:showingPlcHdr` is on when present (an on/off value that can say false) and
    // `w:dataBinding` makes the control data-bound, whatever its attributes; neither is on by
    // default.
    let none = read("<w:text/>");
    assert!(!none.showing_placeholder && !none.data_bound);
    assert!(read("<w:showingPlcHdr/>").showing_placeholder);
    assert!(read("<w:showingPlcHdr w:val=\"1\"/>").showing_placeholder);
    assert!(!read("<w:showingPlcHdr w:val=\"0\"/>").showing_placeholder);
    let bound = read(
        "<w:text/><w:dataBinding w:prefixMappings=\"xmlns:ns0='urn:x'\" w:xpath=\"/ns0:r[1]/ns0:a[1]\" w:storeItemID=\"{00000000-0000-0000-0000-000000000000}\"/>",
    );
    assert!(bound.data_bound);
    assert_eq!(bound.kind, ControlKind::PlainText);
    assert!(read("<w:dataBinding/>").data_bound);
}

#[test]
fn a_run_level_control_is_read_the_same_way() {
    // Proves: `control` reads a `w:sdt` inside a paragraph just as it reads a block-level one.
    let tree = document(
        "<w:p><w:sdt><w:sdtPr><w:alias w:val=\"Date\"/><w:date/></w:sdtPr><w:sdtContent><w:r><w:t>x</w:t></w:r></w:sdtContent></w:sdt></w:p>",
    );
    let c = control(first(&tree, "sdt"));
    assert_eq!(c.kind, ControlKind::Date);
    assert_eq!(c.alias.as_deref(), Some("Date"));
}

#[test]
fn a_strict_control_is_read_like_a_transitional_one() {
    // Proves: `w:sdtPr` is read in the Strict `w` namespace, and the `w14` check box is still found
    // there because the extension namespaces do not change with the dialect.
    let tree = parse(
        "word/document.xml",
        &format!(
            "<w:document xmlns:w=\"{W_S}\" xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\"><w:body>\
             <w:sdt><w:sdtPr><w:alias w:val=\"A\"/><w:tag w:val=\"t\"/><w:id w:val=\"9\"/><w:lock w:val=\"sdtContentLocked\"/>\
             <w:showingPlcHdr/><w:dataBinding w:xpath=\"/a\"/><w:dropDownList/></w:sdtPr><w:sdtContent><w:p/></w:sdtContent></w:sdt>\
             <w:sdt><w:sdtPr><w14:checkbox/></w:sdtPr><w:sdtContent><w:p/></w:sdtContent></w:sdt>\
             </w:body></w:document>"
        ),
    );
    let body = first(&tree, "body");
    let mut sdts = body.child_elements();
    let c = control(sdts.next().unwrap());
    assert_eq!(c.kind, ControlKind::DropDown);
    assert_eq!(c.alias.as_deref(), Some("A"));
    assert_eq!(c.tag.as_deref(), Some("t"));
    assert_eq!(c.id, Some(9));
    assert_eq!(c.lock, Lock::SdtContentLocked);
    assert!(c.showing_placeholder && c.data_bound);
    assert_eq!(control(sdts.next().unwrap()).kind, ControlKind::Checkbox);
}

#[test]
fn a_hyperlink_gives_its_relationship_id_or_its_anchor() {
    // Proves: `w:hyperlink` gives `r:id` for an external or part target and `w:anchor` for a
    // bookmark, with the frame, tooltip and history when written, and `history` as an on/off value.
    let tree = document(
        "<w:p><w:hyperlink r:id=\"rId9\" w:tooltip=\"Go\" w:tgtFrame=\"_blank\" w:history=\"0\"><w:r><w:t>a</w:t></w:r></w:hyperlink>\
         <w:hyperlink w:anchor=\"_Toc1\" w:history=\"1\"><w:r><w:t>b</w:t></w:r></w:hyperlink>\
         <w:hyperlink/></w:p>",
    );
    let p = first(&tree, "p");
    let links: Vec<_> = p.child_elements().map(hyperlink).collect();
    assert_eq!(
        links[0],
        Hyperlink {
            r_id: Some("rId9".into()),
            anchor: None,
            target_frame: Some("_blank".into()),
            tooltip: Some("Go".into()),
            history: Some(false),
        }
    );
    assert_eq!(links[1].anchor.as_deref(), Some("_Toc1"));
    assert_eq!(links[1].r_id, None);
    assert_eq!(links[1].history, Some(true));
    assert_eq!(links[2], Hyperlink::default());
}

#[test]
fn a_hyperlink_in_the_strict_namespaces_and_a_non_hyperlink() {
    // Proves: `r:id` is read in the Strict relationships namespace too, and a node that is not a
    // `w:hyperlink` gives an empty `Hyperlink` even if it carries an `r:id`.
    let tree = parse(
        "word/document.xml",
        &format!(
            "<w:document xmlns:w=\"{W_S}\" xmlns:r=\"{R_S}\"><w:body><w:p>\
             <w:hyperlink r:id=\"rId2\" w:anchor=\"b\"/><w:r r:id=\"rId3\"/></w:p></w:body></w:document>"
        ),
    );
    let para = first(&tree, "p");
    let mut kids = para.child_elements();
    let link = hyperlink(kids.next().unwrap());
    assert_eq!(link.r_id.as_deref(), Some("rId2"));
    assert_eq!(link.anchor.as_deref(), Some("b"));
    assert_eq!(hyperlink(kids.next().unwrap()), Hyperlink::default());
}

#[test]
fn the_controls_around_a_paragraph_are_found_across_cells_and_rows() {
    // Proves: the walk up to the story root crosses the cell, the row and the table, so a control
    // around a whole table encloses the paragraph in it; controls are listed innermost first; and the
    // ones that do not enclose it (a sibling control, and the paragraph's own) are not listed.
    let tree = document(
        "<w:sdt><w:sdtPr><w:tag w:val=\"outer\"/></w:sdtPr><w:sdtContent>\
           <w:tbl><w:tr><w:sdt><w:sdtPr><w:tag w:val=\"rowlevel\"/></w:sdtPr><w:sdtContent><w:tc>\
             <w:sdt><w:sdtPr><w:tag w:val=\"inner\"/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>x</w:t></w:r></w:p></w:sdtContent></w:sdt>\
           </w:tc></w:sdtContent></w:sdt></w:tr></w:tbl>\
         </w:sdtContent></w:sdt>\
         <w:sdt><w:sdtPr><w:tag w:val=\"sibling\"/></w:sdtPr><w:sdtContent><w:p/></w:sdtContent></w:sdt>",
    );
    let p = first(&tree, "p");
    let tags: Vec<_> = enclosing_controls(p)
        .into_iter()
        .map(|id| control(tree.node_ref(id)).tag.unwrap())
        .collect();
    assert_eq!(tags, ["inner", "rowlevel", "outer"]);
    // A run in the same paragraph is enclosed by the same three.
    assert_eq!(enclosing_controls(first(&tree, "r")).len(), 3);
}

#[test]
fn a_node_in_a_control_property_or_outside_every_control_has_no_enclosing_control() {
    // Proves: only a node in a `w:sdtContent` is enclosed: a paragraph with no control above it and an
    // element of the `w:sdtPr` (the tag of the control itself) give an empty list, and a run-level
    // control encloses its runs and the paragraph that holds it is outside it.
    let tree = document(
        "<w:p/><w:p><w:sdt><w:sdtPr><w:tag w:val=\"t\"/></w:sdtPr><w:sdtContent><w:r><w:t>x</w:t></w:r></w:sdtContent></w:sdt></w:p>",
    );
    let first_p = first(&tree, "p");
    assert!(enclosing_controls(first_p).is_empty());
    assert!(enclosing_controls(first(&tree, "tag")).is_empty());
    let run = first(&tree, "r");
    assert_eq!(enclosing_controls(run).len(), 1);
    assert_eq!(enclosing_controls(run)[0], first(&tree, "sdt").id());
    let second_p = first(&tree, "sdt").parent().unwrap();
    assert!(enclosing_controls(second_p).is_empty());
}

#[test]
fn a_text_box_paragraph_is_enclosed_by_the_controls_around_its_host() {
    // Proves: the walk does not stop at a text box, so a lock around the host paragraph reaches the
    // paragraphs inside the box.
    let tree = document(
        "<w:sdt><w:sdtPr><w:lock w:val=\"contentLocked\"/></w:sdtPr><w:sdtContent>\
         <w:p><w:r><w:pict><w:txbxContent><w:p><w:r><w:t>in</w:t></w:r></w:p></w:txbxContent></w:pict></w:r></w:p>\
         </w:sdtContent></w:sdt>",
    );
    let inner = first(&tree, "txbxContent").first_child().unwrap();
    let ids = enclosing_controls(inner);
    assert_eq!(ids.len(), 1);
    assert_eq!(control(tree.node_ref(ids[0])).lock, Lock::ContentLocked);
}

#[test]
fn deeply_nested_controls_are_walked_without_recursion() {
    // Proves: 100 nested controls (200 elements deep, under the parser's limit of 256) are walked up
    // with a loop, and all 100 are found.
    let depth = 100;
    let tree = document(&format!(
        "{}<w:p/>{}",
        "<w:sdt><w:sdtContent>".repeat(depth),
        "</w:sdtContent></w:sdt>".repeat(depth)
    ));
    assert_eq!(enclosing_controls(first(&tree, "p")).len(), depth);
}
