//! `DocFacts`: what must be the same on every page and in every file of a document, computed once.
//!
//! The authors and their codes (so `JD` is the same person everywhere), the common style, the order of the
//! notes (so `{fn3}` is the same in the text and in `notes.md`), and the counts of the summary line. It is a
//! derived value of `state`, built from every story part, so it is rebuilt on the first read after an edit
//! to any of them and not before (design 3.8).

use super::inlines::{is_textual, slices};
use super::marks::note_refs;
use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::projection::model::{Author, NoteKind};
use crate::state::{Dep, PartsView};
use crate::styles::Cascade;
use crate::wordml::{
    Branch, NodeId, Paragraph, RevKind, SpanTree, StoryKind, StyleDefs, UnitKind, Vis,
};
use std::collections::HashMap;
use std::sync::Arc;

/// The counts of the summary line that do not depend on the mode.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub comments: u32,
    pub footnotes: u32,
    pub endnotes: u32,
    pub sections: u32,
}

/// Where a comment is anchored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommentAt {
    /// The part of the story the range is in.
    pub part: Arc<str>,
    /// The paragraph where the range starts, if it starts in one.
    pub node: Option<NodeId>,
    /// The text the range covers, cut short, if there is any.
    pub text: Option<Box<str>>,
}

/// Facts about the whole document, the same for every page and file.
#[derive(Clone, Debug, Default)]
pub(crate) struct DocFacts {
    /// Every author of a revision or a comment, in order of first appearance.
    pub authors: Vec<Author>,
    /// For each author, how many tracked changes it made.
    pub changes: Vec<u32>,
    /// The commonest paragraph style of the document, by name.
    pub common_style: Option<Box<str>>,
    /// The ordinal of each note, by kind and `w:id`: the position of its first reference in the body, counting every
    /// reference. It is the numbering of the `Redline` view, and the one `delete-note` names notes by.
    pub ordinals: HashMap<(NoteKind, i64), u32>,
    /// How many notes each kind has references to, so unreferenced notes continue the numbering.
    pub referenced: [u32; 2],
    /// The marks of the `Final` view: the numbers Word shows after Accept All, without the references that are pending
    /// deletions. A note with an ordinal above and none here goes when its reference is accepted out. Ids stay `ordinals`.
    pub final_numbers: NoteNumbers,
    /// The marks of the `Original` view: the numbers Word shows after Reject All, without the references that are pending
    /// insertions.
    pub original_numbers: NoteNumbers,
    /// The counts of the summary line.
    pub counts: Counts,
    /// Where each comment is anchored: the paragraph its range starts in, and the text it covers.
    pub comment_at: HashMap<i64, CommentAt>,
    by_name: HashMap<Box<str>, u32>,
}

/// The numbering of the notes in one mode: each note's ordinal by kind and `w:id`, and how many notes of each kind have one.
#[derive(Clone, Debug, Default)]
pub(crate) struct NoteNumbers {
    /// The ordinal of each note that a reference shown in the mode names, by kind and `w:id`.
    pub ordinals: HashMap<(NoteKind, i64), u32>,
    /// How many notes of each kind have one.
    pub referenced: [u32; 2],
}

impl NoteNumbers {
    /// Numbers a note at its first reference.
    fn count(&mut self, kind: NoteKind, id: i64) {
        if !self.ordinals.contains_key(&(kind, id))
            && let Some(n) = self.referenced.get_mut(kind_slot(kind))
        {
            *n += 1;
            self.ordinals.insert((kind, id), *n);
        }
    }
}

/// The name a paragraph style is shown by: the `w:name` of its definition, else its id. No style at all is the
/// document's default paragraph style, and a document that has none reads as `Normal`.
pub(crate) fn style_name(defs: &StyleDefs, cascade: &Cascade, id: Option<&str>) -> Box<str> {
    let id = id.or_else(|| cascade.default_paragraph_style());
    let Some(id) = id else {
        return "Normal".into();
    };
    match defs.get(id) {
        Some(def) if !def.name.trim().is_empty() => def.name.trim().into(),
        _ => id.into(),
    }
}

impl DocFacts {
    /// The index of the author with this name, adding it if it is new. A missing name is `Unknown`.
    pub(crate) fn author(&mut self, name: Option<&str>) -> u32 {
        let name = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or("Unknown");
        if let Some(&i) = self.by_name.get(name) {
            return i;
        }
        let index = u32::try_from(self.authors.len()).unwrap_or(u32::MAX);
        let code = self.code_for(name);
        self.authors.push(Author {
            code: code.into(),
            name: name.into(),
        });
        self.changes.push(0);
        self.by_name.insert(name.into(), index);
        index
    }

    /// The index of an author already seen, by exact name.
    pub(crate) fn author_of(&self, name: Option<&str>) -> Option<u32> {
        let name = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or("Unknown");
        self.by_name.get(name).copied()
    }

