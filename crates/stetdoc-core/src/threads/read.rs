//! `Threads`: the comments of a document, with their threads and anchors.

use super::anchors::{self, Markers};
use super::chains;
use super::model::{Anchor, Comment, Counts, Thread};
use super::parts::CommentParts;
use super::rows::{self, Rows};
use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::state::{Dep, PartsView};
use crate::wordml::{NO_UNIT, StoryKind, UnitKind};
use std::collections::HashMap;
use std::sync::Arc;

/// The comments of one state of a document, read from `comments.xml`, its extension parts and the
/// range markers of every story.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Threads {
    comments: Vec<Comment>,
    threads: Vec<Thread>,
    /// The id to the first comment that has it.
    by_id: HashMap<i64, usize>,
    /// For each comment, the index of its thread in `threads`.
    thread_at: Vec<usize>,
    /// For each comment, the comments that name it as parent (after a loop is cut), in file order.
    children: Vec<Vec<usize>>,
    /// The ids that two comments have.
    duplicated: std::collections::HashSet<i64>,
    /// The member of each loop of parents that comes first in the file, which `audit` reports.
    pub(super) loops: Vec<usize>,
    /// The rows of the extension parts.
    pub(super) rows: Rows,
    /// Which parts exist.
    pub(super) parts: CommentParts,
}

impl Threads {
    /// The comments of the state `parts` views, cached under the generations of every part they are
    /// read from. A document with no comments part costs one relationship lookup.
    pub(crate) fn of(view: &PartsView<'_>) -> Result<Arc<Threads>, StetError> {
        let list = view.story_specs()?;
        let Some(main) = list
            .0
            .iter()
            .find(|s| s.kind == StoryKind::Body)
            .and_then(|s| view.id(&s.part))
        else {
            return Ok(Arc::new(Threads::default()));
        };
        let parts = CommentParts::find(view, main)?;
        let mut deps = parts.deps();
        deps.extend(
            list.0
                .iter()
                .filter_map(|s| view.id(&s.part))
                .map(Dep::Part),
        );
        let build = |_: Option<&Threads>| build(view, parts);
        view.derived(0, &deps, &build)
    }

    /// Every comment, in the order of `comments.xml`.
    pub(crate) fn comments(&self) -> &[Comment] {
        &self.comments
    }

    /// Every thread, in the order of their roots.
    pub(crate) fn threads(&self) -> &[Thread] {
        &self.threads
    }

    /// The comment with this `w:id`. If two have it, the first.
    pub(crate) fn comment(&self, id: i64) -> Option<&Comment> {
        self.by_id.get(&id).and_then(|i| self.comments.get(*i))
    }

    /// The thread that holds the comment with this `w:id`. If two comments have the id, the first's.
    pub(crate) fn thread_of(&self, id: i64) -> Option<&Thread> {
        let at = *self.by_id.get(&id)?;
        self.threads.get(*self.thread_at.get(at)?)
    }

    /// The comment parts of the document: which exist, and their part numbers.
    pub(crate) fn parts(&self) -> CommentParts {
        self.parts
    }

    /// True if two comments have this `w:id`. An operation that names such an id is ambiguous.
    pub(crate) fn is_ambiguous(&self, id: i64) -> bool {
        self.duplicated.contains(&id)
    }

    /// The comments below the comment with this id, nearest first: its replies, their replies, and so on.
    /// It follows the parents as the rows name them, by index, so it is exact even when ids repeat. If two
    /// comments have the id, it starts from the first.
    pub(crate) fn descendants_of(&self, id: i64) -> Vec<&Comment> {
        let Some(start) = self.by_id.get(&id).copied() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut frontier = vec![start];
        while let Some(at) = frontier.pop() {
            for &child in self.children.get(at).map_or(&[][..], |c| c.as_slice()) {
                if let Some(c) = self.comments.get(child) {
                    out.push(c);
                    frontier.push(child);
                }
            }
        }
        out
    }

    /// How many comments, threads, resolved threads and unanchored comments there are.
    pub(crate) fn counts(&self) -> Counts {
        Counts {
            comments: self.comments.len(),
            threads: self.threads.len(),
            resolved_threads: self.threads.iter().filter(|t| t.resolved).count(),
            unanchored: self
                .comments
                .iter()
                .filter(|c| c.anchor.is_unanchored())
                .count(),
        }
    }

    /// The text that the comment's range covers in `mode`, cut to `limit` characters, with paragraphs
    /// joined by a line feed. `None` for an unknown comment, one with no start and end in paragraphs.
    pub(crate) fn covered_text(
        &self,
        view: &PartsView<'_>,
        id: i64,
        mode: Mode,
        limit: usize,
    ) -> Result<Option<String>, StetError> {
        match self.comment(id) {
            Some(c) => anchors::covered_text(view, &c.anchor, mode, limit),
            None => Ok(None),
        }
    }
}

