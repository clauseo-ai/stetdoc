//! `dump`, the normal form of what a document says, which the compare laws compare (design `compare.md`, 9.1 and 20).
//!
//! `dump` is written here and shares no code with `build`. For each story (the body, the notes, each note on its own and in a
//! fixed order, and each header and footer) it lists the blocks in order: the structure of tables (a row, the cell a paragraph is
//! in), the style by name (with ` (original)` removed), the direct paragraph properties and the properties of the mark, the list
//! by its definition, the computed clause number where both inputs have the list's definition, the text, and the run properties
//! of each stretch of text with the character style by name. The blocks that the receipt excludes are left out of both sides, and
//! a header or footer slot that exists on one side only is not compared. What `compare` does not compare (design 8.3) is not in
//! it: table, row, cell and section properties, style and numbering definitions, comments.

use super::fmt::canon;
use super::import::list_definitions;
use crate::api::error::StetError;
use crate::state::State;
use crate::wordml::{BlockKind, Mode, Paragraph, RelTarget, Story, StoryKind};
use std::collections::{HashMap, HashSet};

/// The name of each style of the document, by id.
fn style_names(state: &State) -> HashMap<String, String> {
    let view = state.parts();
    let mut out = HashMap::new();
    let Ok(specs) = view.story_specs() else {
        return out;
    };
    let Some(body) = specs.0.first() else {
        return out;
    };
    let Some(id) = view.id(&body.part) else {
        return out;
    };
    let Ok(rels) = view.rels(id) else { return out };
    let Some(RelTarget::Part(name)) = rels
        .iter()
        .find(|r| r.rel_type.ends_with("/styles"))
        .map(|r| r.target.clone())
    else {
        return out;
    };
    let Some(sid) = view.id(&name) else {
        return out;
    };
    let Ok(tree) = view.tree(sid) else { return out };
    for s in tree
        .root()
        .child_elements()
        .filter(|c| c.local_name() == "style")
    {
        let id = s
            .attrs()
            .find(|a| a.local_name() == "styleId")
            .map(|a| a.value().into_owned());
        let nm = s
            .child_elements()
            .find(|c| c.local_name() == "name")
            .and_then(|c| {
                c.attrs()
                    .find(|a| a.local_name() == "val")
                    .map(|a| a.value().into_owned())
            });
        if let (Some(i), Some(n)) = (id, nm) {
            out.insert(i, n.trim_end_matches(" (original)").to_owned());
        }
    }
    out
}

/// Counts the cells of a row that come before `target`, in document order, whatever wraps them.
fn cells_before(n: &crate::xml::NodeRef<'_>, target: crate::wordml::NodeId, k: &mut usize) -> bool {
    for c in n.child_elements() {
        if c.local_name() == "tc" {
            if c.id() == target {
                return true;
            }
            *k += 1;
        } else if c.local_name() != "tbl" && cells_before(&c, target, k) {
            return true;
        }
    }
    false
}

/// What a dump leaves out and adds: the blocks that the receipt excludes (by part and node, a table or row with what is in
/// it), and the list definitions that both inputs have, whose paragraphs show their computed clause numbers.
#[derive(Default)]
pub(super) struct DumpOpts {
    /// (part, node) of each block to leave out.
    pub drop: HashSet<(String, crate::wordml::NodeId)>,
    /// The list definitions whose numbers are compared.
    pub shared_lists: HashSet<String>,
}

/// True if the block or one of its ancestors is to be left out.
fn dropped(
    tree: &crate::xml::SpanTree,
    part: &str,
    node: crate::wordml::NodeId,
    opts: &DumpOpts,
) -> bool {
    if opts.drop.is_empty() {
        return false;
    }
    let mut cur = Some(tree.node_ref(node));
    while let Some(n) = cur {
        if opts.drop.contains(&(part.to_owned(), n.id())) {
            return true;
        }
        cur = n.parent();
    }
    false
}

/// The canonical form of properties with a property listed twice (a tool's slip) kept once, the first: the redline copies such
/// a property once when it copies a paragraph's properties, and either reading is the same formatting.
fn canon_once(props: &crate::wordml::Props) -> String {
    let all = canon(props);
    let mut out = String::new();
    let mut seen: HashSet<String> = HashSet::new();
    let (mut depth, mut start, mut i) = (0usize, 0usize, 0usize);
    let b = all.as_bytes();
    while i < b.len() {
        if all[i..].starts_with("</>") {
            depth = depth.saturating_sub(1);
            i += 3;
            if depth == 0 {
                let el = &all[start..i];
                let name: String = el.chars().take_while(|c| *c != ' ' && *c != '>').collect();
                if seen.insert(name) {
                    out.push_str(el);
                }
                start = i;
            }
        } else if b.get(i) == Some(&b'<') {
            if depth == 0 {
                start = i;
            }
            depth += 1;
            i = all[i..].find('>').map_or(b.len(), |k| i + k + 1);
        } else {
            i += 1;
        }
    }
    if depth != 0 {
        return all;
    }
    out
}

