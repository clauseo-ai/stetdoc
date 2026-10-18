//! `lookup`: a query classified by every rule that applies, and every hit returned, never one chosen.
//!
//! A ref is a block, a noun and a path or a bare path is a clause, a quoted string is a term, and anything else is
//! a term, exactly first and without regard to case if nothing matches, which the hit says. "Schedule 1" may be
//! a clause and a term, and the caller gets both. The answer is plain data keyed by refs as text, at one state
//! (design 7.5).

use super::clauses::parse_query;
use super::edges::Edge;
use super::terms::{DefKind, uses};
use super::{Clause, Index, Source, clean};
use crate::api::error::{ErrorCode, StetError};
use crate::api::lookup::{Hit, Lookup, Reference, Site};
use crate::api::view::Mode;
use crate::state::PartsView;
use crate::wordml::{BlockKey, BlockKind, StoryKind};

/// The most uses a hit lists. The count is always exact.
const MOST_USES: usize = 50;

fn story_name(kind: StoryKind) -> &'static str {
    match kind {
        StoryKind::Body => "body",
        StoryKind::Header | StoryKind::Footer => "headers",
        StoryKind::Footnotes | StoryKind::Endnotes => "notes",
        StoryKind::Comments => "comments",
    }
}

/// The ref of a block as text, or `?` for a block that has none yet.
fn ref_text(parts: &PartsView<'_>, key: &BlockKey) -> String {
    parts
        .ref_of(key)
        .map_or_else(|| "?".to_string(), |r| r.to_string())
}

/// The plain text of the paragraph a key names, in `Final`.
fn text_of(parts: &PartsView<'_>, key: &BlockKey) -> Option<(StoryKind, String)> {
    let part = parts.id(key.part())?;
    let story = parts.story(part).ok()?;
    let tree = parts.tree(part).ok()?;
    let entry = story.index_of(key.node())?;
    let p = story.paragraph(tree, entry)?;
    Some((story.kind(), clean(&p.map().text(Mode::Final)).into_owned()))
}

fn reference(parts: &PartsView<'_>, edge: &Edge, to_this: Option<&BlockKey>) -> Reference {
    let to = match (&edge.to[..], to_this) {
        ([only], _) => Some(ref_text(parts, only)),
        // A relative reading's target is its first; the rest are the candidates it was chosen over.
        ([first, ..], _) if edge.state == super::State::Relative => Some(ref_text(parts, first)),
        (_, Some(this)) => Some(ref_text(parts, this)),
        _ => None,
    };
    let candidates = if edge.to.len() > 1 || edge.state == super::State::Outside {
        edge.to.iter().map(|k| ref_text(parts, k)).collect()
    } else {
        Vec::new()
    };
    Reference {
        from: ref_text(parts, &edge.from),
        to: if edge.state == super::State::Ambiguous {
            None
        } else {
            to
        },
        text: edge.text.clone(),
        state: edge.state.word().to_string(),
        candidates,
    }
}

fn clause_hit(parts: &PartsView<'_>, clause: &Clause, matched_by: &str) -> Hit {
    Hit::Clause {
        block: ref_text(parts, &clause.block),
        story: story_name(clause.story).to_string(),
        label: clause.label.clone(),
        full: clause.full.clone(),
        source: match clause.source {
            Source::Computed => "computed",
            Source::Typed => "typed",
        }
        .to_string(),
        composed: clause.composed,
        noun: clause.noun.map(str::to_string),
        text: clause.start.clone(),
        matched_by: matched_by.to_string(),
    }
}

fn block_hit(parts: &PartsView<'_>, index: &Index, key: &BlockKey) -> Result<Hit, StetError> {
    let edges = parts.edges()?;
    let clause = index.clause_of(key);
    let (story, text) = text_of(parts, key).unwrap_or((StoryKind::Body, String::new()));
    let defines: Vec<String> = index
        .terms()
        .filter(|t| index.definitions(t).iter().any(|d| &d.block == key))
        .map(str::to_string)
        .collect();
    Ok(Hit::Block {
        block: ref_text(parts, key),
        story: story_name(story).to_string(),
        label: clause.map(|c| c.label.clone()),
        full: clause.map(|c| c.full.clone()),
        text,
        inbound: edges
            .inbound(key)
            .into_iter()
            .map(|e| reference(parts, e, Some(key)))
            .collect(),
        outbound: edges
            .outbound(key)
            .into_iter()
            .map(|e| reference(parts, e, None))
            .collect(),
        defines,
    })
}

/// Every place a term is used, in document order, with the exact count.
fn uses_of(parts: &PartsView<'_>, term: &str) -> Result<(Vec<Site>, u32), StetError> {
    let list = parts.story_specs()?;
    let mut sites = Vec::new();
    let mut count = 0u32;
    for spec in list.0.iter().filter(|s| s.kind != StoryKind::Comments) {
        let Some(part) = parts.id(&spec.part) else {
            continue;
        };
        let story = parts.story(part)?;
        let tree = parts.tree(part)?;
        for (i, entry) in story.blocks().iter().enumerate() {
            if entry.kind != BlockKind::Paragraph {
                continue;
            }
            let Some(p) = story.paragraph(tree, i) else {
                continue;
            };
            let map = p.map();
            let raw = map.text(Mode::Final);
            if !raw.contains(term) {
                continue;
            }
            let text = clean(&raw);
            for at in uses(&text, term) {
                count = count.saturating_add(1);
                if sites.len() < MOST_USES {
                    sites.push(Site {
                        block: ref_text(parts, &story.key(entry)),
                        story: story_name(story.kind()).to_string(),
                        kind: "use".to_string(),
                        snippet: super::terms::snippet(&text, at, term.chars().count()),
                    });
                }
            }
        }
    }
    Ok((sites, count))
}

