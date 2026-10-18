//! The cascade: the style definitions resolved once, and the questions asked of them.
//!
//! `Cascade::build` resolves every style along its `basedOn` chain, so a question about a run is a
//! lookup and a short merge. `effective` merges the levels of the hierarchy in order and keeps the
//! source of each value (this file), and the hidden-text rules and the outline level are in `hidden.rs`.
//! See the design, `docs/design/styles-and-numbering.md`, section 3.

use super::bag::{Entry, overlay_entry};
use super::effective::{Effective, combine};
use super::resolve::{Resolved, TableResolved, resolve_defaults, resolve_style, resolve_table};
use super::site::{RunSite, Site};
use super::source::Source;
use super::tables::{Regions, applicable};
use super::theme::Theme;
use crate::wordml::{PropsRef, SpanTree, StyleDefs, StyleKind, TableSite};
use std::collections::HashMap;
use std::sync::Arc;

/// The numbering a paragraph style supplies, from the nearest `w:numPr` along its `basedOn` chain.
/// `numId` and `ilvl` are inherited separately, so a style may give one and its parent the other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StyleNumbering {
    /// The `w:numId`. `Some(0)` means the style removes the numbering its parent gives.
    pub num_id: Option<i64>,
    /// The `w:ilvl`, if some style on the chain writes one.
    pub ilvl: Option<u8>,
}

/// The resolved style cascade of a document. Immutable, and cheap to share.
#[derive(Debug, Clone)]
pub struct Cascade {
    pub(super) theme: Arc<Theme>,
    numbering: HashMap<Box<str>, StyleNumbering>,
    pub(super) styles: HashMap<Box<str>, Resolved>,
    pub(super) tables: HashMap<Box<str>, TableResolved>,
    pub(super) defaults: Resolved,
    default_paragraph: Option<Box<str>>,
    default_character: Option<Box<str>>,
    default_table: Option<Box<str>>,
}

impl Cascade {
    /// Builds the cascade from the style definitions, the theme part and the settings part. A
    /// missing part is `None`, and the cascade then falls back to the cached values Word writes.
    pub fn build(
        defs: &StyleDefs,
        theme: Option<&SpanTree>,
        settings: Option<&SpanTree>,
    ) -> Cascade {
        let theme = Theme::read(theme, settings);
        let mut numbering = HashMap::new();
        let mut styles = HashMap::new();
        let mut tables = HashMap::new();
        for s in defs.styles() {
            if numbering.contains_key(&s.id) {
                continue;
            }
            numbering.insert(s.id.clone(), resolve_numbering(defs, &s.id));
            match s.kind {
                StyleKind::Table => {
                    tables.insert(s.id.clone(), resolve_table(defs, &theme, &s.id));
                }
                _ => {
                    styles.insert(s.id.clone(), resolve_style(defs, &theme, &s.id));
                }
            }
        }
        let default_of = |k: StyleKind| defs.default_for(k).map(|s| s.id.clone());
        Cascade {
            defaults: resolve_defaults(defs, &theme),
            default_paragraph: default_of(StyleKind::Paragraph),
            default_character: default_of(StyleKind::Character),
            default_table: default_of(StyleKind::Table),
            numbering,
            styles,
            tables,
            theme: Arc::new(theme),
        }
    }

    /// The id of the default paragraph style, the style of a paragraph with no `w:pStyle`.
    pub fn default_paragraph_style(&self) -> Option<&str> {
        self.default_paragraph.as_deref()
    }

    /// The numbering the paragraph style `style` supplies. `None` means the default paragraph
    /// style. A style that does not exist supplies nothing.
    pub fn style_numbering(&self, style: Option<&str>) -> StyleNumbering {
        let id = style.or(self.default_paragraph.as_deref());
        id.and_then(|id| self.numbering.get(id))
            .copied()
            .unwrap_or_default()
    }

    /// The resolved paragraph style of a site (the default one when it names none).
    pub(super) fn paragraph_style(&self, style: Option<&str>) -> Option<&Resolved> {
        let id = style.or(self.default_paragraph.as_deref())?;
        self.styles.get(id)
    }

    /// The resolved character style of a run (the default one when it names none).
    pub(super) fn character_style(&self, run: &RunSite<'_>) -> Option<&Resolved> {
        let id = run.style.or(self.default_character.as_deref())?;
        self.styles.get(id)
    }

    /// The table style of a table site (the default one when it names none), and the regions that
    /// apply to the cell, lowest priority first. Allocates nothing.
    pub(super) fn table_view(&self, ts: &TableSite) -> Option<(&TableResolved, Regions)> {
        let id = ts.style_id.as_deref().or(self.default_table.as_deref())?;
        let table = self.tables.get(id)?;
        Some((table, applicable(ts, table.row_band, table.col_band)))
    }

