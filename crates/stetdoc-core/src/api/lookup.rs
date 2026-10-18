//! What `lookup` returns: plain data built by `index` at the end of a query.
//!
//! The types follow `docs/design/projection-and-index.md`, section 7.5. A ref crosses as its
//! text and a story as its name.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A place in the document: a block, the story it is in, and what kind of thing was found there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Site {
    /// The ref of the block.
    pub block: String,
    /// The story: `body`, `notes`, `headers` or `comments`.
    pub story: String,
    /// What was found: `definition`, `parenthetical` or `use`.
    pub kind: String,
    /// A short piece of the block's text around it.
    pub snippet: String,
}

/// One cross-reference: a block that cites something and what it resolved to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Reference {
    /// The ref of the citing block.
    pub from: String,
    /// The ref of the block it resolved to, when it resolved to exactly one or a `relative` reading chose one.
    pub to: Option<String>,
    /// The citation as written.
    pub text: String,
    /// `resolved`, `ambiguous`, `relative`, `outside` or `unresolved`.
    pub state: String,
    /// Every block it may mean, when there are several, or the candidates of an `outside` one. A `relative` one
    /// lists its chosen block first and then the candidates it was chosen over.
    pub candidates: Vec<String>,
}

/// One thing a query found. A query returns every hit and never chooses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Hit {
    /// A block, with what refers to it and what it refers to.
    Block {
        /// The ref.
        block: String,
        /// The story.
        story: String,
        /// The label Word shows, such as `4.2`.
        label: Option<String>,
        /// The label composed across lists, such as `4.2(a)(ii)`.
        full: Option<String>,
        /// The block's text as the format writes it.
        text: String,
        /// The references to this block.
        inbound: Vec<Reference>,
        /// The references this block makes.
        outbound: Vec<Reference>,
        /// The terms this block defines.
        defines: Vec<String>,
    },
    /// A clause: a labelled block.
    Clause {
        /// The ref.
        block: String,
        /// The story.
        story: String,
        /// The label.
        label: String,
        /// The composed label.
        full: String,
        /// `computed` (Word's numbering) or `typed` (the author typed it).
        source: String,
        /// True when the composed label was built across lists.
        composed: bool,
        /// The noun in front of it, such as `Section`.
        noun: Option<String>,
        /// The start of its text.
        text: String,
        /// The rule that matched the query.
        matched_by: String,
    },
    /// A defined term.
    Term {
        /// The term.
        term: String,
        /// `exact` or `case_insensitive`.
        matched: String,
        /// Where it is defined.
        definitions: Vec<Site>,
        /// Where it is used.
        uses: Vec<Site>,
        /// How many uses there are in all.
        use_count: u32,
        /// True when more than one block defines it.
        multiple_definitions: bool,
    },
}

/// The answer to a query, at one state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Lookup {
    /// The base token of the state the answer describes. Every ref in it is a ref of that token.
    pub base: String,
    /// The query as asked.
    pub query: String,
    /// Every hit, in the order block, clause, term.
    pub hits: Vec<Hit>,
    /// The answer as text, in the document's format: what a surface prints.
    #[serde(default)]
    pub text: String,
}
