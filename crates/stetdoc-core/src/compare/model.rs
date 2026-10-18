//! The comparison model of one story: containers of blocks, each with its comparison text, key and word bag.
//!
//! A story is read through `wordml` into a tree of **containers** (the body, a header, a note, a cell), kept as a flat
//! arena so that a container is a number. A container holds paragraphs and tables, a table holds rows, a row holds
//! cells, and a cell is a container. Both documents have been accepted before the model is built, so a paragraph's
//! `Final` text is its text. The comparison text carries no clause number (the text model has none), and an atom (a
//! picture, an equation, a field with no stored result) is one character from a private-use dictionary shared by the
//! two sides, keyed by the atom's identity, so that a replaced picture is a difference and an unchanged one is not
//! (design `compare.md`, 3).

use super::align::{Block, Kind};
use super::text::{CELL_SEP, hash_bytes};
use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::state::{PartId, PartsView};
use crate::wordml::{
    Block as WBlock, ItemKind, NodeId, Paragraph, RelTarget, Rels, SpanTree, Story, StoryKind,
    Table,
};
use crate::xml::NodeRef;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Tables nested deeper than this are read as one block of text, so that a hostile document cannot make the
/// model recurse without bound.
const MAX_DEPTH: usize = 32;

/// The private-use characters that stand for atoms, and the identities they were given. One dictionary serves both
/// sides of a comparison, so that equal atoms get the same character.
#[derive(Default)]
pub(super) struct Atoms {
    ids: BTreeMap<u64, char>,
}

impl Atoms {
    /// The character that stands for the atom with this identity.
    pub(super) fn char_of(&mut self, identity: u64) -> char {
        let next = self.ids.len();
        *self.ids.entry(identity).or_insert_with(|| {
            let n = u32::try_from(next).unwrap_or(u32::MAX);
            let code = if n < 0x1800 {
                0xE000 + n
            } else {
                0xF_0000u32.saturating_add(n - 0x1800)
            };
            char::from_u32(code).unwrap_or('\u{fffd}')
        })
    }
}

/// What a paragraph carries that the plan needs besides its text.
#[derive(Clone, Debug, Default)]
pub(super) struct ParaInfo {
    /// The comparison text, with atoms replaced one for one.
    pub text: String,
    /// True if the paragraph holds a section break.
    pub section: bool,
    /// The number of the group of consecutive paragraphs that one field spans (a TOC), if the paragraph is in one.
    pub group: Option<u32>,
    /// The note references in the paragraph: the offset in the text, the note's `w:id`, and whether it is an endnote.
    pub notes: Vec<(usize, i64, bool)>,
    /// The character ranges of the fields that start and end in this paragraph and have characters: an edit never cuts one.
    pub fields: Vec<std::ops::Range<usize>>,
    /// True if the paragraph holds markup-compatibility content (`mc:AlternateContent`), which only a deleted copy of the whole
    /// paragraph carries.
    pub alternate: bool,
}

/// A row of a table.
#[derive(Clone, Debug)]
pub(super) struct RowInfo {
    /// The `w:tr` element.
    pub id: NodeId,
    /// The row as the aligner sees it: its cells' texts joined by U+001F.
    pub block: Block,
    /// The grid span and the vertical merge of each cell, which must agree for two rows to compare cell by cell.
    pub shape: Vec<(u32, u8)>,
    /// The container of each cell.
    pub cells: Vec<usize>,
}

/// What a block is, apart from its element.
#[derive(Clone, Debug)]
pub(super) enum Body {
    /// A paragraph.
    Para(ParaInfo),
    /// A table, and its rows.
    Table(Vec<RowInfo>),
}

/// One block of a container.
#[derive(Clone, Debug)]
pub(super) struct Node {
    /// The `w:p` or `w:tbl` element.
    pub id: NodeId,
    /// The block as the aligner sees it.
    pub block: Block,
    /// What it is.
    pub body: Body,
}

/// A container: the blocks of a body, a header, a note or a cell, in order.
#[derive(Clone, Debug, Default)]
pub(super) struct Container {
    /// The blocks.
    pub blocks: Vec<Node>,
}

impl Container {
    /// The text of the container for the text of a cell or a table: the paragraphs' texts joined by a line break.
    pub(super) fn flat_text(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for n in &self.blocks {
            match &n.body {
                Body::Para(p) => parts.push(p.text.clone()),
                Body::Table(_) => parts.push(n.block.norm.clone()),
            }
        }
        parts.join("\n")
    }
}

