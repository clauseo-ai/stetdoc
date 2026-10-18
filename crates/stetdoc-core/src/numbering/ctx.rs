//! What the counters need to look up: levels with their overrides and style links applied.

use super::item::Note;
use super::label::{Piece, parse};
use crate::styles::Cascade;
use crate::wordml::{Level, NodeId, NumberingDefs};
use std::collections::HashMap;
use std::rc::Rc;

/// How many `numStyleLink` hops are followed before a chain is called a cycle.
const LINK_HOPS: usize = 8;

/// A level of a list as one `w:num` sees it.
#[derive(Clone, Copy)]
pub(super) struct Lv<'a> {
    /// The level definition: the `w:num`'s replacement if it has one.
    pub level: &'a Level,
    /// Where the level starts: the `w:num`'s `w:startOverride`, else the level's `w:start`.
    pub start: i64,
    /// The `w:abstractNum` the `w:num` names, which keys the list's counters.
    pub key: i64,
}

/// A level of a list, ready for the counters: its definition, its text split into pieces, and what
/// the counters ask of it.
pub(super) struct Entry<'a> {
    /// The level as the `w:num` sees it.
    pub lv: Lv<'a>,
    /// The level's text, split into literals and placeholders.
    pub pieces: Vec<Piece>,
    /// The `w:num` has a `w:startOverride` for this level.
    pub has_override: bool,
    /// The text names every shallower level, so the label carries its ancestors.
    pub carries_ancestors: bool,
}

/// The nine levels of the list a `w:num` names. A level the list does not define is `None`.
pub(super) type Table<'a> = Rc<[Option<Entry<'a>>; 9]>;

/// The read-only inputs of one build, and the notes collected on the way.
pub(super) struct Ctx<'a> {
    pub defs: &'a NumberingDefs,
    pub cascade: &'a Cascade,
    /// The distinct notes, in the order they first arose.
    pub notes: Vec<Note>,
    /// The paragraph being numbered, so that a note can say where it arose.
    pub at: Option<NodeId>,
    /// For each paragraph, the indexes into `notes` of the notes that arose while numbering it.
    pub placed: HashMap<NodeId, Vec<u32>>,
    tables: HashMap<i64, Table<'a>>,
}

/// Level `ilvl` of the list `num_id`, with the `w:num`'s overrides applied, following a `numStyleLink` when
/// the `w:num`'s own `abstractNum` has no such level. A link that loops, or runs too long, is reported
/// through `note` and gives `None`. The counters and `level_site` both find a level here.
pub(super) fn find_level<'d>(
    defs: &'d NumberingDefs,
    cascade: &Cascade,
    num_id: i64,
    ilvl: u8,
    note: &mut dyn FnMut(Note),
) -> Option<Lv<'d>> {
    let abs = defs.abstract_of(num_id)?;
    let own_start = defs
        .num(num_id)?
        .overrides
        .iter()
        .find(|o| o.ilvl == ilvl)
        .and_then(|o| o.start);
    if let Some(r) = defs.level(num_id, ilvl) {
        return Some(Lv {
            level: r.level,
            start: r.start,
            key: abs.id,
        });
    }
    let mut link = abs.num_style_link.as_deref();
    let mut hops = 0;
    while let Some(style) = link {
        hops += 1;
        if hops > LINK_HOPS {
            note(Note::StyleLinkCycle {
                style: style.into(),
            });
            return None;
        }
        let linked = cascade.style_numbering(Some(style)).num_id?;
        if linked == 0 {
            return None;
        }
        if let Some(r) = defs.level(linked, ilvl) {
            return Some(Lv {
                level: r.level,
                start: own_start.unwrap_or(r.start),
                key: abs.id,
            });
        }
        link = defs
            .abstract_of(linked)
            .and_then(|a| a.num_style_link.as_deref());
    }
    None
}

impl<'a> Ctx<'a> {
    /// A context over the definitions and the cascade.
    pub fn new(defs: &'a NumberingDefs, cascade: &'a Cascade) -> Ctx<'a> {
        Ctx {
            defs,
            cascade,
            notes: Vec::new(),
            at: None,
            placed: HashMap::new(),
            tables: HashMap::new(),
        }
    }

    /// The levels of the list a `w:num` names, built once.
    pub fn table(&mut self, num_id: i64) -> Table<'a> {
        if let Some(t) = self.tables.get(&num_id) {
            return t.clone();
        }
        let mut levels: [Option<Entry<'a>>; 9] = Default::default();
        for ilvl in 0..9u8 {
            let Some(lv) = self.lv_exact(num_id, ilvl) else {
                continue;
            };
            let pieces = parse(&lv.level.text);
            let carries = (0..ilvl).all(|j| pieces.contains(&Piece::Level(j)));
            if let Some(slot) = levels.get_mut(usize::from(ilvl)) {
                *slot = Some(Entry {
                    has_override: self.has_start_override(num_id, ilvl),
                    pieces,
                    carries_ancestors: carries,
                    lv,
                });
            }
        }
        let table: Table<'a> = Rc::new(levels);
        self.tables.insert(num_id, table.clone());
        table
    }

    /// Records a note once in the list of notes, and at the paragraph being numbered, if there is one.
    pub fn note(&mut self, note: Note) {
        let index = match self.notes.iter().position(|n| *n == note) {
            Some(i) => i,
            None => {
                self.notes.push(note);
                self.notes.len() - 1
            }
        };
        if let (Some(at), Ok(index)) = (self.at, u32::try_from(index)) {
            let here = self.placed.entry(at).or_default();
            if !here.contains(&index) {
                here.push(index);
            }
        }
    }

    /// Level `ilvl` of the list `num_id`, or `None` if the list does not define it. Follows a
    /// `numStyleLink` when the `w:num`'s own `abstractNum` has no such level.
    pub fn lv_exact(&mut self, num_id: i64, ilvl: u8) -> Option<Lv<'a>> {
        let (defs, cascade) = (self.defs, self.cascade);
        find_level(defs, cascade, num_id, ilvl, &mut |n| self.note(n))
    }

    /// The entry of level `ilvl`, or, for a level the list does not define, the nearest lower defined
    /// level, with a note.
    pub fn entry_or_lower<'t>(
        &mut self,
        table: &'t Table<'a>,
        num_id: i64,
        ilvl: u8,
    ) -> Option<&'t Entry<'a>> {
        if let Some(Some(e)) = table.get(usize::from(ilvl)) {
            return Some(e);
        }
        for lower in (0..ilvl).rev() {
            if let Some(Some(e)) = table.get(usize::from(lower)) {
                self.note(Note::MissingLevel { num_id, ilvl });
                return Some(e);
            }
        }
        None
    }

    /// True if the `w:num` has a `w:startOverride` for the level.
    pub fn has_start_override(&self, num_id: i64, ilvl: u8) -> bool {
        self.defs.num(num_id).is_some_and(|n| {
            n.overrides
                .iter()
                .any(|o| o.ilvl == ilvl && o.start.is_some())
        })
    }
}
