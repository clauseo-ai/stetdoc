//! Content controls and hyperlinks: `control`, `hyperlink` and `enclosing_controls`.
//!
//! `control` reads the `w:sdtPr` of a `w:sdt`. Which kind a control is comes from the one element
//! of `w:sdtPr` that names a type, and an `sdt` with none is a rich text control, which is what Word
//! makes of it. `enclosing_controls` finds the controls around any node by walking up to the story
//! root, across cells and rows, so a lock or a control around a whole table reaches every paragraph in
//! it (`docs/design/wordml.md` 3.8).

use super::names::{W14, W15, int, is_in, is_w, r_attr, w_attr, w_child};
use crate::xml::{NodeId, NodeRef};

/// What a content control holds, from the type element in its `w:sdtPr`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ControlKind {
    /// Plain text, `w:text`.
    PlainText,
    /// Rich text, `w:richText`, and the reading of an `sdt` with no type element.
    RichText,
    /// A date, `w:date`.
    Date,
    /// A drop-down list, `w:dropDownList`.
    DropDown,
    /// A combo box, `w:comboBox`.
    ComboBox,
    /// A check box, `w14:checkbox`.
    Checkbox,
    /// A picture, `w:picture`.
    Picture,
    /// A citation, `w:citation`.
    Citation,
    /// A bibliography, `w:bibliography`.
    Bibliography,
    /// A building block or document part, `w:docPartObj` or `w:docPartList`.
    BuildingBlock,
    /// A group, `w:group`.
    Group,
    /// A repeating section, `w15:repeatingSection`.
    RepeatingSection,
    /// An equation, `w:equation`.
    Equation,
    /// The node is not a `w:sdt`.
    Unknown,
}

/// The lock of a content control, `w:lock`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lock {
    /// No lock: no `w:lock`, `w:val="unlocked"`, or a value that is none of the four.
    Unlocked,
    /// The control cannot be deleted, `sdtLocked`. Its content can be edited.
    SdtLocked,
    /// The content cannot be edited, `contentLocked`. The control can be deleted.
    ContentLocked,
    /// Neither the control can be deleted nor its content edited, `sdtContentLocked`.
    SdtContentLocked,
}

impl Lock {
    /// True if the control cannot be deleted.
    pub fn locks_control(self) -> bool {
        matches!(self, Lock::SdtLocked | Lock::SdtContentLocked)
    }

    /// True if the content cannot be edited.
    pub fn locks_content(self) -> bool {
        matches!(self, Lock::ContentLocked | Lock::SdtContentLocked)
    }
}

/// A `w:sdt`'s properties.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control {
    /// What it holds.
    pub kind: ControlKind,
    /// The title, `w:alias`.
    pub alias: Option<String>,
    /// The tag, `w:tag`.
    pub tag: Option<String>,
    /// The `w:id`, if it is a whole number.
    pub id: Option<i64>,
    /// The lock.
    pub lock: Lock,
    /// True if the control shows its placeholder, `w:showingPlcHdr`.
    pub showing_placeholder: bool,
    /// True if the control is bound to a custom XML part, `w:dataBinding`. Word refreshes the
    /// content from the part, so an edit to it is lost.
    pub data_bound: bool,
}

/// The kind a property element of `w:sdtPr` names, if it names one.
fn kind_of(e: &NodeRef<'_>) -> Option<ControlKind> {
    if is_in(e, &[W14], "checkbox") {
        return Some(ControlKind::Checkbox);
    }
    if is_in(e, &[W15], "repeatingSection") {
        return Some(ControlKind::RepeatingSection);
    }
    if !is_w(e, e.local_name()) {
        return None;
    }
    Some(match e.local_name() {
        "text" => ControlKind::PlainText,
        "richText" => ControlKind::RichText,
        "date" => ControlKind::Date,
        "dropDownList" => ControlKind::DropDown,
        "comboBox" => ControlKind::ComboBox,
        "picture" => ControlKind::Picture,
        "citation" => ControlKind::Citation,
        "bibliography" => ControlKind::Bibliography,
        "docPartObj" | "docPartList" => ControlKind::BuildingBlock,
        "group" => ControlKind::Group,
        "equation" => ControlKind::Equation,
        _ => return None,
    })
}

/// Reads the properties of the content control `node`, a `w:sdt`. For any other node the kind is
/// `Unknown` and everything else is empty.
pub fn control(node: NodeRef<'_>) -> Control {
    let mut c = Control {
        kind: ControlKind::Unknown,
        alias: None,
        tag: None,
        id: None,
        lock: Lock::Unlocked,
        showing_placeholder: false,
        data_bound: false,
    };
    if !is_w(&node, "sdt") {
        return c;
    }
    c.kind = ControlKind::RichText;
    let Some(pr) = w_child(&node, "sdtPr") else {
        return c;
    };
    let mut typed = false;
    for e in pr.child_elements() {
        if !typed && let Some(k) = kind_of(&e) {
            c.kind = k;
            typed = true;
        }
        if !is_w(&e, e.local_name()) {
            continue;
        }
        let val = || w_attr(&e, "val");
        match e.local_name() {
            "alias" if c.alias.is_none() => c.alias = val().map(|v| v.into_owned()),
            "tag" if c.tag.is_none() => c.tag = val().map(|v| v.into_owned()),
            "id" if c.id.is_none() => c.id = val().and_then(|v| int(&v)),
            "lock" => {
                c.lock = match val().as_deref() {
                    Some("sdtLocked") => Lock::SdtLocked,
                    Some("contentLocked") => Lock::ContentLocked,
                    Some("sdtContentLocked") => Lock::SdtContentLocked,
                    _ => Lock::Unlocked,
                }
            }
            "showingPlcHdr" => c.showing_placeholder = super::names::on_off(val().as_deref()),
            "dataBinding" => c.data_bound = true,
            _ => {}
        }
    }
    c
}

/// The content controls that enclose `node`, innermost first: every `w:sdt` above it whose
/// `w:sdtContent` it is in, up to the story root. The walk crosses cells, rows, tables, block-level
/// wrappers and the paragraph that holds a text box, so a control around a whole table encloses the
/// paragraphs in it. The node itself is not among them.
pub fn enclosing_controls(node: NodeRef<'_>) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut child = node;
    while let Some(parent) = child.parent() {
        if is_w(&parent, "sdt") && is_w(&child, "sdtContent") {
            out.push(parent.id());
        }
        child = parent;
    }
    out
}

/// A `w:hyperlink`'s target.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hyperlink {
    /// The relationship id, `r:id`. `state` resolves it through `Rels`.
    pub r_id: Option<String>,
    /// The bookmark in this document, `w:anchor`.
    pub anchor: Option<String>,
    /// The frame to open the target in, `w:tgtFrame`.
    pub target_frame: Option<String>,
    /// The tooltip, `w:tooltip`.
    pub tooltip: Option<String>,
    /// Whether the link counts as followed, `w:history`, if it says.
    pub history: Option<bool>,
}

/// Reads a `w:hyperlink`. For any other node every field is empty.
pub fn hyperlink(node: NodeRef<'_>) -> Hyperlink {
    if !is_w(&node, "hyperlink") {
        return Hyperlink::default();
    }
    let text = |name: &str| w_attr(&node, name).map(|v| v.into_owned());
    Hyperlink {
        r_id: r_attr(&node, "id").map(|v| v.into_owned()),
        anchor: text("anchor"),
        target_frame: text("tgtFrame"),
        tooltip: text("tooltip"),
        history: w_attr(&node, "history").map(|v| super::names::on_off(Some(&v))),
    }
}