    /// The whole-table properties of a table style, then the formats of the regions that apply and
    /// that the style defines, lowest priority first.
    pub(super) fn table_levels(
        table: &TableResolved,
        regions: Regions,
    ) -> impl Iterator<Item = &Resolved> {
        std::iter::once(&table.whole).chain(
            regions
                .iter()
                .filter_map(move |r| table.regions.iter().find(|(x, _)| *x == r).map(|(_, v)| v)),
        )
    }

    /// The conditional regions that apply to a cell, lowest priority first, and that the table style
    /// defines a format for. This is what Word records in `w:cnfStyle` (it leaves out a region the style
    /// has nothing for, such as the even band of a style that only formats the odd one), so it is the
    /// oracle's input.
    pub fn applicable_regions(&self, ts: &TableSite) -> Vec<crate::wordml::TableRegion> {
        let Some((table, regions)) = self.table_view(ts) else {
            return Vec::new();
        };
        regions
            .iter()
            .filter(|r| table.regions.iter().any(|(x, _)| x == r))
            .collect()
    }

    /// The table style's contribution for a cell: the whole-table properties, then each applicable
    /// conditional format. `para` picks paragraph properties instead of run properties.
    fn table_entries(&self, ts: &TableSite, para: bool) -> Vec<Entry> {
        let Some((table, regions)) = self.table_view(ts) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for level in Cascade::table_levels(table, regions) {
            for e in if para { &level.para } else { &level.run } {
                overlay_entry(&mut out, e);
            }
        }
        out
    }

    /// The effective formatting at a site: a run's `rPr` properties, or, with no run, the
    /// paragraph's `pPr` properties.
    pub fn effective(&self, site: &Site<'_>) -> Effective {
        let entries = match &site.run {
            Some(run) => self.run_entries(site, run),
            None => self.paragraph_entries(site),
        };
        Effective::new(entries, self.theme.clone())
    }

    fn run_entries(&self, site: &Site<'_>, run: &RunSite<'_>) -> Vec<Entry> {
        let table = site
            .table
            .map(|t| self.table_entries(t, false))
            .unwrap_or_default();
        let para = self
            .paragraph_style(site.style)
            .map(|r| r.run.as_slice())
            .unwrap_or_default();
        let character = self
            .character_style(run)
            .map(|r| r.run.as_slice())
            .unwrap_or_default();
        let direct = run.direct.map(direct_entries).unwrap_or_default();
        combine(&self.defaults.run, &[&table, para, character], &direct)
    }

    fn paragraph_entries(&self, site: &Site<'_>) -> Vec<Entry> {
        let no_indent = |e: &Entry| !(site.numbering_removed && &*e.prop.name == "ind");
        let mut table: Vec<Entry> = site
            .table
            .map(|t| self.table_entries(t, true))
            .unwrap_or_default();
        table.retain(no_indent);
        let mut para: Vec<Entry> = self
            .paragraph_style(site.style)
            .map(|r| r.para.clone())
            .unwrap_or_default();
        para.retain(no_indent);
        let numbering: Vec<Entry> = site
            .level
            .iter()
            .filter_map(|l| {
                l.props.get("ind").map(|p| Entry {
                    prop: p.clone(),
                    source: Source::Numbering {
                        num_id: l.num_id,
                        ilvl: l.ilvl,
                    },
                    attr_sources: Vec::new(),
                })
            })
            .collect();
        let from_style = site.level.is_some_and(|l| l.from_style);
        let (above, below): (&[Entry], &[Entry]) = if from_style {
            (&[], &numbering)
        } else {
            (&numbering, &[])
        };
        let direct = site.ppr.map(direct_entries).unwrap_or_default();
        // Direct numbering sits above the paragraph style, style numbering below it.
        let mut kinds: Vec<&[Entry]> = vec![&table];
        if from_style {
            kinds.push(below);
            kinds.push(&para);
        } else {
            kinds.push(&para);
            kinds.push(above);
        }
        combine(&self.defaults.para, &kinds, &direct)
    }
}

/// The properties of a direct `w:rPr` or `w:pPr`, as entries.
pub(super) fn direct_entries(p: PropsRef<'_>) -> Vec<Entry> {
    p.to_owned()
        .iter()
        .map(|prop| Entry {
            prop: prop.clone(),
            source: Source::Direct,
            attr_sources: Vec::new(),
        })
        .collect()
}

/// Walks the `basedOn` chain of `id` and takes the nearest `numId` and the nearest `ilvl`.
fn resolve_numbering(defs: &StyleDefs, id: &str) -> StyleNumbering {
    let mut out = StyleNumbering::default();
    for style in defs.chain(id) {
        let Some(num_pr) = style.paragraph.get("numPr") else {
            continue;
        };
        if out.num_id.is_none() {
            out.num_id = num_pr
                .child("numId")
                .and_then(|p| p.val())
                .and_then(|v| v.trim().parse::<i64>().ok());
        }
        if out.ilvl.is_none() {
            out.ilvl = num_pr
                .child("ilvl")
                .and_then(|p| p.val())
                .and_then(|v| v.trim().parse::<u8>().ok());
        }
        if out.num_id.is_some() && out.ilvl.is_some() {
            break;
        }
    }
    out
}
