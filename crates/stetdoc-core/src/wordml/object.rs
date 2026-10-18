//! Text boxes (`Story::text_boxes`), what a drawing, a picture or an embedded object is
//! (`Story::object_info`), and the text of an equation (`math_text`).
//!
//! A text box holds blocks of its own in a `w:txbxContent`, below a run of the paragraph that
//! anchors it. Word writes the content twice when the drawing is inside an `mc:AlternateContent`,
//! once in the `mc:Choice` and once in the `mc:Fallback`, so every box is listed with the branch it is
//! in. The text of a box is not part of its host paragraph's text. The blocks of a box are read with
//! [`Block::within`] on its `content` node (`docs/design/wordml.md` 3.8).

use super::block::Block;
use super::names::{M, W2006, attr_in, is_in, is_w};
use super::revision::{Branch, branch_of};
use super::story::{BlockKind, Story};
use crate::xml::{NodeId, NodeRef, SpanTree};

/// One `w:txbxContent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextBox {
    /// The entry index of the paragraph that holds it. A box inside another box has the host of the
    /// outermost one, because the blocks inside a box are not entries.
    pub host: usize,
    /// The `w:txbxContent` element.
    pub content: NodeId,
    /// Which branch of an `mc:AlternateContent` it is in, if any.
    pub branch: Branch,
}

impl TextBox {
    /// The blocks of the box, read on `tree`, which the story was scanned from.
    pub fn blocks(self, tree: &SpanTree) -> impl Iterator<Item = Block<'_>> {
        Block::within(tree, tree.node_ref(self.content))
    }
}

/// True for `w:txbxContent` and its Word 2006 namespace spelling.
fn is_text_box_content(n: &NodeRef<'_>) -> bool {
    is_w(n, "txbxContent") || is_in(n, &[W2006], "txbxContent")
}

impl Story {
    /// Every text box below a paragraph of this story, in document order, boxes inside boxes
    /// included. Both branches of an `mc:AlternateContent` are listed, each marked: a caller that
    /// shows text reads `Taken` and `Outside`, and one that must change every copy reads all.
    pub fn text_boxes(&self, tree: &SpanTree) -> Vec<TextBox> {
        let mut out = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            if e.kind != BlockKind::Paragraph {
                continue;
            }
            let mut stack = vec![tree.node_ref(e.node).children()];
            while let Some(top) = stack.last_mut() {
                let Some(n) = top.next() else {
                    stack.pop();
                    continue;
                };
                if !n.is_element() {
                    continue;
                }
                if is_text_box_content(&n) {
                    out.push(TextBox {
                        host: i,
                        content: n.id(),
                        branch: branch_of(&n),
                    });
                }
                stack.push(n.children());
            }
        }
        out
    }
}

/// What a drawing, a picture or an embedded object is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    /// A picture: DrawingML `pic:pic`, or VML with an image.
    Picture,
    /// A shape, a line or a text box: `wps:wsp`, or a VML shape without an image.
    Shape,
    /// A chart, `c:chart` or an extended chart.
    Chart,
    /// SmartArt, `dgm:relIds`.
    Diagram,
    /// Ink.
    Ink,
    /// A group of shapes or a drawing canvas, `wpg:wgp`, `wpc:wpc`, a locked canvas or `v:group`.
    Group,
    /// An embedded or linked OLE object, `w:object`.
    Ole,
    /// Anything else.
    Unknown,
}

/// What a `w:drawing`, `w:pict` or `w:object` is, read on demand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectInfo {
    /// What it is.
    pub kind: ObjectKind,
    /// The alternative text: `descr` of the `wp:docPr`, else its `title`, for a drawing, and the `alt`
    /// of the first VML shape for a picture or an object. `None` if there is none or it is empty.
    pub alt: Option<String>,
    /// The text boxes in it, in document order, boxes inside boxes included, each with its branch.
    /// Their text is not part of the paragraph's text.
    pub text_boxes: Vec<TextBox>,
}

/// The part of a DrawingML URI after its namespace, for the Transitional and the Strict spelling.
fn drawingml(uri: &str) -> Option<&str> {
    uri.strip_prefix("http://schemas.openxmlformats.org/drawingml/2006/")
        .or_else(|| uri.strip_prefix("http://purl.oclc.org/ooxml/drawingml/"))
}

/// What the namespace of the content of a drawing says it is.
fn kind_of_uri(uri: &str) -> ObjectKind {
    match drawingml(uri) {
        Some("picture") => return ObjectKind::Picture,
        Some("chart") => return ObjectKind::Chart,
        Some("diagram") => return ObjectKind::Diagram,
        Some("lockedCanvas") => return ObjectKind::Group,
        Some(_) => return ObjectKind::Unknown,
        None => {}
    }
    match uri {
        "http://schemas.microsoft.com/office/word/2010/wordprocessingShape" => ObjectKind::Shape,
        "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"
        | "http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas" => ObjectKind::Group,
        "http://schemas.microsoft.com/office/word/2010/wordprocessingInk" => ObjectKind::Ink,
        "http://schemas.microsoft.com/office/drawing/2014/chartex" => ObjectKind::Chart,
        _ => ObjectKind::Unknown,
    }
}

/// The first child element of `n` named `local` whose namespace `ok` accepts.
fn child_named<'a>(n: &NodeRef<'a>, local: &str, ok: impl Fn(&str) -> bool) -> Option<NodeRef<'a>> {
    n.child_elements()
        .find(|c| c.local_name() == local && ok(c.namespace()))
}

