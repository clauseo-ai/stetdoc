//! The narrow handle that `compare`'s import writes through (design `compare.md`, 7).
//!
//! Rule 2 of `ARCHITECTURE.md` makes the compiler the one writer of the document's stories. The import copies content of
//! another package into the transaction between two calls of the compiler, and an `ImportWriter` is all it gets: it writes
//! the parts that are not stories (the style sheet, the numbering part, copied pictures; never the comments part), it can add a
//! notes part that the document lacks, and in a story it can only create **detached** nodes and bind namespaces on the story's
//! root. Placing those nodes in the story is left to `place_deleted`, `redline_paragraph` and `add_deleted_note`, which write the
//! revisions around them. Every definition and part it adds is declared as an `ImportedDef`, so that `check` can key it as
//! foreign.

use super::errors::{internal, invalid};
use super::outcome::Effect;
use super::redline_types::ImportedDef;
use crate::api::error::StetError;
use crate::state::{NewPart, PartsView, Tx};
use crate::xml::{NodeId, SpanTree};
use std::sync::Arc;

/// The content types of the document's stories, which `add_empty_part` and `add_part` do not add.
const STORY_TYPES: [&str; 5] = [
    "document.main+xml",
    "header+xml",
    "footer+xml",
    "footnotes+xml",
    "endnotes+xml",
];

/// The content types of the notes parts that `add_notes_part` may add.
const NOTES_TYPES: [&str; 2] = ["footnotes+xml", "endnotes+xml"];

/// What the import may write in a transaction.
pub(crate) struct ImportWriter<'a, 's> {
    tx: &'a mut Tx<'s>,
    effects: &'a mut Vec<Effect>,
}

impl<'s> super::compiler::Compiler<'_, 's> {
    /// The handle that `compare`'s import writes through between two calls. What it adds is declared in the compiler's
    /// effects, one `Effect` per definition or part, with nothing else in it.
    pub(crate) fn import_writer(&mut self) -> ImportWriter<'_, 's> {
        ImportWriter {
            tx: &mut *self.tx,
            effects: &mut self.effects,
        }
    }
}

impl<'s> ImportWriter<'_, 's> {
    /// The parts as the transaction sees them, to read.
    pub(crate) fn parts(&self) -> PartsView<'_> {
        self.tx.parts()
    }

    /// The next annotation id: an imported bookmark's.
    pub(crate) fn next_annotation_id(&mut self) -> Result<u32, StetError> {
        self.tx.next_annotation_id()
    }

    /// A part's tree, to change: a part that is no story of the document, the comments part included (the import adds no
    /// comment, design 7.6, and `comments` is that part's one writer). A definition appended to it is declared with `declare`.
    pub(crate) fn part_mut(&mut self, name: &str) -> Result<&mut SpanTree, StetError> {
        let is_story = self
            .tx
            .parts()
            .story_specs()?
            .0
            .iter()
            .any(|s| s.part.eq_ignore_ascii_case(name));
        if is_story {
            return Err(internal(
                "the import writes a story only through detached nodes, and never the comments part",
            ));
        }
        self.tx.part_mut(name)
    }

    /// Adds an empty part that is not a story (a style sheet or a numbering part the document lacks), which declares nothing
    /// because it holds nothing of the other package.
    pub(crate) fn add_empty_part(&mut self, new: NewPart<'_>) -> Result<Option<String>, StetError> {
        self.add_not_story(new)
    }

    /// Adds a part whose content type is not a story's, with its content type and relationship.
    fn add_not_story(&mut self, new: NewPart<'_>) -> Result<Option<String>, StetError> {
        if STORY_TYPES.iter().any(|t| new.content_type.ends_with(t)) {
            return Err(internal(format!(
                "{} is a story of the document, and only the compiler writes those",
                new.name
            )));
        }
        self.tx.add_part(new)
    }

    /// Declares a definition or a part that the import added, with what it was copied from.
    pub(crate) fn declare(&mut self, def: ImportedDef) {
        self.effects.push(Effect {
            imported_defs: vec![def],
            ..Effect::default()
        });
    }

    /// Adds a part that is not a story (a picture), copied from `source` of the other package, and declares it.
    pub(crate) fn add_part(
        &mut self,
        new: NewPart<'_>,
        source: &str,
    ) -> Result<Option<String>, StetError> {
        let name: Arc<str> = Arc::from(new.name);
        let rel = self.add_not_story(new)?;
        self.declare(ImportedDef {
            part: name,
            node: None,
            source_part: Arc::from(source),
            source_node: None,
        });
        Ok(rel)
    }

    /// A story's tree, in which only detached nodes can be made.
    pub(crate) fn story(&mut self, part: &str) -> Result<Detached<'_>, StetError> {
        Ok(Detached {
            tree: self.tx.part_mut(part)?,
        })
    }

    /// Adds a footnotes or endnotes part that the document does not have, with the bytes of the other package's (`source`),
    /// related from `main` by `rel_type`. Only the separator notes (those with a `w:type`) are kept: the notes of the other
    /// package are not this document's, and each note of deleted content is added by `add_deleted_note` with its reference.
    pub(crate) fn add_notes_part(
        &mut self,
        new: NewPart<'_>,
        source: &str,
    ) -> Result<Arc<str>, StetError> {
        if !NOTES_TYPES.iter().any(|t| new.content_type.ends_with(t)) {
            return Err(invalid("only a footnotes or endnotes part is added here"));
        }
        let kind = if new.content_type.ends_with(NOTES_TYPES[1]) {
            crate::wordml::StoryKind::Endnotes
        } else {
            crate::wordml::StoryKind::Footnotes
        };
        let view = self.tx.parts();
        if view.contains(new.name) || view.story_specs()?.0.iter().any(|s| s.kind == kind) {
            return Err(invalid("the document already has this notes part"));
        }
        let name: Arc<str> = Arc::from(new.name);
        self.tx.add_part(new)?;
        let tree = self.tx.part_mut(&name)?;
        let doomed: Vec<NodeId> = tree
            .root()
            .child_elements()
            .filter(|c| !c.attrs().any(|a| a.local_name() == "type"))
            .map(|c| c.id())
            .collect();
        let mut ed = tree.edit();
        for d in doomed {
            ed.detach(d)?;
        }
        self.declare(ImportedDef {
            part: Arc::clone(&name),
            node: None,
            source_part: Arc::from(source),
            source_node: None,
        });
        Ok(name)
    }
}

