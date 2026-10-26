//! Building a [`TextMap`]: the walk over one paragraph.
//!
//! One explicit stack of frames, one per element whose children are being read, so nesting never
//! touches the Rust stack. The frames carry the context (the wrapper chain), which modes include what
//! is read, and where a run is. What each kind of child becomes is in `text_content.rs`; the field
//! machine is in `text_fields.rs`. `docs/design/wordml.md` section 4.

use super::anomaly::{Anomaly, AnomalyKind};
use super::mc;
use super::names::{CELL_CONTENT, ROW_CONTENT, is_w, is_word_ns, leading_child};
use super::text_fields::{End, FieldState, OpenField, Phase, keyword, shows_without_result};
use super::text_model::{
    Context, FIELD_RESULT, FieldRef, Item, ItemKind, NO_CONTEXT, TextMap, Vis, WrapperKind,
};
use crate::xml::{Children, NodeId, NodeRef};

/// What a frame is reading.
#[derive(Clone, Copy)]
pub(super) enum FrameKind {
    /// The content of a paragraph or of a wrapper: runs, wrappers, markers.
    Content,
    /// The content of a run: text, tabs, field characters, references.
    Run,
    /// The content of a `w:fldSimple`, which closes its field when the frame ends.
    Simple {
        node: NodeId,
        items_before: usize,
        displayed: bool,
    },
}

pub(super) struct Frame<'t> {
    pub it: Children<'t>,
    pub info: Info,
    pub kind: FrameKind,
}

/// What a frame passes to what it reads.
#[derive(Clone, Copy)]
pub(super) struct Info {
    /// The innermost wrapper.
    pub ctx: u32,
    /// Which modes include what is read here.
    pub vis: Vis,
    /// A `w:del` encloses it. A `w:moveFrom` does not count: Word writes `w:t` in a move source.
    pub in_del: bool,
    /// The nearest run.
    pub run: Option<NodeId>,
}

pub(super) struct Builder<'t> {
    pub text: String,
    pub items: Vec<Item>,
    pub contexts: Vec<Context>,
    pub fields: Vec<FieldRef>,
    pub anomalies: Vec<Anomaly>,
    pub chars: u32,
    pub state: FieldState,
    pub stack: Vec<Frame<'t>>,
}

/// Builds the map of a paragraph. `carry` is the fields open when it starts and `base_id` the id its
/// first new field gets.
pub(super) fn build(p: &NodeRef<'_>, carry: &[OpenField], base_id: u32) -> TextMap {
    let mut b = Builder::new(carry, base_id);
    // Room for what the paragraph's markup suggests, so that the text and the items are rarely grown: a
    // paragraph's text is about a third of its bytes or less, and an item takes about fifty bytes of them.
    if let Some((start, end)) = p.source_span() {
        let bytes = end.saturating_sub(start);
        b.text.reserve(bytes / 3);
        b.items.reserve(bytes / 48 + 1);
    }
    let info = b.seed(p);
    b.stack.push(Frame {
        it: p.children(),
        info,
        kind: FrameKind::Content,
    });
    while let Some(top) = b.stack.last_mut() {
        let Some(c) = top.it.next() else {
            if let Some(done) = b.stack.pop() {
                b.exit(done);
            }
            continue;
        };
        let info = top.info;
        let in_run = matches!(top.kind, FrameKind::Run);
        if !c.is_element() {
            continue;
        }
        if in_run {
            b.run_child(&c, info);
        } else {
            b.content_child(&c, info);
        }
    }
    b.finish()
}

