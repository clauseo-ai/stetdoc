//! Defined terms, clauses, cross-references and the reference graph.
//!
//! **Owns.** Defined terms, clauses, cross-references and the graph that joins them, queryable in both
//! directions: where a term is defined and used, which block is "clause 4.2(a)", and what refers to this
//! clause. It is derived data, read-only, and never decides for the caller: when a reference or a term is
//! not unique, every candidate is returned.
//!
//! **Interface.** `PartsView::{doc_index, edges, bookmarks}`, `Index::{clauses, clause_of, resolve_clause,
//! definitions, terms}`, `Edges::{outbound, inbound}`, and `lookup`.
//!
//! **Invariants.**
//! - Derived from the parts of a state through `PartsView::derived`, keyed by `BlockKey` and never by ref, so
//!   an index over a staged transaction works for blocks that have no ref yet.
//! - Deterministic: stories in story order, blocks in document order, no iteration over a hash map.
//! - A typed label is flagged as typed and a composed full label as composed. A prose mention ends in one of
//!   five states, and `relative` and `outside` are never presented as `resolved`.
//! - The clauses are those of `Final` text and numbers: a clause number in a contract names the document as it
//!   will be.
//!
//! **Depends on.** `api`, `wordml`, `styles`, `numbering`, `state`.
//!
//! **Start reading.** `index/mod.rs`. Design: `docs/design/projection-and-index.md` section 7.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "`projection::view` and `document` are the callers, and `document` is not built yet"
    )
)]

mod clauses;
mod edges;
mod labels;
mod lookup;
mod mentions;
mod terms;

#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod tests_clauses;
#[cfg(test)]
mod tests_corpus;
#[cfg(test)]
mod tests_lookup;
#[cfg(test)]
mod tests_mentions;
#[cfg(test)]
mod tests_terms;

pub(crate) use clauses::{Clause, Match as ClauseMatch, Reason, Source};
pub(crate) use edges::{Edge, EdgeKind, Edges, State, target_name};
pub(crate) use lookup::lookup;
pub(crate) use lookup::near;
pub(crate) use terms::Definition;

use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::numbering::ItemKind;
use crate::state::{Dep, PartId, PartsView};
use crate::wordml::{BlockKey, BlockKind, SpanTree, Story, StoryKind};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

/// What one story holds that the index is built from.
#[derive(Debug)]
pub(crate) struct StoryFacts {
    /// The story's part name.
    pub part: Arc<str>,
    /// The story's clauses, in order.
    pub clauses: Vec<Clause>,
    /// The terms it defines, in order.
    pub defs: Vec<Definition>,
    /// The prose mentions of the story, by entry, for the paragraphs that have any.
    pub mentions: Vec<(usize, Vec<mentions::Raw>)>,
    /// The internal links of the story, by entry: the bookmark each names.
    pub links: Vec<(usize, String)>,
    /// The entries that are lines of a listing, which make no clause and no edge.
    pub listing: HashSet<usize>,
    /// The characters of the text of each block in `Final`, `Original` and `Redline`, by entry: zero for a block
    /// that is not a paragraph. The one pass over the text of the story reads them, and the sizes of a page are
    /// made from them, so that a view with notes reads the text of a long story once and not twice.
    pub lens: Vec<[u32; 3]>,
}

/// The clauses and the defined terms of a document.
#[derive(Debug, Default)]
pub(crate) struct Index {
    clauses: Vec<Clause>,
    ranges: HashMap<Arc<str>, Range<usize>>,
    by_key: HashMap<BlockKey, usize>,
    by_full: HashMap<String, Vec<usize>>,
    defs: BTreeMap<String, Vec<Definition>>,
}

