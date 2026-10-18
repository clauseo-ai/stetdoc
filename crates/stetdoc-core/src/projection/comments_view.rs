//! The `Comments` scope: the comment threads of a document, one entry each (design 4.2 and 4.6).
//!
//! It is built on `threads`, which reads the comments, their threads and their anchors. A thread is its root
//! and every comment below it, as `Threads` groups them by index, so it is exact when two comments share an
//! id; nothing here climbs `Comment::parent`. Each entry has the root's anchor, every comment's author,
//! date and text, and whether the thread is resolved. The page is filled to the budget like an outline, and
//! the cursor is the root of the last thread shown, `c7`.

use super::appendix::text_of;
use super::build::{Selection, StorySel, project};
use super::formats::Format;
use super::model::{Body, Output, Projection, ThreadComment, ThreadEntry, ThreadList};
use super::page::{estimate, fit, greedy};
use crate::api::error::{ErrorCode, StetError};
use crate::api::view::{CommentFilter, Cursor, Position, View, ViewSpec};
use crate::refs::BaseToken;
use crate::state::PartsView;
use crate::threads::{Thread, Threads};
use std::collections::HashMap;
use std::sync::Arc;

/// How many characters of the text a comment is anchored to a thread shows.
const ANCHORED: usize = 100;

/// The tokens that the head of a page costs, about, for the first guess of how many threads fit.
const OVERHEAD: u32 = 80;

/// One page of the comment threads that `filter` keeps.
pub(super) fn comments_view(
    parts: PartsView<'_>,
    spec: &ViewSpec,
    filter: &CommentFilter,
    format: &dyn Format,
    base: BaseToken,
) -> Result<View, StetError> {
    let threads = Threads::of(&parts)?;
    let facts = parts.doc_facts()?;
    let author_matches = |name: Option<&str>, wanted: &str| -> bool {
        let Some(author) = facts
            .author_of(name)
            .and_then(|a| facts.authors.get(a as usize))
        else {
            return false;
        };
        *author.code == *wanted || author.name.eq_ignore_ascii_case(wanted.trim())
    };
    let kept: Vec<&Thread> = threads
        .threads()
        .iter()
        .filter(|t| filter.resolved.is_none_or(|r| r == t.resolved))
        .filter(|t| match &filter.author {
            Some(wanted) => threads
                .comment(t.root)
                .is_some_and(|c| author_matches(c.author.as_deref(), wanted)),
            None => true,
        })
        .collect();
    let start = match &spec.after {
        Some(cursor) => start_after(&threads, &kept, cursor)?,
        None => 0,
    };
    let texts = comment_texts(parts, base, spec, &threads, &kept)?;
    let mut entries: Vec<ThreadEntry> = Vec::with_capacity(kept.len());
    for thread in &kept {
        let mut comments = Vec::with_capacity(1 + thread.replies.len());
        for id in std::iter::once(thread.root).chain(thread.replies.iter().copied()) {
            let Some(c) = threads.comment(id) else {
                continue;
            };
            let anchored = threads
                .covered_text(&parts, id, spec.mode, ANCHORED)?
                .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|t| !t.is_empty());
            comments.push(ThreadComment {
                id,
                author: facts.author_of(c.author.as_deref()),
                date: c.date.as_deref().map(Arc::from),
                on: c.anchor.blocks.first().and_then(|k| parts.ref_of(k)),
                anchored: anchored.map(String::into_boxed_str),
                text: texts.get(&id).cloned().unwrap_or_default(),
            });
        }
        entries.push(ThreadEntry {
            root: thread.root,
            resolved: thread.resolved,
            comments,
        });
    }
    let resolved = u32::try_from(kept.iter().filter(|t| t.resolved).count()).unwrap_or(u32::MAX);
    let head = project(
        parts,
        base,
        spec.mode,
        Output::View {
            name: "threads".into(),
        },
        &Selection {
            stories: Vec::new(),
        },
    )?;
    let page = |n: usize| -> Page {
        page_of(
            format,
            &head,
            &entries,
            start,
            n,
            ListCounts {
                total: kept.len(),
                resolved,
            },
        )
    };
    let budget = spec.budget.0;
    let left = entries.len().saturating_sub(start);
    let guess = greedy(entries.iter().skip(start).map(weight), budget, OVERHEAD);
    let n = if left == 0 {
        0
    } else {
        fit(left, budget, guess, &mut |n| Ok(estimate(&page(n).text)))?.max(1)
    };
    let shown = page(n);
    let next = (start + n < entries.len())
        .then(|| shown.position.last.clone())
        .flatten()
        .map(Cursor::new);
    Ok(View {
        base: base.to_string(),
        mode: spec.mode,
        tokens: estimate(&shown.text),
        text: shown.text,
        position: shown.position,
        next,
        warnings: head.head.warnings.clone(),
    })
}

