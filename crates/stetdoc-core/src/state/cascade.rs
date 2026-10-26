//! The style cascade and the clause numbers, as cached values of a state.
//!
//! `styles` and `numbering` are pure: they cannot see the state, so they cannot cache. This file finds
//! their inputs, which are the parts that the main part's relationships name (styles, numbering, theme,
//! settings), and keeps their products in `PartsView::derived`, so that a view over a `Tx` that staged
//! `word/styles.xml` sees the cascade of the staged styles, and a text edit rebuilds nothing but the
//! numbers of its own story.
//!
//! Each product has **one** list of dependencies, and it is the exported function that builds it
//! (`cascade_deps`, `numbers_deps`), so that a cache that is built from a product, such as the index, can
//! name exactly the parts the product was built from. Every list holds `Dep::Package`: the parts are found
//! through relationships, which change only when a part is added, and a part that is absent now may be
//! added later. The price is a rebuild of the cascade (a few milliseconds) when a part is added, which
//! happens once per comment thread and not once per edit.
//!
//! | Product | Dependencies | Variant |
//! |---|---|---|
//! | `StyleDefs` | the styles part | 0 |
//! | `NumberingDefs` | the numbering part | 0 |
//! | `Cascade` | the styles, theme and settings parts | 0 |
//! | `StoryNumbers` | the story part, the numbering part and the styles part | the mode |
//!
//! A story's numbers do not depend on the theme or the settings: the numbers read from the cascade only
//! what the styles part says. The sites that `sites.rs` builds are not cached.

use super::derived::Dep;
use super::parts::PartId;
use super::view::PartsView;
use super::{State, sites};
use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::numbering::StoryNumbers;
use crate::styles::Cascade;
use crate::wordml::{NumberingDefs, StyleDefs};
use std::sync::Arc;

/// The start of every relationship type that the parts we read are named by, in the Transitional
/// and the Strict spelling.
const BASES: [&str; 2] = [
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/",
    "http://purl.oclc.org/ooxml/officeDocument/relationships/",
];

/// True if `rel_type` is the relationship type `kind` (`styles`, `numbering`, `theme` or `settings`),
/// in either spelling.
fn is_rel(rel_type: &str, kind: &str) -> bool {
    BASES
        .iter()
        .any(|base| rel_type.strip_prefix(base) == Some(kind))
}

/// The cache variant of a story's numbers: the mode.
fn variant(mode: Mode) -> u64 {
    match mode {
        Mode::Final => 0,
        Mode::Original => 1,
        Mode::Redline => 2,
    }
}

/// `Dep::Package` and the parts that exist.
fn deps_of(parts: &[Option<PartId>]) -> Vec<Dep> {
    std::iter::once(Dep::Package)
        .chain(parts.iter().flatten().copied().map(Dep::Part))
        .collect()
}

/// The parts the cascade is read from. The list of dependencies and the reads both come from here, so
/// the one cannot name a part that the other does not.
struct CascadeParts {
    styles: Option<PartId>,
    theme: Option<PartId>,
    settings: Option<PartId>,
}

impl CascadeParts {
    fn deps(&self) -> Vec<Dep> {
        deps_of(&[self.styles, self.theme, self.settings])
    }
}

/// The parts the numbers of a story are read from, for the same reason.
struct NumbersParts {
    story: PartId,
    numbering: Option<PartId>,
    styles: Option<PartId>,
}

impl NumbersParts {
    fn deps(&self) -> Vec<Dep> {
        deps_of(&[Some(self.story), self.numbering, self.styles])
    }
}

impl<'a> PartsView<'a> {
    /// The part that the main part's relationship of type `kind` names, if the document has one.
    fn related(&self, kind: &str) -> Result<Option<PartId>, StetError> {
        let main = self.base.main_part()?;
        self.related_part(main, &|rel_type| is_rel(rel_type, kind))
    }

    fn cascade_parts(&self) -> Result<CascadeParts, StetError> {
        Ok(CascadeParts {
            styles: self.related("styles")?,
            theme: self.related("theme")?,
            settings: self.related("settings")?,
        })
    }

    fn numbers_parts(&self, story: PartId) -> Result<NumbersParts, StetError> {
        Ok(NumbersParts {
            story,
            numbering: self.related("numbering")?,
            styles: self.related("styles")?,
        })
    }

    /// What the cascade is built from: the styles, the theme and the settings parts, and the package, which
    /// says which of them exist. It is the list that `cascade` itself builds under.
    pub(crate) fn cascade_deps(&self) -> Result<Vec<Dep>, StetError> {
        Ok(self.cascade_parts()?.deps())
    }

