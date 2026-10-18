//! `Story::object_info` and `math_text`: what a drawing, a picture or an object is, its alt text and
//! its text boxes, and the text of an equation. Each test says what it proves.

use super::names::W_S;
use super::testing::{document, parse};
use super::*;
use crate::xml::SpanTree;
use std::sync::Arc;

const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";

fn scan(tree: &SpanTree) -> Story {
    Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, tree).unwrap()
}

/// A body of one paragraph holding `inner` in a run, with the namespaces the objects need declared.
fn body(inner: &str) -> SpanTree {
    document(&format!(
        "<w:p xmlns:wp=\"{WP}\" xmlns:a=\"{A}\" xmlns:pic=\"{PIC}\" xmlns:wps=\"{WPS}\" \
         xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\">\
         <w:r>{inner}</w:r></w:p>"
    ))
}

/// The first `Object` item of the first paragraph, and the info of its element.
fn info_of(tree: &SpanTree) -> Option<ObjectInfo> {
    let story = scan(tree);
    let m = story.paragraph(tree, 0)?.map();
    let item = m.items().iter().find(|i| i.kind == ItemKind::Object)?;
    story.object_info(tree, item.node)
}

fn drawing(doc_pr: &str, uri: &str, content: &str) -> String {
    format!(
        "<w:drawing><wp:inline><wp:extent cx=\"1\" cy=\"1\"/>{doc_pr}<a:graphic>\
         <a:graphicData uri=\"{uri}\">{content}</a:graphicData></a:graphic></wp:inline></w:drawing>"
    )
}

#[test]
fn a_drawing_is_classified_by_the_uri_of_its_content() {
    let cases = [
        (PIC, "<pic:pic/>", ObjectKind::Picture),
        (WPS, "<wps:wsp/>", ObjectKind::Shape),
        (
            "http://schemas.openxmlformats.org/drawingml/2006/chart",
            "<c:chart xmlns:c=\"x\"/>",
            ObjectKind::Chart,
        ),
        (
            "http://schemas.microsoft.com/office/drawing/2014/chartex",
            "<cx:chart xmlns:cx=\"x\"/>",
            ObjectKind::Chart,
        ),
        (
            "http://schemas.openxmlformats.org/drawingml/2006/diagram",
            "<dgm:relIds xmlns:dgm=\"x\"/>",
            ObjectKind::Diagram,
        ),
        (
            "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup",
            "<wpg:wgp xmlns:wpg=\"x\"/>",
            ObjectKind::Group,
        ),
        (
            "http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas",
            "<wpc:wpc xmlns:wpc=\"x\"/>",
            ObjectKind::Group,
        ),
        (
            "http://schemas.microsoft.com/office/word/2010/wordprocessingInk",
            "<wpi:wink xmlns:wpi=\"x\"/>",
            ObjectKind::Ink,
        ),
        (
            "urn:other",
            "<x:y xmlns:x=\"urn:other\"/>",
            ObjectKind::Unknown,
        ),
    ];
    for (uri, content, expected) in cases {
        let tree = body(&drawing("<wp:docPr id=\"1\" name=\"n\"/>", uri, content));
        let info = info_of(&tree).unwrap();
        assert_eq!(info.kind, expected, "{uri}");
        assert!(info.alt.is_none() && info.text_boxes.is_empty(), "{uri}");
    }
}

#[test]
fn a_drawing_with_a_wrong_or_missing_uri_is_classified_by_the_namespace_of_its_content() {
    let tree = body(&drawing(
        "<wp:docPr id=\"1\" name=\"n\"/>",
        "urn:wrong",
        "<pic:pic/>",
    ));
    assert_eq!(info_of(&tree).unwrap().kind, ObjectKind::Picture);
    let tree = body(
        "<w:drawing><wp:anchor><wp:docPr id=\"1\" name=\"n\"/><a:graphic><a:graphicData><wps:wsp/>\
         </a:graphicData></a:graphic></wp:anchor></w:drawing>",
    );
    assert_eq!(info_of(&tree).unwrap().kind, ObjectKind::Shape);
    let tree = body("<w:drawing/>");
    assert_eq!(info_of(&tree).unwrap().kind, ObjectKind::Unknown);
}

