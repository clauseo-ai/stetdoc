//! The level a list item sits on, in the terms the cascade needs.

use super::ctx::find_level;
use super::item::ListItem;
use crate::styles::{Cascade, LevelSite};
use crate::wordml::NumberingDefs;

/// The level of the list that `item` is on, as the cascade reads it: the level's own `w:pPr`, found the
/// way the counters find the level (the `w:num`'s replacement, then a `numStyleLink`, and, when the
/// list does not define this level, the nearest lower one). `None` when the list defines no level at
/// or below it.
pub fn level_site<'a>(
    defs: &'a NumberingDefs,
    cascade: &Cascade,
    item: &ListItem,
) -> Option<LevelSite<'a>> {
    let level = (0..=item.level)
        .rev()
        .find_map(|ilvl| find_level(defs, cascade, item.num_id, ilvl, &mut |_| {}))?
        .level;
    Some(LevelSite {
        props: &level.paragraph,
        num_id: item.num_id,
        ilvl: item.level,
        from_style: item.from_style,
    })
}
