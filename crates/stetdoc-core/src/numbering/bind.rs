//! Which list and level a paragraph belongs to, in a mode. See the design, section 4.2.

use super::ctx::Ctx;
use super::item::Note;
use crate::wordml::NumFact;

/// A paragraph's place in a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Binding {
    /// The `w:numId`.
    pub num_id: i64,
    /// The level, 0 to 8.
    pub ilvl: u8,
    /// The numbering came from the paragraph style.
    pub from_style: bool,
}

impl Ctx<'_> {
    /// The binding of a paragraph, or `None` if it is not a list item. With `original`, a paragraph
    /// that has a `w:pPrChange` is read from the properties the change replaced.
    pub fn bind(&mut self, fact: &NumFact, original: bool) -> Option<Binding> {
        if fact.empty_section_break {
            return None;
        }
        let (style_id, num_pr) = match (&fact.old, original) {
            (Some(old), true) => (old.style_id.as_deref(), old.num_pr),
            _ => (fact.style_id.as_deref(), fact.num_pr),
        };
        if let Some(np) = num_pr {
            // A direct numPr replaces the style's as a unit. No numId, or 0, is not a list item.
            let num_id = np.num_id.filter(|n| *n != 0)?;
            let ilvl = self.clamp_level(num_id, np.ilvl.unwrap_or(0));
            return self.usable(num_id).then_some(Binding {
                num_id,
                ilvl,
                from_style: false,
            });
        }
        let own_style = style_id.or_else(|| self.cascade.default_paragraph_style());
        let sn = self.cascade.style_numbering(own_style);
        let num_id = sn.num_id.filter(|n| *n != 0)?;
        if !self.usable(num_id) {
            return None;
        }
        // ISO 29500: a style's own ilvl is ignored, and the level that names the style gives the level.
        let by_link = own_style.and_then(|s| {
            self.defs
                .abstract_of(num_id)?
                .levels
                .iter()
                .find(|l| l.style.as_deref() == Some(s))
                .map(|l| l.ilvl)
        });
        let ilvl = by_link.or(sn.ilvl).unwrap_or(0);
        let ilvl = self.clamp_level(num_id, ilvl);
        Some(Binding {
            num_id,
            ilvl,
            from_style: true,
        })
    }

    /// True if the `w:num` and its `w:abstractNum` exist. Otherwise a note says so.
    fn usable(&mut self, num_id: i64) -> bool {
        if self.defs.abstract_of(num_id).is_some() {
            return true;
        }
        self.note(Note::UnresolvedNumId { num_id });
        false
    }

    /// A level above 8 is read as 8, with a note.
    fn clamp_level(&mut self, num_id: i64, ilvl: u8) -> u8 {
        if ilvl <= 8 {
            return ilvl;
        }
        self.note(Note::MissingLevel { num_id, ilvl });
        8
    }
}