/// A story's tree as the import sees it: it can read it, make detached nodes and bind namespaces on the root, and nothing else.
pub(crate) struct Detached<'a> {
    tree: &'a mut SpanTree,
}

/// True if `node` is not below the root of `tree`.
fn detached(tree: &SpanTree, node: NodeId) -> bool {
    let root = tree.root().id();
    let mut cur = Some(tree.node_ref(node));
    while let Some(n) = cur {
        if n.id() == root {
            return false;
        }
        cur = n.parent();
    }
    true
}

fn attached() -> StetError {
    internal("the import writes only detached nodes in a story")
}

impl<'a> Detached<'a> {
    /// The same handle over a tree that is not a story, which the caller already holds for writing (the style sheet): the
    /// import writes a definition the same way, detached, and then places it itself.
    pub(crate) fn over(tree: &'a mut SpanTree) -> Detached<'a> {
        Detached { tree }
    }

    /// The tree, to read.
    pub(crate) fn tree(&self) -> &SpanTree {
        self.tree
    }

    /// A new detached element.
    pub(crate) fn create_element(&mut self, ns: &str, local: &str) -> Result<NodeId, StetError> {
        self.tree.edit().create_element(ns, local)
    }

    /// A new detached text node.
    pub(crate) fn create_text(&mut self, text: &str) -> Result<NodeId, StetError> {
        self.tree.edit().create_text(text)
    }

    /// Sets an attribute of a detached element.
    pub(crate) fn set_attr(
        &mut self,
        el: NodeId,
        ns: &str,
        local: &str,
        value: &str,
    ) -> Result<(), StetError> {
        if !detached(self.tree, el) {
            return Err(attached());
        }
        self.tree.edit().set_attr(el, ns, local, value)
    }

    /// Appends a detached node to a detached element.
    pub(crate) fn append_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), StetError> {
        if !detached(self.tree, parent) || !detached(self.tree, child) {
            return Err(attached());
        }
        self.tree.edit().append_child(parent, child)
    }

    /// Binds a namespace on the story's root and returns the prefix it has there.
    pub(crate) fn bind_namespace(
        &mut self,
        uri: &str,
        prefix: &str,
        ignorable: bool,
    ) -> Result<String, StetError> {
        self.tree.edit().bind_namespace(uri, prefix, ignorable)
    }
}
