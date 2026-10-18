//! The derived values that every later module reads: the relationships of a part as plain data,
//! the list of stories, and one `Story` per story part.
//!
//! All of them are caches over `PartsView::derived`, so each records the generations of the parts it
//! was built from and is rebuilt on the next read when one has changed. Nothing is keyed by a
//! story's position in the story list, which shifts when a header is added or removed.

use super::derived::Dep;
use super::errors::internal;
use super::parts::{PartId, PartIndex};
use super::view::PartsView;
use crate::api::error::StetError;
use crate::package::{Package, opc};
use crate::refs::{BlockSpec, Layout, StoryBlocks};
use crate::wordml::{Rel, RelTarget, Rels, Story, StoryKind, StorySpec, stories_of};
use crate::xml::{NodeId, SpanTree};
use std::sync::Arc;

#[cfg(test)]
thread_local! {
    /// The stories this thread has scanned or refreshed, each a walk of the whole story: what a test counts to see that an
    /// edit reads a story a bounded number of times, where a clock would be at the mercy of a busy machine.
    static STORY_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many stories this thread has scanned or refreshed so far (`STORY_BUILDS`).
#[cfg(test)]
pub(crate) fn story_builds() -> usize {
    STORY_BUILDS.with(std::cell::Cell::get)
}

/// The stories of a document, in story order, each part once. A type of its own, so that its cache
/// key cannot collide with another list.
pub(crate) struct StoryList(pub(crate) Vec<StorySpec>);

/// A number for a story kind, for the cache variant: the kind decides how a part scans.
fn kind_code(kind: StoryKind) -> u64 {
    match kind {
        StoryKind::Body => 1,
        StoryKind::Header => 2,
        StoryKind::Footer => 3,
        StoryKind::Footnotes => 4,
        StoryKind::Endnotes => 5,
        StoryKind::Comments => 6,
    }
}

/// The relationships of `source` as `wordml` wants them: an id, a type and a target that is a part
/// name spelled as the package entry spells it, an external string, or invalid. A dangling
/// relationship never fails an open.
fn rels_of(package: &Package, index: &PartIndex, source: &str) -> Result<Rels, StetError> {
    let found = package.relationships(source)?;
    let items = found
        .items
        .iter()
        .map(|r| {
            let target = if r.external {
                RelTarget::External(r.target.clone())
            } else {
                match opc::resolve_target(source, &r.target)
                    .and_then(|name| index.id(&name))
                    .and_then(|id| index.name(id))
                {
                    Some(name) => RelTarget::Part(name.to_string()),
                    None => RelTarget::Invalid,
                }
            };
            Rel {
                id: r.id.clone(),
                rel_type: r.rel_type.clone(),
                target,
            }
        })
        .collect();
    Ok(Rels::new(items))
}

/// A story that has changed since `prev` was built: the blocks `prev` listed against the new tree.
/// A refresh must give exactly what a scan from scratch gives, and the coherence test checks it.
fn refreshed(prev: &Story, tree: &SpanTree, touched: &[NodeId]) -> Result<Story, StetError> {
    Story::refresh(prev, tree, touched).map(|r| r.story)
}

impl<'a> PartsView<'a> {
    /// The package this view reads: the transaction's own after `add_part`, and the state's
    /// otherwise.
    pub(super) fn package(&self) -> &'a Package {
        self.tx
            .and_then(|tx| tx.added.as_ref())
            .map_or(&self.base.package, |added| &added.package)
    }

    /// The decompressed bytes of a part of the package as this view sees it (a part that the transaction added
    /// included), or `None` if there is no such part. `compare` reads the media that two documents name, to tell a
    /// replaced picture from an unchanged one and to copy a part across.
    pub(crate) fn part_bytes(&self, name: &str) -> Result<Option<crate::xml::Bytes>, StetError> {
        self.package().part(name)
    }

    /// The content type registered for a part as this view sees the package, for `compare`'s copy of a part across.
    pub(crate) fn part_content_type(&self, name: &str) -> Result<Option<String>, StetError> {
        self.package().content_type(name)
    }

    /// The relationships of a part, as plain data. They depend on the set of parts and their
    /// relationship parts, which change only through `add_part`, so the entry depends on
    /// `Dep::Package`.
    pub(crate) fn rels(&self, source: PartId) -> Result<Arc<Rels>, StetError> {
        let name = self
            .name(source)
            .ok_or_else(|| internal(format!("part number {source:?} has no name")))?;
        let build = |_: Option<&Rels>| -> Result<Rels, StetError> {
            rels_of(self.package(), self.index(), name)
        };
        self.derived(u64::from(source.number()), &[Dep::Package], &build)
    }

    /// The part that the first relationship of `source` whose type satisfies `is_type` points at,
    /// if there is one and it is a part of the package. A derived value that reads a part found
    /// through a relationship (the styles, the theme, the settings, the numbering) depends on that
    /// part and, because the relationship may be absent now and added later, on `Dep::Package` as
    /// well when this returns `None`.
    pub(crate) fn related_part(
        &self,
        source: PartId,
        is_type: &dyn Fn(&str) -> bool,
    ) -> Result<Option<PartId>, StetError> {
        let rels = self.rels(source)?;
        Ok(rels
            .iter()
            .filter(|r| is_type(&r.rel_type))
            .find_map(|r| match &r.target {
                RelTarget::Part(name) => self.id(name),
                _ => None,
            }))
    }

    /// The stories of the document in story order. It depends on the main part, which names the
    /// header and footer parts, and on the package, which holds the relationships. The body's own
    /// `Story` is what finds the section breaks that name them, and it is the cached one, so the body
    /// is not scanned a second time for the list.
    pub(crate) fn story_specs(&self) -> Result<Arc<StoryList>, StetError> {
        let main = self.base.main_part()?;
        let build = |_: Option<&StoryList>| -> Result<StoryList, StetError> {
            let tree = self.tree(main)?;
            let rels = self.rels(main)?;
            let body = self.story(main)?;
            Ok(StoryList(stories_of(&body, tree, &rels)))
        };
        self.derived(0, &[Dep::Part(main), Dep::Package], &build)
    }

    /// The `Story` of a part: the blocks in document order. A part that is not a story is a caller
    /// bug and is `internal`. A story that was built before and has changed is refreshed from the
    /// previous value, which comes only from this transaction's own store or from the base as it was
    /// when the transaction began, with the nodes of this part that the transaction touched.
    pub(crate) fn story(&self, part: PartId) -> Result<Arc<Story>, StetError> {
        let name = self
            .name(part)
            .ok_or_else(|| internal(format!("part number {part:?} has no name")))?;
        let kind = self.story_kind(part)?;
        let tree = self.tree(part)?;
        // A story built earlier in this transaction was built from the blocks touched before that
        // build, so only the calls since then matter. Without one, `prev` is the base's, or a seed
        // of the audit, and every touched block counts.
        let (touched, calls) = match self.tx {
            Some(tx) => {
                let from = tx.story_cursor.borrow().get(&part).copied().unwrap_or(0);
                tx.touched_since(name, from)
            }
            None => (Vec::new(), 0),
        };
        let build = |prev: Option<&Story>| -> Result<Story, StetError> {
            let story = match prev {
                Some(prev) => refreshed(prev, tree, &touched)?,
                None => Story::scan(name, kind, tree)?,
            };
            #[cfg(test)]
            STORY_BUILDS.with(|n| n.set(n.get() + 1));
            if let Some(tx) = self.tx {
                tx.story_cursor.borrow_mut().insert(part, calls);
            }
            Ok(story)
        };
        self.derived(kind_code(kind), &[Dep::Part(part)], &build)
    }

    /// Whether a part is one of the document's stories as the transaction has left it: the body, or a part the list of stories
    /// names. A header or a footer that no section names any more (an earlier operation accepted the deletion of the section
    /// break that named it) is not.
    pub(crate) fn is_story(&self, part: PartId) -> Result<bool, StetError> {
        if part == self.base.main_part()? {
            return Ok(true);
        }
        let Some(name) = self.name(part) else {
            return Ok(false);
        };
        Ok(self.story_specs()?.0.iter().any(|s| *s.part == **name))
    }

    /// The kind of story a part is, without its `Story`: a caller that needs only the kind does not scan a story that
    /// changed since it was built. A part that is not a story is `internal`.
    pub(crate) fn story_kind(&self, part: PartId) -> Result<StoryKind, StetError> {
        // The main part is the body, and asking the list for it would ask the body's story for the
        // list. Every other story is found in the list.
        if part == self.base.main_part()? {
            return Ok(StoryKind::Body);
        }
        let name = self
            .name(part)
            .ok_or_else(|| internal(format!("part number {part:?} has no name")))?;
        self.story_specs()?
            .0
            .iter()
            .find(|s| *s.part == **name)
            .map(|s| s.kind)
            .ok_or_else(|| {
                internal(format!(
                    "{name} is not a story BT {}",
                    std::backtrace::Backtrace::force_capture()
                ))
            })
    }

    /// Leaves a story that the audit scanned in the store, so the next state has it.
    pub(super) fn seed_story(&self, part: PartId, kind: StoryKind, story: Arc<Story>) {
        self.seed(kind_code(kind), &[Dep::Part(part)], story);
    }

    /// Every story with its blocks, for `RefTable::open`, `persist` and `restore`. The fingerprints
    /// cost a pass over the text of every paragraph, so `open` asks for none, and `export` and
    /// `resume` ask for them.
    pub(crate) fn layout(&self, fingerprints: bool) -> Result<Layout, StetError> {
        let list = self.story_specs()?;
        let mut out = Vec::with_capacity(list.0.len());
        for spec in &list.0 {
            let id = self.id(&spec.part).ok_or_else(|| {
                internal(format!("{} is a story and has no part number", spec.part))
            })?;
            let story = self.story(id)?;
            let prints = if fingerprints {
                story.fingerprints(self.tree(id)?)
            } else {
                Vec::new()
            };
            let blocks = story
                .blocks()
                .iter()
                .enumerate()
                .map(|(i, entry)| BlockSpec {
                    key: story.key(entry),
                    kind: entry.kind,
                    fingerprint: prints.get(i).copied().unwrap_or(0),
                })
                .collect();
            out.push(StoryBlocks {
                part: Arc::clone(story.part()),
                kind: spec.kind,
                blocks,
            });
        }
        Ok(Layout { stories: out })
    }
}

/// The relationships of a part rebuilt from the package, without the cache, for the coherence tests.
#[cfg(test)]
pub(super) fn rels_from_scratch(view: &PartsView<'_>, source: PartId) -> Rels {
    let name = view.name(source).map_or("", |n| n);
    rels_of(view.package(), view.index(), name).unwrap_or_default()
}
