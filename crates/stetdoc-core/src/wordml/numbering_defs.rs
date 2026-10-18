//! Numbering definitions: `NumberingDefs`.
//!
//! Reads `word/numbering.xml` once into owned structures with no borrow of the span tree. These are
//! definitions only: computing a clause number from them is `numbering`'s. Style-linked lists
//! (`w:numStyleLink`, `w:styleLink`) are recorded and not followed, because following them needs
//! the style definitions as well.
//!
//! Reading never fails. What a reader skips or cannot resolve is a `NumberingIssue`.

use super::names::{int, is_w, w_attr};
use super::numbering_levels::{Level, MultiLevel};
use super::numbering_levels::{read_ilvl, read_level};
use crate::xml::{NodeRef, SpanTree};
use std::collections::HashMap;
use std::collections::hash_map::Entry;

/// How many levels a list has.
const MAX_LEVELS: usize = 9;

/// A list definition (`w:abstractNum`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbstractNum {
    /// The `w:abstractNumId`.
    pub id: i64,
    /// `w:multiLevelType`.
    pub multi_level: MultiLevel,
    /// `w:styleLink`: this definition is the list of the numbering style with this id.
    pub style_link: Option<Box<str>>,
    /// `w:numStyleLink`: this definition uses the list of the numbering style with this id.
    pub num_style_link: Option<Box<str>>,
    /// The levels, sorted by `ilvl`, at most nine.
    pub levels: Vec<Level>,
}

impl AbstractNum {
    /// The level `ilvl`, if the definition has it.
    pub fn level(&self, ilvl: u8) -> Option<&Level> {
        self.levels.iter().find(|l| l.ilvl == ilvl)
    }
}

/// One `w:lvlOverride` of a `w:num`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelOverride {
    /// The level the override is for.
    pub ilvl: u8,
    /// `w:startOverride`.
    pub start: Option<i64>,
    /// The replacement `w:lvl`.
    pub level: Option<Level>,
}

/// A list instance (`w:num`), the thing a paragraph's `w:numId` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Num {
    /// The `w:numId`.
    pub id: i64,
    /// The `w:abstractNumId` of the definition it uses.
    pub abstract_id: i64,
    /// The overrides, in the order written. The first one of an `ilvl` wins.
    pub overrides: Vec<LevelOverride>,
}

/// A level of a list with the overrides applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedLevel<'a> {
    /// The level: the override's `w:lvl` if it has one, else the abstract definition's.
    pub level: &'a Level,
    /// The number the level starts at: `w:startOverride` if there is one, else `level.start`.
    pub start: i64,
    /// True if `level` came from a `w:lvlOverride`'s `w:lvl`.
    pub replaced: bool,
    /// The abstract definition the list uses.
    pub abstract_id: i64,
}

/// Where a level that has a problem was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelOwner {
    /// In the `w:abstractNum` with this id.
    AbstractNum(i64),
    /// In the `w:lvlOverride` of the `w:num` with this id.
    Num(i64),
}

/// Something reading `numbering.xml` skipped or could not resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumberingIssue {
    /// The part's root is not `w:numbering`. The definitions are empty.
    NotNumbering {
        /// The root's local name.
        root: Box<str>,
    },
    /// A `w:abstractNum` whose id is missing or not a whole number. It is not kept.
    InvalidAbstractNumId {
        /// The value as written, empty if missing.
        value: Box<str>,
    },
    /// A `w:num` whose id is missing or not a whole number. It is not kept.
    InvalidNumId {
        /// The value as written, empty if missing.
        value: Box<str>,
    },
    /// A `w:num` whose `w:abstractNumId` is missing or not a whole number. It is not kept.
    InvalidNumAbstractId {
        /// The `w:numId`.
        num_id: i64,
    },
    /// A second `w:abstractNum` with an id already used. It is not kept.
    DuplicateAbstractNum {
        /// The id.
        id: i64,
    },
    /// A second `w:num` with an id already used. It is not kept.
    DuplicateNum {
        /// The id.
        id: i64,
    },
    /// A `w:num` whose abstract definition does not exist. It resolves to nothing.
    MissingAbstract {
        /// The `w:numId`.
        num_id: i64,
        /// The `w:abstractNumId` it names.
        abstract_id: i64,
    },
    /// A `w:lvl` or `w:lvlOverride` whose `w:ilvl` is missing, not a whole number, or above eight.
    /// It is not kept.
    InvalidLevel {
        /// Where it was found.
        owner: LevelOwner,
        /// The value as written, empty if missing.
        value: Box<str>,
    },
    /// A second `w:lvl` for a level of an abstract definition. It is not kept.
    DuplicateLevel {
        /// The `w:abstractNumId`.
        abstract_id: i64,
        /// The level.
        ilvl: u8,
    },
    /// A second `w:lvlOverride` for a level of a `w:num`. It is not kept.
    DuplicateOverride {
        /// The `w:numId`.
        num_id: i64,
        /// The level.
        ilvl: u8,
    },
    /// A value that is not a whole number (or not in range) where one is required. The default
    /// applies.
    InvalidValue {
        /// Where it was found.
        owner: LevelOwner,
        /// The level.
        ilvl: u8,
        /// The element, such as `w:start`.
        element: &'static str,
        /// The value as written, empty if missing.
        value: Box<str>,
    },
}

