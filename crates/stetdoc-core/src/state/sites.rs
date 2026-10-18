//! `SiteHandle`: the `Site`s of one story in one mode, assembled once from what each owner knows.
//!
//! A `Site` says where a piece of text is, in the terms the cascade needs: its paragraph style and direct
//! properties, the table around it, the numbering level of its paragraph, the shading behind it. Three
//! owners know the parts of that: `wordml` (the paragraph, its runs, the table site), `numbering` (the
//! level and whether a style gave it) and `styles` (what a tracked formatting change replaced, the shading
//! of a cell, the page colour). `state` holds the products of the last two, so it builds the site, and no
//! caller assembles one by hand.
//!
//! The handle holds the story, the tree and the cascade, and keeps what it read about the paragraph it
//! was last asked for, so a loop over the runs of a paragraph reads the paragraph once. The story's table
//! sites are computed on the first paragraph in a table, in one pass over the story, and not at all for a
//! story with no table. The story's numbers are computed on the first site that needs the numbering
//! level, which is a paragraph site (no run): a run's own properties never read the level, so asking
//! about runs never builds the numbers.

use super::errors::internal;
use super::parts::PartId;
use super::view::PartsView;
use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::numbering::{StoryNumbers, level_site};
use crate::styles::{
    Cascade, DirectParagraph, RunSite, Site, cell_fill, direct_paragraph, direct_run,
    numbering_removed, page_background,
};
use crate::wordml::{BlockKind, Item, NumberingDefs, Paragraph, PropsRef, Story, TableSite};
use crate::xml::{NodeId, SpanTree};
use std::borrow::Cow;
use std::cell::OnceCell;
use std::sync::Arc;

/// The run a site is for: an item of the paragraph's text map, and whether its text is complex script
/// (Arabic, Hebrew, Thai, Hindi). Only the caller knows the second, and it decides whether the size that
/// counts is `sz` or `szCs`.
#[derive(Clone, Copy)]
pub(crate) struct RunKey<'i> {
    pub(crate) item: &'i Item,
    pub(crate) complex_script: bool,
}

/// What the handle read about the paragraph it was last asked for.
struct ParagraphHere<'a> {
    node: NodeId,
    paragraph: Paragraph<'a>,
    direct: DirectParagraph<'a>,
    /// The index of the paragraph among the story's blocks, if it is one of them.
    entry: Option<usize>,
    /// The paragraph is in a row of a table, so the story's table sites have an answer for it.
    in_row: bool,
    cell_fill: Option<Cow<'a, str>>,
    removed: bool,
}

/// What the handle read about the run it was last asked for.
struct RunHere<'a> {
    style: Option<Cow<'a, str>>,
    direct: Option<PropsRef<'a>>,
    complex_script: bool,
}

/// The sites of one story in one mode.
pub(crate) struct SiteHandle<'a> {
    view: PartsView<'a>,
    part: PartId,
    mode: Mode,
    tree: &'a SpanTree,
    story: Arc<Story>,
    cascade: Arc<Cascade>,
    background: Option<Box<str>>,
    numbering: Option<(Arc<StoryNumbers>, Arc<NumberingDefs>)>,
    tables: OnceCell<Vec<Option<TableSite>>>,
    paragraph: Option<ParagraphHere<'a>>,
    run: Option<RunHere<'a>>,
}