/// A drawing: the kind its content says, and its alt text from `wp:docPr`.
fn read_drawing(drawing: &NodeRef<'_>) -> (ObjectKind, Option<String>) {
    let is_wp = |ns: &str| drawingml(ns) == Some("wordprocessingDrawing");
    let container = drawing
        .child_elements()
        .find(|c| matches!(c.local_name(), "inline" | "anchor") && is_wp(c.namespace()));
    let Some(container) = container else {
        return (ObjectKind::Unknown, None);
    };
    let alt = child_named(&container, "docPr", is_wp).and_then(|doc_pr| {
        ["descr", "title"]
            .iter()
            .filter_map(|name| attr_in(&doc_pr, &[""], name))
            .map(|v| v.into_owned())
            .find(|v| !v.trim().is_empty())
    });
    let main = |ns: &str| drawingml(ns) == Some("main");
    let data = child_named(&container, "graphic", main)
        .and_then(|graphic| child_named(&graphic, "graphicData", main));
    let kind = data.map_or(ObjectKind::Unknown, |data| {
        let by_uri = attr_in(&data, &[""], "uri").map_or(ObjectKind::Unknown, |u| kind_of_uri(&u));
        match by_uri {
            ObjectKind::Unknown => data
                .child_elements()
                .next()
                .map_or(ObjectKind::Unknown, |first| kind_of_uri(first.namespace())),
            known => known,
        }
    });
    (kind, alt)
}

/// The namespace of VML.
const VML: &str = "urn:schemas-microsoft-com:vml";

/// The first VML shape below a picture or an object, where the shape holds the `alt`: its kind, and
/// its alt text. A `v:shapetype` is a template and not a shape. The search does not go into a text
/// box and reads no more than a few hundred elements.
fn read_vml(holder: &NodeRef<'_>) -> (ObjectKind, Option<String>) {
    let mut stack = vec![holder.children()];
    let mut seen = 0usize;
    while let Some(top) = stack.last_mut() {
        let Some(c) = top.next() else {
            stack.pop();
            continue;
        };
        seen += 1;
        if seen > 500 {
            break;
        }
        if !c.is_element() || is_text_box_content(&c) {
            continue;
        }
        if c.namespace() == VML && c.local_name() != "shapetype" {
            let alt = attr_in(&c, &[""], "alt")
                .map(|v| v.into_owned())
                .filter(|v| !v.trim().is_empty());
            let has_image = c
                .child_elements()
                .any(|i| i.namespace() == VML && i.local_name() == "imagedata");
            let kind = match c.local_name() {
                "group" => ObjectKind::Group,
                "image" => ObjectKind::Picture,
                "shape" if has_image => ObjectKind::Picture,
                _ => ObjectKind::Shape,
            };
            return (kind, alt);
        }
        stack.push(c.children());
    }
    (ObjectKind::Unknown, None)
}

impl Story {
    /// What the drawing, picture or object `node` is: the element of an `Object` item, a `w:drawing`,
    /// a `w:pict` or a `w:object`. `None` for any other node, and for one that is not below a paragraph
    /// of this story. The text boxes carry the entry index of the paragraph that holds the object, the
    /// outermost one if the object is inside a text box. `tree` is the tree the story was scanned
    /// from.
    pub fn object_info(&self, tree: &SpanTree, node: NodeId) -> Option<ObjectInfo> {
        let n = tree.node_ref(node);
        let (kind, alt) = if is_w(&n, "drawing") {
            read_drawing(&n)
        } else if is_w(&n, "pict") {
            read_vml(&n)
        } else if is_w(&n, "object") {
            (ObjectKind::Ole, read_vml(&n).1)
        } else {
            return None;
        };
        let mut host = None;
        let mut cur = n.parent();
        while let Some(a) = cur {
            if is_w(&a, "p")
                && let Some(i) = self.index_of(a.id())
            {
                host = Some(i);
            }
            cur = a.parent();
        }
        let host = host?;
        let mut text_boxes = Vec::new();
        let mut stack = vec![n.children()];
        while let Some(top) = stack.last_mut() {
            let Some(c) = top.next() else {
                stack.pop();
                continue;
            };
            if !c.is_element() {
                continue;
            }
            if is_text_box_content(&c) {
                text_boxes.push(TextBox {
                    host,
                    content: c.id(),
                    branch: branch_of(&c),
                });
            }
            stack.push(c.children());
        }
        Some(ObjectInfo {
            kind,
            alt,
            text_boxes,
        })
    }
}

/// The text of an equation, `m:oMath` or `m:oMathPara`: its `m:t` elements, in order, joined with no
/// separator, for a search. Revision wrappers inside the equation are not read: the whole equation
/// takes the revision state of the wrappers around it. Empty for any other node.
pub fn math_text(tree: &SpanTree, node: NodeId) -> String {
    let root = tree.node_ref(node);
    if !(is_in(&root, &M, "oMath") || is_in(&root, &M, "oMathPara")) {
        return String::new();
    }
    let mut out = String::new();
    let mut stack = vec![root.children()];
    while let Some(top) = stack.last_mut() {
        let Some(c) = top.next() else {
            stack.pop();
            continue;
        };
        if !c.is_element() {
            continue;
        }
        if is_in(&c, &M, "t") {
            out.push_str(&c.text_content());
        } else {
            stack.push(c.children());
        }
    }
    out
}
