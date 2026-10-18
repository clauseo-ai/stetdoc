//! Input safety bounds.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Bounds on untrusted input. `Default` is the production setting, and a partial JSON object keeps the default of every field it leaves out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    /// Size of the input package in bytes.
    pub max_archive_bytes: u64,
    /// Number of ZIP entries.
    pub max_entries: u32,
    /// Declared uncompressed size of one entry.
    pub max_entry_bytes: u64,
    /// Sum of the declared uncompressed sizes of all entries.
    pub max_total_bytes: u64,
    /// Uncompressed size divided by compressed size, per entry. Applies to entries whose
    /// uncompressed size is at least 16 MiB.
    pub max_compression_ratio: u32,
    /// Element nesting depth in one XML part.
    pub max_xml_depth: u32,
    /// Attributes on one element, including namespace declarations.
    pub max_xml_attributes: u32,
    /// Namespace declarations in scope at one element: all those on the element and its ancestors,
    /// counting a declaration that a closer one of the same prefix hides. Looking a name up costs
    /// time in proportion to this, so it bounds the cost of an edit in a part that declares
    /// thousands of namespaces down one path.
    pub max_xml_namespaces_in_scope: u32,
    /// Nodes plus attributes in one XML part. Bounds the memory a part can make the span tree
    /// allocate: about 60 bytes for each, however few input bytes they take.
    pub max_xml_nodes: u32,
    /// Refs in one ref namespace, live and retired together, and committed versions of one
    /// document. It bounds the ref table, the lineage and what `restore` accepts. The default is
    /// 57 times the 18,260 blocks of the largest document measured (a 12.6 MB body), and 66 times
    /// the 15,878 of the largest of 42,003 Harvey LAB documents.
    pub max_refs: u32,
    /// Operations in one batch, each paragraph of an `insert` counting as one, since each costs what an operation does.
    /// For an edit of readable files, the blocks that the edit changes. The
    /// budget is 50 operations in a second, the largest corpus document has 15,878 blocks, and on a
    /// dense 28 MiB body an operation costs about 4 ms, so a batch at the limit takes about 20 s.
    pub max_batch_ops: u32,
    /// Bytes of one batch, as JSON or as block text: a rewrite of every paragraph of the largest
    /// document at 1 KB each.
    pub max_batch_bytes: u64,
    /// Bytes of one text field of an operation: 300 pages of 3,000 characters is 0.9 MB.
    pub max_op_text_bytes: u32,
    /// Entries of a scope. A scope is a few refs and ranges, and a range is one entry however many blocks
    /// it covers, so this bounds what a hostile scope can make the guard allocate, at about 1 MB.
    pub max_scope_refs: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_archive_bytes: 512 << 20,
            max_entries: 10_000,
            max_entry_bytes: 256 << 20,
            max_total_bytes: 1 << 30,
            max_compression_ratio: 500,
            max_xml_depth: 256,
            max_xml_attributes: 1024,
            max_xml_namespaces_in_scope: 1024,
            max_xml_nodes: 5_000_000,
            max_refs: 1_048_576,
            max_batch_ops: 5_000,
            max_batch_bytes: 16 << 20,
            max_op_text_bytes: 1 << 20,
            max_scope_refs: 10_000,
        }
    }
}
