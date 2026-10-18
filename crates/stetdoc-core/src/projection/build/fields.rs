//! Fields: the items of one field in one paragraph become one `Field` inline.
//!
//! The extent of a field is the run of items whose outermost field is the same one: its code (which is
//! only marks), its result, and whatever fields are nested in either. The result is flattened into one
//! string, an atom inside it counts one character of width and none of text, and a zero-width comment mark
//! inside it is kept, after the field (design 3.4).

use super::cx::StoryCx;
use super::inlines::{Chains, Extent, Para, Stack, comment_mark, is_textual};
use crate::projection::model::{Field, Inline};
use crate::refs::BlockRef;
use crate::wordml::{FieldInfo, ItemKind, NO_CONTEXT, TextMap, Vis};

impl<'a> StoryCx<'a> {
    /// The block that a field points at, when it is a `REF`, `PAGEREF`, `NOTEREF` or `HYPERLINK \l` field whose
    /// bookmark is in a paragraph of the document. `None` for every other field, and for a bookmark that is
    /// gone, which Word shows as an error and the model shows with no target.
    fn field_target(&self, info: &FieldInfo) -> Option<BlockRef> {
        let name = crate::index::target_name(&info.instr)?;
        let marks = self.parts.bookmarks().ok()?;
        self.parts.ref_of(marks.block(&name)?)
    }

    /// Emits the field that `at` is the extent of, into the open wrappers of its first result item.
    pub(super) fn field_extent(
        &mut self,
        stack: &mut Stack,
        chains: &mut Chains,
        para: &Para<'_>,
        map: &TextMap,
        fields: &[FieldInfo],
        at: &Extent<'_>,
    ) {
        let Some(info) = fields.get(at.outer as usize) else {
            return;
        };
        let mut text = String::new();
        let mut width = 0u32;
        let (mut opens, mut closes, mut shown) = (false, false, false);
        let mut first: Option<usize> = None;
        let mut value = false;
        let mut comment_marks: Vec<usize> = Vec::new();
        for (k, it) in at.items.iter().enumerate() {
            let in_mode = it.vis.in_mode(self.mode);
            shown |= in_mode;
            let in_final = it.vis.0 & Vis::FINAL != 0;
            match it.kind {
                ItemKind::FieldBegin if it.field == at.outer => opens = true,
                ItemKind::FieldEnd if it.field == at.outer => closes = true,
                ItemKind::CommentStart | ItemKind::CommentEnd => comment_marks.push(k),
                ItemKind::FieldValue => {
                    value = true;
                    first.get_or_insert(k);
                    if in_final {
                        width += 1;
                    }
                }
                kind if kind.is_mark() => {}
                kind => {
                    first.get_or_insert(k);
                    if in_final {
                        width += it.len;
                    }
                    if in_mode && is_textual(kind) {
                        text.push_str(at.texts.get(k).copied().unwrap_or_default());
                    }
                }
            }
        }
        if !shown {
            return;
        }
        if value {
            text = info.display_text().unwrap_or_default();
        }
        let ctx = at
            .items
            .get(first.unwrap_or(0))
            .map_or(NO_CONTEXT, |it| it.ctx);
        let chain = self.chain_of(para, map, chains, ctx);
        stack.sync(&chain);
        stack.leaf(Inline::Field(Field {
            keyword: info.keyword.clone(),
            text: text.into_boxed_str(),
            width,
            target: self.field_target(info),
            opens,
            closes,
            dirty: info.dirty,
        }));
        for k in comment_marks {
            let Some(it) = at.items.get(k) else { continue };
            if !it.vis.in_mode(self.mode) {
                continue;
            }
            if let Some(id) = para.p.mark_id(it) {
                let chain = self.chain_of(para, map, chains, it.ctx);
                stack.sync(&chain);
                stack.leaf(Inline::Mark(comment_mark(it.kind, id)));
            }
        }
    }
}