fn dump_story(
    state: &State,
    story: &Story,
    part: &str,
    names: &HashMap<String, String>,
    lists: &HashMap<i64, String>,
    opts: &DumpOpts,
) -> Vec<(u32, Vec<String>)> {
    let view = state.parts();
    let Some(id) = view.id(part) else {
        return Vec::new();
    };
    let Ok(tree) = view.tree(id) else {
        return Vec::new();
    };
    let numbers = view.numbers(id, Mode::Final).ok();
    let mut units: Vec<(u32, Vec<String>)> = Vec::new();
    for e in story.blocks().iter() {
        if dropped(tree, part, e.node, opts) {
            continue;
        }
        let line = match e.kind {
            BlockKind::Table => "T".to_owned(),
            BlockKind::Row => "R".to_owned(),
            BlockKind::Paragraph => {
                let Some(p) = Paragraph::new(tree, e.node) else {
                    continue;
                };
                // the cell, if the paragraph is in a table: its place among the cells of its row, whatever wraps them
                let mut cell = String::new();
                let mut cur = tree.node_ref(e.node).parent();
                while let Some(n) = cur {
                    if n.local_name() == "tc" {
                        let mut row = n.parent();
                        while let Some(r) = row {
                            if r.local_name() == "tr" {
                                break;
                            }
                            row = r.parent();
                        }
                        let mut k = 0usize;
                        if let Some(r) = row {
                            cells_before(&r, n.id(), &mut k);
                        }
                        cell = format!("{k}.{cell}");
                    }
                    cur = n.parent();
                }
                let style = p
                    .style_id()
                    .map(|s| {
                        names
                            .get(s.as_ref())
                            .cloned()
                            .unwrap_or_else(|| s.into_owned())
                    })
                    .unwrap_or_default();
                let def = p
                    .num_pr()
                    .and_then(|n| n.num_id)
                    .and_then(|i| lists.get(&i).cloned());
                let list = p
                    .num_pr()
                    .map(|n| {
                        format!(
                            "{}:{}",
                            def.clone()
                                .unwrap_or_else(|| format!("{:?}", n.num_id.filter(|i| *i == 0))),
                            n.ilvl.unwrap_or(0)
                        )
                    })
                    .unwrap_or_default();
                // the clause number Word shows, where both inputs have the list's definition
                let number = match (&def, &numbers) {
                    (Some(d), Some(nums)) if opts.shared_lists.contains(d) => {
                        nums.clause_number(e.node).unwrap_or("").to_owned()
                    }
                    _ => String::new(),
                };
                let ppr = canon_once(&p.base_props().without(&["numPr", "pStyle"]));
                let mark = p
                    .mark_props()
                    .map(|r| canon_once(&r.to_owned().without(&["rStyle"])))
                    .unwrap_or_default();
                let map = p.map();
                let mut runs: Vec<String> = Vec::new();
                for item in map.items() {
                    if item.len == 0 || !item.vis.in_mode(Mode::Final) {
                        continue;
                    }
                    let rpr = p
                        .run_props(item)
                        .map(|r| canon(&r.to_owned().without(&["rStyle"])))
                        .unwrap_or_default();
                    let rs = p
                        .run_style(item)
                        .map(|s| {
                            names
                                .get(s.as_ref())
                                .cloned()
                                .unwrap_or_else(|| s.into_owned())
                        })
                        .unwrap_or_default();
                    let sig = format!("{rs}|{rpr}");
                    if runs.last() != Some(&sig) {
                        runs.push(sig);
                    }
                }
                format!(
                    "P c={cell} s={style} l={list} n={number} p={ppr} m={mark} runs={runs:?} | {}",
                    p.text(Mode::Final)
                        .chars()
                        .map(|c| if c < ' ' {
                            format!("<{:x}>", c as u32)
                        } else {
                            c.to_string()
                        })
                        .collect::<String>()
                )
            }
        };
        units.push((e.unit, vec![line]));
    }
    units
}

/// A slot of a header or footer: the section's place, header or footer, and the type.
pub(super) type Slot = (usize, &'static str, &'static str);

/// A normal form of a document.
#[derive(Default)]
pub(super) struct Dump {
    /// The body, then every note by its content.
    pub body: Vec<String>,
    /// Each header and footer part, by name.
    pub sides: HashMap<String, Vec<String>>,
    /// The part of each slot of each section, inheritance applied, and the number of sections.
    pub slots: Vec<(Slot, String)>,
    /// How many sections the body has.
    pub sections: usize,
}

