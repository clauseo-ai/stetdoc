//! The span tree: XML-level byte identity.
//!
//! **Owns.** The span tree: parsing, nodes that record byte spans in the part's original bytes,
//! dirty tracking, the serializer and namespaces. Only this module serializes XML.
//!
//! **Interface.** `SpanTree::{parse, root, edit, serialize, edit_count, node_count, created_since}`. `parse` takes the part's name, which
//! starts every error message. `root`, `document` and `node_ref` give a `NodeRef`, which reads names,
//! namespaces, attributes, text, children and siblings. `edit` returns an `Editor` with the changes:
//! attributes, text, new elements, insert, detach, clone, and `bind_namespace`. `Bytes` is the
//! shared buffer a part is read into.
//!
//! **Invariants.**
//! - A clean node serializes by copying its original bytes, never by re-serializing.
//! - A node is dirty when an edit changed it or anything inside it, or made it meet another text
//!   node: the writer joins text that meets, which can write either of the two differently from its
//!   bytes (see below), so `is_dirty` is false only for a node that is written as it was read. An
//!   edit changes only the bytes of the nodes that are dirty afterwards, except that the part's
//!   root start tag may gain the namespace declarations and `mc:Ignorable` tokens that new content
//!   needs. An unedited tree serializes to its input exactly, including prefixes, attribute order
//!   and quotes, references, whitespace, line endings, BOM and UTF-16.
//! - The nodes and the attributes are stored in chunks (`arena.rs`) that a tree shares with its
//!   clones until one of them changes something in a chunk, and then only that chunk is copied.
//!   Cloning a tree copies a pointer for each 512 nodes and each 4,096 attributes, and an edit
//!   copies what it changes: the chunks of the node, of the elements above it that it marks, and
//!   of its neighbours. A chunk is allocated whole and nothing is copied to make room, so a tree
//!   holds no more than a chunk more than it uses. The attributes of an element are stored next to
//!   each other, so an element holds at most 4,096 of them, whatever `max_xml_attributes` says.
//! - A `NodeId` names one node for as long as its tree lives, and in the clones of the tree. The
//!   arena only grows: a node that is detached keeps its id and everything under it, a node made
//!   later never takes the id of one that exists, and no id is renumbered. `SpanTree::clone` has the
//!   same ids for the same nodes. After the clone each tree hands out ids on its own, so an id made
//!   later in one of them means nothing in the other.
//! - `SpanTree::edit_count` moves when the editor changes the tree and at no other time (a refused
//!   edit, a read through an `Editor` and a call that finds nothing to change leave it alone). It counts
//!   marks and not calls, so it is compared for equality. A clone starts at the count of the tree it
//!   was cloned from. It says that a tree has changed since a count was read, never that two trees are
//!   equal, so a cache records it with the tree's epoch. Creating a node moves it though no document
//!   changed.
//! - A new element or attribute gets its prefix when it is inserted, from what is in scope there:
//!   the nearest binding of its namespace that no closer declaration of the same prefix hides.
//!   If there is none, the namespace is declared on the root, as Word does, or on the node itself
//!   when a closer declaration hides the root's. A moved or inserted node whose written prefixes
//!   would mean something else at the new position keeps their meaning: the namespace each had is
//!   declared on the highest element of the subtree that writes it. Only a prefix that cannot be
//!   declared (`xml`, `xmlns`) is refused with `unsupported`.
//! - Edits never write a character XML cannot carry (`invalid_payload`), and never write a part
//!   that breaks the limits it was parsed under: attributes per element, nodes, how deep elements
//!   nest, and how many namespace declarations are in scope. An insert that would break either of
//!   the last two is `limit_exceeded` and changes nothing, and it counts the declarations that the
//!   subtree brings with it. The declarations the editor adds itself are counted as an upper bound.
//! - What an edit writes is a document that parses, and reads as the tree does. Text that touches
//!   text is joined so that it reads as the tree reads it: a carriage return before a line feed
//!   stays two line breaks, and brackets that meet `>` never make `]]>`, wherever the join falls
//!   (`]]` before `>`, or `]` before `]>`), and the two nodes are dirty. Outside the root element a
//!   text node holds only white space (`invalid_payload`), a CDATA section cannot go, the XML
//!   declaration cannot be inserted or have anything inserted before it, and the root element
//!   cannot be detached. After an edit, serializing and parsing the result gives the tree's text
//!   and elements.
//! - Moving a node costs the same however many siblings it has: a node knows its previous and its
//!   next sibling.
//! - Reads report what an XML parser reports: references are decoded, line endings in text are
//!   normalized to a line feed, and a tab, line feed or carriage return written literally in an
//!   attribute value is a space.
//! - Parsing hostile input returns an error and never panics: a DOCTYPE or entity declaration,
//!   malformed markup, an undefined entity, `]]>` in text (it is written `]]&gt;`, and no edit
//!   makes it), a bad encoding, a character XML 1.0 does not allow
//!   (a control character other than tab, line feed and carriage return, U+FFFE, U+FFFF), an XML
//!   version other than 1.0, or a limit (depth, attributes, nodes, distinct namespaces, namespace
//!   declarations in scope). Time and memory stay linear in the size of the part: no shape of
//!   declarations or nesting makes name resolution slower per name, and the limit on declarations
//!   in scope bounds what a lookup in an edit costs. Every error message names the part, and says
//!   when a byte offset counts the UTF-8 form of a UTF-16 part.
//! - Nothing recurses on input depth, so a document nested far beyond the limit cannot overflow
//!   the stack.
//!
//! **Depends on.** `api`.
//!
//! **Start reading.** `xml/tree.rs`. The design notes are in
//! [`docs/design/package-and-xml.md`](../../../../docs/design/package-and-xml.md).

#![expect(
    dead_code,
    unused_imports,
    reason = "the re-exports and the read and edit methods are the interface that `schema`, `wordml` and `revisions` build on, and nothing outside `package` uses them yet"
)]

mod arena;
mod bytes;
mod chars;
mod edit;
mod encoding;
mod errors;
mod late;
mod ns;
mod nstable;
mod scan;
mod scan_names;
mod scope;
mod serialize;
mod shape;
mod text;
mod tree;

pub(crate) use bytes::Bytes;
pub use edit::{Editor, MC_NS};
pub use tree::{AttrRef, Children, NodeId, NodeKind, NodeRef, SpanTree};

use crate::api::error::StetError;
use crate::api::limits::Limits;

impl SpanTree {
    /// Parses one part. The bytes may be UTF-8 (with or without a BOM) or UTF-16. `part` is the
    /// part's name, such as `word/document.xml`: it starts the message of every error, so a failure
    /// says which part it is about.
    pub fn parse(part: &str, bytes: Bytes, limits: &Limits) -> Result<SpanTree, StetError> {
        scan::parse(part, bytes, limits)
    }
}

#[cfg(test)]
mod corpus_laws;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_chars;
#[cfg(test)]
mod tests_count;
#[cfg(test)]
mod tests_edit;
#[cfg(test)]
mod tests_joins;
#[cfg(test)]
mod tests_nodes;
#[cfg(test)]
mod tests_ns;
#[cfg(test)]
mod tests_scope;
