//! The context of building one story: everything a block needs that is the same for the whole story.
//!
//! A `StoryCx` holds the cached `Story` and its tree, the relationships of the part, the sites that the
//! cascade answers about, the facts of the document, and the warnings raised so far. Nothing here is a
//! cache of its own: each value is `state`'s, shared, or is built on first use and dropped with the build.

use super::facts::DocFacts;
use super::warnings::Warnings;
use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::projection::model::UnitKind;
use crate::refs::{BlockKey, BlockRef};
use crate::state::{PartId, PartsView, SiteHandle};
use crate::styles::Cascade;
use crate::wordml::{
    BlockEntry, BlockKind, NodeId, Rels, Section, SpanTree, Story, StoryKind, StyleDefs,
};
use std::cell::OnceCell;
use std::sync::Arc;

/// What is needed to build the blocks of one story.
pub(super) struct StoryCx<'a> {
    pub(super) parts: PartsView<'a>,
    pub(super) mode: Mode,
    pub(super) facts: Arc<DocFacts>,
    pub(super) defs: Arc<StyleDefs>,
    pub(super) cascade: Arc<Cascade>,
    pub(super) kind: StoryKind,
    pub(super) part: PartId,
    pub(super) name: Arc<str>,
    pub(super) story: Arc<Story>,
    pub(super) tree: &'a SpanTree,
    pub(super) rels: Arc<Rels>,
    pub(super) sites: SiteHandle<'a>,
    pub(super) warnings: Warnings,
    /// How many paragraphs built so far are read-only as a whole.
    pub(super) readonly: u32,
    sections: OnceCell<Vec<Section>>,
}

impl<'a> StoryCx<'a> {
    /// A context for the story at `part`, in `mode`.
    pub(super) fn new(
        parts: PartsView<'a>,
        mode: Mode,
        facts: Arc<DocFacts>,
        part: PartId,
    ) -> Result<StoryCx<'a>, StetError> {
        let story = parts.story(part)?;
        let name = story.part().clone();
        Ok(StoryCx {
            mode,
            facts,
            defs: parts.style_defs()?,
            cascade: parts.cascade()?,
            kind: story.kind(),
            part,
            name,
            tree: parts.tree(part)?,
            rels: parts.rels(part)?,
            sites: parts.sites(part, mode)?,
            story,
            warnings: Warnings::default(),
            readonly: 0,
            sections: OnceCell::new(),
            parts,
        })
    }

    /// What warnings call the story: `body`, `notes`, `headers` or `comments`.
    pub(super) fn story_label(&self) -> &'static str {
        match self.kind {
            StoryKind::Body => "body",
            StoryKind::Header | StoryKind::Footer => "headers",
            StoryKind::Footnotes | StoryKind::Endnotes => "notes",
            StoryKind::Comments => "comments",
        }
    }

    /// The kind of unit a story's units are, when it has one unit.
    pub(super) fn single_unit_kind(&self) -> UnitKind {
        match self.kind {
            StoryKind::Header => UnitKind::Header,
            StoryKind::Footer => UnitKind::Footer,
            _ => UnitKind::Body,
        }
    }

    /// The entry at `i`.
    pub(super) fn entry(&self, i: usize) -> Option<BlockEntry> {
        self.story.blocks().get(i).copied()
    }

    /// The ref of the block at entry `i`, or `None` for a block with no ref yet.
    pub(super) fn ref_at(&self, i: usize) -> Option<BlockRef> {
        let entry = self.entry(i)?;
        self.parts
            .ref_of(&BlockKey::new(self.name.clone(), entry.node))
    }

    /// True if, in `Final`, the paragraph at entry `i` takes the text of a paragraph before it: it shows nothing and keeps its
    /// mark, and the paragraphs right before it in its container join on to it through deleted marks, the first of them with
    /// text and the rest with none. This is what a tracked delete of a container's last paragraph writes (design
    /// `revisions.md`, 3.2.2): the last paragraph keeps its mark and loses its text, and the one before loses its mark. The
    /// paragraphs read as one, the one with text, so this one is not shown, and neither are the empty ones between.
    pub(super) fn takes_joined(&self, i: usize) -> bool {
        if self.mode != Mode::Final {
            return false;
        }
        let Some(p) = self.para_at(i, None) else {
            return false;
        };
        if p.joins_next(self.mode) || !super::scan::shows_nothing(&p.map(), self.mode) {
            return false;
        }
        let parent = self.entry(i).map(|e| self.parent_of(e.node));
        let mut k = i;
        while let Some(prev) = k.checked_sub(1) {
            let Some(q) = self.para_at(prev, parent) else {
                return false;
            };
            if !q.joins_next(self.mode) {
                return false;
            }
            if !super::scan::shows_nothing(&q.map(), self.mode) {
                return true;
            }
            k = prev;
        }
        false
    }

    /// True if the paragraph at entry `i` joins, through deleted marks and empty paragraphs only, a paragraph that
    /// `takes_joined`: then it stands for them, so it is not marked as joining the next.
    pub(super) fn joins_a_taker(&self, i: usize) -> bool {
        if self.mode != Mode::Final {
            return false;
        }
        let parent = self.entry(i).map(|e| self.parent_of(e.node));
        let mut k = i + 1;
        loop {
            let Some(p) = self.para_at(k, parent) else {
                return false;
            };
            if !p.joins_next(self.mode) {
                return self.takes_joined(k);
            }
            if !super::scan::shows_nothing(&p.map(), self.mode) {
                return false;
            }
            k += 1;
        }
    }

    /// The paragraph at entry `i`, when it is one and, with `parent` given, in that parent.
    fn para_at(
        &self,
        i: usize,
        parent: Option<Option<NodeId>>,
    ) -> Option<crate::wordml::Paragraph<'_>> {
        let e = self.entry(i)?;
        if e.kind != BlockKind::Paragraph || parent.is_some_and(|p| p != self.parent_of(e.node)) {
            return None;
        }
        self.story.paragraph(self.tree, i)
    }

    fn parent_of(&self, node: NodeId) -> Option<NodeId> {
        self.tree.node_ref(node).parent().map(|p| p.id())
    }

    /// The sections of the body, read on first use. Any other story has none.
    pub(super) fn sections(&self) -> &[Section] {
        self.sections
            .get_or_init(|| self.story.sections(self.tree, &self.rels))
    }
}

/// The characters that stand for an atom in a paragraph's text.
pub(super) fn is_sentinel(c: char) -> bool {
    matches!(c, '\u{1}'..='\u{6}' | '\u{c}' | '\u{e}')
}