impl<'a> SiteHandle<'a> {
    /// A handle for the story at `part` in `mode`. It holds the story and the cascade, which are built
    /// on first use and shared, and reads the page colour from the main part.
    pub(super) fn new(
        view: PartsView<'a>,
        part: PartId,
        mode: Mode,
    ) -> Result<SiteHandle<'a>, StetError> {
        let main = view.tree(view.base.main_part()?)?;
        Ok(SiteHandle {
            part,
            mode,
            tree: view.tree(part)?,
            story: view.story(part)?,
            cascade: view.cascade()?,
            background: page_background(main).map(Cow::into_owned).map(Into::into),
            numbering: None,
            tables: OnceCell::new(),
            paragraph: None,
            run: None,
            view,
        })
    }

    /// True if the story's table sites have been computed, which a story with no table never does.
    #[cfg(test)]
    pub(super) fn tables_built(&self) -> bool {
        self.tables.get().is_some()
    }

    /// The numbers of the story in this mode, if a paragraph site has asked for them.
    pub(crate) fn numbers(&self) -> Option<&StoryNumbers> {
        self.numbering.as_ref().map(|(numbers, _)| &**numbers)
    }

    /// Moves the handle to a paragraph, or to one run of it, and lends its site. A node that is not a
    /// paragraph is a caller bug and is `internal`. Cheap in a loop: the paragraph is read again only
    /// when `node` changes.
    pub(crate) fn site_for(
        &mut self,
        node: NodeId,
        run: Option<RunKey<'_>>,
    ) -> Result<Site<'_>, StetError> {
        self.locate(node, run)?;
        Ok(self.site())
    }

    /// Moves the handle to a paragraph, or to one run of it, so that `site` lends its site.
    pub(super) fn locate(
        &mut self,
        node: NodeId,
        run: Option<RunKey<'_>>,
    ) -> Result<(), StetError> {
        if self.paragraph.as_ref().map(|p| p.node) != Some(node) {
            self.paragraph = Some(self.read_paragraph(node)?);
        }
        let paragraph = self.paragraph.as_ref().map(|p| p.paragraph);
        let original = self.mode == Mode::Original;
        self.run = match (run, paragraph) {
            (Some(key), Some(p)) => {
                // In Original the run is what its tracked change replaced, and so is its style.
                let direct = direct_run(p.run_props(key.item), original);
                Some(RunHere {
                    style: direct.and_then(|d| d.val("rStyle")),
                    direct,
                    complex_script: key.complex_script,
                })
            }
            _ => None,
        };
        if self.run.is_some() {
            // A run's own properties never read the numbering level.
            return Ok(());
        }
        if self.numbering.is_none() {
            let numbers = self.view.numbers(self.part, self.mode)?;
            self.numbering = Some((numbers, self.view.numbering_defs()?));
        }
        Ok(())
    }

    /// Everything about a paragraph that its runs share.
    fn read_paragraph(&self, node: NodeId) -> Result<ParagraphHere<'a>, StetError> {
        let paragraph = Paragraph::new(self.tree, node)
            .ok_or_else(|| internal(format!("node {node:?} is not a paragraph")))?;
        let direct = direct_paragraph(&paragraph, self.mode == Mode::Original);
        let entry = self.story.index_of(node);
        let blocks = self.story.blocks();
        let in_row = entry
            .and_then(|i| blocks.get(i))
            .and_then(|e| e.parent)
            .and_then(|p| blocks.get(usize::try_from(p).ok()?))
            .is_some_and(|e| e.kind == BlockKind::Row);
        Ok(ParagraphHere {
            node,
            removed: numbering_removed(direct.ppr),
            cell_fill: cell_fill(paragraph.element()),
            paragraph,
            direct,
            entry,
            in_row,
        })
    }

    /// The site the handle was last moved to. A handle that was never moved lends an empty site.
    pub(crate) fn site(&self) -> Site<'_> {
        let Some(here) = &self.paragraph else {
            return Site::default();
        };
        let run = self.run.as_ref().map(|r| RunSite {
            style: r.style.as_deref(),
            direct: r.direct,
            complex_script: r.complex_script,
        });
        let table = here.entry.filter(|_| here.in_row).and_then(|i| {
            self.tables
                .get_or_init(|| self.story.table_sites(self.tree))
                .get(i)?
                .as_ref()
        });
        let level = match (&run, &self.numbering) {
            (None, Some((numbers, defs))) => numbers
                .item(here.node)
                .and_then(|item| level_site(defs, &self.cascade, item)),
            _ => None,
        };
        Site {
            style: here.direct.style.as_deref(),
            ppr: here.direct.ppr,
            mark: here.direct.mark,
            table,
            level,
            numbering_removed: here.removed,
            cell_fill: here.cell_fill.as_deref(),
            background: self.background.as_deref(),
            run,
        }
    }
}
