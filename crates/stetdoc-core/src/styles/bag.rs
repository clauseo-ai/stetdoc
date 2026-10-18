//! A set of resolved properties, and how one level of the style hierarchy is laid over another.
//!
//! The default rule is replacement: the property at the higher level replaces the one below it. The
//! exceptions are data in `rule`: some properties merge attribute by attribute (`rFonts`, `ind`),
//! some child by child (`pBdr`, `tabs`). Toggle properties are not merged here: `toggle.rs` combines them.
//! See the design, section 3.2.

use super::source::Source;
use crate::wordml::{Attr, Ns, Prop};

/// One resolved property and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The property, with the attributes and children that apply.
    pub prop: Prop,
    /// The level that supplied it. For a property merged attribute by attribute, the level of the
    /// last attribute written.
    pub source: Source,
    /// For a property merged attribute by attribute: the level of each attribute.
    pub attr_sources: Vec<(Box<str>, Source)>,
}

/// How a property combines with the same property at a lower level.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rule {
    /// The higher level replaces the lower one.
    Replace,
    /// Each attribute is its own property.
    Attributes,
    /// Each child element is its own property.
    Children,
    /// Each tab stop, by position, is its own property.
    Tabs,
}

fn rule(name: &str) -> Rule {
    match name {
        "rFonts" | "lang" | "ind" | "spacing" => Rule::Attributes,
        "pBdr" | "tcBorders" | "tblBorders" | "tblCellMar" | "tcMar" => Rule::Children,
        "tabs" => Rule::Tabs,
        _ => Rule::Replace,
    }
}

/// The attributes that a higher level's attribute `name` replaces along with its own earlier value:
/// the two spellings of one font slot, and the exclusive pairs of an indent.
fn conflicts(prop: &str, name: &str) -> &'static [&'static str] {
    match (prop, name) {
        ("rFonts", "ascii") => &["asciiTheme"],
        ("rFonts", "asciiTheme") => &["ascii"],
        ("rFonts", "hAnsi") => &["hAnsiTheme"],
        ("rFonts", "hAnsiTheme") => &["hAnsi"],
        ("rFonts", "eastAsia") => &["eastAsiaTheme"],
        ("rFonts", "eastAsiaTheme") => &["eastAsia"],
        ("rFonts", "cs") => &["cstheme"],
        ("rFonts", "cstheme") => &["cs"],
        ("ind", "firstLine") => &["hanging", "firstLineChars", "hangingChars"],
        ("ind", "hanging") => &["firstLine", "firstLineChars", "hangingChars"],
        ("ind", "firstLineChars") => &["hanging", "hangingChars", "firstLine"],
        ("ind", "hangingChars") => &["firstLine", "firstLineChars", "hanging"],
        ("ind", "left") => &["start"],
        ("ind", "start") => &["left"],
        ("ind", "leftChars") => &["startChars"],
        ("ind", "startChars") => &["leftChars"],
        ("ind", "right") => &["end"],
        ("ind", "end") => &["right"],
        ("ind", "rightChars") => &["endChars"],
        ("ind", "endChars") => &["rightChars"],
        _ => &[],
    }
}

fn same_key(a: &Prop, b: &Prop) -> bool {
    a.ns == b.ns && a.name == b.name
}

fn entry(prop: &Prop, source: &Source) -> Entry {
    let attr_sources = if rule(&prop.name) == Rule::Attributes && prop.ns == Ns::Word {
        prop.attrs
            .iter()
            .map(|a| (a.name.clone(), source.clone()))
            .collect()
    } else {
        Vec::new()
    };
    Entry {
        prop: prop.clone(),
        source: source.clone(),
        attr_sources,
    }
}

/// Lays `prop`, from `source`, over `bag`.
pub(super) fn overlay(bag: &mut Vec<Entry>, prop: &Prop, source: &Source) {
    let how = if prop.ns == Ns::Word {
        rule(&prop.name)
    } else {
        Rule::Replace
    };
    let existing = bag.iter().position(|e| same_key(&e.prop, prop));
    let Some(at) = existing else {
        bag.push(entry(prop, source));
        return;
    };
    if how == Rule::Replace {
        bag.remove(at);
        bag.push(entry(prop, source));
        return;
    }
    let Some(e) = bag.get_mut(at) else { return };
    match how {
        Rule::Attributes => merge_attributes(e, prop, source),
        Rule::Children => merge_children(e, prop, source),
        Rule::Tabs => merge_tabs(e, prop, source),
        Rule::Replace => {}
    }
}

fn merge_attributes(e: &mut Entry, prop: &Prop, source: &Source) {
    let name = prop.name.to_string();
    for a in &prop.attrs {
        for c in conflicts(&name, &a.name) {
            // Two spellings written in one element stay together: the theme attribute wins at read time.
            if prop.attrs.iter().any(|x| &*x.name == *c) {
                continue;
            }
            e.prop.attrs.retain(|x| &*x.name != *c);
            e.attr_sources.retain(|(n, _)| &**n != *c);
        }
        e.prop.attrs.retain(|x| !(x.ns == a.ns && x.name == a.name));
        e.attr_sources.retain(|(n, _)| *n != a.name);
        e.prop.attrs.push(a.clone());
        e.attr_sources.push((a.name.clone(), source.clone()));
    }
    e.source = source.clone();
}

fn merge_children(e: &mut Entry, prop: &Prop, source: &Source) {
    for c in &prop.children {
        e.prop.children.retain(|x| !same_key(x, c));
        e.prop.children.push(c.clone());
    }
    e.source = source.clone();
}

/// Tab stops merge by position: a later `clear` removes the earlier stop.
fn merge_tabs(e: &mut Entry, prop: &Prop, source: &Source) {
    for tab in &prop.children {
        let pos = tab.attr("pos");
        e.prop.children.retain(|x| x.attr("pos") != pos);
        if tab.attr("val") != Some("clear") {
            e.prop.children.push(tab.clone());
        }
    }
    e.source = source.clone();
}

/// An on/off property as it is written: no `w:val` for true, `w:val="0"` for false.
pub(super) fn on_off_prop(name: &str, on: bool) -> Prop {
    Prop {
        ns: Ns::Word,
        name: name.into(),
        attrs: vec![Attr {
            ns: Ns::Word,
            name: "val".into(),
            value: if on { "1" } else { "0" }.into(),
        }],
        children: Vec::new(),
    }
}

/// Lays a resolved entry over `bag`, keeping the level of each attribute of a property that merges
/// attribute by attribute.
pub(super) fn overlay_entry(bag: &mut Vec<Entry>, e: &Entry) {
    let by_attribute = e.prop.ns == Ns::Word
        && rule(&e.prop.name) == Rule::Attributes
        && !e.attr_sources.is_empty();
    if !by_attribute {
        overlay(bag, &e.prop, &e.source);
        return;
    }
    // Attributes that came from one level are laid down together, so two spellings of one slot
    // written in the same element are not taken for a replacement.
    let mut groups: Vec<(&Source, Vec<Attr>)> = Vec::new();
    for a in &e.prop.attrs {
        let source = e
            .attr_sources
            .iter()
            .find(|(n, _)| *n == a.name)
            .map_or(&e.source, |(_, s)| s);
        match groups.last_mut() {
            Some((s, attrs)) if *s == source => attrs.push(a.clone()),
            _ => groups.push((source, vec![a.clone()])),
        }
    }
    for (source, attrs) in groups {
        let group = Prop {
            ns: e.prop.ns.clone(),
            name: e.prop.name.clone(),
            attrs,
            children: Vec::new(),
        };
        overlay(bag, &group, source);
    }
}