fn build(view: &PartsView<'_>, parts: CommentParts) -> Result<Threads, StetError> {
    let rows = Rows::read(view, &parts)?;
    let Some(part) = parts.comments else {
        return Ok(Threads {
            rows,
            parts,
            ..Threads::default()
        });
    };
    let story = view.story(part)?;
    let tree = view.tree(part)?;
    // The last paragraph of each unit, which keys the comment's rows.
    let mut last: Vec<Option<crate::wordml::NodeId>> = vec![None; story.units().len()];
    for entry in story.blocks() {
        if entry.kind == crate::wordml::BlockKind::Paragraph
            && entry.unit != NO_UNIT
            && let Some(slot) = last.get_mut(entry.unit as usize)
        {
            *slot = Some(entry.node);
        }
    }
    let ex_by_para: HashMap<u32, rows::ExRow> =
        first_by(rows.extended.iter().filter_map(|r| Some((r.para?, *r))));
    let durable_by_para: HashMap<u32, u32> =
        first_by(rows.ids.iter().filter_map(|r| Some((r.para?, r.durable?))));
    let mut comments = Vec::new();
    for (unit_index, unit) in story.units().iter().enumerate() {
        let (UnitKind::Comment, Some(id)) = (unit.kind, unit.id) else {
            continue;
        };
        let para_id = last
            .get(unit_index)
            .copied()
            .flatten()
            .and_then(|n| rows::para_id(&tree.node_ref(n)));
        let row = para_id.and_then(|p| ex_by_para.get(&p));
        comments.push(Comment {
            id,
            unit: unit_index,
            author: unit.author.as_deref().map(str::to_owned),
            initials: unit.initials.as_deref().map(str::to_owned),
            date: unit.date.as_deref().map(str::to_owned),
            para_id,
            parent: None,
            resolved: row.is_some_and(|r| r.done),
            durable_id: para_id.and_then(|p| durable_by_para.get(&p).copied()),
            anchor: Anchor::default(),
        });
    }
    // Parents, as the rows name them.
    let by_para: HashMap<u32, usize> = first_by(
        comments
            .iter()
            .enumerate()
            .filter_map(|(i, c)| Some((c.para_id?, i))),
    );
    let raw_parent: Vec<Option<usize>> = comments
        .iter()
        .map(|c| {
            let parent_para = c
                .para_id
                .and_then(|p| ex_by_para.get(&p))
                .and_then(|r| r.parent)?;
            by_para.get(&parent_para).copied()
        })
        .collect();
    let chains = chains::analyse(&raw_parent);
    // `parent` is an id, so it is given only when the id is unambiguous: a parent whose `w:id` two comments
    // share is dropped from `Comment::parent`, and a caller that climbs by id can never loop or be sent to
    // the wrong comment. Threads are built from indexes and are not affected (`Threads::thread_of`).
    let mut id_count: HashMap<i64, usize> = HashMap::new();
    for c in &comments {
        *id_count.entry(c.id).or_default() += 1;
    }
    for (i, p) in chains.parent.iter().enumerate() {
        let id = p
            .and_then(|p| comments.get(p))
            .map(|c| c.id)
            .filter(|id| id_count.get(id) == Some(&1));
        if let Some(c) = comments.get_mut(i) {
            c.parent = id;
        }
    }
    // Anchors, only if there is a comment to anchor.
    if !comments.is_empty() {
        let markers: Arc<Markers> = anchors::markers(view)?;
        let mut first: HashMap<i64, &anchors::Marker> = HashMap::new();
        for m in &markers.0 {
            first.entry(m.anchor.id).or_insert(m);
        }
        for c in &mut comments {
            if let Some(m) = first.get(&c.id) {
                c.anchor = anchors::anchor_of(view, m)?;
            }
        }
    }
    // Threads: a root and everything below it, in file order.
    let root_of = &chains.root_of;
    let roots: Vec<usize> = (0..comments.len())
        .filter(|i| root_of.get(*i) == Some(i))
        .collect();
    let mut replies: HashMap<usize, Vec<i64>> = HashMap::new();
    for (i, at) in root_of.iter().enumerate() {
        if *at != i
            && let Some(c) = comments.get(i)
        {
            replies.entry(*at).or_default().push(c.id);
        }
    }
    let threads: Vec<Thread> = roots
        .iter()
        .filter_map(|&r| {
            let root = comments.get(r)?;
            Some(Thread {
                root: root.id,
                replies: replies.remove(&r).unwrap_or_default(),
                resolved: root.resolved,
            })
        })
        .collect();
    let thread_index: HashMap<usize, usize> =
        roots.iter().enumerate().map(|(t, r)| (*r, t)).collect();
    let thread_at: Vec<usize> = root_of
        .iter()
        .map(|r| thread_index.get(r).copied().unwrap_or(0))
        .collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); comments.len()];
    for (i, p) in chains.parent.iter().enumerate() {
        if let Some(slot) = p.and_then(|p| children.get_mut(p)) {
            slot.push(i);
        }
    }
    let duplicated = id_count
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|(id, _)| *id)
        .collect();
    let mut by_id = HashMap::new();
    for (i, c) in comments.iter().enumerate() {
        by_id.entry(c.id).or_insert(i);
    }
    Ok(Threads {
        comments,
        threads,
        by_id,
        thread_at,
        children,
        duplicated,
        loops: chains.loops,
        rows,
        parts,
    })
}

/// A map that keeps the first value of each key.
fn first_by<K: std::hash::Hash + Eq, V>(items: impl Iterator<Item = (K, V)>) -> HashMap<K, V> {
    let mut map = HashMap::new();
    for (k, v) in items {
        map.entry(k).or_insert(v);
    }
    map
}
