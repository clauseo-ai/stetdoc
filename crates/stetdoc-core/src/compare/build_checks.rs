//! What the build asks of a block before it writes it: whether a tracked change can carry it, and whether a copied note has a
//! reference in the story (design `compare.md`, 19.3 and 20).

use super::build::Builder;
use crate::wordml::BlockKey;

impl Builder<'_, '_, '_> {
    /// True if one of the copies that refer to the note is in its story.
    pub(super) fn holds_reference(&self, note: &super::import::NewNote) -> bool {
        let Some(part) = &note.holder_part else {
            return false;
        };
        let view = self.c.tx().parts();
        let Some(id) = view.id(part) else {
            return false;
        };
        let Ok(tree) = view.tree(id) else {
            return false;
        };
        let root = tree.root().id();
        note.holders.iter().any(|&h| {
            let mut cur = Some(tree.node_ref(h));
            while let Some(n) = cur {
                if n.id() == root {
                    return true;
                }
                cur = n.parent();
            }
            false
        })
    }

    /// True if the paragraph has no block after it in its container and holds what rejecting its insertion leaves behind: a
    /// comment's reference, which the resolver keeps with the comment. Range markers are hoisted out of the paragraph and do not
    /// keep it (N2 holds on every pair of the sets with a bookmark or a comment range there).
    pub(super) fn trailing_with_markers(&self, key: &BlockKey) -> bool {
        let view = self.c.tx().parts();
        let Some(id) = view.id(key.part()) else {
            return false;
        };
        let Ok(tree) = view.tree(id) else {
            return false;
        };
        let n = tree.node_ref(key.node());
        let mut next = n.next_sibling();
        while let Some(x) = next {
            if x.is_element() && matches!(x.local_name(), "p" | "tbl" | "sdt") {
                return false;
            }
            next = x.next_sibling();
        }
        let mut stack = vec![n];
        while let Some(x) = stack.pop() {
            for c in x.child_elements() {
                if c.local_name() == "commentReference" {
                    return true;
                }
                if c.local_name() != "tbl" {
                    stack.push(c);
                }
            }
        }
        false
    }

    /// True if a paragraph element holds another paragraph (a document that nests them is not well formed for Word).
    pub(super) fn holds_nested_paragraph(&self, key: &BlockKey) -> bool {
        let view = self.c.tx().parts();
        let Some(id) = view.id(key.part()) else {
            return false;
        };
        let Ok(tree) = view.tree(id) else {
            return false;
        };
        let n = tree.node_ref(key.node());
        if n.local_name() != "p" {
            return false;
        }
        let mut stack = vec![n];
        while let Some(x) = stack.pop() {
            for c in x.child_elements() {
                if c.local_name() == "p" {
                    return true;
                }
                if c.local_name() != "txbxContent" {
                    stack.push(c);
                }
            }
        }
        false
    }

    /// True if the paragraph carries a section break.
    pub(super) fn ends_section(&self, key: &BlockKey) -> bool {
        let view = self.c.tx().parts();
        let Some(id) = view.id(key.part()) else {
            return false;
        };
        let Ok(tree) = view.tree(id) else {
            return false;
        };
        crate::wordml::Paragraph::new(tree, key.node()).is_some_and(|p| p.section_break().is_some())
    }

    /// True if the paragraph has an equation in it.
    pub(super) fn holds_math(&self, key: &BlockKey) -> bool {
        let view = self.c.tx().parts();
        let Some(id) = view.id(key.part()) else {
            return false;
        };
        let Ok(tree) = view.tree(id) else {
            return false;
        };
        let mut hit = false;
        let mut stack = vec![tree.node_ref(key.node())];
        while let Some(n) = stack.pop() {
            for c in n.child_elements() {
                if c.local_name() == "oMath" || c.local_name() == "oMathPara" {
                    hit = true;
                } else if c.local_name() != "tbl" {
                    stack.push(c);
                }
            }
        }
        hit
    }

    /// True if the block is a paragraph.
    pub(super) fn is_paragraph(&self, key: &BlockKey) -> bool {
        let view = self.c.tx().parts();
        view.id(key.part())
            .and_then(|id| view.tree(id).ok())
            .is_some_and(|t| t.node_ref(key.node()).local_name() == "p")
    }
}
