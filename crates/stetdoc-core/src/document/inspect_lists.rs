//! The listings of `Document::inspect`: `styles`, `numbering` and `sections`.

use super::inspect::{Lines, style_label};
use crate::api::error::StetError;
use crate::state::PartsView;
use crate::wordml::{BlockKind, Prop, Props, SectionBreak, StoryKind, StyleKind};
use std::collections::HashMap;

fn kind_word(kind: StyleKind) -> &'static str {
    match kind {
        StyleKind::Paragraph => "paragraph",
        StyleKind::Character => "character",
        StyleKind::Table => "table",
        StyleKind::Numbering => "numbering",
    }
}

/// Every style: its id and name, what it is based on and what follows it, and how many paragraphs use it.
pub(super) fn styles(parts: PartsView<'_>, out: &mut Lines) -> Result<(), StetError> {
    let defs = parts.style_defs()?;
    let default = defs
        .default_for(StyleKind::Paragraph)
        .map(|d| d.id.to_string());
    let mut used: HashMap<String, usize> = HashMap::new();
    for spec in &parts.story_specs()?.0 {
        let Some(part) = parts.id(&spec.part) else {
            continue;
        };
        let (story, tree) = (parts.story(part)?, parts.tree(part)?);
        for (at, entry) in story.blocks().iter().enumerate() {
            if entry.kind != BlockKind::Paragraph {
                continue;
            }
            let Some(paragraph) = story.paragraph(tree, at) else {
                continue;
            };
            let id = paragraph
                .style_id()
                .map(|s| s.to_string())
                .or_else(|| default.clone());
            if let Some(id) = id {
                *used.entry(id).or_default() += 1;
            }
        }
    }
    for def in defs.styles() {
        let mut facts = vec![kind_word(def.kind).to_string()];
        if def.default {
            facts.push("the default".to_string());
        }
        if let Some(parent) = &def.based_on {
            facts.push(format!("based on {parent}"));
        }
        if let Some(next) = &def.next {
            facts.push(format!("next {next}"));
        }
        if def.kind == StyleKind::Paragraph {
            let n = used.get(&*def.id).copied().unwrap_or(0);
            facts.push(format!(
                "{n} {}",
                if n == 1 { "paragraph" } else { "paragraphs" }
            ));
        }
        out.add(
            "style",
            format!("{}: {}", style_label(&def.name, &def.id), facts.join(", ")),
            None,
        );
    }
    Ok(())
}

/// Every list: its `numId`, its abstract list, and each level's format and text.
pub(super) fn numbering(parts: PartsView<'_>, out: &mut Lines) -> Result<(), StetError> {
    let defs = parts.numbering_defs()?;
    for num in defs.nums() {
        let levels: Vec<String> = defs
            .abstract_num(num.abstract_id)
            .map(|a| {
                a.levels
                    .iter()
                    .map(|l| {
                        let fmt = format!("{:?}", l.fmt).to_lowercase();
                        format!("{} {fmt} \"{}\" from {}", l.ilvl, l.text, l.start)
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.add(
            "list",
            format!(
                "numId {}, abstract list {}, levels: {}",
                num.id,
                num.abstract_id,
                levels.join("; ")
            ),
            None,
        );
    }
    Ok(())
}

fn attr(props: &Props, element: &str, name: &str) -> Option<String> {
    let prop: &Prop = props.iter().find(|p| &*p.name == element)?;
    prop.attr(name).map(str::to_string)
}

/// Every section of the body: where it ends, how it starts, the page, and the headers and footers it names.
pub(super) fn sections(parts: PartsView<'_>, out: &mut Lines) -> Result<(), StetError> {
    let list = parts.story_specs()?;
    let Some(spec) = list.0.iter().find(|s| s.kind == StoryKind::Body) else {
        return Ok(());
    };
    let Some(part) = parts.id(&spec.part) else {
        return Ok(());
    };
    let (story, tree) = (parts.story(part)?, parts.tree(part)?);
    let rels = parts.rels(part)?;
    for (n, section) in story.sections(tree, &rels).iter().enumerate() {
        let ends = match section.ends_at.and_then(|i| story.blocks().get(i)) {
            Some(entry) => parts
                .ref_of(&story.key(entry))
                .map_or_else(|| "a paragraph".to_string(), |r| format!("ends at {r}")),
            None => "the last section".to_string(),
        };
        let starts = match section.break_type {
            SectionBreak::NextPage => "next page",
            SectionBreak::Continuous => "continuous",
            SectionBreak::EvenPage => "even page",
            SectionBreak::OddPage => "odd page",
            SectionBreak::NextColumn => "next column",
        };
        let props = Props::read(tree.node_ref(section.node));
        let mut facts = vec![ends, format!("starts on the {starts}")];
        if let (Some(w), Some(h)) = (attr(&props, "pgSz", "w"), attr(&props, "pgSz", "h")) {
            let orient = attr(&props, "pgSz", "orient")
                .map(|o| format!(", {o}"))
                .unwrap_or_default();
            facts.push(format!("page {w} by {h} twips{orient}"));
        }
        let margins: Vec<String> = ["top", "right", "bottom", "left"]
            .iter()
            .filter_map(|side| attr(&props, "pgMar", side).map(|v| format!("{side} {v}")))
            .collect();
        if !margins.is_empty() {
            facts.push(format!("margins {}", margins.join(" ")));
        }
        if let Some(cols) = attr(&props, "cols", "num") {
            facts.push(format!("{cols} columns"));
        }
        let named = |label: &str, kinds: [(&str, &Option<std::sync::Arc<str>>); 3]| {
            let named: Vec<String> = kinds
                .iter()
                .filter_map(|(k, v)| v.as_ref().map(|p| format!("{k} {p}")))
                .collect();
            (!named.is_empty()).then(|| format!("{label}: {}", named.join(", ")))
        };
        let (h, f) = (&section.headers, &section.footers);
        facts.extend(named(
            "headers",
            [
                ("default", &h.default),
                ("first", &h.first),
                ("even", &h.even),
            ],
        ));
        facts.extend(named(
            "footers",
            [
                ("default", &f.default),
                ("first", &f.first),
                ("even", &f.even),
            ],
        ));
        out.add(format!("section {}", n + 1), facts.join("; "), None);
    }
    Ok(())
}
