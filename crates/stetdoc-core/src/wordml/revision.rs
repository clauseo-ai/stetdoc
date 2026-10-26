//! Reading revisions: every element that records a tracked change, in one place.
//!
//! `revisions` writes revisions, accepts them and rejects them, and never has a second reader.
//! `revision_at` classifies an element by its name and its parent. `Story::revisions` lists every
//! revision in a story's part in document order, and `part_revisions` does the same for any other part
//! (styles and numbering carry property changes too). Each is marked with its branch when it is inside
//! an `mc:AlternateContent`: Word repeats a text box's content in the `mc:Choice` and the
//! `mc:Fallback`, and a tracked change inside it is written in both. What a property change holds
//! inside it is the old properties, and a revision element there is a copy, not a revision
//! (`in_snapshot`). `docs/design/wordml.md` section 7.

use super::mc;
use super::names::{attr_in, int, is_word_ns, w_attr};
use super::story::Story;
use crate::xml::{NodeId, NodeRef, SpanTree};

/// What a revision element records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RevKind {
    /// Text inserted: `w:ins` around runs.
    Ins,
    /// Text deleted: `w:del` around runs.
    Del,
    /// The source of a move: `w:moveFrom`.
    MoveFrom,
    /// The destination of a move: `w:moveTo`.
    MoveTo,
    /// A paragraph mark inserted: `w:pPr/w:rPr/w:ins`.
    MarkIns,
    /// A paragraph mark deleted: `w:pPr/w:rPr/w:del`.
    MarkDel,
    /// A paragraph mark moved away: `w:pPr/w:rPr/w:moveFrom`.
    MarkMoveFrom,
    /// A paragraph mark moved in: `w:pPr/w:rPr/w:moveTo`.
    MarkMoveTo,
    /// Run formatting changed: `w:rPrChange`.
    RunProps,
    /// Paragraph formatting changed: `w:pPrChange`.
    ParagraphProps,
    /// Section properties changed: `w:sectPrChange`.
    SectionProps,
    /// Table properties changed: `w:tblPrChange`.
    TableProps,
    /// The table grid changed: `w:tblGridChange`.
    TableGrid,
    /// Row properties changed: `w:trPrChange`.
    RowProps,
    /// Cell properties changed: `w:tcPrChange`.
    CellProps,
    /// A numbering change: `w:numberingChange`.
    NumberingChange,
    /// A row inserted: `w:trPr/w:ins`.
    RowIns,
    /// A row deleted: `w:trPr/w:del`.
    RowDel,
    /// A cell inserted: `w:tcPr/w:cellIns`.
    CellIns,
    /// A cell deleted: `w:tcPr/w:cellDel`.
    CellDel,
    /// Cells merged: `w:tcPr/w:cellMerge`.
    CellMerge,
    /// Table exception properties changed: `w:tblPrExChange`.
    TableExProps,
    /// A list number inserted on a paragraph: `w:numPr/w:ins`.
    NumPrIns,
    /// The start of a custom XML range that was inserted: `w:customXmlInsRangeStart`. It is the only
    /// record that a custom XML element (a content control's wrapper) was inserted.
    CustomXmlInsStart,
    /// The end of a custom XML range that was inserted: `w:customXmlInsRangeEnd`. It has an id and no
    /// author or date.
    CustomXmlInsEnd,
    /// The start of a custom XML range that was deleted: `w:customXmlDelRangeStart`.
    CustomXmlDelStart,
    /// The end of a custom XML range that was deleted: `w:customXmlDelRangeEnd`.
    CustomXmlDelEnd,
    /// The start of a custom XML range that was moved away: `w:customXmlMoveFromRangeStart`.
    CustomXmlMoveFromStart,
    /// The end of a custom XML range that was moved away: `w:customXmlMoveFromRangeEnd`.
    CustomXmlMoveFromEnd,
    /// The start of a custom XML range that was moved in: `w:customXmlMoveToRangeStart`.
    CustomXmlMoveToStart,
    /// The end of a custom XML range that was moved in: `w:customXmlMoveToRangeEnd`.
    CustomXmlMoveToEnd,
}

/// Where a revision is relative to an `mc:AlternateContent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Branch {
    /// Not inside an `mc:AlternateContent`.
    Outside,
    /// Inside the branch Word reads.
    Taken,
    /// Inside a branch Word does not read, a copy of content that is also in the taken one.
    Untaken,
}

/// The Word 2023 namespace that carries `w16du:dateUtc`.
const W16DU: &str = "http://schemas.microsoft.com/office/word/2023/wordml/word16du";

/// One revision, as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revision {
    /// What it records.
    pub kind: RevKind,
    /// The revision element.
    pub node: NodeId,
    /// The `w:id`, if it is a whole number.
    pub id: Option<i64>,
    /// The `w:author`, if there is one.
    pub author: Option<String>,
    /// The `w:date` as written, if there is one. It is optional.
    pub date: Option<String>,
    /// The `w16du:dateUtc` as written, if there is one.
    pub date_utc: Option<String>,
    /// Which branch of an `mc:AlternateContent` it is in, if any.
    pub branch: Branch,
}

/// The elements that hold the old properties of a property change, whatever is inside them.
const SNAPSHOTS: [&str; 9] = [
    "rPrChange",
    "pPrChange",
    "sectPrChange",
    "tblPrChange",
    "tblPrExChange",
    "tblGridChange",
    "trPrChange",
    "tcPrChange",
    "numberingChange",
];

