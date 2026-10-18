//! The block scan: `Story::scan`.
//!
//! One walk over the block structure of a part, with an explicit stack, so nesting never touches the
//! Rust stack. It reads no paragraph content except what a paragraph says about itself in its first
//! `w:pPr`: a section break and a revised paragraph mark. The rules of what counts as a block, and in
//! which order, are `docs/design/wordml.md` 3.2, and they are frozen by `SCAN_VERSION`.

use super::anomaly::{Anomaly, AnomalyKind};
use super::context_key;
use super::mc;
use super::names::{M, int, is_in, is_w, is_word_ns, w_attr, w_child};
use super::story::{
    BlockEntry, BlockKind, IN_CONTROL, MARK_DELETED, MARK_INSERTED, MARK_MOVED_FROM, MARK_MOVED_TO,
    NO_UNIT, Opaque, OpaqueKind, SECTION_BREAK, Story, StoryKind, Unit, UnitKind,
};
use crate::api::error::{ErrorCode, StetError};
use crate::xml::{Children, NodeRef, SpanTree};
use std::sync::Arc;

/// Which kind of child the walk expects at this depth.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    /// Blocks: the children of a body, a header, a note, a comment, a cell or a transparent wrapper.
    Block,
    /// The children of a table: rows, and what stands among them.
    Table,
    /// The children of a row: cells, and what stands among them.
    Row,
}

struct Frame<'a> {
    it: Children<'a>,
    level: Level,
    parent: Option<u32>,
    in_control: bool,
    /// The key of the wrappers around what this frame holds (`context_key.rs`).
    ctx: u64,
}

/// Elements of the `w` vocabulary that carry no content where a block, a row or a cell may stand.
/// They are skipped without a placeholder.
const SKIPPED: [&str; 28] = [
    "bookmarkStart",
    "bookmarkEnd",
    "commentRangeStart",
    "commentRangeEnd",
    "permStart",
    "permEnd",
    "proofErr",
    "moveFromRangeStart",
    "moveFromRangeEnd",
    "moveToRangeStart",
    "moveToRangeEnd",
    "customXmlInsRangeStart",
    "customXmlInsRangeEnd",
    "customXmlDelRangeStart",
    "customXmlDelRangeEnd",
    "customXmlMoveFromRangeStart",
    "customXmlMoveFromRangeEnd",
    "customXmlMoveToRangeStart",
    "customXmlMoveToRangeEnd",
    "sectPr",
    "tblPr",
    "tblGrid",
    "trPr",
    "tblPrEx",
    "tcPr",
    "customXmlPr",
    "smartTagPr",
    "sdtEndPr",
];

