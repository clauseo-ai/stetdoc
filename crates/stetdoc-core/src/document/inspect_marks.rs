//! The tracked changes and comments of one block, for `Document::inspect`: what is underneath its text.
//!
//! A view shows tracked changes as markers in braces and escapes a literal brace, but text such as `[DELETED: ...]`
//! that a document holds as plain text reads like a change. These lines say which changes and comments the block
//! really carries, by id, kind and author, or that it carries none.

use super::inspect::Lines;
use crate::api::error::StetError;
use crate::projection::{describe, family};
use crate::revisions::changes::changes;
use crate::state::PartsView;
use crate::threads::Threads;
use crate::wordml::{SpanTree, Story};
use std::ops::Range;

/// How many characters of a change's text a line shows.
const SHOWN: usize = 80;

/// The entries of a block and of everything inside it: a table's rows and their cells' blocks.
fn window(story: &Story, at: usize) -> Range<usize> {
    let blocks = story.blocks();
    let inside = |mut i: usize| -> bool {
        while let Some(parent) = blocks.get(i).and_then(|e| e.parent) {
            let parent = parent as usize;
            if parent == at {
                return true;
            }
            i = parent;
        }
        false
    };
    let mut end = at + 1;
    while end < blocks.len() && inside(end) {
        end += 1;
    }
    at..end
}

/// Text on one line, cut short.
fn short(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= SHOWN {
        return text;
    }
    let head: String = text.chars().take(SHOWN).collect();
    format!("{head}…")
}

/// The lines of the tracked changes and the comments in the block at entry `at` of `story`.
pub(super) fn marks(
    parts: PartsView<'_>,
    story: &Story,
    tree: &SpanTree,
    at: usize,
    out: &mut Lines,
) -> Result<(), StetError> {
    let window = window(story, at);
    let kind = story.kind();
    let found = changes(story, tree, Some(window.clone()));
    if found.is_empty() {
        out.add(
            "tracked changes",
            "none in this block: all of its text is the document's own, even text that looks like a revision",
            None,
        );
    }
    for change in &found {
        let info = describe(change, &parts, story, kind);
        let ids: Vec<String> = info.ids.iter().map(|id| format!("#{id}")).collect();
        let mut value = family(info.kind).to_string();
        value.push_str(&format!(
            " by {}",
            info.author.as_deref().unwrap_or("an unknown author")
        ));
        if let Some(date) = &info.date {
            value.push_str(&format!(" on {}", date.split('T').next().unwrap_or(date)));
        }
        if let Some(text) = info.deleted.as_deref() {
            value.push_str(&format!(", deletes {:?}", short(text)));
        }
        if let Some(text) = info.inserted.as_deref() {
            value.push_str(&format!(", inserts {:?}", short(text)));
        }
        out.add(format!("change {}", ids.join(",")), value, None);
    }
    let keys: Vec<_> = window
        .filter_map(|i| story.blocks().get(i).map(|e| story.key(e)))
        .collect();
    let threads = Threads::of(&parts)?;
    let mut any = false;
    for comment in threads.comments() {
        if !comment.anchor.blocks.iter().any(|k| keys.contains(k)) {
            continue;
        }
        any = true;
        let mut value = format!(
            "by {}",
            comment.author.as_deref().unwrap_or("an unknown author")
        );
        if let Some(parent) = comment.parent {
            value.push_str(&format!(", a reply to c{parent}"));
        }
        if threads.thread_of(comment.id).is_some_and(|t| t.resolved) {
            value.push_str(", resolved");
        }
        out.add(format!("comment c{}", comment.id), value, None);
    }
    if !any {
        out.add("comments", "none on this block", None);
    }
    Ok(())
}
