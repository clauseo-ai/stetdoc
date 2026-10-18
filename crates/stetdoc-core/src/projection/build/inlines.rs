//! Items to inlines: the wrapper chain around each piece of a paragraph, built into a tree.
//!
//! One pass over the items of a paragraph's text map. Each piece (some characters, an atom, a mark, a
//! field) has a chain of wrappers around it, outermost first: the tracked changes, the external link, the
//! content control, and last the hidden or looks-hidden run. Pieces that share a prefix of the chain share
//! those wrappers, which is what makes the tree (design 3.8, steps 3 and 4). The wrappers that are not
//! presentation are transparent here, as they are in the text.

use super::cx::{StoryCx, is_sentinel};
use super::marks;
use super::warnings;
use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::projection::model::{
    Atom, ControlInfo, ControlState, HiddenReason, Inline, LinkTarget, LooksHiddenSignal, Mark,
    NoteKind, ObjectInfo, ObjectKind, RevKind, RevTag,
};
use crate::state::RunKey;
use crate::styles::{HiddenReason as StyleHidden, LooksHidden as StyleLooks};
use crate::wordml::{
    Branch, Context, Item, ItemKind, NO_CONTEXT, NO_FIELD, NodeId, ObjectKind as WordObject,
    Paragraph as WPara, RelTarget, RevKind as WordRev, TextBox, TextMap, WrapperKind, math_text,
};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

/// What was built for one paragraph.
pub(super) struct Built {
    /// The content.
    pub inlines: Vec<Inline>,
    /// How many pieces of text there are, and how many of them are in hidden runs.
    pub text_runs: u32,
    pub hidden_runs: u32,
    /// The first item of each run, for the formatting changes that a run holds.
    pub runs: Vec<Item>,
}

/// The identity of a wrapper, so that neighbours with the same wrapper share one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Node(NodeId),
    Hidden,
    Looks(LooksHiddenSignal),
}

/// What a wrapper becomes.
#[derive(Clone)]
enum Wrap {
    Rev(RevKind, RevTag),
    Link(LinkTarget),
    Control(ControlInfo, ControlState),
    Hidden,
    Looks(LooksHiddenSignal),
}

#[derive(Clone)]
pub(super) struct Link {
    key: Key,
    wrap: Wrap,
}

struct Frame {
    key: Key,
    wrap: Wrap,
    inlines: Vec<Inline>,
}

/// The open wrappers, outermost first, and the inlines outside all of them.
#[derive(Default)]
pub(super) struct Stack {
    frames: Vec<Frame>,
    root: Vec<Inline>,
}

impl Stack {
    /// Opens and closes wrappers until the open ones are exactly `chain`.
    pub(super) fn sync(&mut self, chain: &[Link]) {
        let mut common = 0;
        while let (Some(frame), Some(link)) = (self.frames.get(common), chain.get(common)) {
            if frame.key != link.key {
                break;
            }
            common += 1;
        }
        while self.frames.len() > common {
            self.close();
        }
        for link in chain.iter().skip(common) {
            self.frames.push(Frame {
                key: link.key,
                wrap: link.wrap.clone(),
                inlines: Vec::new(),
            });
        }
    }

    fn close(&mut self) {
        if let Some(frame) = self.frames.pop() {
            let inline = match frame.wrap {
                Wrap::Rev(kind, tag) => Inline::Rev {
                    kind,
                    tag,
                    inlines: frame.inlines,
                },
                Wrap::Link(target) => Inline::Link {
                    target,
                    inlines: frame.inlines,
                },
                Wrap::Control(info, state) => Inline::Control {
                    info,
                    state,
                    inlines: frame.inlines,
                },
                Wrap::Hidden => Inline::Hidden {
                    reason: HiddenReason::Vanish,
                    inlines: frame.inlines,
                },
                Wrap::Looks(signal) => Inline::LooksHidden {
                    signal,
                    inlines: frame.inlines,
                },
            };
            self.top().push(inline);
        }
    }

    fn top(&mut self) -> &mut Vec<Inline> {
        match self.frames.last_mut() {
            Some(frame) => &mut frame.inlines,
            None => &mut self.root,
        }
    }

    pub(super) fn text(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let top = self.top();
        if let Some(Inline::Text(last)) = top.last_mut() {
            last.push_str(s);
        } else {
            top.push(Inline::Text(s.to_string()));
        }
    }

    pub(super) fn leaf(&mut self, inline: Inline) {
        self.top().push(inline);
    }

    pub(super) fn finish(mut self) -> Vec<Inline> {
        while !self.frames.is_empty() {
            self.close();
        }
        self.root
    }
}