fn index(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

impl Story {
    /// Scans one part. `part` is the canonical name `state` holds, so a scan allocates no name.
    ///
    /// Only the body can fail, and only when its root is not `w:document` or it has no `w:body`.
    /// Any other story whose root is not the element its kind expects is an empty story with an
    /// anomaly, so a re-scan of an edited tree never fails.
    pub fn scan(part: &Arc<str>, kind: StoryKind, tree: &SpanTree) -> Result<Story, StetError> {
        let mut story = Story::new(Arc::clone(part), kind);
        story.edits = tree.edit_count();
        let root = tree.root();
        match kind {
            StoryKind::Body => {
                if !is_w(&root, "document") {
                    return Err(StetError::new(
                        ErrorCode::MalformedInput,
                        format!(
                            "{part}: the root element is `{}`, and the main part needs `w:document`",
                            root.name()
                        ),
                    ));
                }
                let mut bodies = root.child_elements().filter(|c| is_w(c, "body"));
                let Some(body) = bodies.next() else {
                    return Err(StetError::new(
                        ErrorCode::MalformedInput,
                        format!("{part}: `w:document` has no `w:body`"),
                    ));
                };
                // Word reads the first body. What is in another one is content nobody can see, so
                // it is counted, one anomaly for each body.
                for extra in bodies {
                    story.anomalies.push(Anomaly {
                        kind: AnomalyKind::ExtraBody,
                        node: extra.id(),
                    });
                }
                story.walk(body, NO_UNIT);
            }
            StoryKind::Header | StoryKind::Footer => {
                let expected = if kind == StoryKind::Header {
                    "hdr"
                } else {
                    "ftr"
                };
                if is_w(&root, expected) {
                    story.walk(root, NO_UNIT);
                } else {
                    story.wrong_root(&root);
                }
            }
            StoryKind::Footnotes => {
                story.scan_units(&root, "footnotes", "footnote", UnitKind::Footnote)
            }
            StoryKind::Endnotes => {
                story.scan_units(&root, "endnotes", "endnote", UnitKind::Endnote)
            }
            StoryKind::Comments => {
                story.scan_units(&root, "comments", "comment", UnitKind::Comment)
            }
        }
        Ok(story)
    }

    fn wrong_root(&mut self, root: &NodeRef<'_>) {
        self.anomalies.push(Anomaly {
            kind: AnomalyKind::WrongRoot,
            node: root.id(),
        });
    }

    /// Notes and comments: each unit is scanned as a story of its own blocks.
    fn scan_units(
        &mut self,
        root: &NodeRef<'_>,
        root_name: &str,
        unit_name: &str,
        unit_kind: UnitKind,
    ) {
        if !is_w(root, root_name) {
            self.wrong_root(root);
            return;
        }
        for u in root.child_elements() {
            if !is_w(&u, unit_name) {
                continue;
            }
            if unit_kind != UnitKind::Comment
                && w_attr(&u, "type").is_some_and(|t| {
                    matches!(
                        t.as_ref(),
                        "separator" | "continuationSeparator" | "continuationNotice"
                    )
                })
            {
                continue;
            }
            let id = w_attr(&u, "id").and_then(|v| int(&v));
            if id.is_none() {
                self.anomalies.push(Anomaly {
                    kind: AnomalyKind::UnitWithoutId,
                    node: u.id(),
                });
            }
            let owned = |name: &str| w_attr(&u, name).map(|v| Box::<str>::from(v.as_ref()));
            let unit = Unit {
                id,
                node: u.id(),
                kind: unit_kind,
                author: owned("author"),
                initials: owned("initials"),
                date: owned("date"),
            };
            let at = index(self.units.len());
            self.units.push(unit);
            self.walk(u, at);
        }
    }

    /// Walks the children of `container` as blocks, with an explicit stack.
    fn walk(&mut self, container: NodeRef<'_>, unit: u32) {
        let mut stack = vec![Frame {
            it: container.children(),
            level: Level::Block,
            parent: None,
            in_control: false,
            ctx: context_key::ROOT,
        }];
        while let Some(top) = stack.last_mut() {
            let Some(c) = top.it.next() else {
                stack.pop();
                continue;
            };
            let outer = (top.level, top.parent, top.in_control, top.ctx);
            if !c.is_element() {
                continue;
            }
            if let Some(frame) = self.visit(&c, outer, unit) {
                stack.push(frame);
            }
        }
    }

    fn push(
        &mut self,
        node: &NodeRef<'_>,
        kind: BlockKind,
        parent: Option<u32>,
        unit: u32,
        flags: u8,
        ctx: u64,
    ) -> u32 {
        let at = index(self.entries.len());
        self.entries.push(BlockEntry {
            node: node.id(),
            kind,
            parent,
            unit,
            flags,
        });
        self.ctx.push(ctx);
        at
    }

    /// Handles one child element. Returns the frame to walk into, if the element has blocks in it.
    fn visit<'a>(
        &mut self,
        c: &NodeRef<'a>,
        (level, parent, in_control, ctx): (Level, Option<u32>, bool, u64),
        unit: u32,
    ) -> Option<Frame<'a>> {
        let local = c.local_name();
        if is_word_ns(c.namespace()) {
            match (level, local) {
                (Level::Block, "p") => {
                    let controlled = if in_control { IN_CONTROL } else { 0 };
                    self.push(
                        c,
                        BlockKind::Paragraph,
                        parent,
                        unit,
                        paragraph_flags(c) | controlled,
                        ctx,
                    );
                    return None;
                }
                (Level::Block, "tbl") => {
                    let at = self.push(c, BlockKind::Table, parent, unit, 0, ctx);
                    return Some(Frame {
                        it: c.children(),
                        level: Level::Table,
                        parent: Some(at),
                        in_control,
                        ctx,
                    });
                }
                (Level::Table, "tr") => {
                    let at = self.push(c, BlockKind::Row, parent, unit, 0, ctx);
                    return Some(Frame {
                        it: c.children(),
                        level: Level::Row,
                        parent: Some(at),
                        in_control,
                        ctx: context_key::enter(ctx, c),
                    });
                }
                (Level::Row, "tc") => {
                    return Some(Frame {
                        it: c.children(),
                        level: Level::Block,
                        parent,
                        in_control,
                        ctx: context_key::enter(ctx, c),
                    });
                }
                (_, "sdt") => {
                    let content = w_child(c, "sdtContent")?;
                    return Some(Frame {
                        it: content.children(),
                        level,
                        parent,
                        in_control: true,
                        ctx: context_key::enter(ctx, c),
                    });
                }
                (_, "customXml" | "smartTag" | "ins" | "del" | "moveFrom" | "moveTo") => {
                    return Some(Frame {
                        it: c.children(),
                        level,
                        parent,
                        in_control,
                        ctx: context_key::enter(ctx, c),
                    });
                }
                _ if SKIPPED.contains(&local) => return None,
                _ => {}
            }
        } else {
            match mc::disposition(c) {
                mc::Mc::Branch(branch) => {
                    return Some(Frame {
                        it: branch.children(),
                        level,
                        parent,
                        in_control,
                        ctx: context_key::enter(ctx, &branch),
                    });
                }
                mc::Mc::NoBranch => {
                    self.anomalies.push(Anomaly {
                        kind: AnomalyKind::NoBranch,
                        node: c.id(),
                    });
                    return None;
                }
                mc::Mc::ProcessContent => {
                    return Some(Frame {
                        it: c.children(),
                        level,
                        parent,
                        in_control,
                        ctx,
                    });
                }
                // ignorable and not named by `mc:ProcessContent`: Word skips it
                mc::Mc::Skip => return None,
                mc::Mc::Normal => {}
            }
        }
        self.opaque_here(c, level);
        None
    }

    /// An element the scan does not read where it stands: listed, not a block.
    fn opaque_here(&mut self, c: &NodeRef<'_>, level: Level) {
        let kind = if is_w(c, "altChunk") {
            OpaqueKind::AltChunk
        } else if is_in(c, &M, "oMath") || is_in(c, &M, "oMathPara") {
            OpaqueKind::Math
        } else {
            OpaqueKind::Unknown
        };
        if level != Level::Block {
            self.anomalies.push(Anomaly {
                kind: AnomalyKind::MisplacedElement,
                node: c.id(),
            });
        }
        self.opaque.push(Opaque {
            node: c.id(),
            kind,
            after: self.entries.len().checked_sub(1),
        });
    }
}

/// What a paragraph says about itself in its first `w:pPr`: a section break and a revised mark.
pub(super) fn paragraph_flags(p: &NodeRef<'_>) -> u8 {
    let Some(ppr) = p.child_elements().next().filter(|e| is_w(e, "pPr")) else {
        return 0;
    };
    let mut flags = 0u8;
    for g in ppr.child_elements() {
        if is_w(&g, "sectPr") {
            flags |= SECTION_BREAK;
        } else if is_w(&g, "rPr") {
            for h in g.child_elements() {
                if !is_word_ns(h.namespace()) {
                    continue;
                }
                match h.local_name() {
                    "ins" => flags |= MARK_INSERTED,
                    "del" => flags |= MARK_DELETED,
                    "moveFrom" => flags |= MARK_MOVED_FROM,
                    "moveTo" => flags |= MARK_MOVED_TO,
                    _ => {}
                }
            }
        }
    }
    flags
}