#[test]
fn the_strict_spelling_of_drawingml_is_read_too() {
    let tree = parse(
        "word/document.xml",
        &format!(
            "<w:document xmlns:w=\"{W_S}\" xmlns:wp=\"http://purl.oclc.org/ooxml/drawingml/wordprocessingDrawing\" \
             xmlns:a=\"http://purl.oclc.org/ooxml/drawingml/main\"><w:body><w:p><w:r><w:drawing><wp:inline>\
             <wp:docPr id=\"1\" name=\"n\" descr=\"a chart\"/><a:graphic>\
             <a:graphicData uri=\"http://purl.oclc.org/ooxml/drawingml/chart\"/></a:graphic></wp:inline>\
             </w:drawing></w:r></w:p></w:body></w:document>"
        ),
    );
    let info = info_of(&tree).unwrap();
    assert_eq!(info.kind, ObjectKind::Chart);
    assert_eq!(info.alt.as_deref(), Some("a chart"));
}

#[test]
fn the_alt_text_of_a_drawing_is_its_description_else_its_title() {
    let alt = |doc_pr: &str| {
        info_of(&body(&drawing(doc_pr, PIC, "<pic:pic/>")))
            .unwrap()
            .alt
    };
    assert_eq!(
        alt("<wp:docPr id=\"1\" name=\"n\" descr=\"A cat\" title=\"Cat\"/>").as_deref(),
        Some("A cat")
    );
    assert_eq!(
        alt("<wp:docPr id=\"1\" name=\"n\" descr=\"\" title=\"Cat\"/>").as_deref(),
        Some("Cat")
    );
    assert_eq!(
        alt("<wp:docPr id=\"1\" name=\"n\" descr=\"  \"/>"),
        None,
        "blank is none"
    );
    assert_eq!(alt("<wp:docPr id=\"1\" name=\"n\"/>"), None);
    assert_eq!(alt(""), None);
}

#[test]
fn a_vml_picture_shape_group_and_object_are_told_apart() {
    let cases = [
        (
            "<w:pict><v:shapetype id=\"t\"/><v:shape id=\"s\" alt=\"A dog\"><v:imagedata r:id=\"rId1\" xmlns:r=\"x\"/></v:shape></w:pict>",
            ObjectKind::Picture,
            Some("A dog"),
        ),
        (
            "<w:pict><v:shape id=\"s\"><v:textbox/></v:shape></w:pict>",
            ObjectKind::Shape,
            None,
        ),
        (
            "<w:pict><v:rect alt=\"A box\"/></w:pict>",
            ObjectKind::Shape,
            Some("A box"),
        ),
        (
            "<w:pict><v:group><v:shape/></v:group></w:pict>",
            ObjectKind::Group,
            None,
        ),
        ("<w:pict><v:image/></w:pict>", ObjectKind::Picture, None),
        ("<w:pict/>", ObjectKind::Unknown, None),
        (
            "<w:object><v:shape alt=\"Sheet\"><v:imagedata/></v:shape><o:OLEObject Type=\"Embed\"/></w:object>",
            ObjectKind::Ole,
            Some("Sheet"),
        ),
        ("<w:object/>", ObjectKind::Ole, None),
    ];
    for (xml, kind, alt) in cases {
        let info = info_of(&body(xml)).unwrap();
        assert_eq!((info.kind, info.alt.as_deref()), (kind, alt), "{xml}");
    }
}