/// The containers of one story, and where its roots are.
#[derive(Debug, Default)]
pub(super) struct StoryModel {
    /// Every container of the story, the roots first in the order they were read.
    pub containers: Vec<Container>,
    /// The roots: one for a body, a header or a footer, and one for each note of a notes part.
    pub roots: Vec<Root>,
}

/// A root container: the whole story, or one note.
#[derive(Clone, Copy, Debug)]
pub(super) struct Root {
    /// The note's `w:id`, or `None` for a body, header or footer.
    pub note: Option<i64>,
    /// The container.
    pub container: usize,
}

/// Reads the story of `part` into a model. `atoms` is the dictionary shared with the other document.
pub(super) fn story_model(
    view: &PartsView<'_>,
    part: PartId,
    story: &Story,
    atoms: &mut Atoms,
) -> Result<StoryModel, StetError> {
    let tree = view.tree(part)?;
    let rels = view.rels(part)?;
    let mut reader = Reader {
        view,
        tree,
        rels: &rels,
        atoms,
        out: StoryModel::default(),
        spans: field_spans(tree, story),
    };
    match story.kind() {
        StoryKind::Body => {
            let body = tree
                .root()
                .child_elements()
                .find(|c| c.local_name() == "body");
            if let Some(body) = body {
                reader.root(None, body, 0);
            }
        }
        StoryKind::Header | StoryKind::Footer => reader.root(None, tree.root(), 0),
        StoryKind::Footnotes | StoryKind::Endnotes => {
            for unit in story.units() {
                reader.root(unit.id, tree.node_ref(unit.node), 0);
            }
        }
        StoryKind::Comments => {}
    }
    Ok(reader.out)
}

/// The character ranges of the fields that start and end in the paragraph: the characters of every item inside the field,
/// the fields it contains included. A field with no characters has no range.
fn field_ranges(map: &crate::wordml::TextMap) -> Vec<std::ops::Range<usize>> {
    use crate::wordml::NO_FIELD;
    let fields = map.fields();
    let mut spans: Vec<Option<std::ops::Range<usize>>> = vec![None; fields.len()];
    for item in map.items() {
        if item.len == 0 || item.field == NO_FIELD || !item.vis.in_mode(Mode::Final) {
            continue;
        }
        let (start, end) = (item.at as usize, (item.at + item.len) as usize);
        let mut cur = Some(item.field);
        let mut guard = 0;
        while let Some(id) = cur {
            guard += 1;
            let Some(at) = fields.iter().position(|f| f.id == id) else {
                break;
            };
            if let Some(slot) = spans.get_mut(at) {
                *slot = Some(match slot.take() {
                    Some(r) => r.start.min(start)..r.end.max(end),
                    None => start..end,
                });
            }
            cur = fields.get(at).and_then(|f| f.parent);
            if guard > fields.len() {
                break;
            }
        }
    }
    fields
        .iter()
        .zip(spans)
        .filter(|(f, _)| f.begins_here && f.end.is_some())
        .filter_map(|(_, r)| r)
        .collect()
}

/// The paragraph that holds an element: its nearest `w:p` ancestor.
fn paragraph_of(tree: &SpanTree, node: NodeId) -> Option<NodeId> {
    let mut cur = Some(tree.node_ref(node));
    while let Some(n) = cur {
        if n.is_element() && n.local_name() == "p" {
            return Some(n.id());
        }
        cur = n.parent();
    }
    None
}

/// The fields of the story whose start and end are in different paragraphs, as the two paragraphs.
fn field_spans(tree: &SpanTree, story: &Story) -> Vec<(NodeId, NodeId)> {
    story
        .fields(tree)
        .iter()
        .filter_map(|f| {
            let (b, e) = (paragraph_of(tree, f.begin)?, paragraph_of(tree, f.end?)?);
            (b != e).then_some((b, e))
        })
        .collect()
}

/// The state of reading one story.
struct Reader<'v, 'a> {
    view: &'v PartsView<'a>,
    tree: &'a SpanTree,
    rels: &'v Rels,
    atoms: &'v mut Atoms,
    out: StoryModel,
    /// The paragraphs where each field that spans paragraphs starts and ends.
    spans: Vec<(NodeId, NodeId)>,
}

