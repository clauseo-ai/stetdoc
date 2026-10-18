//! Edges: what refers to what. A field, an internal link or a prose mention goes from a block to a clause, a
//! bookmarked block or nothing, and the edges of a document are kept in both directions (design 7.4).
//!
//! A field edge is exact: `REF`, `PAGEREF`, `NOTEREF` and `HYPERLINK \l` name a bookmark, and the bookmark's
//! paragraph is the target. So is an internal link, `w:anchor`. A prose mention is resolved against the clause
//! table and ends in one of five states, so that `relative` and `outside` are never presented as `resolved`.

use super::Index;
use super::mentions::Raw;
use crate::api::error::StetError;
use crate::state::{Dep, PartsView};
use crate::wordml::{BlockKey, StoryKind};
use std::collections::HashMap;
use std::sync::Arc;

/// How a mention or an edge ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    /// One clause.
    Resolved,
    /// Several, all listed.
    Ambiguous,
    /// A bare `(b)`, resolved against the clauses around the paragraph that cites it: the weakest.
    Relative,
    /// A citation of a law, such as `Section 501(c)(6) of the Code`.
    Outside,
    /// Nothing matches.
    Unresolved,
}

impl State {
    /// The state in the word the public types use.
    pub(crate) fn word(self) -> &'static str {
        match self {
            State::Resolved => "resolved",
            State::Ambiguous => "ambiguous",
            State::Relative => "relative",
            State::Outside => "outside",
            State::Unresolved => "unresolved",
        }
    }
}

/// Where an edge came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EdgeKind {
    /// A field that names a bookmark.
    Field,
    /// An internal hyperlink.
    Link,
    /// A mention in the text.
    Prose,
}

/// One reference: a block that cites something.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Edge {
    /// The citing block.
    pub from: BlockKey,
    /// Where it came from.
    pub kind: EdgeKind,
    /// The citation as written.
    pub text: String,
    /// How it ended.
    pub state: State,
    /// The blocks it may mean: one when resolved, all when ambiguous, the chosen one and then the candidates it
    /// was chosen over when relative, the in-document candidates when outside, none when unresolved.
    pub to: Vec<BlockKey>,
    /// Where the citing block is: its story's place in the story list, and its entry in the story. The order of
    /// the document.
    pub pos: (u32, usize),
}

/// The bookmarks of a document by name, each with the paragraph that holds its start.
#[derive(Debug, Default)]
pub(crate) struct Bookmarks {
    by_name: HashMap<String, BlockKey>,
}

impl Bookmarks {
    /// The paragraph where the bookmark starts.
    pub(crate) fn block(&self, name: &str) -> Option<&BlockKey> {
        self.by_name.get(name)
    }
}

