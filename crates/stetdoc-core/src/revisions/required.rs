//! Which paragraphs a container needs (design `revisions.md`, 3.2.2): `delete_block` removes a paragraph unless this says why
//! it must stay, and then it deletes only the paragraph's content and keeps the empty paragraph.
//!
//! The neighbours are read as accepting everything would leave them: a paragraph whose mark is deleted joins the paragraph
//! after it, so it is passed over, and range markers between blocks are passed over too. The rules: a cell, a header, a
//! footer, a note, a comment, a text box and the body each keep at least one paragraph and end with one directly, so the last
//! paragraph stays when only tables, content controls or other blocks follow it; two tables are never
//! left next to each other, since Word would join them; the body's last paragraph stays after a paragraph that ends a section,
//! so that the last section is not left empty; a paragraph that holds a section break stays, on purpose (`FIDELITY.md`); and
//! the last paragraph of a block-level content control or another wrapper of blocks stays. `left_on_accept` says whether a
//! paragraph with no paragraph after it is left with nothing but markers once its deletion is accepted, which `delete_block`
//! needs before it deletes such a mark.

use super::blocks::mark_of;
use super::join::{MARKERS, is_empty_run, is_leftover};
use super::names::{is_blank, is_w};
use super::props_edit::find_chain;
use crate::wordml::NodeId;
use crate::xml::{NodeRef, SpanTree};

/// Why a deleted paragraph was emptied and kept rather than removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeptReason {
    /// It holds a section break, which stetdoc keeps.
    SectionBreak,
    /// It is the only paragraph left in its container, named by the words given, such as `a cell`.
    Only(&'static str),
    /// It ends its container, the first words, right after a table or another block that is not a paragraph, the second
    /// words, which cannot end it.
    AfterBlock(&'static str, &'static str),
    /// It is the last paragraph of its container, the first words, and blocks that are not paragraphs follow it, so the
    /// container would end with the last of them, the second words.
    EndsBefore(&'static str, &'static str),
    /// It stands between two tables, which would join without it.
    BetweenTables,
    /// It is the body's last paragraph, right after a paragraph that ends a section.
    AfterSectionBreak,
    /// It is the last paragraph of a wrapper of blocks, named by the words given, such as `a content control`.
    EndsWrapper(&'static str),
    /// It holds range markers or comment references, and no paragraph beside it can take them.
    Markers,
    /// It stands right before content that is not a block, such as a `w:altChunk`, across which a deleted paragraph cannot be
    /// joined.
    BeforeOpaque,
    /// It holds content that the deletion leaves, such as the shell of a content control, so accepting could not remove it.
    Holds,
    /// It holds another author's pending deletion, which accepting the author's revisions does not remove.
    Pending,
    /// It ends its container, and it cannot be joined with the paragraph before it, whose mark a tracked deletion deletes
    /// in its place, for the cause the words give, such as `the paragraph before it is read-only`.
    CannotJoin(&'static str),
    /// It ends its container, and the paragraph before it is empty: joined, the two would read as that empty paragraph,
    /// whose ref accepting retires, so the line left would be named by the deleted paragraph's ref.
    EmptyBefore,
}

impl KeptReason {
    /// The receipt's sentence for the paragraph `shown`: `p2 is the only paragraph of a cell; its text was deleted and the
    /// empty paragraph kept`.
    pub(crate) fn sentence(self, shown: &str) -> String {
        let why = match self {
            KeptReason::SectionBreak => format!(
                "{shown} holds a section break, which stetdoc keeps so that the section keeps its layout, headers and footers"
            ),
            KeptReason::Only(container) => {
                format!("{shown} is the only paragraph of {container}")
            }
            KeptReason::AfterBlock(container, block) => format!(
                "{shown} ends {container} right after {block}, which cannot end {container}"
            ),
            KeptReason::EndsBefore(container, block) => format!(
                "{shown} is the last paragraph of {container}, and without it {container} would end with {block}, which cannot end it"
            ),
            KeptReason::BetweenTables => format!(
                "{shown} stands between two tables, which Word would join into one without it"
            ),
            KeptReason::AfterSectionBreak => format!(
                "{shown} is the body's last paragraph and follows a paragraph that ends a section, which would leave the last section empty without it"
            ),
            KeptReason::EndsWrapper(wrapper) => {
                format!("{shown} is the last paragraph of {wrapper}")
            }
            KeptReason::Holds => format!(
                "{shown} holds content that stays when its text is deleted, such as a content control, and no paragraph follows it to take that content"
            ),
            KeptReason::BeforeOpaque => format!(
                "{shown} stands right before an imported document part, and a deleted paragraph cannot be joined across it"
            ),
            KeptReason::Pending => format!(
                "{shown} holds another author's pending deletion, and no paragraph follows it to take that, so it can go only once that change is resolved"
            ),
            KeptReason::CannotJoin(cause) => format!(
                "{shown} ends its container and cannot be joined with the paragraph before it, because {cause}"
            ),
            KeptReason::EmptyBefore => format!(
                "{shown} ends its container and the paragraph before it is empty, so there is no text to take its place"
            ),
            KeptReason::Markers => format!(
                "{shown} holds bookmarks, comment ranges or other markers, and no paragraph stands beside it to take them"
            ),
        };
        format!("{why}; its text was deleted and the empty paragraph kept")
    }
}

/// What stands on one side of a paragraph, once the paragraphs whose marks are deleted are passed over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    /// Nothing: the paragraph is at that end of its container.
    Nothing,
    /// A paragraph whose mark stays.
    Paragraph(NodeId),
    /// A table.
    Table,
    /// Another block, such as a content control or a `w:altChunk`, named by the words given.
    Block(&'static str),
}

/// True if `c` is passed over when the neighbours of a block are read: whitespace, range markers, a proofing mark, and the
/// properties of the container (a cell's `w:tcPr`, a control's `w:sdtPr`, the body's last `w:sectPr`).
fn passed(c: &NodeRef<'_>) -> bool {
    !c.is_element()
        || is_blank(c)
        || MARKERS.iter().any(|m| is_w(c, m))
        || [
            "proofErr",
            "tcPr",
            "sdtPr",
            "sdtEndPr",
            "sectPr",
            "customXmlPr",
        ]
        .iter()
        .any(|m| is_w(c, m))
}

/// What stands after `para` (`forward`) or before it among its siblings.
fn side(tree: &SpanTree, para: NodeId, forward: bool) -> Side {
    fn step<'t>(n: &NodeRef<'t>, forward: bool) -> Option<NodeRef<'t>> {
        if forward {
            n.next_sibling()
        } else {
            n.prev_sibling()
        }
    }
    let mut cur = step(&tree.node_ref(para), forward);
    while let Some(c) = cur {
        if passed(&c) {
            cur = step(&c, forward);
            continue;
        }
        if is_w(&c, "p") {
            if mark_of(tree, c.id()).is_deleted() {
                cur = step(&c, forward);
                continue;
            }
            return Side::Paragraph(c.id());
        }
        return if is_w(&c, "tbl") {
            Side::Table
        } else {
            Side::Block(block_words(&c))
        };
    }
    Side::Nothing
}