impl<'t> Builder<'t> {
    fn new(carry: &[OpenField], base_id: u32) -> Builder<'t> {
        let mut fields = Vec::new();
        let mut parent = None;
        for f in carry {
            fields.push(FieldRef {
                id: f.id,
                begin: f.begin,
                begins_here: false,
                separate: None,
                end: None,
                simple: f.simple,
                instr: String::new(),
                parent,
            });
            parent = Some(f.id);
        }
        Builder {
            text: String::new(),
            items: Vec::new(),
            contexts: Vec::new(),
            fields,
            anomalies: Vec::new(),
            chars: 0,
            state: FieldState::new(carry, base_id),
            stack: Vec::new(),
        }
    }

    /// The wrappers above the paragraph, up to the story root, outermost first, become the first
    /// contexts, and decide which modes include the paragraph.
    fn seed(&mut self, p: &NodeRef<'t>) -> Info {
        let mut info = Info {
            ctx: NO_CONTEXT,
            vis: Vis::BOTH,
            in_del: false,
            run: None,
        };
        for (kind, node) in wrappers_above(p) {
            info = self.wrap(info, kind, node);
        }
        info
    }

    /// A new context for a wrapper around what `info` describes, and what changes inside it.
    pub fn wrap(&mut self, info: Info, kind: WrapperKind, node: NodeId) -> Info {
        let at = u32::try_from(self.contexts.len()).unwrap_or(u32::MAX);
        self.contexts.push(Context {
            parent: (info.ctx != NO_CONTEXT).then_some(info.ctx),
            kind,
            node,
        });
        let mut vis = info.vis.0;
        if kind.is_insertion() {
            vis &= !Vis::ORIGINAL;
        }
        if kind.is_deletion() {
            vis &= !Vis::FINAL;
        }
        Info {
            ctx: at,
            vis: Vis(vis),
            in_del: info.in_del || kind == WrapperKind::Del,
            ..info
        }
    }

    pub fn anomaly(&mut self, kind: AnomalyKind, node: NodeId) {
        self.anomalies.push(Anomaly { kind, node });
    }

    /// Reads `children` in a frame inside a new wrapper.
    pub fn push_wrapper(
        &mut self,
        info: Info,
        kind: WrapperKind,
        node: &NodeRef<'t>,
        children: Children<'t>,
        frame: FrameKind,
    ) {
        let info = self.wrap(info, kind, node.id());
        self.stack.push(Frame {
            it: children,
            info,
            kind: frame,
        });
    }

    /// Adds something with characters (or none) to the paragraph. Anything that has characters and
    /// sits in a field's code is field code: it is in no text, and is kept as a mark.
    pub fn item(
        &mut self,
        kind: ItemKind,
        node: NodeId,
        info: Info,
        text: &str,
        from: u32,
        flags: u8,
    ) {
        let width = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
        self.item_of_width(kind, node, info, text, width, from, flags);
    }

    /// `item`, for a caller that has counted the characters of `text` already: `width` is their number.
    #[expect(
        clippy::too_many_arguments,
        reason = "the arguments of `item` and the width it would count"
    )]
    pub fn item_of_width(
        &mut self,
        kind: ItemKind,
        node: NodeId,
        info: Info,
        text: &str,
        width: u32,
        from: u32,
        flags: u8,
    ) {
        if width > 0 && self.state.in_code() {
            if kind == ItemKind::Text {
                self.anomaly(AnomalyKind::TextInFieldCode, node);
            }
            self.mark(ItemKind::FieldCode, node, info);
            return;
        }
        let byte = u32::try_from(self.text.len()).unwrap_or(u32::MAX);
        let in_result = width > 0 && self.state.in_field();
        self.text.push_str(text);
        self.items.push(Item {
            kind,
            node,
            run: info.run,
            at: self.chars,
            len: width,
            from,
            ctx: info.ctx,
            vis: info.vis,
            field: self.state.top_id(),
            flags: flags | if in_result { FIELD_RESULT } else { 0 },
            byte,
            blen: u32::try_from(text.len()).unwrap_or(u32::MAX),
        });
        self.chars = self.chars.saturating_add(width);
    }

    /// An atom: one character.
    pub fn atom(&mut self, kind: ItemKind, node: NodeId, info: Info) {
        let mut buf = [0u8; 4];
        let text = kind.sentinel().unwrap_or('\u{fffd}').encode_utf8(&mut buf);
        self.item(kind, node, info, text, 0, 0);
    }

    /// A mark: no width.
    pub fn mark(&mut self, kind: ItemKind, node: NodeId, info: Info) {
        self.item(kind, node, info, "", 0, 0);
    }

    /// The end of a frame: a `w:fldSimple` closes its field.
    fn exit(&mut self, frame: Frame<'t>) {
        let FrameKind::Simple {
            node,
            items_before,
            displayed,
        } = frame.kind
        else {
            return;
        };
        let empty = self
            .items
            .iter()
            .skip(items_before)
            .all(|i| i.kind.is_mark());
        if empty && displayed {
            self.atom(ItemKind::FieldValue, node, frame.info);
        }
        self.mark(ItemKind::FieldEnd, node, frame.info);
        self.pop_field(node);
    }

    /// Closes the innermost field and records on it where it ended in this paragraph.
    pub fn pop_field(&mut self, node: NodeId) {
        if let End::Closed(f) = self.state.end()
            && let Some(r) = self.field_ref(f.id)
        {
            r.end = Some(node);
        }
    }

    /// Field code cannot cross a paragraph, so a field still in its code phase here is closed, and
    /// counted. A field in its result phase stays open: a table of contents crosses many paragraphs.
    fn finish(mut self) -> TextMap {
        let has_revisions = self.items.iter().any(|i| i.len > 0 && i.vis != Vis::BOTH);
        let mut open_at_end = std::mem::take(&mut self.state.stack);
        let mut unterminated = Vec::new();
        open_at_end.retain(|f| {
            if f.phase == Phase::Code {
                unterminated.push(f.begin);
            }
            f.phase != Phase::Code
        });
        for begin in unterminated {
            self.anomaly(AnomalyKind::UnterminatedField, begin);
        }
        TextMap {
            text: self.text,
            items: self.items,
            contexts: self.contexts,
            fields: self.fields,
            anomalies: self.anomalies,
            has_revisions,
            open_at_end,
            began: self.state.began,
        }
    }

    /// The field with this id, as this paragraph saw it.
    pub fn field_ref(&mut self, id: u32) -> Option<&mut FieldRef> {
        self.fields.iter_mut().rev().find(|f| f.id == id)
    }

    /// Opens a field and notes it. `None` if it is too deep.
    pub fn open_field(&mut self, node: &NodeRef<'t>, simple: bool, instr: String) -> Option<u32> {
        let parent = self.state.stack.last().map(|f| f.id);
        let id = self.state.begin(node.id(), simple, instr.clone())?;
        self.fields.push(FieldRef {
            id,
            begin: node.id(),
            begins_here: true,
            separate: None,
            end: None,
            simple,
            instr: if simple { instr } else { String::new() },
            parent,
        });
        Some(id)
    }
}

