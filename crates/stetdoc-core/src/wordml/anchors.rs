//! Range markers that span paragraphs: bookmarks, comment ranges and move ranges, paired.
//!
//! A paragraph's text model sees the markers inside it as marks with a position and no width. It
//! cannot pair a start in one paragraph with an end in another, which 23 bookmarks in 5 curated
//! documents and 4 comment ranges in 2 do. `Story::anchors` is one read-only walk over the part, run
//! on demand and not cached (the callers' caches are `state`'s). Markers that sit between blocks, as
//! children of the body, a cell, a row or a table, are 282 of 1,993 in the curated set, so they are paired too,
//! with no paragraph. `docs/design/wordml.md` 3.9.

use super::mc;
use super::names::{W, W2006, int, is_in, is_word_ns, w_attr};
use super::story::Story;
use crate::xml::{Children, NodeId, NodeRef, SpanTree};
use std::collections::HashMap;

/// Where a marker is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loc {
    /// The paragraph that holds the marker. `None` when the marker sits between blocks: a child of the
    /// body, of a cell, of a row or of a table, or of a block-level control.
    pub paragraph: Option<NodeId>,
    /// The marker element.
    pub node: NodeId,
}

/// A bookmark, paired.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bookmark {
    /// The `w:id`, if it is a whole number. A bookmark with none is never paired.
    pub id: Option<i64>,
    /// The `w:name`, empty if the start has none or there is no start.
    pub name: String,
    /// The `w:bookmarkStart`, if the bookmark has one.
    pub start: Option<Loc>,
    /// The `w:bookmarkEnd`, if the bookmark has one.
    pub end: Option<Loc>,
    /// True if the name starts with an underscore (`_Toc`, `_Ref`, `_GoBack`): Word hides it, and it
    /// must survive edits all the same.
    pub hidden: bool,
}

/// A comment's anchors in the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentAnchor {
    /// The comment's `w:id`.
    pub id: i64,
    /// The `w:commentRangeStart`, if there is one.
    pub start: Option<Loc>,
    /// The `w:commentRangeEnd`, if there is one.
    pub end: Option<Loc>,
    /// The `w:commentReference` in a run, if there is one.
    pub reference: Option<Loc>,
}

/// A move range, paired.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveAnchor {
    /// The `w:id`, if it is a whole number.
    pub id: Option<i64>,
    /// The `w:name`, empty if the start has none or there is no start.
    pub name: String,
    /// True for `w:moveFromRangeStart` and `w:moveFromRangeEnd`, false for the destination.
    pub from: bool,
    /// The range start, if there is one.
    pub start: Option<Loc>,
    /// The range end, if there is one.
    pub end: Option<Loc>,
}

/// Every pair of range markers in a part.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Anchors {
    /// Bookmarks, in order of their first marker.
    pub bookmarks: Vec<Bookmark>,
    /// Comment anchors, in order of their first marker.
    pub comments: Vec<CommentAnchor>,
    /// Move ranges, in order of their first marker.
    pub moves: Vec<MoveAnchor>,
}

/// The unpaired starts seen so far for each id, nearest last.
#[derive(Default)]
struct Open {
    by_id: HashMap<(bool, i64), Vec<usize>>,
}

impl Open {
    fn push(&mut self, from: bool, id: Option<i64>, at: usize) {
        if let Some(id) = id {
            self.by_id.entry((from, id)).or_default().push(at);
        }
    }

    fn take(&mut self, from: bool, id: Option<i64>) -> Option<usize> {
        self.by_id.get_mut(&(from, id?))?.pop()
    }
}

struct Frame<'t> {
    it: Children<'t>,
    paragraph: Option<NodeId>,
}

