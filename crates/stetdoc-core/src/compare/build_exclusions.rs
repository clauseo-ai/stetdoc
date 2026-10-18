//! The blocks the redline does not show as it should (design `compare.md`, 7.7 and 8.2): each is an `Exclusion` from the compare laws, and
//! the receipt names it. What it names is always a block of the redline, or nothing: an original block that could not be copied
//! is named by the block of the redline its copy would have stood next to, when the build knows it, and is otherwise described
//! by its first words. A key of the original is never named, because the redline's refs read another tree.

use super::build::Builder;
use super::receipt::{Exclusion, Site};
use crate::state::State;
use crate::wordml::{BlockKey, NodeId};
use crate::xml::SpanTree;
use std::sync::Arc;

/// How many words of an original block the receipt quotes.
const WORDS: usize = 8;

impl Builder<'_, '_, '_> {
    /// Notes blocks that are not written as they should be: `redline` are blocks of the redline, `original` blocks of the
    /// original, and the compare laws leave both out.
    pub(super) fn exclude(
        &mut self,
        redline: Vec<BlockKey>,
        original: Vec<BlockKey>,
        kind: &'static str,
        reason: impl Into<String>,
    ) {
        self.exclude_all(redline, Vec::new(), original, kind, reason);
    }

    /// As `exclude`, with the copies of the original that were placed for the blocks. The receipt names the first block of
    /// the redline, or describes the original's first block when there is none.
    pub(super) fn exclude_all(
        &mut self,
        redline: Vec<BlockKey>,
        copies: Vec<BlockKey>,
        original: Vec<BlockKey>,
        kind: &'static str,
        reason: impl Into<String>,
    ) {
        let site = match redline.first().or(copies.first()) {
            Some(block) => Site::Block(block.clone()),
            None if original.is_empty() => return,
            None => Site::Original,
        };
        self.push_exclusion(site, redline, copies, original, kind, reason.into());
    }

    /// A paragraph that an exclusion names and that took a container's inserted end mark is joined, by a rejection, into the
    /// paragraph that ends the container: the exclusion names that one too.
    pub(super) fn follow_end_marks(&mut self, part: &Arc<str>, given: &[(NodeId, NodeId)]) {
        for (live, end) in given {
            let live = BlockKey::new(part.clone(), *live);
            let end = BlockKey::new(part.clone(), *end);
            for e in &mut self.rc.exclusions {
                if (e.copies.contains(&live) || e.redline.contains(&live))
                    && !e.redline.contains(&end)
                {
                    e.redline.push(end.clone());
                }
            }
        }
    }

    /// Notes original blocks that could not be copied, whose copies would have stood next to `anchor`, a block of the redline.
    pub(super) fn exclude_beside(
        &mut self,
        anchor: BlockKey,
        original: Vec<BlockKey>,
        kind: &'static str,
        reason: impl Into<String>,
    ) {
        self.push_exclusion(
            Site::Beside(anchor),
            Vec::new(),
            Vec::new(),
            original,
            kind,
            reason.into(),
        );
    }

    fn push_exclusion(
        &mut self,
        site: Site,
        redline: Vec<BlockKey>,
        copies: Vec<BlockKey>,
        original: Vec<BlockKey>,
        kind: &'static str,
        reason: String,
    ) {
        self.rc.exclusions.push(Exclusion {
            site,
            text: String::new(),
            kind,
            reason,
            redline,
            copies,
            original,
        });
    }

    /// Gives each exclusion that names no block of the redline that is it the first words of its original block.
    pub(super) fn finish_exclusions(&mut self, original: &State) {
        for e in &mut self.rc.exclusions {
            if matches!(e.site, Site::Block(_)) {
                continue;
            }
            if let Some(first) = e.original.first() {
                e.text = first_words(&start_of(original, first.part(), first.node()));
            }
        }
    }
}

/// The text of an original block: a paragraph's own, or for a table or a row the text of its first paragraph.
fn start_of(original: &State, part: &Arc<str>, node: NodeId) -> String {
    let own = Builder::text_of(original, part, node);
    if !own.is_empty() {
        return own;
    }
    let view = original.parts();
    let Some(tree) = view.id(part).and_then(|id| view.tree(id).ok()) else {
        return String::new();
    };
    match first_paragraph(tree, node) {
        Some(p) if p != node => Builder::text_of(original, part, p),
        _ => String::new(),
    }
}

/// The first paragraph at or below `node`, in document order.
fn first_paragraph(tree: &SpanTree, node: NodeId) -> Option<NodeId> {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        let at = tree.node_ref(n);
        if at.local_name() == "p" {
            return Some(n);
        }
        let kids: Vec<NodeId> = at.child_elements().map(|c| c.id()).collect();
        stack.extend(kids.into_iter().rev());
    }
    None
}

/// The first words of a text, with an ellipsis when there are more.
fn first_words(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out = words
        .iter()
        .take(WORDS)
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    if words.len() > WORDS {
        out.push_str("...");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::first_words;

    #[test]
    fn a_long_text_is_cut_after_its_first_words() {
        assert_eq!(first_words("  a  b c "), "a b c");
        assert_eq!(
            first_words("one two three four five six seven eight nine"),
            "one two three four five six seven eight..."
        );
        assert_eq!(first_words(""), "");
    }
}