/// The text of a paragraph for the index: without the characters that stand for atoms, and with runs of white
/// space as one space. Text that is that already, or that has only spaces at its ends, is borrowed, which most
/// paragraphs are.
pub(super) fn clean(text: &str) -> Cow<'_, str> {
    if let Some(same) = trimmed_if_clean(text) {
        return Cow::Borrowed(same);
    }
    Cow::Owned(
        text.chars()
            .filter(|c| !matches!(c, '\u{1}'..='\u{6}' | '\u{c}' | '\u{e}'))
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// The text without the spaces at its ends, if that is all `clean` would change: no atom character, and no white
/// space inside but single spaces between words. Bytes are read while the text is ASCII, which almost all of it is.
fn trimmed_if_clean(text: &str) -> Option<&str> {
    let t = text.trim_matches(' ');
    let mut after_space = false;
    for &b in t.as_bytes() {
        match b {
            b' ' => {
                if after_space {
                    return None;
                }
                after_space = true;
            }
            1..=6 | 9..=14 => return None,
            0x80.. => return trimmed_if_clean_unicode(t),
            _ => after_space = false,
        }
    }
    Some(t)
}

/// `trimmed_if_clean` for text with a character outside ASCII, by characters.
fn trimmed_if_clean_unicode(t: &str) -> Option<&str> {
    let mut after_space = false;
    for c in t.chars() {
        if matches!(c, '\u{1}'..='\u{6}' | '\u{c}' | '\u{e}') {
            return None;
        }
        if c.is_whitespace() {
            if c != ' ' || after_space {
                return None;
            }
            after_space = true;
        } else {
            after_space = false;
        }
    }
    Some(t)
}

/// The bookmarks that the hyperlinks of a paragraph name, in order, each hyperlink once.
fn links_of(
    p: &crate::wordml::Paragraph<'_>,
    map: &crate::wordml::TextMap,
    entry: usize,
    out: &mut Vec<(usize, String)>,
) {
    let mut last = u32::MAX;
    let mut seen: Vec<crate::wordml::NodeId> = Vec::new();
    for item in map.items() {
        if item.ctx == last {
            continue;
        }
        last = item.ctx;
        for ctx in map.context(item.ctx) {
            if ctx.kind == crate::wordml::WrapperKind::Hyperlink
                && !seen.contains(&ctx.node)
                && let Some(anchor) = p.hyperlink_of(ctx).and_then(|h| h.anchor)
            {
                seen.push(ctx.node);
                out.push((entry, anchor));
            }
        }
    }
}

/// The entries of a story that are lines of a table of contents, of figures, of authorities or of an index: the
/// paragraphs a `TOC`, `TOF`, `TOA` or `INDEX` field spans. Their text repeats the headings they list, so each
/// would be a second clause with the heading's number, and a mention of it.
pub(crate) fn listing_entries(story: &Story, tree: &SpanTree) -> HashSet<usize> {
    let mut out = HashSet::new();
    for f in story.fields(tree) {
        if !matches!(&*f.keyword, "TOC" | "TOF" | "TOA" | "INDEX") {
            continue;
        }
        let Some(from) = story.index_of(f.paragraph_begin) else {
            continue;
        };
        let to = f
            .paragraph_end
            .and_then(|n| story.index_of(n))
            .unwrap_or(from);
        out.extend(from..=to.max(from));
    }
    out
}

impl PartsView<'_> {
    /// The facts of one story, from one pass over its paragraphs: its clauses, its definitions, the prose
    /// mentions and the internal links, cached until the story or its numbering change. The edges are
    /// resolved from the mentions and the links, so the text is read once.
    pub(crate) fn story_facts(&self, part: PartId) -> Result<Arc<StoryFacts>, StetError> {
        let mut deps = self.numbers_deps(part)?;
        deps.push(Dep::Part(part));
        let build = |_: Option<&StoryFacts>| -> Result<StoryFacts, StetError> {
            let story = self.story(part)?;
            let tree = self.tree(part)?;
            let numbers = self.numbers(part, Mode::Final)?;
            let mut builder = clauses::Builder::default();
            let mut defs = Vec::new();
            let mut found_mentions = Vec::new();
            let mut links = Vec::new();
            let mut lens = vec![[0u32; 3]; story.blocks().len()];
            let listing = listing_entries(&story, tree);
            for (i, entry) in story.blocks().iter().enumerate() {
                if entry.kind != BlockKind::Paragraph {
                    continue;
                }
                let Some(p) = story.paragraph(tree, i) else {
                    continue;
                };
                let map = p.map();
                if let Some(slot) = lens.get_mut(i) {
                    let len = |mode| u32::try_from(map.len(mode)).unwrap_or(u32::MAX);
                    *slot = [len(Mode::Final), len(Mode::Original), len(Mode::Redline)];
                }
                if listing.contains(&i) {
                    continue;
                }
                links_of(&p, &map, i, &mut links);
                let raw = map.text(Mode::Final);
                let text = clean(&raw);
                if text.is_empty() {
                    continue;
                }
                let at = clauses::Labelled {
                    story: &story,
                    entry: i,
                    text: &text,
                };
                match numbers.item(entry.node) {
                    Some(item) if matches!(item.kind, ItemKind::Counted { .. }) => {
                        builder.computed(&at, &item.kind, item.list.0, item.level, &item.full);
                    }
                    _ => builder.typed(&at),
                }
                for (term, kind) in terms::definitions(&text) {
                    defs.push(Definition {
                        term,
                        block: story.key(entry),
                        story: story.kind(),
                        kind,
                        snippet: text.chars().take(80).collect(),
                    });
                }
                let raws = mentions::find(&text);
                if !raws.is_empty() {
                    found_mentions.push((i, raws));
                }
            }
            Ok(StoryFacts {
                part: story.part().clone(),
                clauses: builder.finish(),
                defs,
                mentions: found_mentions,
                links,
                listing,
                lens,
            })
        };
        self.derived(u64::from(part.number()), &deps, &build)
    }

    /// The index of the document, cached until a story or its numbering changes. Comments are not indexed.
    pub(crate) fn doc_index(&self) -> Result<Arc<Index>, StetError> {
        let list = self.story_specs()?;
        let mut deps: Vec<Dep> = vec![Dep::Package];
        let mut parts = Vec::new();
        for spec in list.0.iter().filter(|s| s.kind != StoryKind::Comments) {
            if let Some(part) = self.id(&spec.part) {
                deps.push(Dep::Part(part));
                deps.extend(self.numbers_deps(part)?);
                parts.push(part);
            }
        }
        let build = |_: Option<&Index>| -> Result<Index, StetError> {
            let mut index = Index::default();
            for part in &parts {
                let facts = self.story_facts(*part)?;
                let start = index.clauses.len();
                for clause in &facts.clauses {
                    let at = index.clauses.len();
                    index.by_key.insert(clause.block.clone(), at);
                    for key in std::iter::once(&clause.full).chain(clause.alias.as_ref()) {
                        index
                            .by_full
                            .entry(labels::normalize(key))
                            .or_default()
                            .push(at);
                    }
                    index.clauses.push(clause.clone());
                }
                index
                    .ranges
                    .insert(facts.part.clone(), start..index.clauses.len());
                for def in &facts.defs {
                    index
                        .defs
                        .entry(def.term.clone())
                        .or_default()
                        .push(def.clone());
                }
            }
            Ok(index)
        };
        self.derived(3, &deps, &build)
    }
}