/// The chains already built for this paragraph, by the context that is innermost.
pub(super) type Chains = HashMap<u32, Rc<Vec<Link>>>;

/// True for the item kinds whose characters are text, as opposed to atoms and marks.
pub(super) fn is_textual(kind: ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Text
            | ItemKind::Tab
            | ItemKind::LineBreak
            | ItemKind::Hyphen
            | ItemKind::SoftHyphen
            | ItemKind::Symbol
    )
}

/// True if the text has a character of a complex script (Arabic, Hebrew, Thai, Hindi and their
/// neighbours), whose size is `szCs` and not `sz`.
fn is_complex_script(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c as u32,
            0x0590..=0x08FF | 0x0900..=0x0DFF | 0x0E00..=0x0FFF | 0x1000..=0x109F | 0x1780..=0x17FF
                | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF
        )
    })
}

/// The pieces of a paragraph's Redline text that each item holds, in item order.
pub(super) fn slices<'t>(text: &'t str, items: &[Item]) -> Vec<&'t str> {
    let mut out = Vec::with_capacity(items.len());
    let mut byte = 0usize;
    for item in items {
        if item.len == 0 {
            out.push("");
            continue;
        }
        let rest = text.get(byte..).unwrap_or_default();
        let end = rest
            .char_indices()
            .nth(item.len as usize)
            .map_or(rest.len(), |(i, _)| i);
        out.push(rest.get(..end).unwrap_or_default());
        byte += end;
    }
    out
}

/// What is known about the run the last text came from, so that a run is asked about once.
struct RunStatus {
    run: Option<NodeId>,
    hidden: bool,
    looks: Option<LooksHiddenSignal>,
}

/// The items of one field in one paragraph, and the text of each.
pub(super) struct Extent<'e> {
    pub outer: u32,
    pub items: &'e [Item],
    pub texts: &'e [&'e str],
}

/// The things one paragraph's build needs besides the story: its element and its ref text, for warnings.
pub(super) struct Para<'p> {
    pub node: NodeId,
    pub block: Option<&'p str>,
    /// The paragraph, which reads the ids of its marks and the wrappers of its text map.
    pub p: WPara<'p>,
}

impl<'a> StoryCx<'a> {
    /// The inlines of a paragraph, built from its text map.
    pub(super) fn inlines(&mut self, para: &Para<'_>, map: &TextMap) -> Result<Built, StetError> {
        let redline = map.text(Mode::Redline);
        let items = map.items();
        let texts = slices(&redline, items);
        let story = self.story.clone();
        let fields = story.fields(self.tree);
        let mut stack = Stack::default();
        let mut chains: Chains = HashMap::new();
        let mut status = RunStatus {
            run: None,
            hidden: false,
            looks: None,
        };
        let mut text_runs = 0u32;
        let mut hidden_runs = 0u32;
        let mut runs: Vec<Item> = Vec::new();
        let mut i = 0usize;
        while let Some(item) = items.get(i).copied() {
            if item.run.is_some() && runs.last().map(|r| r.run) != Some(item.run) {
                runs.push(item);
            }
            if item.field != NO_FIELD {
                let outer = outermost(fields, item.field);
                let mut j = i;
                while items
                    .get(j)
                    .is_some_and(|it| it.field != NO_FIELD && outermost(fields, it.field) == outer)
                {
                    j += 1;
                }
                let extent = items.get(i..j).unwrap_or_default();
                let pieces = texts.get(i..j).unwrap_or_default();
                let at = Extent {
                    outer,
                    items: extent,
                    texts: pieces,
                };
                self.field_extent(&mut stack, &mut chains, para, map, fields, &at);
                i = j;
                continue;
            }
            i += 1;
            let in_mode = item.vis.in_mode(self.mode);
            match item.kind {
                ItemKind::CommentStart | ItemKind::CommentEnd if in_mode => {
                    if let Some(id) = para.p.mark_id(&item) {
                        let chain = self.chain_of(para, map, &mut chains, item.ctx);
                        stack.sync(&chain);
                        stack.leaf(Inline::Mark(comment_mark(item.kind, id)));
                    }
                }
                kind if is_textual(kind) && in_mode => {
                    let text = texts.get(i - 1).copied().unwrap_or_default();
                    if text.is_empty() {
                        continue;
                    }
                    text_runs += 1;
                    if status.run != item.run || item.run.is_none() {
                        status = self.run_status(para, &item, text)?;
                    }
                    let mut chain = (*self.chain_of(para, map, &mut chains, item.ctx)).clone();
                    if status.hidden {
                        hidden_runs += 1;
                        chain.push(Link {
                            key: Key::Hidden,
                            wrap: Wrap::Hidden,
                        });
                    } else if let Some(signal) = status.looks {
                        chain.push(Link {
                            key: Key::Looks(signal),
                            wrap: Wrap::Looks(signal),
                        });
                    }
                    stack.sync(&chain);
                    stack.text(text);
                }
                kind if in_mode => {
                    if let Some(atom) = self.atom(para, kind, &item) {
                        let chain = self.chain_of(para, map, &mut chains, item.ctx);
                        stack.sync(&chain);
                        stack.leaf(Inline::Atom(atom));
                    }
                }
                _ => {}
            }
        }
        Ok(Built {
            inlines: stack.finish(),
            text_runs,
            hidden_runs,
            runs,
        })
    }