impl Story {
    /// Pairs the bookmarks, comment ranges and move ranges of the part, in document order. A bookmark
    /// end pairs with the nearest preceding unpaired start of the same id, so an id reused after its
    /// end is a second bookmark. An end with no start has no start, a start with no end has no end,
    /// and neither is an error. Text boxes are not entered: their markers are not paired with the
    /// host's. Of an `mc:AlternateContent` only the branch Word reads is walked.
    pub fn anchors(&self, tree: &SpanTree) -> Anchors {
        let mut out = Anchors::default();
        let mut books = Open::default();
        let mut comments = Open::default();
        let mut moves = Open::default();
        let mut stack = vec![Frame {
            it: tree.root().children(),
            paragraph: None,
        }];
        while let Some(top) = stack.last_mut() {
            let Some(c) = top.it.next() else {
                stack.pop();
                continue;
            };
            if !c.is_element() {
                continue;
            }
            let paragraph = top.paragraph;
            let loc = |p: Option<NodeId>| Loc {
                paragraph: p,
                node: c.id(),
            };
            let name = if is_word_ns(c.namespace()) {
                c.local_name()
            } else {
                ""
            };
            match name {
                "bookmarkStart" => {
                    let id = w_attr(&c, "id").and_then(|v| int(&v));
                    let text = w_attr(&c, "name")
                        .map(|n| n.into_owned())
                        .unwrap_or_default();
                    books.push(false, id, out.bookmarks.len());
                    out.bookmarks.push(Bookmark {
                        id,
                        hidden: text.starts_with('_'),
                        name: text,
                        start: Some(loc(paragraph)),
                        end: None,
                    });
                }
                "bookmarkEnd" => {
                    let id = w_attr(&c, "id").and_then(|v| int(&v));
                    match books.take(false, id).and_then(|i| out.bookmarks.get_mut(i)) {
                        Some(b) => b.end = Some(loc(paragraph)),
                        None => out.bookmarks.push(Bookmark {
                            id,
                            name: String::new(),
                            start: None,
                            end: Some(loc(paragraph)),
                            hidden: false,
                        }),
                    }
                }
                "commentRangeStart" => {
                    if let Some(id) = w_attr(&c, "id").and_then(|v| int(&v)) {
                        comments.push(false, Some(id), out.comments.len());
                        out.comments.push(CommentAnchor {
                            id,
                            start: Some(loc(paragraph)),
                            end: None,
                            reference: None,
                        });
                    }
                }
                "commentRangeEnd" => {
                    if let Some(id) = w_attr(&c, "id").and_then(|v| int(&v)) {
                        match comments
                            .take(false, Some(id))
                            .and_then(|i| out.comments.get_mut(i))
                        {
                            Some(a) => a.end = Some(loc(paragraph)),
                            None => out.comments.push(CommentAnchor {
                                id,
                                start: None,
                                end: Some(loc(paragraph)),
                                reference: None,
                            }),
                        }
                    }
                }
                "commentReference" => {
                    if let Some(id) = w_attr(&c, "id").and_then(|v| int(&v)) {
                        let free = out
                            .comments
                            .iter_mut()
                            .rev()
                            .find(|a| a.id == id && a.reference.is_none());
                        match free {
                            Some(a) => a.reference = Some(loc(paragraph)),
                            None => out.comments.push(CommentAnchor {
                                id,
                                start: None,
                                end: None,
                                reference: Some(loc(paragraph)),
                            }),
                        }
                    }
                }
                "moveFromRangeStart" | "moveToRangeStart" => {
                    let from = name == "moveFromRangeStart";
                    let id = w_attr(&c, "id").and_then(|v| int(&v));
                    moves.push(from, id, out.moves.len());
                    out.moves.push(MoveAnchor {
                        id,
                        name: w_attr(&c, "name")
                            .map(|n| n.into_owned())
                            .unwrap_or_default(),
                        from,
                        start: Some(loc(paragraph)),
                        end: None,
                    });
                }
                "moveFromRangeEnd" | "moveToRangeEnd" => {
                    let from = name == "moveFromRangeEnd";
                    let id = w_attr(&c, "id").and_then(|v| int(&v));
                    match moves.take(from, id).and_then(|i| out.moves.get_mut(i)) {
                        Some(m) => m.end = Some(loc(paragraph)),
                        None => out.moves.push(MoveAnchor {
                            id,
                            name: String::new(),
                            from,
                            start: None,
                            end: Some(loc(paragraph)),
                        }),
                    }
                }
                "p" => stack.push(Frame {
                    it: c.children(),
                    paragraph: Some(c.id()),
                }),
                // a drawing, a picture and an object hold no range marker of their own, and the text boxes
                // in them are other stories, so the walk does not go in: a drawing has dozens of elements
                "drawing" | "pict" | "object" => {}
                _ if is_text_box(&c) => {}
                _ => descend(&mut stack, &c, paragraph),
            }
        }
        out
    }
}

/// True for the content of a text box, in either namespace spelling.
fn is_text_box(n: &NodeRef<'_>) -> bool {
    is_in(n, &W, "txbxContent") || is_in(n, &[W2006], "txbxContent")
}

/// Pushes the children Word reads of `c`, if it reads any, as the text model reads them: all the
/// children of an element of the Word namespace; of any other element, what `mc` says
/// (`mc::disposition`): the children of the branch an `mc:AlternateContent` takes, of an element that
/// `mc:ProcessContent` names, or of one Word reads as it stands, and none of an element Word skips.
fn descend<'t>(stack: &mut Vec<Frame<'t>>, c: &NodeRef<'t>, paragraph: Option<NodeId>) {
    let children = if is_word_ns(c.namespace()) {
        c.children()
    } else {
        match mc::disposition(c) {
            mc::Mc::Normal | mc::Mc::ProcessContent => c.children(),
            mc::Mc::Branch(branch) => branch.children(),
            mc::Mc::NoBranch | mc::Mc::Skip => return,
        }
    };
    stack.push(Frame {
        it: children,
        paragraph,
    });
}
