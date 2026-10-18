//! Style definitions: `StyleDefs`.
//!
//! Reads `word/styles.xml` once into owned structures with no borrow of the span tree. These are
//! definitions only: applying the style cascade (document defaults, then the `basedOn` chain, then
//! direct formatting) to get effective formatting belongs to `styles`. Latent styles and theme fonts
//! are not definitions here.
//!
//! Reading never fails. What a reader skips or cannot resolve is a `StyleIssue`, so `check` can
//! report problems that were already in the input.

use super::names::{int, is_w, is_word_ns, on_off, w_attr};
use super::props::Props;
use crate::xml::{NodeRef, SpanTree};
use std::collections::HashMap;
use std::collections::hash_map::Entry;

/// The longest `basedOn` chain `StyleDefs::chain` follows. Real chains reach nine.
const CHAIN_LIMIT: usize = 64;

/// What a style applies to (`w:type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StyleKind {
    /// A paragraph style.
    Paragraph,
    /// A character style.
    Character,
    /// A table style.
    Table,
    /// A numbering style.
    Numbering,
}

/// The part of a table a conditional format applies to (`w:tblStylePr` `w:type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TableRegion {
    /// The whole table.
    WholeTable,
    /// The first row.
    FirstRow,
    /// The last row.
    LastRow,
    /// The first column.
    FirstCol,
    /// The last column.
    LastCol,
    /// Odd columns.
    Band1Vert,
    /// Even columns.
    Band2Vert,
    /// Odd rows.
    Band1Horz,
    /// Even rows.
    Band2Horz,
    /// The top right cell.
    NeCell,
    /// The top left cell.
    NwCell,
    /// The bottom right cell.
    SeCell,
    /// The bottom left cell.
    SwCell,
}

impl TableRegion {
    fn parse(value: &str) -> Option<TableRegion> {
        Some(match value.trim() {
            "wholeTable" => TableRegion::WholeTable,
            "firstRow" => TableRegion::FirstRow,
            "lastRow" => TableRegion::LastRow,
            "firstCol" => TableRegion::FirstCol,
            "lastCol" => TableRegion::LastCol,
            "band1Vert" => TableRegion::Band1Vert,
            "band2Vert" => TableRegion::Band2Vert,
            "band1Horz" => TableRegion::Band1Horz,
            "band2Horz" => TableRegion::Band2Horz,
            "neCell" => TableRegion::NeCell,
            "nwCell" => TableRegion::NwCell,
            "seCell" => TableRegion::SeCell,
            "swCell" => TableRegion::SwCell,
            _ => return None,
        })
    }
}

/// The five property containers a style, or one conditional table format, can hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleProps {
    /// `w:pPr`.
    pub paragraph: Props,
    /// `w:rPr`.
    pub run: Props,
    /// `w:tblPr`.
    pub table: Props,
    /// `w:trPr`.
    pub row: Props,
    /// `w:tcPr`.
    pub cell: Props,
}

impl StyleProps {
    /// Reads `c` into its slot if it is one of the five containers. The first one of each wins.
    fn absorb(&mut self, c: &NodeRef<'_>) {
        let slot = match c.local_name() {
            "pPr" => &mut self.paragraph,
            "rPr" => &mut self.run,
            "tblPr" => &mut self.table,
            "trPr" => &mut self.row,
            "tcPr" => &mut self.cell,
            _ => return,
        };
        if slot.is_empty() {
            *slot = Props::read(*c);
        }
    }
}

/// One `w:style`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleDef {
    /// The `w:styleId`, which is what `w:pStyle` and the others name.
    pub id: Box<str>,
    /// What the style applies to. A missing `w:type` is a paragraph style.
    pub kind: StyleKind,
    /// The `w:name`, empty if there is none. It is not the id, and it can be lower case or localized.
    pub name: Box<str>,
    /// The `w:basedOn` id.
    pub based_on: Option<Box<str>>,
    /// The `w:next` id: the style of the paragraph after this one.
    pub next: Option<Box<str>>,
    /// The `w:link` id: the paired paragraph or character style.
    pub link: Option<Box<str>>,
    /// `w:default`: the default style of its kind.
    pub default: bool,
    /// `w:customStyle`: the user made the style.
    pub custom: bool,
    /// `w:hidden`.
    pub hidden: bool,
    /// `w:semiHidden`.
    pub semi_hidden: bool,
    /// `w:qFormat`: the style appears in the style gallery.
    pub q_format: bool,
    /// `w:uiPriority`.
    pub ui_priority: Option<i32>,
    /// The comma separated `w:aliases`, split and trimmed.
    pub aliases: Vec<Box<str>>,
    /// The style's `w:pPr`.
    pub paragraph: Props,
    /// The style's `w:rPr`.
    pub run: Props,
    /// The style's `w:tblPr`.
    pub table: Props,
    /// The style's `w:trPr`.
    pub row: Props,
    /// The style's `w:tcPr`.
    pub cell: Props,
    /// The `w:tblStylePr` blocks, in the order written.
    pub conditional: Vec<(TableRegion, StyleProps)>,
}

