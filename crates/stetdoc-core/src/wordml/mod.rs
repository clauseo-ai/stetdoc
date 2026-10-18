//! What WordprocessingML means: stories, blocks, definitions and the text model.
//!
//! **Owns.** Typed views over span trees: paragraphs, runs, tables, sections, notes,
//! comments, fields and content controls, plus style and numbering *definitions* and the
//! `text_model` that maps characters to XML and back.
//!
//! **Interface.** `Story`, `Block`, `Paragraph::{text, map}`, `TextMap`, `StyleDefs`, `NumberingDefs`.
//! The seam with `refs` and `state` is `BlockKey`, `Story::scan`, `stories` and `Rels`, the id
//! readers `existing_ids` and `id_parts`, and the constants `SCAN_VERSION` and `FINGERPRINT_VERSION`.
//!
//! **Invariants.**
//! - Style and numbering definitions live here, so `styles` and `numbering` do not depend
//!   on each other.
//! - Views read span trees and do not serialize them.
//! - Reading is total: a tree that parsed is read, and what was skipped is reported.
//!
//! **Depends on.** `api`, `xml`, `schema`.
//!
//! **Start reading.** `wordml/text_model.rs`. The design notes are in
//! [`docs/design/wordml.md`](../../../../docs/design/wordml.md).

#![expect(
    dead_code,
    unused_imports,
    reason = "`styles`, `numbering`, `refs` and `state` use the story scan, the paragraph facts and the text model, and `wordml` is a private module, so the readers that `revisions` and `projection` build on (blocks, sections, anchors, controls, objects, joins, revisions, the map operations) and the re-exports that go with them count as unused until those modules land"
)]

mod anchors;
mod anomaly;
mod block;
mod context_key;
mod control;
mod fingerprint;
mod ids;
mod mc;
mod names;
mod num_facts;
mod numbering_defs;
mod numbering_levels;
mod object;
mod own;
mod paragraph;
mod props;
mod refresh;
mod rels;
mod revision;
mod sections;
mod story;
mod story_scan;
mod styles_defs;
mod table_site;
#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod tests_anchors;
#[cfg(test)]
mod tests_block;
#[cfg(test)]
mod tests_chain;
#[cfg(test)]
mod tests_context_views;
#[cfg(test)]
mod tests_control;
#[cfg(test)]
mod tests_corpus;
#[cfg(test)]
mod tests_corpus_count;
#[cfg(test)]
mod tests_corpus_zip;
#[cfg(test)]
mod tests_current;
#[cfg(test)]
mod tests_fields;
#[cfg(test)]
mod tests_fields_recovery;
#[cfg(test)]
mod tests_fields_size;
#[cfg(test)]
mod tests_gen;
#[cfg(test)]
mod tests_gen_data;
#[cfg(test)]
mod tests_gen_laws;
#[cfg(test)]
mod tests_gen_oracle;
#[cfg(test)]
mod tests_hostile;
#[cfg(test)]
mod tests_hostile_fields;
#[cfg(test)]
mod tests_hostile_slow;
#[cfg(test)]
mod tests_join;
#[cfg(test)]
mod tests_locate;
#[cfg(test)]
mod tests_mark_revs;
#[cfg(test)]
mod tests_meta;
#[cfg(test)]
mod tests_meta_same;
#[cfg(test)]
mod tests_numbering;
#[cfg(test)]
mod tests_numfacts;
#[cfg(test)]
mod tests_object_info;
#[cfg(test)]
mod tests_objects;
#[cfg(test)]
mod tests_process_content;
#[cfg(test)]
mod tests_props;
#[cfg(test)]
mod tests_refresh;
#[cfg(test)]
mod tests_refresh_more;
#[cfg(test)]
mod tests_refresh_random;
#[cfg(test)]
mod tests_revision;
#[cfg(test)]
mod tests_scan_edit;
#[cfg(test)]
mod tests_scan_more;
#[cfg(test)]
mod tests_scan_prints;
#[cfg(test)]
mod tests_sections;
#[cfg(test)]
mod tests_story;
#[cfg(test)]
mod tests_story_ids;
#[cfg(test)]
mod tests_strict;
#[cfg(test)]
mod tests_styles;
#[cfg(test)]
mod tests_table_site;
#[cfg(test)]
mod tests_text;
mod text_build;
mod text_content;
mod text_fields;
mod text_join;
mod text_locate;
mod text_model;

pub use crate::api::view::Mode;
pub use crate::xml::{NodeId, SpanTree};
pub use anchors::{Anchors, Bookmark, CommentAnchor, Loc, MoveAnchor};
pub use anomaly::{Anomaly, AnomalyKind};
pub use block::{Block, Cell, Row, Table, VMerge};
pub use control::{Control, ControlKind, Hyperlink, Lock, control, enclosing_controls, hyperlink};
pub use ids::{IdKind, existing_ids, id_parts};
pub use num_facts::{NumFact, Presence};
pub use numbering_defs::{NumberingDefs, NumberingIssue};
pub use numbering_levels::{Level, NumFmt, Suff};
pub use object::{ObjectInfo, ObjectKind, TextBox, math_text};
pub use own::OwnParts;
pub use paragraph::{NumPr, Paragraph};
pub use props::{Attr, Ns, Prop, Props, PropsRef};
pub use refresh::Refreshed;
pub use rels::{Rel, RelTarget, Rels, StorySpec, stories, stories_of};
pub use revision::{Branch, RevKind, Revision, part_revisions, revision_at};
pub use sections::{Section, SectionBreak};
pub use story::{
    BlockEntry, BlockKey, BlockKind, FINGERPRINT_VERSION, IN_CONTROL, MARK_DELETED, MARK_INSERTED,
    MARK_MOVED_FROM, MARK_MOVED_TO, MarkRevs, NO_UNIT, Opaque, OpaqueKind, SCAN_VERSION,
    SECTION_BREAK, Story, StoryKind, Unit, UnitKind,
};
pub use styles_defs::{StyleDef, StyleDefs, StyleIssue, StyleKind, TableRegion};
pub use table_site::{TableSite, TblLook};
pub use text_content::space_preserved;
pub use text_fields::FieldInfo;
pub use text_join::{Joined, ends_container};
pub use text_locate::{Gap, Located, Span};
pub use text_model::{
    CDATA, Context, FIELD_RESULT, FieldRef, Item, ItemKind, NO_CONTEXT, NO_FIELD, TextMap, Vis,
    WrapperKind,
};