impl<'a> Reader<'_, 'a> {
    fn root(&mut self, note: Option<i64>, element: NodeRef<'a>, depth: usize) {
        let container = self.container(element, depth);
        self.out.roots.push(Root { note, container });
    }

    /// Reads the blocks of `element` into a new container and returns its number.
    fn container(&mut self, element: NodeRef<'a>, depth: usize) -> usize {
        let at = self.out.containers.len();
        self.out.containers.push(Container::default());
        let mut blocks = Vec::new();
        for block in WBlock::within(self.tree, element) {
            match block {
                WBlock::Paragraph(p) => {
                    let info = self.paragraph(&p);
                    let norm = Block::new(Kind::Paragraph, &info.text);
                    blocks.push(Node {
                        id: p.id(),
                        block: norm,
                        body: Body::Para(info),
                    });
                }
                WBlock::Table(t) => blocks.push(self.table(&t, depth)),
                WBlock::Row(_) => {}
            }
        }
        self.mark_groups(&mut blocks);
        if let Some(c) = self.out.containers.get_mut(at) {
            c.blocks = blocks;
        }
        at
    }

    /// Gives the paragraphs that one field spans a group number: from the paragraph of its start to the paragraph of its end,
    /// with overlapping spans one group.
    fn mark_groups(&self, blocks: &mut [Node]) {
        let index_of = |id: NodeId| blocks.iter().position(|n| n.id == id);
        let mut ranges: Vec<(usize, usize)> = self
            .spans
            .iter()
            .filter_map(|&(b, e)| Some((index_of(b)?, index_of(e)?)))
            .filter(|(b, e)| b < e)
            .collect();
        ranges.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (b, e) in ranges {
            match merged.last_mut() {
                Some(last) if b <= last.1 => last.1 = last.1.max(e),
                _ => merged.push((b, e)),
            }
        }
        for (g, (b, e)) in merged.into_iter().enumerate() {
            for n in blocks.iter_mut().skip(b).take(e + 1 - b) {
                if let Body::Para(p) = &mut n.body {
                    p.group = u32::try_from(g + 1).ok();
                }
            }
        }
    }

    fn table(&mut self, table: &Table<'a>, depth: usize) -> Node {
        let mut rows = Vec::new();
        let mut table_text: Vec<String> = Vec::new();
        if depth < MAX_DEPTH {
            for row in table.rows() {
                let mut cells = Vec::new();
                let mut shape = Vec::new();
                let mut texts = Vec::new();
                for cell in row.cells() {
                    let cid = self.container(cell.element(), depth + 1);
                    let text = self
                        .out
                        .containers
                        .get(cid)
                        .map(Container::flat_text)
                        .unwrap_or_default();
                    texts.push(text.replace('\n', " "));
                    cells.push(cid);
                    shape.push((cell.grid_span(), v_merge_code(cell.v_merge())));
                }
                let text = texts.join(&CELL_SEP.to_string());
                table_text.push(text.clone());
                rows.push(RowInfo {
                    id: row.id(),
                    block: Block::new(Kind::Row, &text),
                    shape,
                    cells,
                });
            }
        } else {
            table_text.push(table.element().text_content());
        }
        Node {
            id: table.id(),
            block: Block::new(Kind::Table, &table_text.join("\n")),
            body: Body::Table(rows),
        }
    }

    /// The text of a paragraph and what goes with it.
    fn paragraph(&mut self, p: &Paragraph<'a>) -> ParaInfo {
        let map = p.map();
        let text = map.text(Mode::Final).into_owned();
        let mut chars: Vec<char> = text.chars().collect();
        let mut notes = Vec::new();
        for item in map.items() {
            if item.len != 1 || !item.vis.in_mode(Mode::Final) {
                continue;
            }
            let at = item.at as usize;
            match item.kind {
                ItemKind::Object | ItemKind::Math | ItemKind::FieldValue => {
                    let identity = self.identity(item.node);
                    let c = self.atoms.char_of(identity);
                    if let Some(slot) = chars.get_mut(at) {
                        *slot = c;
                    }
                }
                ItemKind::NoteRef => {
                    let element = self.tree.node_ref(item.node);
                    let id = element
                        .attrs()
                        .find(|a| a.local_name() == "id")
                        .and_then(|a| a.value().trim().parse::<i64>().ok());
                    let endnote = element.local_name() == "endnoteReference";
                    if let Some(id) = id {
                        notes.push((at, id, endnote));
                    }
                    // a footnote and an endnote are not the same mark
                    if endnote {
                        let c = self.atoms.char_of(0x0065_6e64_6e6f_7465);
                        if let Some(slot) = chars.get_mut(at) {
                            *slot = c;
                        }
                    }
                }
                _ => {}
            }
        }
        ParaInfo {
            text: chars.into_iter().collect(),
            section: p.section_break().is_some(),
            group: None,
            notes,
            fields: field_ranges(&map),
            alternate: holds_alternate(&self.tree.node_ref(p.id())),
        }
    }

    /// The identity of an atom: a hash of its element with the ids and names left out and the media it names read
    /// by content, so two packages that hold the same picture give the same value.
    fn identity(&self, node: NodeId) -> u64 {
        let mut canon = String::new();
        let mut stack = vec![self.tree.node_ref(node)];
        while let Some(n) = stack.pop() {
            if n.is_element() {
                canon.push('<');
                canon.push_str(n.namespace());
                canon.push('|');
                canon.push_str(n.local_name());
                let mut attrs: Vec<(String, String, String)> = Vec::new();
                for a in n.attrs() {
                    if a.is_namespace_declaration() {
                        continue;
                    }
                    let local = a.local_name();
                    let value = if a.namespace().ends_with("/relationships") {
                        format!("#{:x}", self.media(&a.value()))
                    } else if matches!(
                        local,
                        "id" | "name" | "paraId" | "textId" | "anchorId" | "editId"
                    ) {
                        continue;
                    } else {
                        a.value().into_owned()
                    };
                    attrs.push((a.namespace().to_owned(), local.to_owned(), value));
                }
                attrs.sort();
                for (ns, local, value) in attrs {
                    canon.push(' ');
                    canon.push_str(&ns);
                    canon.push('|');
                    canon.push_str(&local);
                    canon.push('=');
                    canon.push_str(&value);
                }
                canon.push('>');
            } else {
                canon.push_str(&n.text());
            }
            let mark = stack.len();
            stack.extend(n.children());
            if let Some(kids) = stack.get_mut(mark..) {
                kids.reverse();
            }
        }
        hash_bytes(canon.as_bytes())
    }

    /// A hash of what a relationship id names: the bytes of the part, or the external target.
    fn media(&self, rid: &str) -> u64 {
        let Some(rel) = self.rels.get(rid) else {
            return 0;
        };
        match &rel.target {
            RelTarget::Part(name) => self
                .view
                .part_bytes(name)
                .ok()
                .flatten()
                .map_or(0, |b| hash_bytes(&b)),
            RelTarget::External(t) => hash_bytes(t.as_bytes()),
            RelTarget::Invalid => 0,
        }
    }
}