/// The `w:docDefaults`: the base of the style cascade.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocDefaults {
    /// `w:rPrDefault`'s `w:rPr`.
    pub run: Props,
    /// `w:pPrDefault`'s `w:pPr`.
    pub paragraph: Props,
}

/// Something reading `styles.xml` skipped or could not resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleIssue {
    /// The part's root is not `w:styles`. The definitions are empty.
    NotStyles {
        /// The root's local name.
        root: Box<str>,
    },
    /// A `w:style` with no `w:styleId`. It is not kept.
    MissingId {
        /// The style's `w:name`, empty if it has none.
        name: Box<str>,
    },
    /// Two styles with this id. The first is the one `get` returns, and both are kept.
    DuplicateId {
        /// The id.
        id: Box<str>,
    },
    /// A `w:type` that is not paragraph, character, table or numbering. Read as a paragraph style.
    UnknownType {
        /// The style's id.
        id: Box<str>,
        /// The value as written.
        value: Box<str>,
    },
    /// A `w:tblStylePr` with a missing or unknown `w:type`. It is not kept.
    UnknownRegion {
        /// The style's id.
        id: Box<str>,
        /// The value as written, empty if missing.
        value: Box<str>,
    },
    /// A `w:basedOn` that names no style.
    BasedOnMissing {
        /// The style's id.
        id: Box<str>,
        /// The id it names.
        target: Box<str>,
    },
    /// A `w:next` that names no style.
    NextMissing {
        /// The style's id.
        id: Box<str>,
        /// The id it names.
        target: Box<str>,
    },
    /// A `w:link` that names no style.
    LinkMissing {
        /// The style's id.
        id: Box<str>,
        /// The id it names.
        target: Box<str>,
    },
    /// The `basedOn` chain from this style returns to a style already on it. Reported once for each
    /// cycle, at the first style of it that reading reached.
    BasedOnCycle {
        /// The id of the style the chain returned to.
        id: Box<str>,
    },
}

/// The style definitions of a document.
#[derive(Debug, Clone, Default)]
pub struct StyleDefs {
    /// The `w:docDefaults`.
    pub doc_defaults: DocDefaults,
    styles: Vec<StyleDef>,
    by_id: HashMap<Box<str>, usize>,
    by_name: HashMap<Box<str>, usize>,
    defaults: Vec<(StyleKind, usize)>,
    issues: Vec<StyleIssue>,
}

impl StyleDefs {
    /// Reads `styles.xml`. A root that is not `w:styles` gives empty definitions and an issue.
    pub fn read(tree: &SpanTree) -> StyleDefs {
        let mut defs = StyleDefs::default();
        let root = tree.root();
        if !is_w(&root, "styles") {
            defs.issues.push(StyleIssue::NotStyles {
                root: root.local_name().into(),
            });
            return defs;
        }
        let mut seen_defaults = false;
        for c in root.child_elements() {
            if is_w(&c, "docDefaults") && !seen_defaults {
                seen_defaults = true;
                defs.doc_defaults = read_doc_defaults(&c);
            } else if is_w(&c, "style")
                && let Some(s) = read_style(&c, &mut defs.issues)
            {
                defs.styles.push(s);
            }
        }
        defs.build_indexes();
        defs.check_links();
        defs.check_cycles();
        defs
    }

    /// Every style, in the order written, including the later one of a duplicate id.
    pub fn styles(&self) -> &[StyleDef] {
        &self.styles
    }

    /// The style with this id. The first definition wins when an id is duplicated.
    pub fn get(&self, id: &str) -> Option<&StyleDef> {
        self.styles.get(self.index_of(id)?)
    }

    /// The style with this name, ignoring ASCII case. A name match wins over an alias match. Only
    /// the style `get` returns for its id can be found, so the three lookups agree.
    pub fn by_name(&self, name: &str) -> Option<&StyleDef> {
        let i = self.by_name.get(name.to_ascii_lowercase().as_str())?;
        self.styles.get(*i)
    }

    /// The default style (`w:default`) of a kind. The first one wins if a file has several.
    pub fn default_for(&self, kind: StyleKind) -> Option<&StyleDef> {
        let (_, i) = self.defaults.iter().find(|(k, _)| *k == kind)?;
        self.styles.get(*i)
    }