impl Index {
    /// Every clause, in story order and document order.
    pub(crate) fn clauses(&self) -> &[Clause] {
        &self.clauses
    }

    /// The clause at a position of the table.
    pub(crate) fn clause(&self, at: usize) -> Option<&Clause> {
        self.clauses.get(at)
    }

    /// The clause that a block is, if it is one.
    pub(crate) fn clause_of(&self, block: &BlockKey) -> Option<&Clause> {
        self.by_key.get(block).and_then(|i| self.clauses.get(*i))
    }

    /// The position in the table of the clause that a block is.
    pub(crate) fn position_of(&self, block: &BlockKey) -> Option<usize> {
        self.by_key.get(block).copied()
    }

    /// The positions of the clauses whose full label is `path`, in order.
    pub(crate) fn by_full(&self, path: &str) -> Vec<usize> {
        self.by_full
            .get(&labels::normalize(path))
            .cloned()
            .unwrap_or_default()
    }

    /// The positions of the clauses that a mention of a noun and a reference can mean. A clause that has that
    /// noun is the answer when there is one. Otherwise the clauses with no noun (Word numbered them) and the ones
    /// with a noun of the same family (`Section` for `clause`). A clause with another noun is never one:
    /// `clause 1` does not mean `Schedule 1`.
    pub(crate) fn matches_of(&self, noun: labels::Noun, reference: &str) -> Vec<usize> {
        let mut same = Vec::new();
        let mut loose = Vec::new();
        let mut found = self.by_full(reference);
        if found.is_empty() && labels::is_bare_numeral(reference) {
            // `Recital 61` is a paragraph that Word numbers `(61)`.
            found = self.by_full(&format!("({reference})"));
        }
        for at in found {
            let Some(clause) = self.clause(at) else {
                continue;
            };
            match clause.noun.and_then(labels::Noun::of) {
                Some(n) if n == noun => same.push(at),
                Some(n) if n.same_family(noun) => loose.push(at),
                Some(_) => {}
                None => loose.push(at),
            }
        }
        if same.is_empty() { loose } else { same }
    }