/// The numbering definitions of a document.
#[derive(Debug, Clone, Default)]
pub struct NumberingDefs {
    abstracts: Vec<AbstractNum>,
    nums: Vec<Num>,
    abstract_ix: HashMap<i64, usize>,
    num_ix: HashMap<i64, usize>,
    issues: Vec<NumberingIssue>,
}

impl NumberingDefs {
    /// Reads `numbering.xml`. A root that is not `w:numbering` gives empty definitions and an issue.
    pub fn read(tree: &SpanTree) -> NumberingDefs {
        let mut defs = NumberingDefs::default();
        let root = tree.root();
        if !is_w(&root, "numbering") {
            defs.issues.push(NumberingIssue::NotNumbering {
                root: root.local_name().into(),
            });
            return defs;
        }
        for c in root.child_elements() {
            if is_w(&c, "abstractNum") {
                defs.read_abstract(&c);
            } else if is_w(&c, "num") {
                defs.read_num(&c);
            }
        }
        for n in &defs.nums {
            if !defs.abstract_ix.contains_key(&n.abstract_id) {
                defs.issues.push(NumberingIssue::MissingAbstract {
                    num_id: n.id,
                    abstract_id: n.abstract_id,
                });
            }
        }
        defs
    }

    /// Every abstract definition, in the order written.
    pub fn abstracts(&self) -> &[AbstractNum] {
        &self.abstracts
    }

    /// Every list instance, in the order written.
    pub fn nums(&self) -> &[Num] {
        &self.nums
    }

    /// The largest `w:numId`, or zero if there is no list instance. A new list instance takes the
    /// number after it.
    pub fn max_num_id(&self) -> i64 {
        self.nums.iter().map(|n| n.id).max().unwrap_or(0).max(0)
    }

    /// The abstract definition with this id.
    pub fn abstract_num(&self, abstract_id: i64) -> Option<&AbstractNum> {
        self.abstracts.get(*self.abstract_ix.get(&abstract_id)?)
    }

    /// The list instance with this `w:numId`. It exists even if its abstract definition does not.
    pub fn num(&self, num_id: i64) -> Option<&Num> {
        self.nums.get(*self.num_ix.get(&num_id)?)
    }

    /// The abstract definition a list instance uses. `None` if the instance or the definition does
    /// not exist.
    pub fn abstract_of(&self, num_id: i64) -> Option<&AbstractNum> {
        self.abstract_num(self.num(num_id)?.abstract_id)
    }