    /// The style, then its `basedOn` parents. It stops at an id with no style, at a style it has
    /// already yielded, and after 64 styles, so a cycle in a hostile file ends.
    pub fn chain(&self, id: &str) -> StyleChain<'_> {
        StyleChain {
            defs: self,
            next: self.index_of(id),
            seen: Vec::new(),
        }
    }

    /// What reading skipped or could not resolve, in the order found.
    pub fn issues(&self) -> &[StyleIssue] {
        &self.issues
    }

    fn index_of(&self, id: &str) -> Option<usize> {
        self.by_id.get(id).copied()
    }

    /// Fills the id, name and default indexes, and reports duplicate ids.
    fn build_indexes(&mut self) {
        for (i, s) in self.styles.iter().enumerate() {
            match self.by_id.entry(s.id.clone()) {
                Entry::Vacant(v) => {
                    v.insert(i);
                }
                Entry::Occupied(_) => self
                    .issues
                    .push(StyleIssue::DuplicateId { id: s.id.clone() }),
            }
        }
        let winners: Vec<usize> = self
            .styles
            .iter()
            .enumerate()
            .filter(|(i, s)| self.by_id.get(&s.id) == Some(i))
            .map(|(i, _)| i)
            .collect();
        for &i in &winners {
            let Some(s) = self.styles.get(i) else {
                continue;
            };
            if !s.name.is_empty() {
                let key: Box<str> = s.name.to_ascii_lowercase().into();
                self.by_name.entry(key).or_insert(i);
            }
            if s.default && !self.defaults.iter().any(|(k, _)| *k == s.kind) {
                self.defaults.push((s.kind, i));
            }
        }
        for &i in &winners {
            let Some(s) = self.styles.get(i) else {
                continue;
            };
            for a in &s.aliases {
                let key: Box<str> = a.to_ascii_lowercase().into();
                self.by_name.entry(key).or_insert(i);
            }
        }
    }

    /// Reports a `basedOn`, `next` or `link` that names no style.
    fn check_links(&mut self) {
        for s in &self.styles {
            let missing = |t: &Option<Box<str>>| {
                t.as_ref()
                    .filter(|t| !self.by_id.contains_key(&***t))
                    .cloned()
            };
            if let Some(target) = missing(&s.based_on) {
                let id = s.id.clone();
                self.issues.push(StyleIssue::BasedOnMissing { id, target });
            }
            if let Some(target) = missing(&s.next) {
                let id = s.id.clone();
                self.issues.push(StyleIssue::NextMissing { id, target });
            }
            if let Some(target) = missing(&s.link) {
                let id = s.id.clone();
                self.issues.push(StyleIssue::LinkMissing { id, target });
            }
        }
    }

    /// Reports each `basedOn` cycle once. Every style is entered at most once, so the work is
    /// linear: a walk marks the styles on its path as open, and meeting an open style is a cycle.
    fn check_cycles(&mut self) {
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            New,
            Open,
            Done,
        }
        let mut marks: Vec<Mark> = self
            .styles
            .iter()
            .enumerate()
            .map(|(i, s)| {
                if self.by_id.get(&s.id) == Some(&i) {
                    Mark::New
                } else {
                    Mark::Done
                }
            })
            .collect();
        for start in 0..marks.len() {
            let mut path = Vec::new();
            let mut cur = Some(start);
            while let Some(i) = cur {
                match marks.get(i).copied() {
                    Some(Mark::New) => {
                        if let Some(m) = marks.get_mut(i) {
                            *m = Mark::Open;
                        }
                        path.push(i);
                        cur = self
                            .styles
                            .get(i)
                            .and_then(|s| s.based_on.as_deref())
                            .and_then(|b| self.index_of(b));
                    }
                    Some(Mark::Open) => {
                        if let Some(s) = self.styles.get(i) {
                            self.issues
                                .push(StyleIssue::BasedOnCycle { id: s.id.clone() });
                        }
                        break;
                    }
                    _ => break,
                }
            }
            for i in path {
                if let Some(m) = marks.get_mut(i) {
                    *m = Mark::Done;
                }
            }
        }
    }
}

/// The styles a `basedOn` chain visits, from `StyleDefs::chain`.
#[derive(Debug, Clone)]
pub struct StyleChain<'a> {
    defs: &'a StyleDefs,
    next: Option<usize>,
    seen: Vec<usize>,
}

impl<'a> Iterator for StyleChain<'a> {
    type Item = &'a StyleDef;