/// The words for a block that is not a paragraph.
fn block_words(c: &NodeRef<'_>) -> &'static str {
    match c.local_name() {
        "tbl" => "a table",
        "sdt" => "a content control",
        "customXml" => "a custom XML element",
        "altChunk" => "an imported document part",
        _ => "a block that is not a paragraph",
    }
}

/// After `para` among its siblings: whether a paragraph whose mark stays follows it, and the words for the last block that
/// is not a paragraph, if any follows.
fn after(tree: &SpanTree, para: NodeId) -> (bool, Option<&'static str>) {
    let mut live = false;
    let mut last = None;
    let mut cur = tree.node_ref(para).next_sibling();
    while let Some(c) = cur {
        if !passed(&c) {
            if is_w(&c, "p") {
                live |= !mark_of(tree, c.id()).is_deleted();
            } else {
                last = Some(block_words(&c));
            }
        }
        cur = c.next_sibling();
    }
    (live, last)
}

/// The words for a container that must end with a paragraph, or, as the error, the words for a wrapper of blocks.
fn container_words(parent: &NodeRef<'_>) -> Result<&'static str, &'static str> {
    Ok(match parent.local_name() {
        "body" => "the body",
        "tc" => "a cell",
        "hdr" => "a header",
        "ftr" => "a footer",
        "footnote" => "a footnote",
        "endnote" => "an endnote",
        "comment" => "a comment",
        "txbxContent" => "a text box",
        "sdtContent" => return Err("a content control"),
        _ => return Err("a wrapper of blocks"),
    })
}

/// Why `para` must stay when it is deleted, or `None` when it may go.
pub(super) fn required(tree: &SpanTree, para: NodeId) -> Option<KeptReason> {
    if find_chain(tree, para, &["pPr", "sectPr"]).is_some() {
        return Some(KeptReason::SectionBreak);
    }
    let parent = tree.node_ref(para).parent()?;
    let before = side(tree, para, false);
    let (live_after, last_block) = after(tree, para);
    match side(tree, para, true) {
        Side::Paragraph(_) => None,
        // A container ends with a paragraph directly: the last paragraph stays when blocks that are not paragraphs follow it.
        Side::Table | Side::Block(_) if !live_after => Some(match container_words(&parent) {
            Err(wrapper) => KeptReason::EndsWrapper(wrapper),
            Ok(container) => KeptReason::EndsBefore(
                container,
                last_block.unwrap_or("a block that is not a paragraph"),
            ),
        }),
        Side::Block(_) => None,
        Side::Table => (before == Side::Table).then_some(KeptReason::BetweenTables),
        Side::Nothing => match container_words(&parent) {
            Err(wrapper) => Some(KeptReason::EndsWrapper(wrapper)),
            Ok(container) => match before {
                Side::Nothing => Some(KeptReason::Only(container)),
                Side::Table => Some(KeptReason::AfterBlock(container, "a table")),
                Side::Block(block) => Some(KeptReason::AfterBlock(container, block)),
                Side::Paragraph(prev) => (is_w(&parent, "body")
                    && find_chain(tree, prev, &["pPr", "sectPr"]).is_some())
                .then_some(KeptReason::AfterSectionBreak),
            },
        },
    }
}

/// True if a paragraph whose mark stays follows `para` among its siblings, past the paragraphs whose marks are deleted: only
/// then does a deleted mark of `para` join it with a paragraph that stays.
pub(super) fn live_after(tree: &SpanTree, para: NodeId) -> bool {
    matches!(side(tree, para, true), Side::Paragraph(_))
}

/// The last paragraph of the container after `para`, when the paragraphs between are joined on to it through deleted marks:
/// the paragraph a pending merge of `para` ends in, when that is the container's end.
pub(super) fn end_joined_from(tree: &SpanTree, para: NodeId) -> Option<NodeId> {
    match side(tree, para, true) {
        Side::Paragraph(last) if side(tree, last, true) == Side::Nothing => Some(last),
        Side::Paragraph(_) | Side::Nothing | Side::Table | Side::Block(_) => None,
    }
}

/// The paragraph whose mark a tracked deletion of `para` deletes in place of `para`'s own, when `para` is the last
/// paragraph of a container and a paragraph whose mark stays comes before it. Word cannot accept a deleted mark on the last
/// paragraph of a container: in the body the revision stays pending, and in a cell Word removes the table (hand check, Word
/// for Mac 16.114.4). Word's own gesture deletes the mark of the paragraph before and the last paragraph's text, and so does
/// stetdoc.
pub(super) fn previous_to_join(tree: &SpanTree, para: NodeId) -> Option<NodeId> {
    let parent = tree.node_ref(para).parent()?;
    container_words(&parent).ok()?;
    if side(tree, para, true) != Side::Nothing {
        return None;
    }
    match side(tree, para, false) {
        Side::Paragraph(prev) => Some(prev),
        Side::Nothing | Side::Table | Side::Block(_) => None,
    }
}

/// What accepting the author's revisions would leave in `para`, for a paragraph with no paragraph after it, whose deleted
/// mark accepting removes only when nothing but range markers and comment references is left (`resolve_mark`, design 3.2.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Left {
    /// Nothing but markers: accepting removes the paragraph.
    Markers,
    /// Another author's pending deletion, which accepting the author's revisions does not remove.
    Pending,
    /// Other content, such as the shell of a content control or a simple field that keeps some of its content.
    Content,
}