    /// Asks the cascade about the run that `item` is in, once, and raises the warnings it calls for.
    fn run_status(
        &mut self,
        para: &Para<'_>,
        item: &Item,
        text: &str,
    ) -> Result<RunStatus, StetError> {
        let key = RunKey {
            item,
            complex_script: is_complex_script(text),
        };
        let site = self.sites.site_for(para.node, Some(key))?;
        let hidden = matches!(self.cascade.hidden(&site), Some(StyleHidden::Vanish));
        let looks = if hidden {
            None
        } else {
            self.cascade.looks_hidden(&site).map(|l| match l {
                StyleLooks::TinyText => LooksHiddenSignal::Tiny,
                StyleLooks::WhiteText => LooksHiddenSignal::White,
            })
        };
        let story = self.story_label();
        let block = para.block.map(str::to_string);
        if hidden {
            self.warnings
                .push(warnings::hidden_text(story, block, HiddenReason::Vanish));
        } else if let Some(signal) = looks {
            self.warnings
                .push(warnings::looks_hidden(story, block, signal));
        }
        Ok(RunStatus {
            run: item.run,
            hidden,
            looks,
        })
    }

    /// The wrappers around items whose innermost wrapper is `ctx`, outermost first.
    pub(super) fn chain_of(
        &self,
        para: &Para<'_>,
        map: &TextMap,
        cache: &mut Chains,
        ctx: u32,
    ) -> Rc<Vec<Link>> {
        if ctx == NO_CONTEXT {
            return Rc::new(Vec::new());
        }
        if let Some(chain) = cache.get(&ctx) {
            return chain.clone();
        }
        let mut links: Vec<Link> = map
            .context(ctx)
            .filter_map(|c| self.link_for(para, c))
            .collect();
        links.reverse();
        let chain = Rc::new(links);
        cache.insert(ctx, chain.clone());
        chain
    }

    /// What one wrapper of the text map becomes, if it is presentation.
    fn link_for(&self, para: &Para<'_>, c: &Context) -> Option<Link> {
        match c.kind {
            WrapperKind::Ins | WrapperKind::Del | WrapperKind::MoveFrom | WrapperKind::MoveTo => {
                if self.mode != Mode::Redline {
                    return None;
                }
                let rev = para.p.revision_of(c)?;
                let kind = match rev.kind {
                    WordRev::Ins => RevKind::Ins,
                    WordRev::Del => RevKind::Del,
                    WordRev::MoveFrom => RevKind::MoveFrom,
                    WordRev::MoveTo => RevKind::MoveTo,
                    _ => return None,
                };
                Some(Link {
                    key: Key::Node(c.node),
                    wrap: Wrap::Rev(kind, self.tag_of(&rev)),
                })
            }
            WrapperKind::Hyperlink => {
                let link = para.p.hyperlink_of(c)?;
                let url = link
                    .r_id
                    .as_deref()
                    .and_then(|id| match &self.rels.get(id)?.target {
                        RelTarget::External(url) => Some(url.clone()),
                        _ => None,
                    })?;
                Some(Link {
                    key: Key::Node(c.node),
                    wrap: Wrap::Link(LinkTarget::External(url.into_boxed_str())),
                })
            }
            WrapperKind::Control => {
                if !self.is_inside(c.node, para.node) {
                    return None;
                }
                let found = para.p.control_of(c)?;
                let state = if found.lock.locks_content() {
                    ControlState::Locked
                } else if found.data_bound {
                    ControlState::DataBound
                } else if found.showing_placeholder {
                    ControlState::Placeholder
                } else {
                    ControlState::Editable
                };
                Some(Link {
                    key: Key::Node(c.node),
                    wrap: Wrap::Control(
                        ControlInfo {
                            kind: found.kind,
                            alias: found.alias.map(String::into_boxed_str),
                            tag: found.tag.map(String::into_boxed_str),
                        },
                        state,
                    ),
                })
            }
            _ => None,
        }
    }