    /// Level `ilvl` of the list `num_id`, with the overrides applied. A `w:lvlOverride` with a
    /// `w:lvl` replaces the level, and one with a `w:startOverride` changes only the start. If an
    /// override has both, the replacement level is used and then given the overridden start.
    pub fn level(&self, num_id: i64, ilvl: u8) -> Option<ResolvedLevel<'_>> {
        let abs = self.abstract_of(num_id)?;
        let over = self.num(num_id)?.overrides.iter().find(|o| o.ilvl == ilvl);
        let replacement = over.and_then(|o| o.level.as_ref());
        let level = replacement.or_else(|| abs.level(ilvl))?;
        Some(ResolvedLevel {
            level,
            start: over.and_then(|o| o.start).unwrap_or(level.start),
            replaced: replacement.is_some(),
            abstract_id: abs.id,
        })
    }

    /// What reading skipped or could not resolve, in the order found.
    pub fn issues(&self) -> &[NumberingIssue] {
        &self.issues
    }

    fn read_abstract(&mut self, n: &NodeRef<'_>) {
        let raw = w_attr(n, "abstractNumId");
        let Some(id) = raw.as_deref().and_then(int) else {
            self.issues.push(NumberingIssue::InvalidAbstractNumId {
                value: raw.as_deref().unwrap_or("").into(),
            });
            return;
        };
        if self.abstract_ix.contains_key(&id) {
            self.issues
                .push(NumberingIssue::DuplicateAbstractNum { id });
            return;
        }
        let owner = LevelOwner::AbstractNum(id);
        let mut a = AbstractNum {
            id,
            multi_level: MultiLevel::Unknown,
            style_link: None,
            num_style_link: None,
            levels: Vec::new(),
        };
        let mut seen_multi = false;
        for c in n.child_elements() {
            let val = || w_attr(&c, "val");
            if is_w(&c, "lvl") {
                let Some(ilvl) = read_ilvl(&c, owner, &mut self.issues) else {
                    continue;
                };
                if a.level(ilvl).is_some() {
                    self.issues.push(NumberingIssue::DuplicateLevel {
                        abstract_id: id,
                        ilvl,
                    });
                } else if a.levels.len() < MAX_LEVELS {
                    a.levels.push(read_level(&c, ilvl, owner, &mut self.issues));
                }
            } else if is_w(&c, "multiLevelType") && !seen_multi {
                seen_multi = true;
                a.multi_level = match val().as_deref().map(str::trim) {
                    Some("singleLevel") => MultiLevel::Single,
                    Some("hybridMultilevel") => MultiLevel::Hybrid,
                    Some("multilevel") => MultiLevel::Multi,
                    _ => MultiLevel::Unknown,
                };
            } else if is_w(&c, "styleLink") && a.style_link.is_none() {
                a.style_link = val().map(|v| v.into());
            } else if is_w(&c, "numStyleLink") && a.num_style_link.is_none() {
                a.num_style_link = val().map(|v| v.into());
            }
        }
        a.levels.sort_by_key(|l| l.ilvl);
        self.abstract_ix.insert(id, self.abstracts.len());
        self.abstracts.push(a);
    }

    fn read_num(&mut self, n: &NodeRef<'_>) {
        let raw = w_attr(n, "numId");
        let Some(id) = raw.as_deref().and_then(int) else {
            self.issues.push(NumberingIssue::InvalidNumId {
                value: raw.as_deref().unwrap_or("").into(),
            });
            return;
        };
        let Entry::Vacant(slot) = self.num_ix.entry(id) else {
            self.issues.push(NumberingIssue::DuplicateNum { id });
            return;
        };
        let abstract_id = n
            .child_elements()
            .find(|c| is_w(c, "abstractNumId"))
            .and_then(|c| w_attr(&c, "val").as_deref().and_then(int));
        let Some(abstract_id) = abstract_id else {
            self.issues
                .push(NumberingIssue::InvalidNumAbstractId { num_id: id });
            return;
        };
        slot.insert(self.nums.len());
        let owner = LevelOwner::Num(id);
        let mut overrides: Vec<LevelOverride> = Vec::new();
        for o in n.child_elements().filter(|c| is_w(c, "lvlOverride")) {
            let Some(ilvl) = read_ilvl(&o, owner, &mut self.issues) else {
                continue;
            };
            if overrides.iter().any(|x| x.ilvl == ilvl) {
                self.issues
                    .push(NumberingIssue::DuplicateOverride { num_id: id, ilvl });
                continue;
            }
            let mut start = None;
            if let Some(s) = o.child_elements().find(|c| is_w(c, "startOverride")) {
                let raw = w_attr(&s, "val");
                start = raw.as_deref().and_then(int);
                if start.is_none() {
                    self.issues.push(NumberingIssue::InvalidValue {
                        owner,
                        ilvl,
                        element: "w:startOverride",
                        value: raw.as_deref().unwrap_or("").into(),
                    });
                }
            }
            let level = o
                .child_elements()
                .find(|c| is_w(c, "lvl"))
                .map(|l| read_level(&l, ilvl, owner, &mut self.issues));
            overrides.push(LevelOverride { ilvl, start, level });
        }
        self.nums.push(Num {
            id,
            abstract_id,
            overrides,
        });
    }
}
