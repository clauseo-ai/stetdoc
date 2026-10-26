//! What numbering needs to know about each paragraph, read in one pass.
//!
//! `numbering` counts list items from the paragraphs' direct properties, and in `Original` from the
//! values a paragraph property change replaced. It may not read XML, so `Story::num_facts` reads what
//! it needs: the direct style and numbering, the same two as they were before a tracked paragraph
//! property change, the revisions on the paragraph mark, which modes the paragraph is in, and whether
//! it is the empty paragraph that only carries a section break. No field pass runs.

use super::names::{int, is_w, w_attr, w_child};
use super::paragraph::NumPr;
use super::story::{BlockKind, MarkRevs, Story};
use super::text_build::wrappers_above;
use super::text_model::Vis;
use crate::xml::{NodeId, NodeRef, SpanTree};
use std::borrow::Cow;

/// The paragraph style of a `w:pPr`: `w:pStyle/@w:val`.
pub(super) fn style_in<'a>(ppr: &NodeRef<'a>) -> Option<Cow<'a, str>> {
    w_attr(&w_child(ppr, "pStyle")?, "val")
}

/// The direct numbering of a `w:pPr`: its `w:numPr`.
pub(super) fn num_pr_in(ppr: &NodeRef<'_>) -> Option<NumPr> {
    let num_pr = w_child(ppr, "numPr")?;
    let read = |name: &str| {
        w_child(&num_pr, name)
            .and_then(|e| w_attr(&e, "val"))
            .and_then(|v| int(&v))
    };
    Some(NumPr {
        num_id: read("numId"),
        ilvl: read("ilvl").and_then(|v| u8::try_from(v).ok()),
    })
}

/// In which modes a paragraph exists, from the wrappers above it: an inserted row or cell, a
/// deleted one, and block-level revision wrappers. The rule is the text model's, so the two agree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence {
    /// In `Final` and in `Original`.
    Always,
    /// In `Final` only: it is inside an insertion.
    FinalOnly,
    /// In `Original` only: it is inside a deletion.
    OriginalOnly,
    /// In neither: an insertion inside a deletion.
    Never,
}

/// The properties a tracked paragraph property change replaced: `w:pPrChange/w:pPr`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OldPPr {
    /// The paragraph style the paragraph had before, if it had one.
    pub style_id: Option<Box<str>>,
    /// The direct numbering the paragraph had before, if it had any.
    pub num_pr: Option<NumPr>,
}

/// What numbering reads about one paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumFact {
    /// The entry index of the paragraph in the story.
    pub entry: usize,
    /// The paragraph element.
    pub node: NodeId,
    /// The direct paragraph style, `w:pPr/w:pStyle`.
    pub style_id: Option<Box<str>>,
    /// The direct numbering, `w:pPr/w:numPr`.
    pub num_pr: Option<NumPr>,
    /// What a tracked paragraph property change replaced, when there is one. `Some` exactly when
    /// `w:pPr/w:pPrChange/w:pPr` exists, so a missing old style reads as `Some` with no style.
    pub old: Option<OldPPr>,
    /// The revisions on the paragraph mark, all of them: a mark that was inserted and then deleted
    /// holds both. Read it with [`MarkRevs::gone_in`].
    pub mark_revs: MarkRevs,
    /// In which modes the paragraph exists.
    pub presence: Presence,
    /// True if `w:pPr` has a `w:sectPr` and no `w:t` is below the paragraph outside `w:pPr`.
    pub empty_section_break: bool,
    /// True if the next entry is a paragraph in the same container (the same cell, note, comment,
    /// header, footer or body). Independent of mode: it is the structural half of the rule that a
    /// revised paragraph mark joins the next paragraph, which Word ignores at the end of a cell or a
    /// story. The mark joins in `Final` when it is deleted or moved away and `join_applies`, and in
    /// `Original` when it is inserted or moved in and `join_applies` ([`Story::join_run`]).
    pub join_applies: bool,
}

/// True if any `w:t` is below the paragraph, `w:pPr` aside.
fn has_text(p: &NodeRef<'_>) -> bool {
    let mut stack = vec![p.children()];
    while let Some(top) = stack.last_mut() {
        let Some(c) = top.next() else {
            stack.pop();
            continue;
        };
        if !c.is_element() || is_w(&c, "pPr") {
            continue;
        }
        if is_w(&c, "t") {
            return true;
        }
        stack.push(c.children());
    }
    false
}

fn presence_above(p: &NodeRef<'_>) -> Presence {
    let mut vis = Vis::FINAL | Vis::ORIGINAL;
    for (kind, _) in wrappers_above(p) {
        if kind.is_insertion() {
            vis &= !Vis::ORIGINAL;
        }
        if kind.is_deletion() {
            vis &= !Vis::FINAL;
        }
    }
    match vis {
        v if v == Vis::FINAL | Vis::ORIGINAL => Presence::Always,
        Vis::FINAL => Presence::FinalOnly,
        Vis::ORIGINAL => Presence::OriginalOnly,
        _ => Presence::Never,
    }
}

impl Story {
    /// One fact for every paragraph of the story, in entry order. One pass, no field pass.
    pub fn num_facts(&self, tree: &SpanTree) -> Vec<NumFact> {
        let mut out = Vec::new();
        let mut known = None;
        for (entry, e) in self.entries.iter().enumerate() {
            if e.kind != BlockKind::Paragraph {
                continue;
            }
            let p = tree.node_ref(e.node);
            let ppr = w_child(&p, "pPr");
            let old = ppr
                .as_ref()
                .and_then(|ppr| w_child(ppr, "pPrChange"))
                .and_then(|change| w_child(&change, "pPr"))
                .map(|old| OldPPr {
                    style_id: style_in(&old).map(|s| s.into()),
                    num_pr: num_pr_in(&old),
                });
            out.push(NumFact {
                entry,
                node: e.node,
                style_id: ppr.as_ref().and_then(style_in).map(|s| s.into()),
                num_pr: ppr.as_ref().and_then(num_pr_in),
                old,
                mark_revs: e.mark_revs(),
                presence: presence_above(&p),
                empty_section_break: ppr
                    .as_ref()
                    .is_some_and(|ppr| w_child(ppr, "sectPr").is_some())
                    && !has_text(&p),
                join_applies: self.joins_with_next_in_order(tree, entry, &mut known),
            });
        }
        out
    }
}