    /// True if `inner` is `outer` or below it.
    fn is_inside(&self, inner: NodeId, outer: NodeId) -> bool {
        let mut cur = Some(self.tree.node_ref(inner));
        let mut steps = 0usize;
        while let Some(n) = cur {
            if n.id() == outer {
                return true;
            }
            steps += 1;
            if steps > 512 {
                return false;
            }
            cur = n.parent();
        }
        false
    }

    /// The tag a revision shows as: its id, who made it and when.
    pub(super) fn tag_of(&self, rev: &crate::wordml::Revision) -> RevTag {
        RevTag {
            id: rev.id.unwrap_or(0),
            author: self.facts.author_of(rev.author.as_deref()).unwrap_or(0),
            date: rev.date.as_deref().map(Arc::from),
            peer: None,
        }
    }

    /// What an item that is not text is, as an atom. `None` for the ones that have no place in the model.
    fn atom(&mut self, para: &Para<'_>, kind: ItemKind, item: &Item) -> Option<Atom> {
        Some(match kind {
            ItemKind::PageBreak => Atom::PageBreak,
            ItemKind::ColumnBreak => Atom::ColumnBreak,
            ItemKind::Object => Atom::Object(self.object(item)),
            ItemKind::NoteRef => {
                let kind = marks::note_kind(self.tree, item.node);
                let ordinal = para
                    .p
                    .mark_id(item)
                    .and_then(|id| self.facts.ordinal(kind, id))
                    .unwrap_or(0);
                Atom::NoteRef { kind, ordinal }
            }
            ItemKind::NoteMark => Atom::NoteMark {
                kind: match self.kind {
                    crate::wordml::StoryKind::Endnotes => NoteKind::Endnote,
                    _ => NoteKind::Footnote,
                },
            },
            ItemKind::Math => Atom::Math(math_text(self.tree, item.node).into_boxed_str()),
            ItemKind::Auto => Atom::Auto(marks::auto_kind(self.tree, item.node)),
            _ => return None,
        })
    }

    /// What a drawing, picture or object is and says: its kind, its alt text and the text of its text boxes
    /// in the branch Word reads.
    fn object(&self, item: &Item) -> ObjectInfo {
        let Some(found) = self.story.object_info(self.tree, item.node) else {
            return ObjectInfo {
                kind: ObjectKind::Object,
                alt: None,
                text_box: None,
            };
        };
        let lines: Vec<String> = found
            .text_boxes
            .iter()
            .filter(|b| b.branch != Branch::Untaken)
            .map(|b| self.text_box_text(b))
            .filter(|t| !t.is_empty())
            .collect();
        let text_box = (!lines.is_empty()).then(|| lines.join(" / ").into_boxed_str());
        let kind = match found.kind {
            WordObject::Picture => ObjectKind::Image,
            WordObject::Chart => ObjectKind::Chart,
            WordObject::Shape if text_box.is_some() => ObjectKind::TextBox,
            WordObject::Shape => ObjectKind::Shape,
            _ => ObjectKind::Object,
        };
        ObjectInfo {
            kind,
            alt: found.alt.map(String::into_boxed_str),
            text_box,
        }
    }

    /// The paragraphs of a text box, in this mode, joined by ` / `.
    fn text_box_text(&self, b: &TextBox) -> String {
        let lines: Vec<String> = b
            .blocks(self.tree)
            .filter_map(|block| match block {
                crate::wordml::Block::Paragraph(p) => Some(p.text(self.mode)),
                _ => None,
            })
            .map(|t| t.chars().filter(|c| !is_sentinel(*c)).collect::<String>())
            .filter(|t| !t.trim().is_empty())
            .collect();
        lines.join(" / ")
    }
}

/// The mark of one end of a comment's range.
pub(super) fn comment_mark(kind: ItemKind, id: i64) -> Mark {
    if kind == ItemKind::CommentStart {
        Mark::CommentStart(id)
    } else {
        Mark::CommentEnd(id)
    }
}

/// The outermost field that field `id` is nested in, or `id` itself.
pub(super) fn outermost(fields: &[crate::wordml::FieldInfo], id: u32) -> u32 {
    let mut cur = id;
    let mut guard = 0;
    while let Some(parent) = fields.get(cur as usize).and_then(|f| f.parent) {
        guard += 1;
        if guard > 64 {
            break;
        }
        cur = parent;
    }
    cur
}
