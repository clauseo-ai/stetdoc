//! Where a piece of text is, in the terms the cascade needs.
//!
//! A `Site` is built by the caller (`PartsView::site` does it for everyone) from `wordml`'s accessors.
//! It borrows the tree and holds no data of its own. The reads that only `styles` knows how to
//! interpret (what a tracked formatting change replaced, the shading of the cell around a paragraph,
//! the page colour) are the functions at the end of this file, so the one builder asks them and
//! no caller has to know where in the XML each lives.

use crate::wordml::{Ns, Paragraph, Props, PropsRef, SpanTree, TableSite};
use crate::xml::NodeRef;
use std::borrow::Cow;

/// The numbering level that applies to a paragraph, and where the numbering came from.
#[derive(Clone, Copy, Debug)]
pub struct LevelSite<'a> {
    /// The level's `w:pPr`.
    pub props: &'a Props,
    /// The `w:numId`.
    pub num_id: i64,
    /// The level, 0 to 8.
    pub ilvl: u8,
    /// The numbering came from the paragraph style, not from a direct `w:numPr`.
    pub from_style: bool,
}

/// A run: its character style and its direct formatting.
#[derive(Clone, Copy, Debug, Default)]
pub struct RunSite<'a> {
    /// The `w:rStyle` id.
    pub style: Option<&'a str>,
    /// The run's `w:rPr`. In Original mode, the snapshot of a `w:rPrChange`.
    pub direct: Option<PropsRef<'a>>,
    /// The run's text is complex script (Arabic, Hebrew, Thai, Hindi), so its size is `szCs` and not `sz`.
    /// The caller knows the text, the cascade does not.
    pub complex_script: bool,
}

/// A paragraph, and optionally one of its runs.
#[derive(Clone, Debug, Default)]
pub struct Site<'a> {
    /// The `w:pStyle` id. `None` means the default paragraph style.
    pub style: Option<&'a str>,
    /// The direct `w:pPr`. In Original mode, the snapshot of a `w:pPrChange`.
    pub ppr: Option<PropsRef<'a>>,
    /// The paragraph mark's `w:pPr/w:rPr`.
    pub mark: Option<PropsRef<'a>>,
    /// The innermost table around the paragraph.
    pub table: Option<&'a TableSite>,
    /// The numbering level, if the paragraph is numbered.
    pub level: Option<LevelSite<'a>>,
    /// The paragraph's own `w:numPr` says `w:numId` 0: the indent that comes from its styles goes.
    pub numbering_removed: bool,
    /// The fill of the cell's own `w:tcPr/w:shd`, for the white-text check.
    pub cell_fill: Option<&'a str>,
    /// The document's `w:background` colour, for the white-text check.
    pub background: Option<&'a str>,
    /// The run. `None` means the paragraph itself: `effective` gives its `pPr`, and `hidden` its mark.
    pub run: Option<RunSite<'a>>,
}

/// What the caller needs of a paragraph's direct properties to build a `Site`.
#[derive(Clone, Debug, Default)]
pub struct DirectParagraph<'a> {
    /// The `w:pStyle` id.
    pub style: Option<Cow<'a, str>>,
    /// The `w:pPr`.
    pub ppr: Option<PropsRef<'a>>,
    /// The paragraph mark's `w:pPr/w:rPr`.
    pub mark: Option<PropsRef<'a>>,
}

/// The properties that a tracked change inside `props` replaced: the `inner` container of the `change`
/// element, such as the `w:pPr` inside a `w:pPrChange`. `None` when there is no such change, or when
/// the change holds no snapshot.
fn snapshot<'a>(props: Option<PropsRef<'a>>, change: &str, inner: &str) -> Option<PropsRef<'a>> {
    let change = props?.get(change)?;
    PropsRef::new(change).get(inner).map(PropsRef::new)
}

/// The direct properties of a paragraph as `original` mode sees them, or as every other mode does.
///
/// In `Original` a paragraph whose properties were changed under tracking has the properties the
/// change replaced (`w:pPrChange/w:pPr`), and its mark has the ones its own change replaced
/// (`w:rPr/w:rPrChange/w:rPr`). A snapshot with no `w:pStyle` means the default paragraph style.
pub fn direct_paragraph<'a>(paragraph: &Paragraph<'a>, original: bool) -> DirectParagraph<'a> {
    let (ppr, mark) = (paragraph.ppr(), paragraph.mark_props());
    if !original {
        return DirectParagraph {
            style: paragraph.style_id(),
            ppr,
            mark,
        };
    }
    let old = snapshot(ppr, "pPrChange", "pPr");
    DirectParagraph {
        style: match old {
            Some(old) => old.val("pStyle"),
            None => paragraph.style_id(),
        },
        ppr: old.or(ppr),
        mark: snapshot(mark, "rPrChange", "rPr").or(mark),
    }
}

/// The direct `w:rPr` of a run as `original` mode sees it: with a tracked change, what the change
/// replaced (`w:rPrChange/w:rPr`).
pub fn direct_run(rpr: Option<PropsRef<'_>>, original: bool) -> Option<PropsRef<'_>> {
    if original {
        snapshot(rpr, "rPrChange", "rPr").or(rpr)
    } else {
        rpr
    }
}

/// True if the paragraph's own `w:numPr` says `w:numId` 0, which takes away the numbering, and the
/// indent that goes with it, that its styles give.
pub fn numbering_removed(ppr: Option<PropsRef<'_>>) -> bool {
    ppr.and_then(|p| p.get("numPr"))
        .and_then(|n| PropsRef::new(n).val("numId"))
        .is_some_and(|v| v.trim().parse::<i64>() == Ok(0))
}

/// The `w:fill` of the shading of the cell that holds the paragraph element `paragraph`: the nearest
/// `w:tc` above it, and its `w:tcPr/w:shd`. `None` outside a table and for a cell with no shading.
pub fn cell_fill(paragraph: NodeRef<'_>) -> Option<Cow<'_, str>> {
    let mut at = paragraph.parent();
    while let Some(node) = at {
        if node.local_name() == "tc" && Ns::Word.matches_uri(node.namespace()) {
            let tc_pr = PropsRef::new(node).get("tcPr")?;
            return PropsRef::new(tc_pr).attr("shd", "fill");
        }
        at = node.parent();
    }
    None
}

/// The page colour of a document: `w:background/@w:color` in the main part.
pub fn page_background(main: &SpanTree) -> Option<Cow<'_, str>> {
    PropsRef::new(main.root()).attr("background", "color")
}