#[test]
fn the_text_boxes_of_an_object_are_listed_with_the_host_and_the_branch() {
    // Proves: the boxes below an object, boxes in boxes included, carry the entry index of the
    // paragraph that holds the object, the outermost one for an object inside a box, and the branch.
    let inner = "<w:p><w:r><w:t>in</w:t></w:r></w:p>";
    let tree = document(&format!(
        "<w:p/><w:p><w:r><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing>\
         <wp:inline xmlns:wp=\"{WP}\"><wp:docPr id=\"1\" name=\"n\"/><a:graphic xmlns:a=\"{A}\">\
         <a:graphicData uri=\"{WPS}\"><wps:wsp><wps:txbx><w:txbxContent>{inner}<w:p><w:r><w:pict>\
         <w:txbxContent>{inner}</w:txbxContent></w:pict></w:r></w:p></w:txbxContent></wps:txbx></wps:wsp>\
         </a:graphicData></a:graphic></wp:inline></w:drawing></mc:Choice>\
         <mc:Fallback><w:pict><w:txbxContent>{inner}</w:txbxContent></w:pict></mc:Fallback>\
         </mc:AlternateContent></w:r></w:p>"
    ));
    let story = scan(&tree);
    let m = story.paragraph(&tree, 1).unwrap().map();
    let objects: Vec<&Item> = m
        .items()
        .iter()
        .filter(|i| i.kind == ItemKind::Object)
        .collect();
    assert_eq!(objects.len(), 1, "the taken branch only");
    let info = story.object_info(&tree, objects[0].node).unwrap();
    assert_eq!(info.kind, ObjectKind::Shape);
    assert_eq!(info.text_boxes.len(), 2, "the box and the box inside it");
    assert!(
        info.text_boxes
            .iter()
            .all(|b| b.host == 1 && b.branch == Branch::Taken)
    );
    assert_eq!(info.text_boxes[0].blocks(&tree).count(), 2);
    // the object inside the outer box is not a block's own, and its host is the outer paragraph
    let boxed = info.text_boxes[0].blocks(&tree).nth(1).unwrap();
    let Block::Paragraph(p) = boxed else {
        panic!("a paragraph");
    };
    let nested = p
        .map()
        .items()
        .iter()
        .find(|i| i.kind == ItemKind::Object)
        .map(|i| i.node)
        .unwrap();
    let inner_info = story.object_info(&tree, nested).unwrap();
    assert_eq!(inner_info.text_boxes.len(), 1);
    assert_eq!(inner_info.text_boxes[0].host, 1);
    // the same story lists them all from the story side
    assert_eq!(story.text_boxes(&tree).len(), 3, "with the fallback copy");
}

#[test]
fn a_node_that_is_not_an_object_has_no_info() {
    let tree = body("<w:drawing/><w:t>x</w:t>");
    let story = scan(&tree);
    let m = story.paragraph(&tree, 0).unwrap().map();
    let text_node = m
        .items()
        .iter()
        .find(|i| i.kind == ItemKind::Text)
        .unwrap()
        .node;
    assert!(story.object_info(&tree, text_node).is_none());
    assert!(story.object_info(&tree, story.blocks()[0].node).is_none());
    assert_eq!(math_text(&tree, story.blocks()[0].node), "");
}

#[test]
fn the_text_of_an_equation_is_its_math_text_in_order() {
    let tree = document(
        "<w:p><m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r><m:f><m:num><m:r><m:t>1</m:t></m:r></m:num>\
         <m:den><m:r><m:t>2</m:t></m:r></m:den></m:f><w:ins w:id=\"1\"><m:r><m:t>+y</m:t></m:r></w:ins></m:oMath>\
         <m:oMath><m:r><m:t> = z</m:t></m:r></m:oMath></m:oMathPara>\
         <m:oMath><m:r><m:t>q</m:t></m:r></m:oMath></w:p>",
    );
    let story = scan(&tree);
    let m = story.paragraph(&tree, 0).unwrap().map();
    let maths: Vec<NodeId> = m
        .items()
        .iter()
        .filter(|i| i.kind == ItemKind::Math)
        .map(|i| i.node)
        .collect();
    assert_eq!(
        maths.len(),
        2,
        "a paragraph of equations and a bare equation"
    );
    assert_eq!(math_text(&tree, maths[0]), "x12+y = z");
    assert_eq!(math_text(&tree, maths[1]), "q");
}