/// What accepting the revisions of `author` leaves in `para`. A deletion of the author's goes, and so do an empty run, a
/// proofing mark, and a simple field whose content all goes, which accepting removes with its `w:fldData` (`cleanup`). A deletion of another author's stays until that author's change is resolved, so a deleted mark of the
/// author's would not remove the paragraph then, and the Final view and accept of the author's revisions would disagree.
pub(super) fn left_on_accept(tree: &SpanTree, para: NodeId, author: Option<&str>) -> Left {
    fn walk(n: &NodeRef<'_>, author: Option<&str>, left: &mut Left) {
        for c in n.children() {
            if is_blank(&c) || !c.is_element() {
                if !is_blank(&c) {
                    *left = Left::Content;
                }
                continue;
            }
            if is_w(&c, "del") || is_w(&c, "moveFrom") {
                let mine = crate::wordml::revision_at(&c)
                    .is_some_and(|r| author.is_none() || r.author.as_deref() == author);
                if !mine && *left == Left::Markers {
                    *left = Left::Pending;
                }
            } else if is_w(&c, "ins")
                || is_w(&c, "moveTo")
                || is_w(&c, "hyperlink")
                || is_w(&c, "smartTag")
                || is_w(&c, "fldSimple")
            {
                walk(&c, author, left);
            } else if !(is_leftover(&c)
                || is_empty_run(&c)
                || is_w(&c, "proofErr")
                || is_w(&c, "pPr") && n.local_name() == "p"
                || is_w(&c, "fldData") && n.local_name() == "fldSimple")
            {
                *left = Left::Content;
            }
        }
    }
    let mut left = Left::Markers;
    walk(&tree.node_ref(para), author, &mut left);
    left
}

/// True if `para` holds range markers or comment references anywhere, which accepting its deletion leaves in it.
pub(super) fn holds_markers(tree: &SpanTree, para: NodeId) -> bool {
    let mut stack = vec![tree.node_ref(para)];
    while let Some(n) = stack.pop() {
        for c in n.child_elements() {
            if MARKERS.iter().any(|m| is_w(&c, m)) || is_w(&c, "commentReference") {
                return true;
            }
            stack.push(c);
        }
    }
    false
}