fn term_hit(
    parts: &PartsView<'_>,
    index: &Index,
    term: &str,
    matched: &str,
) -> Result<Hit, StetError> {
    let defs = index.definitions(term);
    let (found, count) = uses_of(parts, term)?;
    Ok(Hit::Term {
        term: term.to_string(),
        matched: matched.to_string(),
        definitions: defs
            .iter()
            .map(|d| Site {
                block: ref_text(parts, &d.block),
                story: story_name(d.story).to_string(),
                kind: match d.kind {
                    DefKind::Paragraph => "definition",
                    DefKind::Parenthetical => "parenthetical",
                }
                .to_string(),
                snippet: d.snippet.clone(),
            })
            .collect(),
        uses: found,
        use_count: count,
        multiple_definitions: defs.len() > 1,
    })
}

/// Up to five terms and clauses that start the way the query does, for the message of a miss.
pub(crate) fn near(index: &Index, query: &str) -> Vec<String> {
    let lower = query.to_lowercase();
    let norm = super::labels::normalize(query);
    let mut out: Vec<String> = index
        .terms()
        .filter(|t| t.to_lowercase().starts_with(&lower))
        .take(5)
        .map(|t| format!("the term {t:?}"))
        .collect();
    for c in index
        .clauses()
        .iter()
        .filter(|c| super::labels::normalize(&c.full).starts_with(&norm))
    {
        if out.len() >= 5 {
            break;
        }
        out.push(format!("clause {}", c.full));
    }
    out
}

/// The answer to a query at one state. `base` is the token of the state, as text. `block` is the block the
/// query names when it is a ref, which the caller resolves because the syntax of a ref is not this module's.
pub(crate) fn lookup(
    parts: PartsView<'_>,
    base: &str,
    query: &str,
    block: Option<BlockKey>,
) -> Result<Lookup, StetError> {
    let q = query.trim();
    if q.is_empty() {
        return Err(StetError::new(
            ErrorCode::InvalidPayload,
            "a lookup needs a term, a clause or a ref to look up.",
        )
        .with_next(
            "Give a quoted term, a clause such as `clause 4.2(a)`, or a ref such as `p42`.",
        ));
    }
    let index = parts.doc_index()?;
    let mut hits = Vec::new();
    // A ref is a block.
    if let Some(key) = block {
        hits.push(block_hit(&parts, &index, &key)?);
    }
    let quoted = q.len() >= 2 && q.starts_with(['"', '\u{201c}']) && q.ends_with(['"', '\u{201d}']);
    // A noun and a path, or a bare path, is a clause.
    if !quoted && parse_query(q).is_some() {
        for m in index.resolve_clause(q) {
            if let Some(c) = index.clause(m.at) {
                hits.push(clause_hit(&parts, c, m.reason.words()));
            }
        }
    }
    // Anything else, and a quoted string, is a term.
    let term = if quoted {
        q.get(
            q.chars().next().map_or(1, char::len_utf8)
                ..q.len() - q.chars().next_back().map_or(1, char::len_utf8),
        )
        .unwrap_or(q)
        .trim()
    } else {
        q
    };
    if !term.is_empty() {
        if !index.definitions(term).is_empty() {
            hits.push(term_hit(&parts, &index, term, "exact")?);
        } else {
            let lower = term.to_lowercase();
            let folded: Vec<String> = index
                .terms()
                .filter(|t| t.to_lowercase() == lower)
                .map(str::to_string)
                .collect();
            for t in folded {
                hits.push(term_hit(&parts, &index, &t, "case_insensitive")?);
            }
        }
    }
    if hits.is_empty() {
        let similar = near(&index, q);
        let hint = if similar.is_empty() {
            "Nothing in the document starts the same way.".to_string()
        } else {
            format!(
                "Terms and clauses that start the same way: {}.",
                similar.join(", ")
            )
        };
        return Err(StetError::new(
            ErrorCode::NotFound,
            format!("{q:?} is not a ref, a clause or a defined term of this document. {hint}"),
        )
        .with_next("Check the spelling, or find the text with a view of the document."));
    }
    // The fixed order: block, clause, term.
    let rank = |h: &Hit| match h {
        Hit::Block { .. } => 0,
        Hit::Clause { .. } => 1,
        Hit::Term { .. } => 2,
    };
    hits.sort_by_key(rank);
    Ok(Lookup {
        base: base.to_string(),
        query: query.to_string(),
        hits,
        // The text is the format's: `projection::render_lookup` writes it, and `Document::lookup` puts it here.
        text: String::new(),
    })
}
