//! Effective formatting: the resolved properties of a paragraph or a run, each with its source.

use super::bag::{Entry, on_off_prop, overlay_entry};
use super::source::Source;
use super::theme::{Rgb, Theme};
use super::toggle::is_toggle;
use crate::wordml::Ns;
use std::sync::Arc;

/// A font slot of `w:rFonts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontSlot {
    /// `ascii`.
    Ascii,
    /// `hAnsi`.
    HAnsi,
    /// `eastAsia`.
    EastAsia,
    /// `cs`.
    Complex,
}

/// The properties that apply, with where each came from.
#[derive(Clone, Debug)]
pub struct Effective {
    entries: Vec<Entry>,
    theme: Arc<Theme>,
}

impl Effective {
    pub(super) fn new(entries: Vec<Entry>, theme: Arc<Theme>) -> Effective {
        Effective { entries, theme }
    }

    /// Every property, in the order they were laid down.
    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    /// The Word-namespace property `name`.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.prop.ns == Ns::Word && &*e.prop.name == name)
    }

    /// An on/off property: false when it is absent.
    pub fn on(&self, name: &str) -> bool {
        self.get(name).is_some_and(|e| e.prop.on_off())
    }

    /// The `w:val` of the property `name`.
    pub fn val(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(|e| e.prop.val())
    }

    /// The typeface of a slot. A theme attribute beats the explicit one in the same element.
    pub fn font(&self, slot: FontSlot) -> Option<String> {
        let e = self.get("rFonts")?;
        let (plain, themed) = match slot {
            FontSlot::Ascii => ("ascii", "asciiTheme"),
            FontSlot::HAnsi => ("hAnsi", "hAnsiTheme"),
            FontSlot::EastAsia => ("eastAsia", "eastAsiaTheme"),
            FontSlot::Complex => ("cs", "cstheme"),
        };
        if let Some(face) = e.prop.attr(themed).and_then(|t| self.theme.font(t)) {
            return Some(face);
        }
        e.prop.attr(plain).map(str::to_string)
    }

    /// The text colour, with the theme resolved. `None` for `auto` or no colour.
    pub fn color(&self) -> Option<Rgb> {
        let e = self.get("color")?;
        let hex = |s: Option<&str>| s.and_then(|v| u8::from_str_radix(v.trim(), 16).ok());
        if let Some(tc) = e.prop.attr("themeColor")
            && let Some(rgb) = self.theme.color(
                tc,
                hex(e.prop.attr("themeTint")),
                hex(e.prop.attr("themeShade")),
            )
        {
            return Some(rgb);
        }
        e.prop.attr("val").and_then(Rgb::parse)
    }
}

/// Folds the document defaults, the three style kinds (table, paragraph, character) and direct
/// formatting. A toggle property that two or more levels mention is the exclusive-or of their values.
pub(super) fn combine(default: &[Entry], kinds: &[&[Entry]], direct: &[Entry]) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for e in default {
        overlay_entry(&mut out, e);
    }
    for kind in kinds {
        for e in *kind {
            if e.prop.ns == Ns::Word && is_toggle(&e.prop.name) {
                xor(&mut out, e);
            } else {
                overlay_entry(&mut out, e);
            }
        }
    }
    for e in direct {
        overlay_entry(&mut out, e);
    }
    out
}

/// Exclusive-ors a toggle property into `out`.
fn xor(out: &mut Vec<Entry>, e: &Entry) {
    let name = &e.prop.name;
    let Some(i) = out
        .iter()
        .position(|x| x.prop.ns == Ns::Word && x.prop.name == *name)
    else {
        out.push(e.clone());
        return;
    };
    let Some(existing) = out.get_mut(i) else {
        return;
    };
    let current = existing.prop.on_off();
    let mut contributions = match &existing.source {
        Source::Toggle(v) => v.clone(),
        s => vec![(s.clone(), current)],
    };
    contributions.push((e.source.clone(), e.prop.on_off()));
    *existing = Entry {
        prop: on_off_prop(name, current ^ e.prop.on_off()),
        source: Source::Toggle(contributions),
        attr_sources: Vec::new(),
    };
}