/// The slots of the body's sections.
fn slots_of(state: &State) -> Result<(Vec<(Slot, String)>, usize), StetError> {
    let view = state.parts();
    let specs = view.story_specs()?;
    let Some(body) = specs.0.iter().find(|s| s.kind == StoryKind::Body) else {
        return Ok((Vec::new(), 0));
    };
    let Some(id) = view.id(&body.part) else {
        return Ok((Vec::new(), 0));
    };
    let (story, tree) = (view.story(id)?, view.tree(id)?);
    let sections = story.sections(tree, &*view.rels(id)?);
    let mut out = Vec::new();
    for (i, s) in sections.iter().enumerate() {
        for (kind, slots) in [("header", &s.headers), ("footer", &s.footers)] {
            for (ty, part) in [
                ("default", &slots.default),
                ("first", &slots.first),
                ("even", &slots.even),
            ] {
                if let Some(p) = part {
                    out.push(((i, kind, ty), p.to_string()));
                }
            }
        }
    }
    Ok((out, sections.len()))
}

/// A normal form of a document: the body, then every note by its content, and each header and footer.
pub(super) fn dump(state: &State, opts: &DumpOpts) -> Result<Dump, StetError> {
    let view = state.parts();
    let names = style_names(state);
    let lists = list_definitions(state)?;
    let specs = view.story_specs()?;
    let mut out = Dump::default();
    let mut notes: Vec<String> = Vec::new();
    // the notes that a reference names: a note that nothing names is not shown, and a redline does not carry one
    let mut referenced: HashSet<(bool, i64)> = HashSet::new();
    for spec in specs.0.iter() {
        let Some(id) = view.id(&spec.part) else {
            continue;
        };
        let tree = view.tree(id)?;
        let mut stack = vec![tree.root()];
        while let Some(n) = stack.pop() {
            for c in n.child_elements() {
                let l = c.local_name();
                if (l == "footnoteReference" || l == "endnoteReference")
                    && let Some(v) = c
                        .attrs()
                        .find(|a| a.local_name() == "id")
                        .and_then(|a| a.value().parse::<i64>().ok())
                {
                    referenced.insert((l == "endnoteReference", v));
                }
                stack.push(c);
            }
        }
    }
    for spec in specs.0.iter() {
        let Some(id) = view.id(&spec.part) else {
            continue;
        };
        let story = view.story(id)?;
        let lines = dump_story(state, &story, &spec.part, &names, &lists, opts);
        match spec.kind {
            StoryKind::Body => out.body.extend(lines.into_iter().flat_map(|(_, l)| l)),
            StoryKind::Footnotes | StoryKind::Endnotes => {
                let endnote = spec.kind == StoryKind::Endnotes;
                let mut by_unit: Vec<(u32, Vec<String>)> = Vec::new();
                for (u, l) in lines {
                    let named = story
                        .units()
                        .get(u as usize)
                        .and_then(|x| x.id)
                        .is_some_and(|i| referenced.contains(&(endnote, i)));
                    if !named {
                        continue;
                    }
                    match by_unit.last_mut() {
                        Some((last, ls)) if *last == u => ls.extend(l),
                        _ => by_unit.push((u, l)),
                    }
                }
                let tag = if spec.kind == StoryKind::Endnotes {
                    "E"
                } else {
                    "F"
                };
                notes.extend(
                    by_unit
                        .into_iter()
                        .map(|(_, l)| format!("{tag} {}", l.join("\n"))),
                );
            }
            StoryKind::Header | StoryKind::Footer => {
                out.sides.insert(
                    spec.part.to_string(),
                    lines.into_iter().flat_map(|(_, l)| l).collect(),
                );
            }
            _ => {}
        }
    }
    notes.sort();
    out.body.push("--notes--".to_owned());
    out.body.extend(notes);
    let (slots, sections) = slots_of(state)?;
    out.slots = slots;
    out.sections = sections;
    Ok(out)
}

/// The header and footer parts of two documents that correspond, slot by slot: section k to section k when both have as many,
/// else the last to the last; a part pairs with one part only, and a slot on one side only is left out (it is a slot that
/// `compare_story_unmatched` counts).
pub(super) fn paired_slots(a: &Dump, b: &Dump) -> Vec<(Slot, String, String)> {
    let section_pairs: Vec<(usize, usize)> = if a.sections == b.sections {
        (0..a.sections).map(|i| (i, i)).collect()
    } else if a.sections > 0 && b.sections > 0 {
        vec![(a.sections - 1, b.sections - 1)]
    } else {
        Vec::new()
    };
    let mut out: Vec<(Slot, String, String)> = Vec::new();
    let mut used_a: HashSet<String> = HashSet::new();
    let mut used_b: HashSet<String> = HashSet::new();
    for (i, j) in section_pairs {
        for &((si, kind, ty), ref pa) in a.slots.iter().filter(|s| s.0.0 == i) {
            let _ = si;
            let Some((_, pb)) = b
                .slots
                .iter()
                .find(|((sj, k, t), _)| *sj == j && *k == kind && *t == ty)
            else {
                continue;
            };
            if out.iter().any(|(_, x, y)| x == pa && y == pb) {
                continue;
            }
            if used_a.contains(pa) || used_b.contains(pb) {
                continue;
            }
            used_a.insert(pa.clone());
            used_b.insert(pb.clone());
            out.push(((i, kind, ty), pa.clone(), pb.clone()));
        }
    }
    out
}