/// True if `n` is inside the old properties of a property change. Word 2010 and later copy the
/// paragraph mark's `w:ins` or `w:del` into the `w:rPr` of a `w:pPrChange`, and that copy is not a
/// revision of its own.
fn in_snapshot(n: &NodeRef<'_>) -> bool {
    let mut cur = n.parent();
    while let Some(a) = cur {
        if is_word_ns(a.namespace()) && SNAPSHOTS.contains(&a.local_name()) {
            return true;
        }
        cur = a.parent();
    }
    false
}

/// Classifies an element as a revision by its name and its parent. `None` for any other element,
/// and for one inside the old properties of a property change. The branch is `Outside`:
/// [`Story::revisions`] works it out.
pub fn revision_at(n: &NodeRef<'_>) -> Option<Revision> {
    if !n.is_element() || !is_word_ns(n.namespace()) {
        return None;
    }
    let name = n.local_name();
    let kind = match name {
        "ins" | "del" | "moveFrom" | "moveTo" => mark_kind(n, name)?,
        "rPrChange" => RevKind::RunProps,
        "pPrChange" => RevKind::ParagraphProps,
        "sectPrChange" => RevKind::SectionProps,
        "tblPrChange" => RevKind::TableProps,
        "tblGridChange" => RevKind::TableGrid,
        "trPrChange" => RevKind::RowProps,
        "tcPrChange" => RevKind::CellProps,
        "numberingChange" => RevKind::NumberingChange,
        "cellIns" => RevKind::CellIns,
        "cellDel" => RevKind::CellDel,
        "cellMerge" => RevKind::CellMerge,
        "tblPrExChange" => RevKind::TableExProps,
        "customXmlInsRangeStart" => RevKind::CustomXmlInsStart,
        "customXmlInsRangeEnd" => RevKind::CustomXmlInsEnd,
        "customXmlDelRangeStart" => RevKind::CustomXmlDelStart,
        "customXmlDelRangeEnd" => RevKind::CustomXmlDelEnd,
        "customXmlMoveFromRangeStart" => RevKind::CustomXmlMoveFromStart,
        "customXmlMoveFromRangeEnd" => RevKind::CustomXmlMoveFromEnd,
        "customXmlMoveToRangeStart" => RevKind::CustomXmlMoveToStart,
        "customXmlMoveToRangeEnd" => RevKind::CustomXmlMoveToEnd,
        _ => return None,
    };
    if in_snapshot(n) {
        return None;
    }
    let text = |name: &str| w_attr(n, name).map(|v| v.into_owned());
    Some(Revision {
        kind,
        node: n.id(),
        id: w_attr(n, "id").and_then(|v| int(&v)),
        author: text("author"),
        date: text("date"),
        date_utc: attr_in(n, &[W16DU], "dateUtc").map(|v| v.into_owned()),
        branch: Branch::Outside,
    })
}

/// The kind of an `ins`, `del`, `moveFrom` or `moveTo` element, which its parent decides: a numbering or a
/// row change, a change of a paragraph mark (in `w:pPr/w:rPr`), or a change of content. The parent is read only for
/// these four names, which few elements have.
fn mark_kind(n: &NodeRef<'_>, name: &str) -> Option<RevKind> {
    let parent = n.parent();
    let under = |name: &str| {
        parent
            .as_ref()
            .is_some_and(|p| p.local_name() == name && is_word_ns(p.namespace()))
    };
    let in_mark = under("rPr")
        && n.parent()
            .and_then(|p| p.parent())
            .is_some_and(|g| g.local_name() == "pPr" && is_word_ns(g.namespace()));
    Some(match name {
        "ins" if under("numPr") => RevKind::NumPrIns,
        "ins" if under("trPr") => RevKind::RowIns,
        "del" if under("trPr") => RevKind::RowDel,
        "ins" if in_mark => RevKind::MarkIns,
        "del" if in_mark => RevKind::MarkDel,
        "moveFrom" if in_mark => RevKind::MarkMoveFrom,
        "moveTo" if in_mark => RevKind::MarkMoveTo,
        "ins" => RevKind::Ins,
        "del" => RevKind::Del,
        "moveFrom" => RevKind::MoveFrom,
        "moveTo" => RevKind::MoveTo,
        _ => return None,
    })
}

/// Which branch of an `mc:AlternateContent` the element is in, looking at every ancestor: in an
/// untaken branch anywhere above makes it untaken.
pub(super) fn branch_of(n: &NodeRef<'_>) -> Branch {
    let mut branch = Branch::Outside;
    let mut child = *n;
    while let Some(parent) = child.parent() {
        if mc::is_branch(&parent)
            && let Some(ac) = parent.parent()
        {
            if mc::taken_branch(&ac).is_some_and(|t| t == parent) {
                branch = Branch::Taken;
            } else {
                return Branch::Untaken;
            }
        }
        child = parent;
    }
    branch
}

/// Every revision in a part, in document order, each marked with its branch. This is the reader for
/// the parts that have no [`Story`], such as `word/styles.xml` and `word/numbering.xml`, whose property
/// changes carry ids of their own. Both branches of an `mc:AlternateContent` are listed.
pub fn part_revisions(tree: &SpanTree) -> Vec<Revision> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root().children()];
    while let Some(top) = stack.last_mut() {
        let Some(n) = top.next() else {
            stack.pop();
            continue;
        };
        if !n.is_element() {
            continue;
        }
        if let Some(mut r) = revision_at(&n) {
            r.branch = branch_of(&n);
            out.push(r);
        }
        stack.push(n.children());
    }
    out
}

impl Story {
    /// Every revision in the part, in document order, each marked with its branch. Both branches of an
    /// `mc:AlternateContent` are listed: a caller that counts or shows revisions skips
    /// [`Branch::Untaken`], and one that must resolve every copy reads both.
    pub fn revisions(&self, tree: &SpanTree) -> Vec<Revision> {
        part_revisions(tree)
    }
}