    fn next(&mut self) -> Option<&'a StyleDef> {
        if self.seen.len() >= CHAIN_LIMIT {
            self.next = None;
        }
        let i = self.next?;
        if self.seen.contains(&i) {
            self.next = None;
            return None;
        }
        self.seen.push(i);
        let style = self.defs.styles.get(i);
        self.next = style
            .and_then(|s| s.based_on.as_deref())
            .and_then(|b| self.defs.index_of(b));
        style
    }
}

/// Reads `w:docDefaults`.
fn read_doc_defaults(n: &NodeRef<'_>) -> DocDefaults {
    let mut d = DocDefaults::default();
    for c in n.child_elements() {
        if is_w(&c, "rPrDefault") && d.run.is_empty() {
            if let Some(p) = c.child_elements().find(|p| is_w(p, "rPr")) {
                d.run = Props::read(p);
            }
        } else if is_w(&c, "pPrDefault")
            && d.paragraph.is_empty()
            && let Some(p) = c.child_elements().find(|p| is_w(p, "pPr"))
        {
            d.paragraph = Props::read(p);
        }
    }
    d
}

/// The `w:val` of `c`, if it has one.
fn val(c: &NodeRef<'_>) -> Option<Box<str>> {
    w_attr(c, "val").map(|v| v.into())
}

/// Reads one `w:style`, or reports why it is skipped.
fn read_style(n: &NodeRef<'_>, issues: &mut Vec<StyleIssue>) -> Option<StyleDef> {
    let name: Box<str> = n
        .child_elements()
        .find(|c| is_w(c, "name"))
        .and_then(|c| val(&c))
        .unwrap_or_default();
    let id: Box<str> = match w_attr(n, "styleId") {
        Some(id) if !id.is_empty() => id.into(),
        _ => {
            issues.push(StyleIssue::MissingId { name });
            return None;
        }
    };
    let kind = match w_attr(n, "type").as_deref().map(str::trim) {
        None | Some("paragraph") => StyleKind::Paragraph,
        Some("character") => StyleKind::Character,
        Some("table") => StyleKind::Table,
        Some("numbering") => StyleKind::Numbering,
        Some(other) => {
            issues.push(StyleIssue::UnknownType {
                id: id.clone(),
                value: other.into(),
            });
            StyleKind::Paragraph
        }
    };
    let flag = |attr: &str| w_attr(n, attr).is_some_and(|v| on_off(Some(&v)));
    let mut s = StyleDef {
        kind,
        name,
        based_on: None,
        next: None,
        link: None,
        default: flag("default"),
        custom: flag("customStyle"),
        hidden: false,
        semi_hidden: false,
        q_format: false,
        ui_priority: None,
        aliases: Vec::new(),
        paragraph: Props::new(),
        run: Props::new(),
        table: Props::new(),
        row: Props::new(),
        cell: Props::new(),
        conditional: Vec::new(),
        id,
    };
    let mut own = StyleProps::default();
    for c in n.child_elements() {
        if !is_word_ns(c.namespace()) {
            continue;
        }
        let flag = || on_off(w_attr(&c, "val").as_deref());
        match c.local_name() {
            "basedOn" if s.based_on.is_none() => s.based_on = val(&c),
            "next" if s.next.is_none() => s.next = val(&c),
            "link" if s.link.is_none() => s.link = val(&c),
            "hidden" => s.hidden = flag(),
            "semiHidden" => s.semi_hidden = flag(),
            "qFormat" => s.q_format = flag(),
            "uiPriority" if s.ui_priority.is_none() => {
                s.ui_priority = w_attr(&c, "val")
                    .and_then(|v| int(&v))
                    .and_then(|v| i32::try_from(v).ok());
            }
            "aliases" if s.aliases.is_empty() => {
                if let Some(v) = w_attr(&c, "val") {
                    s.aliases = v
                        .split(',')
                        .map(str::trim)
                        .filter(|a| !a.is_empty())
                        .map(Into::into)
                        .collect();
                }
            }
            "tblStylePr" => {
                let region = w_attr(&c, "type");
                match region.as_deref().and_then(TableRegion::parse) {
                    Some(r) => {
                        let mut sp = StyleProps::default();
                        c.child_elements().for_each(|p| sp.absorb(&p));
                        s.conditional.push((r, sp));
                    }
                    None => issues.push(StyleIssue::UnknownRegion {
                        id: s.id.clone(),
                        value: region.as_deref().unwrap_or("").into(),
                    }),
                }
            }
            _ => own.absorb(&c),
        }
    }
    s.paragraph = own.paragraph;
    s.run = own.run;
    s.table = own.table;
    s.row = own.row;
    s.cell = own.cell;
    Some(s)
}