/// The words of an instruction, with a quoted phrase as one word.
fn words(instr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in instr.chars() {
        match c {
            '"' | '\u{201c}' | '\u{201d}' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The bookmark that `REF`, `PAGEREF`, `NOTEREF` or `HYPERLINK \l` names.
pub(crate) fn target_name(instr: &str) -> Option<String> {
    let words = words(instr);
    let (keyword, rest) = words.split_first()?;
    match keyword.to_ascii_uppercase().as_str() {
        "REF" | "PAGEREF" | "NOTEREF" => rest.iter().find(|w| !w.starts_with('\\')).cloned(),
        "HYPERLINK" => {
            let at = rest.iter().position(|w| w.eq_ignore_ascii_case("\\l"))?;
            rest.get(at + 1).cloned()
        }
        _ => None,
    }
}

impl PartsView<'_> {
    /// The bookmarks of the document, cached until a story changes.
    pub(crate) fn bookmarks(&self) -> Result<Arc<Bookmarks>, StetError> {
        let list = self.story_specs()?;
        let mut deps: Vec<Dep> = vec![Dep::Package];
        deps.extend(
            list.0
                .iter()
                .filter_map(|s| self.id(&s.part))
                .map(Dep::Part),
        );
        let build = |_: Option<&Bookmarks>| -> Result<Bookmarks, StetError> {
            let mut out = Bookmarks::default();
            for spec in &list.0 {
                let Some(part) = self.id(&spec.part) else {
                    continue;
                };
                let story = self.story(part)?;
                let tree = self.tree(part)?;
                for b in story.anchors(tree).bookmarks {
                    let Some(node) = b.start.and_then(|s| s.paragraph) else {
                        continue;
                    };
                    if !b.name.is_empty() {
                        out.by_name
                            .entry(b.name)
                            .or_insert_with(|| BlockKey::new(story.part().clone(), node));
                    }
                }
            }
            Ok(out)
        };
        self.derived(1, &deps, &build)
    }
}

/// The edges of a document, in both directions.
#[derive(Debug, Default)]
pub(crate) struct Edges {
    all: Vec<Edge>,
    from: HashMap<BlockKey, Vec<usize>>,
    to: HashMap<BlockKey, Vec<usize>>,
}

/// An ambiguous mention with more candidates than this is listed with the block that cites it and not with each
/// of the candidates: a document that restarts its numbering in every article has 45 clauses called `2`, and a
/// mention of `paragraph 2` is no more about one of them than about another.
pub(crate) const WIDEST_INBOUND: usize = 8;

impl Edges {
    fn push(&mut self, edge: Edge) {
        let at = self.all.len();
        self.from.entry(edge.from.clone()).or_default().push(at);
        // A relative edge cites its chosen target, and the alternatives after it are not cited by it.
        let cited = if edge.state == State::Relative {
            edge.to.get(..1).unwrap_or_default()
        } else {
            edge.to.as_slice()
        };
        if cited.len() <= WIDEST_INBOUND {
            for t in cited {
                self.to.entry(t.clone()).or_default().push(at);
            }
        }
        self.all.push(edge);
    }

    /// What a block cites, in the order it cites it.
    pub(crate) fn outbound(&self, block: &BlockKey) -> Vec<&Edge> {
        self.from
            .get(block)
            .into_iter()
            .flatten()
            .filter_map(|i| self.all.get(*i))
            .collect()
    }

    /// What cites a block, in document order.
    pub(crate) fn inbound(&self, block: &BlockKey) -> Vec<&Edge> {
        let mut out: Vec<&Edge> = self
            .to
            .get(block)
            .into_iter()
            .flatten()
            .filter_map(|i| self.all.get(*i))
            .collect();
        out.sort_by_key(|e| e.pos);
        out
    }

    /// How many edges there are.
    pub(crate) fn len(&self) -> usize {
        self.all.len()
    }
}

/// What a mention came to against the clause table, or `None` when it is the paragraph naming itself (`this
/// Section 4`, inside Section 4), which is no reference to anything. `here` is the position of the citing
/// paragraph: its story, its entry in it, and the clause in force there.
pub(crate) fn resolve_mention(
    index: &Index,
    raw: &Raw,
    here: Here<'_>,
) -> Option<(State, Vec<usize>)> {
    let mut found = index.matches_of(raw.noun, &raw.reference);
    let before = found.len();
    if let Some(own) = here.own {
        found.retain(|c| *c != own);
        if before > 0 && found.is_empty() && !raw.outside {
            return None;
        }
    }
    // A bare `(b)` is read against the clauses around the citing paragraph first, nearest first: inside 4.2(c),
    // `(b)` is 4.2(b), and it is the weakest kind of resolution, so it says so.
    if raw.reference.starts_with('(')
        && !raw.outside
        && let Some(context) = here.clause
    {
        let mut chain = vec![context];
        chain.extend(index.ancestors(context));
        for a in chain {
            let Some(clause) = index.clause(a) else {
                continue;
            };
            let wanted = format!("{}{}", clause.full, raw.reference);
            let hits: Vec<usize> = index
                .by_full(&wanted)
                .into_iter()
                .filter(|h| Some(*h) != here.own)
                .collect();
            if let [only] = hits.as_slice() {
                return Some((State::Relative, chosen_first(index, *only, found, here)));
            }
        }
    }
    // In a document that starts over in every Article, `paragraph 2` is the one in the citing paragraph's own
    // Article, unless it is `paragraph 2 of Article 9`. Only the clauses of that Article are candidates then.
    if found.len() > 1
        && !raw.qualified
        && !raw.outside
        && let Some(division) = here.division
    {
        let narrow: Vec<usize> = found
            .iter()
            .copied()
            .filter(|c| {
                index
                    .clause(*c)
                    .is_some_and(|cl| cl.block.part() == here.part && cl.division == Some(division))
            })
            .collect();
        if let [only] = narrow.as_slice() {
            return Some((State::Relative, chosen_first(index, *only, found, here)));
        }
        if !narrow.is_empty() {
            found = narrow;
        }
    }
    // Nearest first, so that the first candidate of an ambiguous mention is the most likely one.
    found.sort_by_key(|at| index.distance(*at, here.part, here.entry));
    if raw.outside {
        return Some((State::Outside, found));
    }
    match found.len() {
        1 => return Some((State::Resolved, found)),
        n if n > 1 => return Some((State::Ambiguous, found)),
        _ => {}
    }
    if raw.statutory {
        return Some((State::Outside, Vec::new()));
    }
    Some((State::Unresolved, Vec::new()))
}

/// The targets of a relative reading: the chosen clause first, then the candidates it was chosen over, nearest
/// first, so that an agent sees the alternatives (design 7.4).
fn chosen_first(
    index: &Index,
    chosen: usize,
    mut others: Vec<usize>,
    here: Here<'_>,
) -> Vec<usize> {
    others.retain(|c| *c != chosen);
    others.sort_by_key(|at| index.distance(*at, here.part, here.entry));
    let mut out = Vec::with_capacity(others.len() + 1);
    out.push(chosen);
    out.extend(others);
    out
}

/// Where a citing paragraph is: its story, its entry in it, and the clause in force there.
#[derive(Clone, Copy)]
pub(crate) struct Here<'a> {
    pub part: &'a Arc<str>,
    pub entry: usize,
    /// The clause in force at the paragraph: the last one at or before it.
    pub clause: Option<usize>,
    /// The clause that the paragraph itself is, if it is one.
    pub own: Option<usize>,
    /// The entry of the division (`Article 6`) the paragraph is in.
    pub division: Option<usize>,
}

impl PartsView<'_> {
    /// The edges of the document: fields and links that name a bookmark, and the mentions in the text.
    pub(crate) fn edges(&self) -> Result<Arc<Edges>, StetError> {
        let list = self.story_specs()?;
        let mut deps: Vec<Dep> = vec![Dep::Package];
        for spec in &list.0 {
            if let Some(part) = self.id(&spec.part) {
                deps.push(Dep::Part(part));
                deps.extend(self.numbers_deps(part)?);
            }
        }
        let build = |_: Option<&Edges>| -> Result<Edges, StetError> { self.build_edges() };
        self.derived(2, &deps, &build)
    }

    fn build_edges(&self) -> Result<Edges, StetError> {
        let index = self.doc_index()?;
        let list = self.story_specs()?;
        // The facts of every story come from one pass over its text, so the edges read none. The bookmarks
        // cost a walk of the whole story, and are asked for only when a field or a link names one.
        let mut stories = Vec::new();
        let mut needs_marks = false;
        for (rank, spec) in list
            .0
            .iter()
            .enumerate()
            .filter(|(_, s)| s.kind != StoryKind::Comments)
        {
            let Some(part) = self.id(&spec.part) else {
                continue;
            };
            let story = self.story(part)?;
            let tree = self.tree(part)?;
            let facts = self.story_facts(part)?;
            needs_marks |= !facts.links.is_empty()
                || story
                    .fields(tree)
                    .iter()
                    .any(|f| target_name(&f.instr).is_some());
            stories.push((u32::try_from(rank).unwrap_or(u32::MAX), story, tree, facts));
        }
        let marks = if needs_marks {
            self.bookmarks()?
        } else {
            Arc::new(Bookmarks::default())
        };
        let mut out = Edges::default();
        for (rank, story, tree, facts) in &stories {
            let rank = *rank;
            // What the exact edges of each paragraph point at, so that a mention of the same place in words
            // (a REF field's result, `clause 4.2` beside it) is not counted twice.
            let mut exact: HashMap<crate::wordml::NodeId, Vec<BlockKey>> = HashMap::new();
            for f in story.fields(tree) {
                let Some(name) = target_name(&f.instr) else {
                    continue;
                };
                if story
                    .index_of(f.paragraph_begin)
                    .is_some_and(|i| facts.listing.contains(&i))
                {
                    continue;
                }
                let from = BlockKey::new(story.part().clone(), f.paragraph_begin);
                let to: Vec<BlockKey> = marks
                    .block(&name)
                    .filter(|t| **t != from)
                    .cloned()
                    .into_iter()
                    .collect();
                if let Some(t) = to.first() {
                    exact.entry(f.paragraph_begin).or_default().push(t.clone());
                }
                let pos = (rank, story.index_of(f.paragraph_begin).unwrap_or(0));
                out.push(Edge {
                    from,
                    kind: EdgeKind::Field,
                    text: f.instr.split_whitespace().collect::<Vec<_>>().join(" "),
                    state: if to.is_empty() {
                        State::Unresolved
                    } else {
                        State::Resolved
                    },
                    to,
                    pos,
                });
            }
            for (i, anchor) in &facts.links {
                let Some(entry) = story.blocks().get(*i) else {
                    continue;
                };
                let from = story.key(entry);
                let to: Vec<BlockKey> = marks
                    .block(anchor)
                    .filter(|t| **t != from)
                    .cloned()
                    .into_iter()
                    .collect();
                if let Some(t) = to.first() {
                    exact.entry(entry.node).or_default().push(t.clone());
                }
                out.push(Edge {
                    from,
                    kind: EdgeKind::Link,
                    text: format!("#{anchor}"),
                    state: if to.is_empty() {
                        State::Unresolved
                    } else {
                        State::Resolved
                    },
                    to,
                    pos: (rank, *i),
                });
            }
            for (i, raws) in &facts.mentions {
                let Some(entry) = story.blocks().get(*i) else {
                    continue;
                };
                let from = story.key(entry);
                let context = index.context_at(story.part(), *i);
                let here = Here {
                    part: story.part(),
                    entry: *i,
                    clause: context,
                    own: index.position_of(&from),
                    division: context
                        .and_then(|c| index.clause(c))
                        .and_then(super::Clause::division_entry),
                };
                // A heading that starts `Schedule 1.` names itself, and that is not a citation.
                let own_label = index
                    .clause_of(&from)
                    .is_some_and(|c| c.source == super::Source::Typed && c.noun.is_some());
                let already = exact.get(&entry.node);
                for raw in raws {
                    if own_label && raw.at == 0 {
                        continue;
                    }
                    let Some((state, found)) = resolve_mention(&index, raw, here) else {
                        continue;
                    };
                    let to: Vec<BlockKey> = found
                        .iter()
                        .filter_map(|c| index.clause(*c))
                        .map(|c| c.block.clone())
                        .collect();
                    let repeats = state == State::Resolved
                        && to
                            .first()
                            .is_some_and(|t| already.is_some_and(|a| a.contains(t)));
                    if repeats {
                        continue;
                    }
                    out.push(Edge {
                        from: from.clone(),
                        kind: EdgeKind::Prose,
                        text: raw.text.clone(),
                        state,
                        to,
                        pos: (rank, *i),
                    });
                }
            }
        }
        Ok(out)
    }
}