fn v_merge_code(v: crate::wordml::VMerge) -> u8 {
    match v {
        crate::wordml::VMerge::None => 0,
        crate::wordml::VMerge::Restart => 1,
        crate::wordml::VMerge::Continue => 2,
    }
}

/// The root container of a story model that is not a note part, or the one of a given note.
pub(super) fn root_of(model: &StoryModel, note: Option<i64>) -> Option<usize> {
    model
        .roots
        .iter()
        .find(|r| r.note == note)
        .map(|r| r.container)
}

/// A shared handle to the part name, for the models that need to say where a block lives.
pub(super) type PartName = Arc<str>;

/// A canonical form of an element: its name, its attributes sorted, and its children's forms in order.
fn canon(n: &NodeRef<'_>) -> String {
    let mut attrs: Vec<String> = n
        .attrs()
        .filter(|a| !a.is_namespace_declaration())
        .map(|a| format!("{}|{}={}", a.namespace(), a.local_name(), a.value()))
        .collect();
    attrs.sort();
    let kids: String = n.child_elements().map(|c| canon(&c)).collect();
    format!(
        "<{}|{} {}>{kids}",
        n.namespace(),
        n.local_name(),
        attrs.join(" ")
    )
}

/// True if an element holds an `mc:AlternateContent` anywhere below it.
fn holds_alternate(n: &NodeRef<'_>) -> bool {
    let mut stack = vec![*n];
    while let Some(e) = stack.pop() {
        if e.local_name() == "AlternateContent"
            && e.namespace() == "http://schemas.openxmlformats.org/markup-compatibility/2006"
        {
            return true;
        }
        stack.extend(e.child_elements());
    }
    false
}