/// The wrappers above a block, up to the story root, outermost first. Rows and cells are wrappers
/// when a tracked change marks them, so a paragraph in an inserted row has `RowIns` in its chain, and
/// one in a row marked as both inserted and deleted has both.
pub(super) fn wrappers_above(p: &NodeRef<'_>) -> Vec<(WrapperKind, NodeId)> {
    let mut chain = Vec::new();
    let mut cur = p.parent();
    while let Some(a) = cur {
        for kind in ancestor_wrappers(&a).into_iter().flatten() {
            chain.push((kind, a.id()));
        }
        cur = a.parent();
    }
    chain.reverse();
    chain
}

/// The wrappers an element is, if any. Cells and rows are wrappers when a tracked change marks
/// them, and one marked as both inserted and deleted is two wrappers, so that it is in neither
/// mode (`docs/design/wordml.md` 4.3).
pub(super) fn ancestor_wrappers(a: &NodeRef<'_>) -> [Option<WrapperKind>; 2] {
    if mc::is_branch(a) {
        return [Some(WrapperKind::Alternate), None];
    }
    if !a.is_element() || !is_word_ns(a.namespace()) {
        return [None, None];
    }
    let one = |kind| [Some(kind), None];
    match a.local_name() {
        "sdt" => one(WrapperKind::Control),
        "customXml" => one(WrapperKind::CustomXml),
        "smartTag" => one(WrapperKind::SmartTag),
        "hyperlink" => one(WrapperKind::Hyperlink),
        "fldSimple" => one(WrapperKind::FieldSimple),
        "dir" => one(WrapperKind::Dir),
        "bdo" => one(WrapperKind::Bdo),
        "ruby" => one(WrapperKind::Ruby),
        "ins" => one(WrapperKind::Ins),
        "del" => one(WrapperKind::Del),
        "moveFrom" => one(WrapperKind::MoveFrom),
        "moveTo" => one(WrapperKind::MoveTo),
        "tr" => property_revisions(
            a,
            ("trPr", &ROW_CONTENT),
            ["ins", "del"],
            [WrapperKind::RowIns, WrapperKind::RowDel],
        ),
        "tc" => property_revisions(
            a,
            ("tcPr", &CELL_CONTENT),
            ["cellIns", "cellDel"],
            [WrapperKind::CellIns, WrapperKind::CellDel],
        ),
        _ => [None, None],
    }
}

/// How many children of a property container are looked at for a revision. A row holds a handful of
/// properties, so the limit only matters for input built to make every paragraph of a row read a
/// container of 100,000.
pub(super) const PROPERTIES_LIMIT: usize = 64;

/// A row or a cell is inserted or deleted by a tracked change when its properties say so. Both can
/// be said at once.
fn property_revisions(
    holder: &NodeRef<'_>,
    (props, content): (&str, &[&str]),
    names: [&str; 2],
    kinds: [WrapperKind; 2],
) -> [Option<WrapperKind>; 2] {
    let Some(pr) = leading_child(holder, props, content) else {
        return [None, None];
    };
    let (mut inserted, mut deleted) = (false, false);
    for c in pr.child_elements().take(PROPERTIES_LIMIT) {
        inserted |= is_w(&c, names[0]);
        deleted |= is_w(&c, names[1]);
    }
    [inserted.then_some(kinds[0]), deleted.then_some(kinds[1])]
}

/// True if Word shows something for a field with this instruction when it has no stored result.
pub(super) fn displayed(instr: &str) -> bool {
    shows_without_result(&keyword(instr))
}
