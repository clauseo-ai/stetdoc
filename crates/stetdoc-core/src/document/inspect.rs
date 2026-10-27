//! `Document::inspect`: effective formatting with its source for a block, and the structure of the styles, lists and sections.
//!
//! Everything here is read from what `styles` (the cascade and the source of each value), `numbering`, `index` and `wordml`
//! already know, so this file formats and does not decide. A paragraph answers with its kind and story, its style, its number,
//! its clause, every effective paragraph property with the level of the hierarchy that gave it, and the distinct run
//! formattings with the characters each covers. Every block then lists its tracked changes and comments
//! (`inspect_marks.rs`). The listings are in `inspect_lists.rs`.

use super::Document;
use crate::api::error::{ErrorCode, StetError};
use crate::api::inspect::{InspectItem, Inspection};
use crate::api::view::Mode;
use crate::numbering::ItemKind as NumberKind;
use crate::projection::story_name;
use crate::refs::{BlockRef, Claim};
use crate::state::{PartsView, RunKey};
use crate::styles::{Effective, Source};
use crate::wordml::{BlockKey, BlockKind, StyleKind};

/// The lines of an answer.
#[derive(Default)]
pub(super) struct Lines(pub Vec<InspectItem>);

impl Lines {
    /// One line: what it is about, its value, and where the value comes from.
    pub(super) fn add(
        &mut self,
        label: impl Into<String>,
        value: impl Into<String>,
        source: Option<String>,
    ) {
        self.0.push(InspectItem {
            label: label.into(),
            value: value.into(),
            source,
        });
    }
}

/// Where a value of effective formatting came from, in words.
fn source_text(source: &Source) -> String {
    match source {
        Source::Default => "default".to_string(),
        Source::TableStyle { id, region: None } => format!("table style {id}"),
        Source::TableStyle {
            id,
            region: Some(region),
        } => format!("table style {id}, {region:?}").to_lowercase(),
        Source::Numbering { num_id, ilvl } => format!("numbering {num_id} level {ilvl}"),
        Source::ParagraphStyle { id } => format!("paragraph style {id}"),
        Source::CharacterStyle { id } => format!("character style {id}"),
        Source::Direct => "direct".to_string(),
        Source::Toggle(levels) => {
            let said: Vec<String> = levels
                .iter()
                .map(|(s, on)| format!("{} {}", source_text(s), if *on { "on" } else { "off" }))
                .collect();
            format!("toggle: {}", said.join(", "))
        }
    }
}

/// A property's value: its attributes, or `on` for one that has none.
fn value_text(prop: &crate::wordml::Prop) -> String {
    let attrs: Vec<String> = prop
        .attrs
        .iter()
        .map(|a| format!("{}={}", a.name, a.value))
        .collect();
    let children: Vec<&str> = prop.children.iter().map(|c| &*c.name).collect();
    match (attrs.is_empty(), children.is_empty()) {
        (true, true) => "on".to_string(),
        (false, true) => attrs.join(" "),
        (true, false) => format!("[{}]", children.join(" ")),
        (false, false) => format!("{} [{}]", attrs.join(" "), children.join(" ")),
    }
}

/// Properties of a paragraph that say what it is and not how it looks: the style and the list have their own lines.
const STRUCTURE: [&str; 5] = ["pStyle", "numPr", "rPr", "sectPr", "pPrChange"];

/// The character formatting that a reader of a document asks about, in the order it is written.
const RUN_PROPS: [&str; 12] = [
    "rFonts",
    "sz",
    "b",
    "i",
    "u",
    "strike",
    "caps",
    "smallCaps",
    "color",
    "highlight",
    "vertAlign",
    "vanish",
];

/// The run formatting of an effective set: the properties of `RUN_PROPS` that are on or have a value.
fn run_signature(effective: &Effective) -> Vec<(String, String)> {
    RUN_PROPS
        .iter()
        .filter_map(|name| {
            let entry = effective.get(name)?;
            let value = value_text(&entry.prop);
            let off = matches!(
                value.as_str(),
                "val=0" | "val=false" | "val=off" | "val=none"
            );
            (!off).then(|| ((*name).to_string(), value))
        })
        .collect()
}