    /// What the numbers of a story are built from: the story, the numbering part and the styles part, and
    /// the package. The numbers do not depend on the theme or on the settings. It is the list that `numbers`
    /// itself builds under, in every mode.
    pub(crate) fn numbers_deps(&self, story_part: PartId) -> Result<Vec<Dep>, StetError> {
        Ok(self.numbers_parts(story_part)?.deps())
    }

    /// The style definitions: the styles part, read once. A document with no styles part has none.
    pub(crate) fn style_defs(&self) -> Result<Arc<StyleDefs>, StetError> {
        let part = self.related("styles")?;
        let build = |_: Option<&StyleDefs>| -> Result<StyleDefs, StetError> {
            part.map_or_else(
                || Ok(StyleDefs::default()),
                |p| Ok(StyleDefs::read(self.tree(p)?)),
            )
        };
        self.derived(0, &deps_of(&[part]), &build)
    }

    /// The numbering definitions: the numbering part, read once. A document with no numbering part has
    /// none.
    pub(crate) fn numbering_defs(&self) -> Result<Arc<NumberingDefs>, StetError> {
        let part = self.related("numbering")?;
        let build = |_: Option<&NumberingDefs>| -> Result<NumberingDefs, StetError> {
            part.map_or_else(
                || Ok(NumberingDefs::default()),
                |p| Ok(NumberingDefs::read(self.tree(p)?)),
            )
        };
        self.derived(0, &deps_of(&[part]), &build)
    }

    /// The style cascade of the document: every style resolved along its `basedOn` chain, with the theme.
    /// It is rebuilt when the styles, the theme or the settings part changes, or a part is added.
    pub(crate) fn cascade(&self) -> Result<Arc<Cascade>, StetError> {
        let parts = self.cascade_parts()?;
        let build = |_: Option<&Cascade>| -> Result<Cascade, StetError> {
            let tree = |part: Option<PartId>| part.map(|p| self.tree(p)).transpose();
            Ok(Cascade::build(
                &*self.style_defs()?,
                tree(parts.theme)?,
                tree(parts.settings)?,
            ))
        };
        self.derived(0, &parts.deps(), &build)
    }

    /// The clause numbers of one story in one mode. They are computed when first asked for and kept until
    /// the story, the numbering part or the styles part changes: nothing on the edit path reads them.
    pub(crate) fn numbers(
        &self,
        story_part: PartId,
        mode: Mode,
    ) -> Result<Arc<StoryNumbers>, StetError> {
        let parts = self.numbers_parts(story_part)?;
        let build = |_: Option<&StoryNumbers>| -> Result<StoryNumbers, StetError> {
            let story = self.story(parts.story)?;
            let tree = self.tree(parts.story)?;
            let facts = story.num_facts(tree);
            // The field pass is the story's own, run once and kept with it.
            let list_num = story.fields(tree).iter().any(|f| &*f.keyword == "LISTNUM");
            let numbers =
                StoryNumbers::build(&facts, &*self.numbering_defs()?, &*self.cascade()?, mode);
            Ok(numbers.with_list_num_fields(list_num))
        };
        self.derived(variant(mode), &parts.deps(), &build)
    }

    /// The sites of one story in one mode, for the questions the cascade answers. `site` is the same for
    /// a caller that asks about one place, and `sites` for one that visits many.
    pub(crate) fn sites(
        &self,
        story_part: PartId,
        mode: Mode,
    ) -> Result<sites::SiteHandle<'a>, StetError> {
        sites::SiteHandle::new(*self, story_part, mode)
    }

    /// The site of a paragraph, or of one run of it, as a handle that lends the `Site`.
    pub(crate) fn site(
        &self,
        story_part: PartId,
        node: crate::xml::NodeId,
        run: Option<sites::RunKey<'_>>,
        mode: Mode,
    ) -> Result<sites::SiteHandle<'a>, StetError> {
        let mut handle = self.sites(story_part, mode)?;
        handle.locate(node, run)?;
        Ok(handle)
    }
}

impl State {
    /// The style cascade of the committed state.
    pub(crate) fn cascade(&self) -> Result<Arc<Cascade>, StetError> {
        self.parts().cascade()
    }

    /// The clause numbers of a story of the committed state, in a mode.
    pub(crate) fn numbers(
        &self,
        story_part: PartId,
        mode: Mode,
    ) -> Result<Arc<StoryNumbers>, StetError> {
        self.parts().numbers(story_part, mode)
    }
}