/// A rendered page and where it is.
struct Page {
    text: String,
    position: Position,
}

/// How many threads the filter keeps, and how many of them are resolved.
#[derive(Clone, Copy)]
struct ListCounts {
    total: usize,
    resolved: u32,
}

/// The weight of a thread for the first guess of how many fit: a quarter of its characters and a few tokens
/// for each line.
fn weight(entry: &ThreadEntry) -> u32 {
    let chars: usize = entry
        .comments
        .iter()
        .map(|c| c.text.chars().count() + c.anchored.as_deref().map_or(0, |a| a.chars().count()))
        .sum();
    let lines = u32::try_from(entry.comments.len() + 1).unwrap_or(u32::MAX);
    u32::try_from(chars.div_ceil(4))
        .unwrap_or(u32::MAX)
        .saturating_add(lines.saturating_mul(8))
}

/// The page that holds the threads `start..start + n`.
fn page_of(
    format: &dyn Format,
    head: &Projection,
    entries: &[ThreadEntry],
    start: usize,
    n: usize,
    counts: ListCounts,
) -> Page {
    let end = (start + n).min(entries.len());
    let shown = entries.get(start.min(end)..end).unwrap_or_default();
    let remaining = entries
        .iter()
        .skip(end)
        .map(weight)
        .fold(0u32, u32::saturating_add);
    let position = Position {
        scope: "threads".to_string(),
        first: shown.first().map(|t| format!("c{}", t.root)),
        last: shown.last().map(|t| format!("c{}", t.root)),
        shown: u32::try_from(shown.len()).unwrap_or(u32::MAX),
        total: u32::try_from(counts.total).unwrap_or(u32::MAX),
        remaining_tokens: remaining,
    };
    let mut projection = Projection {
        head: head.head.clone(),
        body: Body::Threads(ThreadList {
            total: u32::try_from(counts.total).unwrap_or(u32::MAX),
            resolved: counts.resolved,
            threads: shown.to_vec(),
        }),
        appendix: Vec::new(),
    };
    projection.head.position = Some(position.clone());
    let text = format
        .render(&projection)
        .files
        .into_iter()
        .next()
        .map(|(_, text)| text)
        .unwrap_or_default();
    Page { text, position }
}

/// The text of every comment of the threads kept, by `w:id`: its paragraphs without deletions, joined by
/// ` / `, as the appendix shows a comment.
fn comment_texts(
    parts: PartsView<'_>,
    base: BaseToken,
    spec: &ViewSpec,
    threads: &Threads,
    kept: &[&Thread],
) -> Result<HashMap<i64, Box<str>>, StetError> {
    let mut out = HashMap::new();
    let Some(part) = threads.parts().comments else {
        return Ok(out);
    };
    let ids: Vec<i64> = kept
        .iter()
        .flat_map(|t| std::iter::once(t.root).chain(t.replies.iter().copied()))
        .collect();
    if ids.is_empty() {
        return Ok(out);
    }
    let built = project(
        parts,
        base,
        spec.mode,
        Output::View {
            name: "threads".into(),
        },
        &Selection {
            stories: vec![(part, StorySel::Units(ids))],
        },
    )?;
    for story in built.stories() {
        for unit in &story.units {
            if let Some(id) = unit.id {
                out.entry(id)
                    .or_insert_with(|| text_of(&unit.blocks).into_boxed_str());
            }
        }
    }
    Ok(out)
}

/// Where a page that continues after `cursor` starts: after the thread whose root it names, or, for a
/// comment of a thread the filter does not keep, at the next thread kept. `c7` and `comment:7` are both read.
fn start_after(threads: &Threads, kept: &[&Thread], cursor: &Cursor) -> Result<usize, StetError> {
    let text = cursor.as_str();
    let id: i64 = text
        .strip_prefix("comment:")
        .or_else(|| text.strip_prefix('c'))
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| {
            StetError::new(
                ErrorCode::InvalidPayload,
                format!("{text:?} is not a cursor of the comment threads, which is a comment such as c7."),
            )
            .with_next("Use the cursor that the last page of the comment threads gave.")
        })?;
    let Some(thread) = threads.thread_of(id) else {
        return Err(StetError::new(
            ErrorCode::NotFound,
            format!("c{id} is not a comment of this document."),
        )
        .with_next("View the comment threads again from the start."));
    };
    if let Some(at) = kept.iter().position(|t| t.root == thread.root) {
        return Ok(at + 1);
    }
    let order = |root: i64| threads.threads().iter().position(|t| t.root == root);
    let here = order(thread.root);
    Ok(kept
        .iter()
        .position(|t| order(t.root) > here)
        .unwrap_or(kept.len()))
}