impl Document {
    /// Effective formatting and its source for a block, or the structure of the document: `p42`, `t3`, `r17`, `styles`,
    /// `numbering` or `sections`.
    pub fn inspect(&self, target: &str) -> Result<Inspection, StetError> {
        let target = target.trim();
        let parts = self.state.parts();
        let mut lines = Lines::default();
        match target {
            "styles" => super::inspect_lists::styles(parts, &mut lines)?,
            "numbering" => super::inspect_lists::numbering(parts, &mut lines)?,
            "sections" => super::inspect_lists::sections(parts, &mut lines)?,
            _ if BlockRef::parse(target).is_ok() => {
                let key = self.state.resolve(Claim::Implied, target)?;
                block(parts, &key, target, &mut lines)?;
            }
            _ => {
                return Err(StetError::new(
                    ErrorCode::InvalidPayload,
                    format!("`{target}` is not something to inspect."),
                )
                .with_next(
                    "Name a block (`p42`, `t3`, `r17`), or `styles`, `numbering` or `sections`.",
                ));
            }
        }
        let text = lines
            .0
            .iter()
            .map(|l| match &l.source {
                Some(source) => format!("{}: {} ({source})", l.label, l.value),
                None => format!("{}: {}", l.label, l.value),
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(Inspection {
            base: self.base(),
            target: target.to_string(),
            text,
            items: lines.0,
        })
    }
}

/// A stretch of the Final text with one run formatting.
struct RunGroup {
    from: u32,
    to: u32,
    signature: Vec<(String, String)>,
}

/// What a block is, and for a paragraph everything its formatting comes to.
fn block(
    parts: PartsView<'_>,
    key: &BlockKey,
    shown: &str,
    out: &mut Lines,
) -> Result<(), StetError> {
    let internal = |what: &str| StetError::new(ErrorCode::Internal, format!("internal: {what}"));
    let part = parts
        .id(key.part())
        .ok_or_else(|| internal("a block's part is not in the document"))?;
    let story = parts.story(part)?;
    let tree = parts.tree(part)?;
    let at = story
        .index_of(key.node())
        .ok_or_else(|| internal("a block is not in its story"))?;
    let entry = story
        .blocks()
        .get(at)
        .ok_or_else(|| internal("a block has no entry"))?;
    let kind = match entry.kind {
        BlockKind::Paragraph => "paragraph",
        BlockKind::Table => "table",
        BlockKind::Row => "row",
    };
    out.add(
        "block",
        format!(
            "{shown} is a {kind} in the {} story",
            story_name(story.kind())
        ),
        None,
    );
    let defs = parts.style_defs()?;
    if entry.kind != BlockKind::Paragraph {
        // A table or a row: its style, and what it holds.
        let node = tree.node_ref(key.node());
        let style = node
            .child_elements()
            .find(|c| c.local_name() == "tblPr")
            .and_then(|pr| {
                crate::wordml::PropsRef::new(pr)
                    .val("tblStyle")
                    .map(|v| v.to_string())
            })
            .or_else(|| defs.default_for(StyleKind::Table).map(|d| d.id.to_string()));
        if let Some(id) = style {
            let name = defs
                .get(&id)
                .map(|d| d.name.to_string())
                .unwrap_or_default();
            out.add("style", style_label(&name, &id), None);
        }
        let children = story
            .blocks()
            .iter()
            .filter(|e| e.parent == u32::try_from(at).ok())
            .count();
        let what = if entry.kind == BlockKind::Table {
            "rows"
        } else {
            "blocks in its cells"
        };
        out.add(what, children.to_string(), None);
        return super::inspect_marks::marks(parts, &story, tree, at, out);
    }
    let paragraph = story
        .paragraph(tree, at)
        .ok_or_else(|| internal("a paragraph block is not a paragraph"))?;
    let own = paragraph.style_id();
    let style = own
        .as_deref()
        .and_then(|id| defs.get(id))
        .or_else(|| defs.default_for(StyleKind::Paragraph));
    if let Some(def) = style {
        let source = own
            .is_none()
            .then(|| "the default paragraph style".to_string());
        out.add("style", style_label(&def.name, &def.id), source);
    }
    let numbers = parts.numbers(part, Mode::Final)?;
    if let Some(item) = numbers.item(key.node()) {
        let label = match &item.kind {
            NumberKind::Counted { text } => format!("number {text}"),
            NumberKind::Bullet { .. } => "bullet".to_string(),
            NumberKind::Plain => "no label".to_string(),
        };
        let from = if item.from_style {
            "the paragraph style"
        } else {
            "direct"
        };
        out.add(
            "list",
            format!("numId {}, level {}, {label}", item.num_id, item.level),
            Some(from.to_string()),
        );
    }
    if let Some(clause) = parts.doc_index()?.clause_of(key) {
        let how = if clause.composed {
            "composed across lists"
        } else {
            "as shown"
        };
        out.add("clause", clause.full.clone(), Some(how.to_string()));
    }
    let cascade = parts.cascade()?;
    let mut sites = parts.sites(part, Mode::Final)?;
    let effective = cascade.effective(&sites.site_for(key.node(), None)?);
    for e in effective
        .iter()
        .filter(|e| !STRUCTURE.contains(&&*e.prop.name))
    {
        let sources: Vec<String> = if e.attr_sources.is_empty() {
            vec![source_text(&e.source)]
        } else {
            let mut distinct: Vec<String> = Vec::new();
            for (_, s) in &e.attr_sources {
                let text = source_text(s);
                if !distinct.contains(&text) {
                    distinct.push(text);
                }
            }
            distinct
        };
        out.add(&*e.prop.name, value_text(&e.prop), Some(sources.join(", ")));
    }
    // The run formattings, each with the characters of the Final text that it covers.
    let map = paragraph.map();
    let mut at_char: u32 = 0;
    let mut groups: Vec<RunGroup> = Vec::new();
    for item in map.items() {
        if item.len == 0 || !item.vis.in_mode(Mode::Final) {
            continue;
        }
        let run = RunKey {
            item,
            complex_script: false,
        };
        let signature = run_signature(&cascade.effective(&sites.site_for(key.node(), Some(run))?));
        match groups.last_mut() {
            Some(last) if last.signature == signature && last.to == at_char => {
                last.to = at_char + item.len;
            }
            _ => groups.push(RunGroup {
                from: at_char,
                to: at_char + item.len,
                signature,
            }),
        }
        at_char += item.len;
    }
    for RunGroup {
        from,
        to,
        signature,
    } in groups
    {
        let value = if signature.is_empty() {
            "plain".to_string()
        } else {
            signature
                .iter()
                .map(|(n, v)| format!("{n} {v}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        out.add(format!("characters {from}..{to}"), value, None);
    }
    super::inspect_marks::marks(parts, &story, tree, at, out)
}

/// `Heading 1 (Heading1)`: the name, and the id that an operation takes.
pub(super) fn style_label(name: &str, id: &str) -> String {
    if name.is_empty() || name == id {
        id.to_string()
    } else {
        format!("{name} ({id})")
    }
}