    /// A short code: the capital of each word of the name, up to three, in ASCII, and a digit to tell two
    /// authors with the same code apart.
    fn code_for(&self, name: &str) -> String {
        let mut base: String = name
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .filter_map(|w| w.chars().next())
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_uppercase())
            .take(3)
            .collect();
        if base.is_empty() {
            base.push('X');
        }
        let taken = |code: &str| self.authors.iter().any(|a| &*a.code == code);
        if !taken(&base) {
            return base;
        }
        (2u32..)
            .map(|n| format!("{base}{n}"))
            .find(|c| !taken(c))
            .unwrap_or(base)
    }

    /// The ordinal of a note by kind and `w:id`, as the agent sees it (`{fn3}`).
    pub(crate) fn ordinal(&self, kind: NoteKind, id: i64) -> Option<u32> {
        self.ordinals.get(&(kind, id)).copied()
    }

    /// The `w:id` of the note with this ordinal, if the body references one.
    pub(crate) fn note_id(&self, kind: NoteKind, ordinal: u32) -> Option<i64> {
        self.ordinals
            .iter()
            .find(|((k, _), n)| *k == kind && **n == ordinal)
            .map(|((_, id), _)| *id)
    }

    /// How many notes of this kind the body references.
    pub(crate) fn referenced_count(&self, kind: NoteKind) -> u32 {
        self.referenced.get(kind_slot(kind)).copied().unwrap_or(0)
    }

    /// The numbering of a mode: every reference in `Redline`, the ones shown in `Final` and in `Original`.
    fn numbers(&self, mode: Mode) -> Option<&NoteNumbers> {
        match mode {
            Mode::Redline => None,
            Mode::Final => Some(&self.final_numbers),
            Mode::Original => Some(&self.original_numbers),
        }
    }

    /// The number Word shows as a note's mark in the view of `mode`, once that mode's changes are resolved. The note's id in
    /// refs and operations is `ordinal`, the same in every view.
    pub(crate) fn ordinal_in(&self, mode: Mode, kind: NoteKind, id: i64) -> Option<u32> {
        match self.numbers(mode) {
            Some(n) => n.ordinals.get(&(kind, id)).copied(),
            None => self.ordinal(kind, id),
        }
    }

    /// Whether a note's every reference is gone in the view of `mode` (a pending deletion in `Final`, a pending insertion
    /// in `Original`), so that resolving takes it out with them.
    pub(crate) fn gone_in(&self, mode: Mode, kind: NoteKind, id: i64) -> bool {
        self.ordinal(kind, id).is_some() && self.ordinal_in(mode, kind, id).is_none()
    }
}

fn kind_slot(kind: NoteKind) -> usize {
    match kind {
        NoteKind::Footnote => 0,
        NoteKind::Endnote => 1,
    }
}

/// True for a kind of revision element that is a change in the count of the summary line: not the end marker of
/// a custom XML range, and not the source of a move, which is counted at its destination.
fn counts_as_change(kind: RevKind) -> bool {
    !matches!(
        kind,
        RevKind::CustomXmlInsEnd
            | RevKind::CustomXmlDelEnd
            | RevKind::CustomXmlMoveFromEnd
            | RevKind::CustomXmlMoveToEnd
            | RevKind::MoveFrom
            | RevKind::MarkMoveFrom
    )
}