    /// Every clause that a query names, with why: `clause 4.2(a)`, `Article III`, `4.2`.
    pub(crate) fn resolve_clause(&self, text: &str) -> Vec<ClauseMatch> {
        let Some(query) = clauses::parse_query(text) else {
            return Vec::new();
        };
        let all: Vec<&Clause> = self.clauses.iter().collect();
        clauses::resolve(&all, &query)
    }

    /// How far the clause at `at` is from an entry of a story, in blocks, and infinitely far when it is in another
    /// story. A mention that fits several clauses lists the nearest first.
    pub(crate) fn distance(&self, at: usize, part: &Arc<str>, entry: usize) -> usize {
        match self.clause(at) {
            Some(c) if c.block.part() == part => c.entry.abs_diff(entry),
            _ => usize::MAX,
        }
    }

    /// The clauses that a query names, as a view of one clause needs them: those that have the query's noun when
    /// any does (`Schedule 1` is not `clause 1`), and otherwise every clause with the path. `None` when the text
    /// is not a clause number at all.
    pub(crate) fn resolve_for_view(&self, text: &str) -> Option<Vec<usize>> {
        clauses::parse_query(text)?;
        let found = self.resolve_clause(text);
        let named: Vec<usize> = found
            .iter()
            .filter(|m| m.reason == Reason::NounAndLabel)
            .map(|m| m.at)
            .collect();
        Some(if named.is_empty() {
            found.iter().map(|m| m.at).collect()
        } else {
            named
        })
    }

    /// The clause that is in force at an entry of a story: the last one at or before it.
    pub(crate) fn context_at(&self, part: &Arc<str>, entry: usize) -> Option<usize> {
        let range = self.ranges.get(part)?;
        let slice = self.clauses.get(range.clone())?;
        let after = slice.partition_point(|c| c.entry <= entry);
        after.checked_sub(1).map(|i| range.start + i)
    }

    /// The clauses that the one at `at` sits inside, nearest first.
    pub(crate) fn ancestors(&self, at: usize) -> Vec<usize> {
        let Some(me) = self.clause(at) else {
            return Vec::new();
        };
        let Some(range) = self.ranges.get(me.block.part()) else {
            return Vec::new();
        };
        let mut depth = me.depth;
        let mut out = Vec::new();
        for i in (range.start..at).rev() {
            let Some(c) = self.clause(i) else { continue };
            if c.depth < depth {
                out.push(i);
                depth = c.depth;
            }
        }
        out
    }

    /// The entry of the story where the blocks under the clause at `at` stop: the next clause of the same
    /// or a shallower depth, or the end of the story.
    pub(crate) fn ends_at(&self, at: usize) -> Option<usize> {
        let me = self.clause(at)?;
        let range = self.ranges.get(me.block.part())?;
        (at + 1..range.end)
            .filter_map(|i| self.clause(i))
            .find(|c| c.depth <= me.depth)
            .map(|c| c.entry)
    }

    /// Where a term is defined, in order.
    pub(crate) fn definitions(&self, term: &str) -> &[Definition] {
        self.defs.get(term).map_or(&[], Vec::as_slice)
    }

    /// Every defined term, in order.
    pub(crate) fn terms(&self) -> impl Iterator<Item = &str> {
        self.defs.keys().map(String::as_str)
    }
}