impl PartsView<'_> {
    /// The facts of the document, from every story part, cached until one changes.
    pub(crate) fn doc_facts(&self) -> Result<Arc<DocFacts>, StetError> {
        let list = self.story_specs()?;
        let mut deps: Vec<Dep> = self.cascade_deps()?;
        deps.push(Dep::Package);
        for spec in &list.0 {
            if let Some(id) = self.id(&spec.part) {
                deps.push(Dep::Part(id));
            }
        }
        let build = |_: Option<&DocFacts>| -> Result<DocFacts, StetError> { self.build_facts() };
        self.derived(1, &deps, &build)
    }

    fn build_facts(&self) -> Result<DocFacts, StetError> {
        let list = self.story_specs()?;
        let defs = self.style_defs()?;
        let cascade = self.cascade()?;
        let mut facts = DocFacts::default();
        // name -> (count, first seen)
        let mut styles: HashMap<Box<str>, (u32, u32)> = HashMap::new();
        let mut seen_styles = 0u32;
        for spec in &list.0 {
            let Some(part) = self.id(&spec.part) else {
                continue;
            };
            let story = self.story(part)?;
            let tree = self.tree(part)?;
            for rev in story.revisions(tree) {
                if rev.branch == Branch::Untaken || !counts_as_change(rev.kind) {
                    continue;
                }
                let a = facts.author(rev.author.as_deref());
                if let Some(n) = facts.changes.get_mut(a as usize) {
                    *n += 1;
                }
            }
            for unit in story.units() {
                if spec.kind == StoryKind::Comments {
                    facts.author(unit.author.as_deref());
                }
                match unit.kind {
                    UnitKind::Footnote => facts.counts.footnotes += 1,
                    UnitKind::Endnote => facts.counts.endnotes += 1,
                    UnitKind::Comment => facts.counts.comments += 1,
                }
            }
            for entry in story.blocks() {
                if entry.kind != crate::wordml::BlockKind::Paragraph {
                    continue;
                }
                let Some(p) = Paragraph::new(tree, entry.node) else {
                    continue;
                };
                let name = style_name(&defs, &cascade, p.style_id().as_deref());
                let slot = styles.entry(name).or_insert_with(|| {
                    seen_styles += 1;
                    (0, seen_styles)
                });
                slot.0 += 1;
            }
            if spec.kind == StoryKind::Body {
                let rels = self.rels(part)?;
                facts.counts.sections =
                    u32::try_from(story.sections(tree, &rels).len()).unwrap_or(u32::MAX);
                let (gone_final, gone_original) = refs_gone(&story, tree);
                let mut all = NoteNumbers::default();
                for (kind, id, node) in note_refs(tree) {
                    all.count(kind, id);
                    if !gone_final.contains(&node) {
                        facts.final_numbers.count(kind, id);
                    }
                    if !gone_original.contains(&node) {
                        facts.original_numbers.count(kind, id);
                    }
                }
                facts.ordinals = all.ordinals;
                facts.referenced = all.referenced;
            }
        }
        facts.common_style = styles
            .into_iter()
            .max_by(|a, b| a.1.0.cmp(&b.1.0).then(b.1.1.cmp(&a.1.1)))
            .map(|(name, _)| name);
        if facts.counts.comments > 0 {
            self.anchor_comments(&mut facts)?;
        }
        Ok(facts)
    }

    /// Finds where each comment is anchored, in the first story that holds its range.
    fn anchor_comments(&self, facts: &mut DocFacts) -> Result<(), StetError> {
        let list = self.story_specs()?;
        for spec in list.0.iter().filter(|s| s.kind != StoryKind::Comments) {
            let Some(part) = self.id(&spec.part) else {
                continue;
            };
            let story = self.story(part)?;
            let tree = self.tree(part)?;
            for c in story.anchors(tree).comments {
                if facts.comment_at.contains_key(&c.id) {
                    continue;
                }
                let Some(start) = c.start.or(c.reference) else {
                    continue;
                };
                let text = start.paragraph.and_then(|p| {
                    anchored_text(tree, p, start.node, c.end.map(|e| (e.paragraph, e.node)))
                });
                facts.comment_at.insert(
                    c.id,
                    CommentAt {
                        part: spec.part.clone(),
                        node: start.paragraph,
                        text,
                    },
                );
            }
        }
        Ok(())
    }
}

/// The note references of a story that are gone in `Final` (in a pending deletion) and in `Original` (in a pending
/// insertion), by node. A reference the text model does not read (in a text box) is in neither.
fn refs_gone(
    story: &crate::wordml::Story,
    tree: &SpanTree,
) -> (
    std::collections::HashSet<NodeId>,
    std::collections::HashSet<NodeId>,
) {
    let mut gone = (
        std::collections::HashSet::new(),
        std::collections::HashSet::new(),
    );
    for entry in story.blocks() {
        if entry.kind != crate::wordml::BlockKind::Paragraph {
            continue;
        }
        let Some(p) = Paragraph::new(tree, entry.node) else {
            continue;
        };
        for item in p.map().items() {
            if item.kind != crate::wordml::ItemKind::NoteRef {
                continue;
            }
            if !item.vis.in_mode(crate::wordml::Mode::Final) {
                gone.0.insert(item.node);
            }
            if !item.vis.in_mode(crate::wordml::Mode::Original) {
                gone.1.insert(item.node);
            }
        }
    }
    gone
}

/// The most characters of anchored text that is kept.
const ANCHOR_CHARS: usize = 100;

/// The text between a comment's range marks, from the start mark to the end mark when both are in the same
/// paragraph and to the end of the paragraph otherwise: the text of `Final` if there is any, and else all
/// of it. Cut to a hundred characters.
fn anchored_text(
    tree: &SpanTree,
    paragraph: NodeId,
    start: NodeId,
    end: Option<(Option<NodeId>, NodeId)>,
) -> Option<Box<str>> {
    let p = Paragraph::new(tree, paragraph)?;
    let map = p.map();
    let items = map.items();
    let from = items.iter().position(|i| i.node == start)?;
    let to = match end {
        Some((Some(ep), node)) if ep == paragraph => items
            .iter()
            .position(|i| i.node == node)
            .unwrap_or(items.len()),
        _ => items.len(),
    };
    let redline = map.text(Mode::Redline);
    let texts = slices(&redline, items);
    let pick = |final_only: bool| -> String {
        (from..to.max(from))
            .filter_map(|k| Some((items.get(k)?, texts.get(k)?)))
            .filter(|(i, _)| is_textual(i.kind) && (!final_only || i.vis.0 & Vis::FINAL != 0))
            .map(|(_, t)| *t)
            .collect()
    };
    let mut text = pick(true);
    if text.trim().is_empty() {
        text = pick(false);
    }
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let cut: String = text.chars().take(ANCHOR_CHARS).collect();
    let more = text.chars().count() > ANCHOR_CHARS;
    Some(if more { format!("{cut}…") } else { cut }.into_boxed_str())
}
