# Design: `wordml`

Design notes for the `wordml` module, which says what WordprocessingML
means. `ARCHITECTURE.md` is the map and the contract in `wordml/mod.rs` is the interface; this
document explains why the module is shaped as it is, what real documents ask of it, and how it is
tested. Terms follow [`GLOSSARY.md`](../../GLOSSARY.md): story, block, ref, text range, mode,
revision, field, content control. "Span" means a byte span in the span tree, never a range of text.
`refs` and `state` have their own notes in
[`refs-and-state.md`](refs-and-state.md). Where the two meet (block identity, fingerprints, story
discovery, caches, versions) both documents say the same thing in the same words, and the wording
that was agreed is marked **Agreed with `state`** below.

This is a design only. Nothing here is built yet, and the build order is in section 11. It was
revised once after an independent review (12.1 lists what changed).

This document covers:

1. what real documents contain, from the corpus
2. the interface at a glance
3. stories and blocks, and the seams agreed with `state`
4. the text model: what every inline element becomes
5. the character-to-XML map, its invariants and its operations
6. style and numbering definitions
7. reading revisions
8. how `open` stays within 300 ms, and what an edit costs
9. strange input
10. how it will be tested, and the independent oracle
11. build order
12. changes to other documents, what the review changed, and open questions

## The decisions that matter most

1. **One list of items per paragraph, and the modes are masks over it** (4, 5). Every piece of a
   paragraph is an item with its XML node, its run, its chain of wrappers, its field and which of
   `Final` and `Original` contain it. Nothing is flattened, so a revision inside a revision, a field
   under a different wrapper from its end, and a range across a hyperlink all have exact answers.
   The map is the same structure: `locate`, `gap` and `offset_of` turn text offsets into XML positions
   and back, in characters, in terms of what `Editor::replace_text_range` takes.
2. **Block identity is a node id in a part, enumerated by one scan** (3.3). `BlockKey` is
   (part, `NodeId`), `Story::scan` lists blocks in pre-order, and what is persisted is the ordinal
   and a frozen fingerprint, with `SCAN_VERSION` and `FINGERPRINT_VERSION` that change only in a major
   release. Agreed with `state`, word for word.
3. **Fields are carried across paragraphs, lazily and incrementally** (4.4, 8): 19 curated fields span
   paragraphs, up to 61. Field code is not text: it is a zero-width mark and in no mode. A field
   with no result that Word still shows (`MACROBUTTON`, `EQ`, `PAGE`) is one read-only atom.
4. **Atoms are one character each, using characters XML cannot carry** (4.1), so a tab, a page break,
   a footnote reference or a drawing has a position and can never be confused with text.
5. **Reading is total** (9): one error, at the scan of the body, anomalies for everything else, no
   panics, linear time.
6. **The text model gets direct unit tests, and an independent oracle** (10): pandoc in all three
   modes agrees with the reference reading on all 399 readable documents of a 400-document LAB
   sample, and a table of named divergence classes covers the curated set.
7. **Branches of an `mc:AlternateContent` are read by the one rule in `schema`** (4.5), and the
   revisions in both branches are reported, each marked with its branch (7).

## 1. What real documents contain

Two populations were counted with a throwaway script (lxml over the XML, not committed). **Curated**
is the 243 documents of the curated set that are expected to open and are not Harvey LAB. It was
chosen to cover features, so it over-represents rare shapes, and its counts show which shapes
exist, not how often. For frequency, a census of the wider corpus
counts documents over the 7,039 non-LAB documents: tracked insertions 254 and deletions 270, moves
35, property changes 116, comments 253, content controls 773, complex fields 1,056, direct list
numbering 993, text boxes 623, `mc:AlternateContent` 774, Strict OOXML 73. **LAB** is a sample of
2,801 Harvey LAB documents (every 15th in path order), all written by python-docx, with 1.87
million paragraphs.

Counts are occurrences, and documents in brackets.

| Where | Curated | LAB sample |
|---|---|---|
| Paragraphs by parent: body | 15,900 (236) | 938,175 (2,800) |
| table cell | 4,997 (82) | 924,300 (2,468) |
| header, footer | 214 (70), 173 (68) | 2,541 (2,541), 5,583 (2,795) |
| footnote, endnote, comment | 241 (72), 189 (69), 112 (13) | 12 (6), 0, 0 |
| text box content | 220 (20) | 0 |
| block-level content control | 155 (22) | 0 |
| directly under another `w:p` (not valid) | 2 (1) | 0 |
| Runs per paragraph: none, 1, 2 to 5, 6 to 20, 21 to 100, over 100 | 4,334, 12,231, 3,288, 1,700, 656, 3 | 30,359, 716,473, 1,075,976, 47,104, 687, 12 |
| Largest paragraph | 114 runs, 702 nodes | 213 runs, 1,536 nodes |

Paragraphs are small. A paragraph almost never needs more than a few hundred nodes, so any
algorithm that is linear in one paragraph is cheap, and one that is quadratic in a paragraph is
still fine. The quantities that grow are blocks per part (up to about 16,000) and paragraphs per
table.

**Where runs sit inside a paragraph** (curated; LAB has only the first row, plus 4,971 runs in
`w:del` and 5,584 in `w:ins` in 51 documents):

| Between `w:p` and `w:r` | Runs (documents) |
|---|---|
| nothing | 59,756 (234) |
| `w:hyperlink` | 1,042 (33) |
| `w:ins` | 169 (16) |
| `w:del` | 338 (15) |
| `w:sdt` | 113 (22) |
| `w:fldSimple` | 96 (18) |
| `w:smartTag`, and `smartTag` inside `smartTag` | 20 (2), 30 (2) |
| `w:moveFrom`, `w:moveTo` | 7 (5), 5 (4) |
| `w:customXml` | 7 (1) |
| `w:sdt` inside `w:sdt`, `sdt` then `ins`, `sdt` then `fldSimple` | 18 (1), 1 (1), 1 (1) |
| **revision inside revision:** `ins` then `del`; `moveFrom` then `del`; `moveTo` then `del`; `moveFrom` then `ins` | 1 (1), 18 (1), 19 (2), 1 (1) |
| `hyperlink` then `del` | 8 (1) |
| `mc:AlternateContent` (`Choice`, `Fallback`) | 1 (1), 1 (1) |
| `w:ruby` (base, annotation) | 1 (1) |

Revision wrappers nest in revision wrappers in real documents, and the synthetic fixtures
include a deletion nested in another author's insertion. A model that has one "revision"
slot per piece is wrong for these.

**What a run holds** (curated; LAB in brackets where it differs): `w:t` 55,440 (LAB 2,495,567),
`w:tab` 1,291, `w:br` 534 (page 72, column 9, line 453; LAB 24,939, almost all page),
`w:fldChar` 2,504 (LAB 19,107), `w:instrText` 1,018, `w:pict` 461, `w:drawing` 177, `w:object`
15, `w:sym` 20, `w:noBreakHyphen` 39, `w:softHyphen` 11, `w:cr` 7, `w:ptab` 1,
`w:footnoteReference` 27, `w:endnoteReference` 11, `w:footnoteRef` 37, `w:endnoteRef` 23,
`w:commentReference` 108, `w:annotationRef` 104, `w:delText` 202, `w:delInstrText` 4,
`w:lastRenderedPageBreak` 276 (32 documents, zero width), `w:pgNum` 3, the six legacy date
elements 1 each, `mc:AlternateContent` inside a run 104 (36). Paragraph children that are not
runs: `w:bookmarkStart` 760 (83) and `w:bookmarkEnd` 732, `w:proofErr` 750 (26), `w:commentRangeStart`
108 (12) and `w:commentRangeEnd` 99, `w:hyperlink` 197 (33), `w:sdt` 95 (23), `w:fldSimple` 93 (19),
`m:oMathPara` 136 (13), `m:oMath` 17 (7), `w:smartTag` 31 (3), `w:customXml` 4 (1), the eight
`customXml*Range` markers 1 each, `w:permStart` 1, `w:moveFromRangeStart/End` and `w:moveToRangeStart/End` 4 to 6.

**Fields.** 855 complex fields in 67 curated documents, and 93 `w:fldSimple`. Nineteen complex fields
(12 documents) begin in one paragraph and end in a later one, up to 61 paragraphs apart (a table
of contents). Sixty-three have no `separate`: 33 of them show something in Word (`EQ` 11, `MACROBUTTON` 9, `LISTNUM` 4, `SYMBOL` 4, `PAGE` 2, `GOTOBUTTON` 1, `QUOTE` 1, `BARCODE` 1) and the rest show nothing (`IF` 16, `REF` 4, `XE` 4, `TA` 1 and a few more). Three have `begin` and `end` under different
wrappers (the `begin` inside a `w:ins`, the `end` outside). Nesting goes three deep. One field is
never closed. There is no `end` without a `begin` and no `separate` without a `begin`. In LAB there
are 6,369 fields in 2,795 documents, all `PAGE` and similar in headers and footers, none spanning
paragraphs. `w:fldChar w:dirty` appears 149 times in one document.

**Revisions.** Paragraph-mark revisions in `w:pPr/w:rPr`: `w:ins` 121 (7 documents), `w:del` 179
(6), `w:moveFrom` 1, `w:moveTo` 1. Property-change elements: `w:rPrChange` 476 (8), `w:pPrChange`
103 (6). `w:date` is absent from 269 revision elements in 4 documents, so it is optional. Row
revisions: `w:trPr/w:ins` 11 (2), `w:trPr/w:del` 33 (3); cell: `w:cellIns` 1, `w:cellDel` 1,
`w:cellMerge` 3. A paragraph can sit directly inside `w:moveFrom` and `w:moveTo` at body level
(one document each): not valid, but present. 121 paragraphs in 3 documents sit in rows marked by `w:trPr/w:ins` or `w:del`, with runs that carry no wrapper of their own. A tracked change inside a text box is written in both the `mc:Choice` and the `mc:Fallback` copy, with distinct ids (`Comment042.docx` has 6 and 6).

**Blocks.** Body children: `w:p`, `w:tbl` 186 (78), `w:sdt` 46 (16), `w:altChunk` 12 (4),
`w:sectPr` 229, and one each of `mc:AlternateContent`, `w:moveFrom`, `w:moveTo`, `w:permEnd`, an
unknown `x:block`. Tables nest up to four levels (a table with three table ancestors). A `w:tc` has `w:sdt` children 12 (4), a `w:tr` has
`w:sdt` children 4 (2) and `w:tblPrEx` 321 (11), a `w:tbl` has `w:sdt` children 2 (1). Five cells
(3 documents) do not end with a paragraph, and one table sits inside a paragraph. Content controls
sit in a paragraph 83 times, in the body 46, in another content control 25, in a cell 18, in a
text box 14, in a row 4, in a table 2. Properties that matter: `w:showingPlcHdr` 75 (11),
`w:dataBinding` 68 (12), `w:text` 95 (14), `w14:checkbox` 11 (7), `w:comboBox` 8, `w:date` 3,
`w:dropDownList` 2, `w:richText` 4, `w14:repeatingSection` 3.

**Sections, text boxes, drawings.** 699 `w:sectPr` in 241 documents, 467 of them inside a
paragraph (25 documents); `w:titlePg` 24 (20 documents), `w:pgNumType` 240 (67), `w:lnNumType` 5.
Headers 178 parts in 71 documents, footers 131 in 68. `w:txbxContent` 151 in 22 documents:
69 paragraphs sit in an `mc:Choice`, 70 in the matching `mc:Fallback` (the same text twice), 120
outside any `mc:AlternateContent`; depth one only; two Strict documents use the Word 2006
namespace `http://schemas.microsoft.com/office/word/2006/wordml` for it. `w:drawing` 303 (85):
inline 185, anchored 118; picture 119, shape 82, chart 53, chartex 24, diagram 11, group 7, ink 3.
`mc:AlternateContent` 122 (38), always `Choice` plus `Fallback`; `Requires` is `wps` 65, `cx1` 17,
`wp14` 15, `wpg` 9, and a handful of others.

**Characters.** `w:t` with leading or trailing whitespace: with `xml:space="preserve"` 11,057 (111
documents; LAB 335,605 in all 2,801); without it 9,501 (15 documents). Of those without it, most
are non-breaking spaces (not XML whitespace), and one document, a Council of the EU draft whose
`app.xml` says Word 2007, has 9,409 of them (7,549 are a single space). Section 4.7 says what we do. A
literal tab or newline inside `w:t`: 25 (4 documents). Empty `w:t`: 626 (11 documents; LAB 500 in 64). `w:t` inside
`w:del`: 7 (5). `w:delText` outside `w:del`: 1. There is no `w:t` with element children.

**Definitions.** Styles: 7,083 in 229 documents (up to 561 in one), LAB 164 per document; `basedOn`
chains up to nine deep; 48 `basedOn` ids with no style (10 documents) and 30 duplicate style ids
(5 documents); no cycle in the corpus. Every document has `docDefaults` and `latentStyles` (up to
377 entries). Numbering: 2,268 abstract definitions in 95 documents (one document has 1,432), 2,414
`w:num`, 1,815 `w:lvlOverride` (20 documents) nearly all `w:startOverride`; `w:multiLevelType`
hybrid 1,929, multilevel 157, single 120; `w:isLgl` 173 (3 documents); `w:lvlRestart` 7;
`w:suff` 1,777; `w:pStyle` links 207; `w:numStyleLink` 2 and `w:styleLink` 9; one document each with
a duplicate `abstractNumId` and a duplicate `numId`; about 80 distinct `w:numFmt` values, almost
all from one comprehensive test document. 812 levels have no `w:start`, 21 of them counting levels.

What follows from this:

- Revision wrappers, hyperlinks, content controls, smart tags and fields nest freely, so the
  model keeps the wrapper chain of every piece of text and does not flatten it.
- Fields cross paragraphs and cross wrappers. Field state has to be carried from paragraph to
  paragraph, in document order, independent of the wrappers.
- Text boxes duplicate their content under `mc:Choice` and `mc:Fallback`, so the model reads
  exactly one branch, by a rule that agrees with the one `schema` uses.
- Documents are not clean: the reader has to be total, and report what it skipped.

## 2. The interface at a glance

```rust
// identity, discovery and versions (shared with `refs` and `state`, section 3)
pub use crate::xml::NodeId;                                  // opaque handle, re-exported
pub struct BlockKey { part: Arc<str>, node: NodeId }          // new, part(), node(); Clone, Eq, Hash; no Ord
pub enum BlockKind { Paragraph, Table, Row }
pub enum StoryKind { Body, Header, Footer, Footnotes, Endnotes, Comments }
pub struct Rels; pub struct Rel { id, rel_type, target: RelTarget }  // plain data from `state`
pub struct StorySpec { part: Arc<str>, kind: StoryKind }
pub fn stories(main_part: &str, main: &SpanTree, rels: &Rels) -> Vec<StorySpec>;
pub const SCAN_VERSION: u32 = 1;          // changes only in a major release
pub const FINGERPRINT_VERSION: u32 = 1;   // changes only in a major release
pub enum IdKind { ParaId, TextId, CommentDurableId, AnnotationId }
pub fn existing_ids(kind: IdKind, tree: &SpanTree, out: &mut Vec<u32>);      // read-only; state mints
pub fn id_parts(kind: IdKind, stories: &[StorySpec], main_rels: &Rels) -> Vec<Arc<str>>;
pub fn revision_at(node: NodeRef) -> Option<Revision>;       // one reader for every revision element
pub fn part_revisions(tree: &SpanTree) -> Vec<Revision>;     // the same over a part that has no Story (styles, numbering)
pub fn control(node: NodeRef) -> Control;                    // a w:sdt's properties
pub fn hyperlink(node: NodeRef) -> Hyperlink;                // r:id and w:anchor

pub struct Story;                                            // owned index: Clone, Send, Sync
impl Story {
    pub fn scan(part: &Arc<str>, kind: StoryKind, tree: &SpanTree) -> Result<Story, StetError>;
    pub fn refresh(prev: &Story, tree: &SpanTree, touched: &[NodeId]) -> Result<Refreshed, StetError>;
    pub fn blocks(&self) -> &[BlockEntry];                   // pre-order, document order
    pub fn key(&self, e: &BlockEntry) -> BlockKey;
    pub fn index_of(&self, node: NodeId) -> Option<usize>;
    pub fn fingerprints(&self, tree: &SpanTree) -> Vec<u64>; // frozen, FINGERPRINT_VERSION = 1
    pub fn opaque(&self) -> &[Opaque];                       // altChunk and every unknown block-level element
    pub fn units(&self) -> &[Unit];                          // the notes or comments of the story
    pub fn anomalies(&self) -> &[Anomaly];
    pub fn block<'a>(&'a self, tree: &'a SpanTree, i: usize) -> Option<Block<'a>>;
    pub fn paragraph<'a>(&'a self, tree: &'a SpanTree, i: usize) -> Option<Paragraph<'a>>;
    pub fn sections(&self, tree: &SpanTree, rels: &Rels) -> Vec<Section>;
    pub fn fields(&self, tree: &SpanTree) -> &[FieldInfo];   // lazy, cached (4.4)
    pub fn anchors(&self, tree: &SpanTree) -> Anchors;       // bookmarks, comment ranges, move ranges (3.9)
    pub fn revisions(&self, tree: &SpanTree) -> Vec<Revision>;
    pub fn text_boxes(&self, tree: &SpanTree) -> Vec<TextBox>;
    pub fn join_run(&self, tree: &SpanTree, i: usize, mode: Mode) -> Range<usize>;   // paragraphs that read as one (3.6)
    pub fn own_parts(&self, tree: &SpanTree, i: usize) -> Option<OwnParts>;          // what a table or row owns itself (3.6)
    pub fn num_facts(&self, tree: &SpanTree) -> Vec<NumFact>;                        // what numbering reads of each paragraph (3.6)
    pub fn table_site(&self, tree: &SpanTree, i: usize) -> Option<TableSite>;        // the innermost table around a paragraph (3.6)
    pub fn table_sites(&self, tree: &SpanTree) -> Vec<Option<TableSite>>;            // the same for every entry, linear in a large table
}

// views over a tree (borrow it; cost nothing until used)
pub enum Block<'a> { Paragraph(Paragraph<'a>), Table(Table<'a>), Row(Row<'a>) }
impl Paragraph<'_> { pub fn map(&self) -> TextMap; pub fn text(&self, mode: Mode) -> String; /* style, pPr, mark ... */ }

// the text model (sections 4 and 5)
pub use crate::api::view::Mode;                              // Final, Original, Redline: data only, in `api`
impl TextMap { text, len, items, context, locate, gap, offset_of, find_all, char_to_utf16, utf16_to_char, anomalies }

// definitions (section 6)
impl StyleDefs     { pub fn read(tree: &SpanTree) -> StyleDefs; get, by_name, default_for, chain, issues }
impl NumberingDefs { pub fn read(tree: &SpanTree) -> NumberingDefs; abstract_of, num, level, issues }
```

The contract in `wordml/mod.rs` keeps its one-line interface (`Story`, `Block`,
`Paragraph::{text, map}`, `StyleDefs`, `NumberingDefs`) and gains `BlockKey`, `stories`, `Rels`,
`existing_ids`, `id_parts` and the two version constants. Everything is read-only over `&SpanTree`.
`wordml` never serializes, never edits, never names `package`, `state`, `refs` or `revisions`, and
holds no cache except the one a `Story` fills for itself (4.4). Derived-cache ownership and
invalidation are `state`'s (3.3).

`wordml` uses `schema` in two small places and needs nothing else from it: the relationship type
URIs (`PartKind::relationship_type`), and the one rule for which children Word reads under an
`mc:AlternateContent` or an `mc:ProcessContent` (4.5).

## 3. Stories and blocks

### 3.1 What a story is, and finding them

A story is a WordprocessingML idea, so the knowledge of which relationships make stories lives
here. `state` converts `package::Relationships` into plain data with `package::opc::resolve_target`
and never inspects a relationship type:

```rust
pub struct Rels(Vec<Rel>);
pub struct Rel { pub id: String, pub rel_type: String, pub target: RelTarget }
pub enum RelTarget { Part(String) /* the part name as the package entry spells it */, External(String), Invalid }
pub struct StorySpec { pub part: Arc<str>, pub kind: StoryKind }
```

**Agreed with `state`, verbatim:** "`Rels` targets are part names spelled as the package entry spells
them. An external target is `External`, and a target that names no part of the package is `Invalid`.
`state` builds `Rels`, and a dangling relationship never fails an open."

`stories(main_part, main_tree, main_rels)` returns, in this order and with each part once:

1. the body (`main_part`, `Body`)
2. headers and footers, in the order the `w:sectPr` elements name them (document order of the
   `sectPr` elements, and within one, the order of its `w:headerReference` and
   `w:footerReference`), resolved through `main_rels` by `r:id`
3. footnotes, endnotes, comments, if the main part has a relationship of that type

The order is deterministic because ref numbers follow it. **The relationship type must match the
kind of the reference:** a `w:headerReference` counts only when its `r:id` names a relationship of
the header type, a `w:footerReference` only one of the footer type. A reference whose `r:id` is
missing, `External` or `Invalid`, or whose relationship has another type, is skipped; so is a header
part that no `sectPr` names. A part is therefore a story only when its relationship type and the
reference agree, and a part shared by several sections appears once. This is what makes the scan's
check of a part's root safe (3.2): a story's kind never comes from a reference alone. A relationship
type matches when it is the Transitional URI that `schema::PartKind::relationship_type` gives, or the
same type under the Strict prefix `http://purl.oclc.org/ooxml/officeDocument/relationships/`.

Each story is one part, with one root element: `w:document/w:body` (`Body`), `w:hdr`, `w:ftr`,
`w:footnotes`, `w:endnotes`, `w:comments`. A footnote, endnote or comment is a **unit** inside its
story: `w:footnote`, `w:endnote` or `w:comment` with a `w:id`. Notes of type `separator`,
`continuationSeparator` and `continuationNotice` are not shown or edited, so the scan skips them
(Word writes two or three of them per part, and 72 curated documents have them). Text boxes are not
listed: they are found from a paragraph (3.8), and in 1.0 they have no refs.

### 3.2 The block scan

`Story::scan(part, kind, tree)` walks the block structure of one part with an explicit stack and
returns an owned index. `part` is the canonical `&Arc<str>` that `state` holds, so a scan allocates no
name. The scan does **the block scan only**: it does not read paragraph content except for what a
paragraph says about itself in its first `w:pPr`, which is a section break (`w:sectPr`) and a
revised paragraph mark (`w:rPr/w:ins` or `w:del`, `w:moveFrom`, `w:moveTo`). The field pass is not
part of it (4.4).

```rust
pub struct BlockEntry {
    pub node: NodeId,
    pub kind: BlockKind,          // Paragraph | Table | Row
    pub parent: Option<u32>,      // index of the enclosing entry: a table for a row, a row for the blocks in its cells
    pub unit: u32,                // index into story.units() of the enclosing note or comment, or NO_UNIT
    pub flags: u8,                // SECTION_BREAK, MARK_INSERTED, MARK_DELETED, IN_CONTROL
}
```

What counts as a block, and in which order:

- **Order** is pre-order and document order: a table, then each of its rows, then the blocks of
  each row's cells, cell by cell. A cell is not an entry. This is the order `refs` allocates in.
- **Transparent containers.** A block-level `w:sdt` (through its `w:sdtContent`), `w:customXml`,
  `w:smartTag`, a taken `mc:AlternateContent` branch (4.5), and the revision wrappers `w:ins`,
  `w:del`, `w:moveFrom`, `w:moveTo` are walked through when they stand where blocks stand. They add
  no level. A wrapper that holds no blocks contributes nothing. A paragraph inside `w:moveFrom` at
  body level (the one real case) is therefore a block; this is more lenient than the schema, on
  purpose. The same applies to a `w:tr` inside `w:sdt` inside `w:tbl`, and a `w:tc` inside a
  row-level `w:sdt`.
- **Not blocks, and not worth a placeholder.** The elements that are known to carry no content at
  block level are skipped silently: `w:bookmarkStart`, `w:bookmarkEnd`, `w:commentRangeStart`,
  `w:commentRangeEnd`, `w:permStart`, `w:permEnd`, `w:proofErr`, the move range markers
  (`w:moveFromRangeStart` and the three like it), the `customXml*Range` markers, the final
  `w:sectPr`, the property and grid children of tables, rows and cells (`w:tblPr`, `w:tblGrid`, `w:trPr`,
  `w:tblPrEx`, `w:tcPr`), comments, processing instructions and whitespace.
- **Opaque.** **Agreed with `state`, verbatim:** "`Story::opaque` lists `w:altChunk` and every
  unknown block-level element. They are not blocks, so they have no ref." `Opaque { node, kind, after:
  Option<usize> }` gives the element, a kind (`AltChunk`, `Math` for an `m:oMath` or `m:oMathPara` at
  block level, `Unknown`), and the index of the preceding entry, so `projection` can show a
  placeholder in the right place and `check` can say content was not shown. A `w:tbl` found inside a
  paragraph (not valid, one document) is not a block and is not listed: it is paragraph content the
  text model skips (4.2).
- A table's rows are the `w:tr` elements directly in it, or under transparent wrappers. A row's
  blocks are the paragraphs and tables of its `w:tc` elements.

**Which scan can fail.** **Agreed with `state`, verbatim:** "Only the body story can fail
`Story::scan`, and only when its root is not the document element or `w:body` is missing. A document
that fails there is refused at open, and an edit cannot cause it. Any other story whose root is not
the expected element scans as an empty story with an anomaly, which `check` reports, so a re-scan
after an edit never fails." The anomaly is in `story.anomalies()`. The body check is a
`document`-stage refusal (`malformed_input`). It needs the two things of 3.1: `stories()` requires the
relationship type to match the reference, and an unreferenced or mismatched part is not a story.
Every other shape, including everything the editor can build, scans.

### 3.3 Block identity, caches and failures, as agreed with `state`

```rust
pub use crate::xml::NodeId;   // an opaque Copy + Eq + Hash handle; refs never reads its contents
pub struct BlockKey { part: Arc<str>, node: NodeId }   // Clone, Eq, Hash; deliberately no Ord
impl BlockKey {
    pub fn new(part: Arc<str>, node: NodeId) -> BlockKey;
    pub fn part(&self) -> &Arc<str>;
    pub fn node(&self) -> NodeId;
}
```

`refs` may name only `wordml` types, and `NodeId` lives in `xml`, so `wordml` re-exports it. `refs`
stores a compact (part index, node) pair per block rather than whole keys.

- `part` is the canonical entry name exactly as `state` passes it to `scan`. `wordml` never
  normalizes it. OPC compares names ignoring ASCII case, so `state` canonicalizes.
- `NodeId` is an arena index. **Assumption on `xml`, stated here so it can be tested there (and
  recorded in the `xml` contract):** an id is never renumbered and never reused
  within a tree or its clones. Cloning a tree (copy-on-write staging) keeps every id, an edit never
  changes the id of an existing node, and a detached node keeps its id. A cloned subtree gets new
  ids. So a `BlockKey` stays valid for as long as the block's element is in the tree, which is what
  `refs` tombstones follow.
- `NodeId` does not survive a re-parse, so a `BlockKey` is never persisted. The persisted form is
  `(part name, ordinal in blocks() order, fingerprint)`. `resume` re-parses the head package,
  re-scans every story, and requires exact alignment: the same stories, the same counts, the same
  kinds, the same fingerprints. The ordinal is the flattened form of the structural position, so
  there is no `BlockPath` in the interface.
- `Ord` is not derived because `NodeId` has none and `xml` is not ours. `refs` keys a hash map by
  `BlockKey`, and anything it persists or iterates follows scan order.
- `Story` is `Clone + Send + Sync` plain data, with one lazily filled cache (4.4). A `Story` is valid
  for the tree it was scanned from, as that tree was at scan time.
- Text-box paragraphs get no entry, so they get no ref in 1.0.

**Agreed with `state`, verbatim:**

- **Story cache.** "`state` caches one `Story` per story part. The cache key is the part, and an entry
  is rebuilt on the next read when the generation of its part has changed. Nothing is keyed by a
  story's position in the story list."
- **Invalidation.** "Derived caches are not invalidated when a part is dirtied. An entry records the
  generation of every part it was built from, and the next read rebuilds it if any generation has
  changed. A generation is a pair (epoch, edits): the epoch counts replacements of a part's tree, and
  `edits` is `SpanTree::edit_count`."
- **Refresh.** "A cache builder receives the stale previous value, if there is one, so that a refresh
  can reuse it. A refresh must give exactly the value a scan from scratch gives, and the coherence
  test compares them after every edit." For a `Story` the refresh is
  `Story::refresh(prev, tree, touched)`, which returns a `Refreshed`. The block scan is run again in
  full (under 1 ms at 13 MB). **`refresh` does not trust `touched` for the shape of the story:** it compares
  `prev`'s block list with the new scan by `NodeId` and treats every new, removed and moved block as
  touched, whatever `touched` says. So `touched` only has to cover edits **inside** existing blocks,
  which a before and after comparison of block bytes can check, and which a new block (no before), a
  removed block (no after) and a moved block (the same bytes) would defeat. The field index of `prev` is
  shared or rebuilt from what it knows (4.4). `state` passes the blocks the `Tx` declared as touched,
  and without `touched` it calls `scan`.
- **What a refresh reports.** `Refreshed { story, added, removed, moved, context_changed,
  field_starts_changed, fields_carried }`. The first three are the blocks, by `NodeId`, that the
  comparison of block lists found (a block is moved when it left the longest run of blocks that kept
  their order, or its parent or its note or comment changed). Two more are blocks whose **text changes while their
  bytes and place do not**, which a comparison of bytes cannot see, so `wordml` owns finding them and `state`
  stamps them as touched:
  - `context_changed` is the blocks, in both stories, whose wrappers changed: wrapped into or out of a
    block-level control, `w:ins`, `w:del`, `w:moveFrom` or `w:moveTo`, or in a row or a cell that gained,
    lost or changed a `trPr` or `tcPr` revision. What the block reads in each mode depends on its wrappers
    (4.3). The scan keeps one key per block, hashed from the same wrappers the text model reads
    (`context_key.rs`), so the comparison costs nothing and is exact. It can name a block that is
    also in `moved`.
  - `field_starts_changed` is the paragraphs, by `NodeId`, that start in a different field state than
    they did in `prev` (4.4), a new paragraph counting as having started in none. It is empty when
    `prev` had no field index. `fields_carried` says whether the field index was shared (4.4).
  A block whose mark revision changes changes how the paragraph before it joins the next (3.6), and
  `state` stamps the next paragraph too, from the mark bits of the entries.
- **Field pass.** "`Story::scan` does the block scan only. The field pass runs the first time a
  paragraph's text is read, once per story and tree, and the `Story` caches it. It is not part of
  `open` or of a commit's re-scan."
- **Version policy.** "`SCAN_VERSION` and `FINGERPRINT_VERSION` change only in a major release. The
  complement records both. A release that reads a working copy recording other values answers
  `unsupported_version`, and the namespace id is recomputed with the recorded `SCAN_VERSION`." So
  the rules of 3.2 are part of the frozen interface for the life of a major version: widening the
  transparent containers, or giving text boxes refs, changes the block list and waits for the next
  major release, or for `scan` to take a version parameter that keeps the old rules, which 1.x does
  not need.

### 3.4 Fingerprints

`Story::fingerprints(&self, tree) -> Vec<u64>`, one entry per block, computed in one pass, with
`pub const FINGERPRINT_VERSION: u32 = 1` that `state` persists. The definition is frozen, and
`wordml` owns its exact words. `state` quotes this section and does not paraphrase it.

- The hash is FNV-1a 64.
- A **paragraph** hashes one kind byte `1`, then the UTF-8 of every text and CDATA node below the
  `w:p`, in document order, decoded exactly as `NodeRef::text` decodes them (references decoded,
  line endings normalized). That includes `w:delText`, `w:instrText`, whitespace-only nodes and the
  contents of text boxes inside the paragraph. It does not allocate a `String` per paragraph, and it
  does not use the text model.
- A **table** hashes kind byte `2`, then the 8 little-endian bytes of the fingerprint of every scan
  entry whose parent is this table, in scan order. A **row** hashes kind byte `3`, then the 8
  little-endian bytes of the fingerprint of every scan entry whose parent is this row, in scan order.
- A fingerprint is a pure function of the tree's decoded text nodes and the block structure of 3.2.
  It does not depend on any other state of the `Story`: the order or number of opaque entries, the
  field pass and the text model have no part in it. A mismatch on `resume` is read by `state` as a
  corrupt complement, so nothing else may ever change one. A change of the definition needs a new
  `FINGERPRINT_VERSION`, which is a major-release event (3.3).

A fingerprint says "same words in the same XML text nodes", not "same formatting". It is an
alignment check for `resume`, and `state` records a SHA-256 of the head bytes beside it for the
integrity of the whole.

**Test support.** `wordml::testing` (`#[cfg(test)]`, `pub(crate)`) builds a `Story`, or a few
distinct `BlockKey`s, from a small XML string: `story(xml_body: &str) -> Story` and
`keys(&Story) -> Vec<BlockKey>`. `refs` may not depend on `xml`, even in tests, and `NodeId` cannot
be built outside `xml`, so `refs` and `state` unit tests need only `wordml`.

### 3.5 Ids that already exist

`state` mints new `w14:paraId`, `w14:textId` and `w16cid:durableId` values, deterministically, from
the namespace, the state version and a position, and bumps them past collisions. Only `state` mints;
`wordml` only reports what is already there:

```rust
pub enum IdKind { ParaId, TextId, CommentDurableId, AnnotationId }
pub fn existing_ids(kind: IdKind, tree: &SpanTree, out: &mut Vec<u32>);
pub fn id_parts(kind: IdKind, stories: &[StorySpec], main_rels: &Rels) -> Vec<Arc<str>>;
```

`existing_ids` appends every value found in one read-only pass over the tree's elements and
attributes, with no allocation per element, and skips a value that is not eight hexadecimal digits
(`AnnotationId` reads a decimal number instead). Prefixes do not matter: attributes are matched by
namespace URI and local name. It reads **both branches** of an `mc:AlternateContent`, so a value used
in either is reported. `id_parts` names the parts to read for a kind, so the relationship-type
knowledge stays in `wordml`: it takes the story list and the main part's `Rels`, finds the comment
extension parts by relationship type, and returns part names in a fixed order with each once.

- `ParaId`: `paraId` in the `w14`, `w15` (also in the 2010/11 namespace of Word 2013's preview) and `w16cid`
  namespaces, on any element. In the corpus that
  is `w:p` (8,085 in 85 curated documents) and `w:tr` (873 in 35), and comment paragraphs; and
  `w15:commentEx` in `commentsExtended.xml` and `w16cid:commentId` in `commentsIds.xml`, which name
  existing paragraphs, so a new id can never equal one of those. Parts: every story part, and
  `commentsExtended.xml` and `commentsIds.xml`.
- `TextId`: `textId` in the `w14` namespace on any element (`w:p` 7,220, `w:tr` 819). Parts: every
  story part.
- `CommentDurableId`: `durableId` on `w16cid:commentId` and on `w16cex:commentExtensible`. A
  `w16cid:durableId` on a `w:num` in `numbering.xml` (274 in 20 documents) is another id space, and
  is not reported. Parts: `commentsIds.xml` and `commentsExtensible.xml`.
- `AnnotationId`: `w:id` as a non-negative decimal number on any element (revisions, bookmarks, comment
  ranges, comments, move ranges), so `revisions` has one reader for "the highest existing id plus
  one". Parts: every story part, and `styles.xml` and `numbering.xml`, found by relationship type,
  because the property changes in styles and in list levels (`w:pPrChange`, `w:rPrChange`) carry ids
  of the same space.

**Agreed with `state`, verbatim in substance:** "`existing_ids` reports `w14:paraId` and `w14:textId`
from every story part, and `paraId` from `commentsExtended.xml` and `commentsIds.xml`. It reports
`durableId` from `commentsIds.xml` and `commentsExtensible.xml`. It reads both branches of an
`mc:AlternateContent`. `state` mints, and never mints a value that either branch uses." Whether a new
paragraph gets a `paraId` at all is `revisions`' decision: one of 42,003 Harvey LAB documents has any,
and Word-authored documents carry one per paragraph. Whether two paragraphs share a `paraId` is
`check`'s question, and it ignores the `mc:Fallback` copy, where Word repeats a text box's ids.

### 3.6 Views over blocks

`Block`, `Table`, `Row`, `Cell` and `Paragraph` are cheap structs over `(&SpanTree, NodeRef)`. They
hold no data of their own and cost nothing until a method is called. They exist for the callers
that need to look inside one block; the flat `blocks()` list is for enumeration.

- `Table`: `rows()`, the grid (`w:tblGrid`), properties (`PropsRef` over `w:tblPr`).
- `Row`: `cells()`, row properties, the revisions that are the row itself (`w:trPr/w:ins` and
  `w:del`, both when two authors wrote them, as `revisions()` and as `MarkRevs` from `revs()`), `w:tblPrEx`.
- `Cell`: `blocks()` (through transparent wrappers), `grid_span`, vertical merge state, properties,
  cell revisions (`w:cellIns`, `w:cellDel`, `w:cellMerge`).
- `Paragraph`: built by `Story::paragraph`, or straight from a tree and a node with
  `Paragraph::new` (for text-box content and for tests; it starts with no open field). It has
  `id()`, `style_id()`, `ppr()` (a `PropsRef`), `num_pr()` (the direct `w:numPr` only; the style's is
  `numbering`'s business), `section_break()`, `mark_revs()` (every revision on the paragraph mark),
  `joins_next(mode)` (below), and the text model: `map()` and `text(mode)`. From an item or a wrapper
  of the map, so that nobody downstream names `xml`: `mark_id(item)` (the `w:id` of a comment range, a
  comment or note reference, a bookmark or a move range, and for the number inside a note the id of
  the note), and `revision_of`, `control_of` and `hyperlink_of` over a `Context` (the revision a wrapper is,
  with a row or a cell marked both ways giving two; the properties of a content control; the target
  of a hyperlink).

**What numbering and styles read, without reading XML.** `numbering` and `styles` may not name `xml`, so
`wordml` reads for them. `story.num_facts(tree)` returns one `NumFact` for every paragraph in one pass,
with no field pass: the direct style and `numPr`, the same two as they were before a tracked paragraph
property change (`old: Option<OldPPr>`, `Some` exactly when `w:pPr/w:pPrChange/w:pPr` exists, so a
missing old style is distinguishable from no change), the mark revisions (`mark_revs: MarkRevs`), `Presence` (`Always`, `FinalOnly`,
`OriginalOnly`, `Never`, from the same wrapper rule as the text model, so an inserted row, a deleted
cell or a block-level `w:ins` above the paragraph is counted the same way in both), and whether the
paragraph is the empty one that only carries a section break. `Paragraph::{ppr, mark_props, run_props,
run_style}` give `PropsRef` and the run style id of an item, and `Paragraph::base_props` gives the
paragraph's own formatting as an owned `Props` (the `pPr` children without `rPr`, `sectPr` and
`pPrChange`) that compares with `==`, so a merge can tell whether two paragraphs are formatted alike, and `wordml` re-exports `SpanTree` next to
`NodeId`, so a caller takes `&Story` and `&SpanTree` without naming `xml`. `story.table_site(tree, i)`
gives the innermost table around a paragraph (a table style is not inherited into a nested table): its
style id, `w:tblLook` flags, the row index and count, the grid column and column count with `gridSpan`
and `gridBefore` honoured, the cell's span and vertical merge, and the row's and cell's `w:cnfStyle`.
`NumFmt::Custom` carries the `w:format` of a Word 2010 custom number format, read through the branch of the
`mc:AlternateContent` in `w:lvl` that Word reads (6).

**What a table or a row owns.** A table's bytes contain its rows and a row's its cells, so comparing a
table's bytes before and after an edit would call every cell edit an edit of the table and of the
row. `story.own_parts(tree, i)` says what the block owns itself: for a table, the `w:tblPr`, the
`w:tblGrid` and every other child of the `w:tbl` that is not a row, and the **list** of its rows
(their `NodeId`s in order, so a reorder shows); for a row, the `w:trPr`, the `w:tblPrEx`, the
`w:tcPr` of each cell, and the list of its cells. `OwnParts { nodes, list }` holds the nodes whose own
bytes count and the list. A paragraph owns all of its bytes, text boxes inside it included, so
`own_parts` is `None` for one. `state`'s touch oracle compares these, so an edit inside a cell is
an edit of the paragraphs in it, and the scope guard does not ask for the table's ref.

**A revised paragraph mark.** In `Final`, a paragraph whose mark is deleted or moved away is joined
to the next paragraph, and in `Original`, one whose mark is inserted or moved in is. The text model
gives each paragraph's own text and `Paragraph::joins_next(mode)`. The join applies only when the next
block is a paragraph in the same container, and is ignored at the end of a cell or story, as Word
ignores it. Which paragraph's properties survive the merge, and which ref the merged block shows
under, are decided by `projection` and `revisions`, and need a Word oracle (open question 2 in 12.3). What
`wordml` does give them is the helper they would otherwise each write: `story.join_run(tree, i, mode)`
returns the range of entries, starting at paragraph `i`, that read as one paragraph in `mode`, and
`Joined` (5.4) maps text ranges across it. **The same container** means the same cell, note, comment,
header, footer or body, read through the elements that only group blocks (`w:sdt`, `w:customXml`,
`w:smartTag`, the revision wrappers and the branches of an `mc:AlternateContent`): a deleted mark at
the end of a cell joins nothing, and one before a control that wraps the next paragraph does join.
`story.joined(tree, i, mode)` returns the range and the `Joined` over the maps of its paragraphs.

**A mark can hold more than one revision.** An insertion by one author that another deleted is a
`w:ins` and a `w:del` side by side in the `w:rPr` of the `w:pPr`, in either order; the corpus census in
`revisions.md` has 19 such marks in 4 documents. Such a mark does not exist in either mode: the deletion
removes it from `Final` and the insertion from `Original`, so the paragraph joins the next one in both.
So the mark is not one `RevKind` but `MarkRevs { inserted, deleted, moved_from, moved_to }`, and
`MarkRevs::gone_in(mode)` is the rule: deleted or moved away in `Final`, inserted or moved in in
`Original`, never in `Redline`. The bits are the `MARK_*` flags of the block entry, so the scan, the
paragraph (`Paragraph::mark_revs`), `NumFact::mark_revs` and `Section::mark_revs` cannot disagree. An
earlier version returned the first revision it found, which read the insertion and missed the
deletion.

### 3.7 Sections

`story.sections(tree, rels)` (body only) returns one `Section` per `w:sectPr`: those inside a
paragraph's `w:pPr` (the paragraph that ends the section, given as an entry index) and the final one
in `w:body`. A `Section` has `node`, `ends_at: Option<usize>`, the break type, `title_pg`, **`mark_revs:
MarkRevs`**, and the headers and footers as part names (default, first, even; header and
footer separately), already resolved through `rels`. **Inheritance is applied here:** a section with
no reference of a kind takes the one of the previous section, and the first section with none has
none. **Sections depend on the mode:** a `sectPr` on a paragraph whose mark is deleted is gone in
`Final` (7 in the curated `n830205.docx`), and one on a paragraph whose mark is inserted is gone in
`Original`. `mark_revs` is that paragraph's mark revisions, and `Section::exists_in(mode)` applies them, so
`projection` does not have to repeat the rule. When a section disappears its page setup goes with it,
and the section before it continues. `PropsRef` over the `sectPr` gives the rest (page size, margins,
columns, `w:lnNumType`, `w:pgNumType`). `state` never sees a relationship id.

### 3.8 Notes, comments, text boxes, content controls

- **Units.** `story.units()` is the side table that `BlockEntry.unit` indexes: the notes or comments
  of a story with `id`, `node`, and for a comment its author, initials and date. Threads
  (`commentsExtended` and the other comment parts) belong to `threads` (`comments-and-notes.md`).
- **Text boxes.** `story.text_boxes(tree)` finds each `w:txbxContent` (and the Word 2006 namespace
  spelling of it) below a paragraph and returns `TextBox { host: usize, content: NodeId, branch: Branch }`,
  with `host` the entry index of the paragraph that holds it. The text box under the **taken** branch
  of an `mc:AlternateContent` is listed with `branch: Taken`, and the copy under the other branch with
  `branch: Untaken`, so a caller that shows text reads only the first and a caller that must change
  every copy (`revisions`, accepting a tracked change) reads both (4.5). The blocks inside are read
  with `Block` views built on the `content` node, and their text with `Paragraph::map`, exactly like
  any other. They have no entries and no refs in 1.0. They are **shown read-only inside the host
  paragraph's placeholder**, and an edit inside one returns `unsupported` (`projection` and `ops`
  implement it, and `FIDELITY.md` records it). The copy under the untaken branch goes stale when
  the taken one is edited, and Word ignores it when it understands the Choice.
- **Content controls.** `control(node) -> Control` reads a `w:sdt`'s `w:sdtPr`: kind (plain text,
  rich text, date, drop-down, combo box, checkbox, picture, citation, bibliography, building block,
  group, repeating section, equation), alias, tag, id, lock, whether it shows its placeholder
  (`w:showingPlcHdr`), and whether it is data-bound. **The controls that enclose a paragraph are found
  by walking up to the story root**, across cells and rows, so a lock or a control around a whole table
  reaches the paragraphs in it. A data-bound control (70 in the curated set have text) is refreshed by
  Word from the custom XML part, so an edit to its text is lost unless the edit is refused as
  `readonly_region`; `projection` and `ops` decide, and `Control::data_bound` is what they read. At run
  level a control is a wrapper in the text model (4.2).
- **Hyperlinks.** `hyperlink(node)` gives `r:id` and `w:anchor` from a `w:hyperlink`, and the field
  index (4.4) gives the target of a `HYPERLINK` field. `state` resolves the id through `Rels`.

### 3.9 Anchors across paragraphs

`story.anchors(tree) -> Anchors` pairs the range markers that span paragraphs, which a paragraph's
text model cannot (23 bookmarks in 5 curated documents and 4 comment ranges in 2 start in one
paragraph and end in another). It is one read-only walk over the story's marks, run on demand and not
cached (the callers' caches are `state`'s):

```rust
pub struct Anchors { pub bookmarks: Vec<Bookmark>, pub comments: Vec<CommentAnchor>, pub moves: Vec<MoveAnchor> }
pub struct Loc { pub paragraph: Option<NodeId>, pub node: NodeId }   // the marker element and the paragraph that holds it, None between blocks
pub struct Bookmark { pub id: Option<i64>, pub name: String, pub start: Option<Loc>, pub end: Option<Loc>, pub hidden: bool }
pub struct CommentAnchor { pub id: i64, pub start: Option<Loc>, pub end: Option<Loc>, pub reference: Option<Loc> }
pub struct MoveAnchor { pub id: Option<i64>, pub name: String, pub from: bool, pub start: Option<Loc>, pub end: Option<Loc> }
```

A bookmark end pairs with the nearest preceding unpaired start of the same `w:id`, so an id reused
after its end is a second bookmark. An end with no start has `start: None`, a start with no end has
`end: None`, and neither is an error (3 `bookmarkEnd` elements in the corpus have no start before them).
Hidden bookmarks (`_Toc`, `_Ref`, `_GoBack`) are flagged and kept: they must survive edits. A `Loc`
becomes an offset with `TextMap::offset_of(mode, loc.node, 0)` in the paragraph found by
`story.index_of(loc.paragraph)`. **A marker can sit between blocks**, as a child of the body, a cell,
a row, a table or a block-level control: 282 of the 1,993 range markers in the curated set do, in 21
documents (141 `bookmarkEnd` and 126 `bookmarkStart` among them, most directly in `w:body`, and a few in
cells, rows and tables). Its
`paragraph` is `None`, and it is paired like any other because the walk covers the whole part and not
only the paragraphs. Marks inside text boxes are not paired with the host's, and of an
`mc:AlternateContent` only the branch Word reads is walked.

## 4. The text model

A paragraph's text is assembled from many element kinds, and every later module reads it
through one structure, `TextMap`, built by `Paragraph::map()`. The model is **one list of items in
XML document order**, from which the three modes are masks. Nothing is flattened that someone
might need: every item keeps its XML node, its run, the wrappers around it, its revision state and
its field.

### 4.1 Items

An item is the smallest piece of a paragraph's content the model addresses. Text is counted in
Unicode scalar values (`char`), in every offset the model takes or returns.

| XML | Item kind | Text it contributes |
|---|---|---|
| each text or CDATA child of `w:t` or `w:delText` | `Text` | its characters |
| `w:tab`, `w:ptab` | `Tab` | U+0009 |
| `w:br` without a type or `textWrapping`, `w:cr` | `LineBreak` | U+000A |
| `w:br w:type="page"` | `PageBreak` | U+000C |
| `w:br w:type="column"` | `ColumnBreak` | U+000E |
| `w:noBreakHyphen` | `Hyphen` | U+2011 |
| `w:softHyphen` | `SoftHyphen` | U+00AD |
| `w:sym` | `Symbol` | the code point of `w:char` (a private-use code for a symbol font); U+FFFD when there is none, or when it is one XML cannot carry (a C0 control, U+FFFE, U+FFFF) or not a character at all, with an anomaly |
| `w:drawing`, `w:pict`, `w:object`, and an `mc:AlternateContent` in a run that resolves to one | `Object` | U+0001 |
| `w:footnoteReference`, `w:endnoteReference` | `NoteRef` | U+0002 |
| `w:footnoteRef`, `w:endnoteRef` (the automatic number inside a note) | `NoteMark` | U+0003 |
| `m:oMath`, `m:oMathPara` | `Math` | U+0004 |
| `w:pgNum` and the six legacy date elements | `Auto` | U+0005 |
| a complex field with no `separate` that Word still shows (4.4) | `FieldValue` | U+0006 |
| bookmarks, comment ranges and references, field control, permission and move range markers | marks | nothing (zero width) |

**Why these characters.** The control characters U+0001 to U+0006, U+000C and U+000E cannot be
written in an XML part (`xml` refuses them), so an atom's character can never collide with real
text. They are our own codes. They resemble the ones Word's object model uses for a few of the same
things, but they are not the same set, and nothing relies on the resemblance. U+2011 and U+00AD
stand for `w:noBreakHyphen` and `w:softHyphen`. An
atom is exactly one character wide and is never split. The characters are for matching, diffing and
measuring. They never reach an agent: formats render each item by its kind (`Placeholder`,
`NoteReference`).

**A `Text` item can carry any character XML can, including ones a line-based format must escape.**
A literal tab or newline inside a `w:t` is a `Text` item whose character happens to be U+0009 or
U+000A, so a character does not say what produced it: the item kind does. `Text` can also hold
U+000D (written `&#13;`), U+2028 and U+2029 (17 curated `w:t` have a line feed, 8 have a tab). A format
that writes text one block to a line must escape these, or a hand edit that splits a line reads as an
insertion of a new paragraph. That is a rule for `projection`'s formats, and `TextMap` does not
rewrite text to avoid it.

**Marks** have no width but a position in XML order, so a mark sits between two items. They are
`BookmarkStart` and `BookmarkEnd` (id, name), `CommentStart`, `CommentEnd` and `CommentRef` (id),
`FieldBegin`, `FieldSeparate` and `FieldEnd`, `FieldCode` (an `w:instrText` or `w:delInstrText`),
`PermStart` and `PermEnd`, and `MoveRangeStart` and `MoveRangeEnd`. Marks never split a word,
because they have no character. Not recorded at all, because nothing uses them: `w:proofErr`,
`w:lastRenderedPageBreak`, `w:annotationRef`, `w:rt` (ruby annotation text), elements the model
does not know. Unknown elements are skipped with their content, and counted as an anomaly.

```rust
pub struct Item {
    pub kind: ItemKind,
    pub node: NodeId,     // the text node for Text, else the element
    pub run: Option<NodeId>,   // the nearest w:r, for formatting and hidden-text rules
    pub at: u32,          // character offset in the Redline text where the item starts
    pub len: u32,         // characters; 0 for a mark
    pub from: u32,        // for Text: where in the node's decoded characters this item starts
    pub ctx: u32,         // the innermost wrapper, an index into the map's contexts
    pub vis: Vis,         // FINAL and ORIGINAL bits
    pub field: u32,       // innermost field it belongs to, or NO_FIELD
    pub flags: u8,        // FIELD_RESULT, CDATA
}
```

`Item.from` lets a `Text` item map only part of its node, characters `[from, from + len)`, should a
reading rule ever need it. None does: Word shows every character of a `w:t`, white space at its ends included
whatever `xml:space` says (4.7), so `from` is zero and `len` the node's length.

### 4.2 Wrappers: the context tree

Between a paragraph and a run there can be any number of wrappers (section 1). The builder keeps a
**context tree**: each distinct wrapper element visited gets one context
`{ parent: Option<u32>, kind: Wrapper, node: NodeId }`, and every item points at its innermost
context. The chain from an item to the root is its enclosing wrappers, innermost first. Wrappers:
`Ins`, `Del`, `MoveFrom`, `MoveTo`, **`RowIns`, `RowDel`, `CellIns`, `CellDel`**, `Hyperlink`,
`FieldSimple`, `SmartTag`, `CustomXml`, `Control` (a `w:sdt`), `Dir`, `Bdo`, `Alternate` (the taken
branch of `mc:AlternateContent`), `Ruby`. That is exactly the information an edit compiler needs to
place an insertion "inside the same hyperlink or content control as the neighbour" and to see
"inside someone else's insertion", and it costs one small struct per wrapper element instead of per
item.

**The chain does not stop at the paragraph, the cell or the row.** The root of the context tree is
the story root: the builder walks up from the paragraph, through the transparent block-level
wrappers of 3.2 (a block-level `w:sdt`, `w:customXml`, `w:ins`, `w:del`, `w:moveFrom`, `w:moveTo`, a
taken `mc:AlternateContent` branch), across the cells and rows of every table the paragraph is in,
and starts with what it finds in the chain. Three things follow:

- A paragraph inside a body-level `w:moveFrom` (one real document each for `moveFrom` and `moveTo`)
  is deleted text. The independent comparison of 10.3 found this: the first extractor looked only
  inside the paragraph.
- A paragraph in a row marked by `w:trPr/w:ins` or `w:del`, or in a cell marked by `w:tcPr/w:cellIns`
  or `w:cellDel`, takes `RowIns`, `RowDel`, `CellIns` or `CellDel` in its chain. A row inserted by a
  tracked change contains runs that are not wrapped in a `w:ins` of their own, and it is absent from
  `Original` all the same (121 paragraphs in 3 curated documents, among them `RA001-Tracked-Revisions-02`).
- A content control, or a lock, around a whole table is in the chain of every paragraph in the
  table, because the walk crosses the cells (3.8).

An element the model does not know as a wrapper is skipped with its content (not descended into),
because descending into unknown markup would put text we do not understand into the paragraph.
Which children Word reads under an element in an ignorable namespace is the rule of 4.5: an element
that `mc:ProcessContent` lists is replaced by its children, and one that it does not is skipped.

### 4.3 Revisions in the text model

Whether an item exists in each mode comes only from its wrappers (those inside the paragraph and
those above it, 4.2), never from the element name. Call `Ins`, `MoveTo`, `RowIns` and `CellIns`
**insertion wrappers**, and `Del`, `MoveFrom`, `RowDel` and `CellDel` **deletion wrappers**.

- `Final` contains the item iff none of its wrappers is a deletion wrapper.
- `Original` contains the item iff none of its wrappers is an insertion wrapper.
- `Redline` contains every item that is not field code.

So an item inside `Ins` then `Del` (a deletion in another author's insertion, one real document) is
in neither `Final` nor `Original`, and only in `Redline`. A run in `MoveFrom` then `Ins` (one
document) is in neither. An item is in both modes exactly when no revision wrapper encloses it.

Element names do not matter: a `w:t` inside a `w:del` (7 cases) is deleted text, a `w:delText`
outside a `w:del` (1 case) is plain text. Both are anomalies, reported and read by position. A
revision with no `w:id`, `w:author` or `w:date` is still a revision (`w:date` is absent in 269
elements). Reading the attributes is in section 7.

A paragraph whose items are all in all modes (almost every paragraph) has one text. `TextMap`
records `has_revisions`, and `text(mode)` then returns a borrow of the one string.

### 4.4 Fields

Field code is not text, and a field's result is flagged so that a caller can decide what to do with
it, so the model tracks fields precisely.

- **Complex fields.** `w:fldChar` `begin` pushes a field in the **code** phase, `separate` moves the
  innermost field to the **result** phase, `end` pops it. **An item is field code whenever any field
  on the stack is in its code phase**, not only the innermost: a field nested inside another field's
  code is part of that code (77 nested fields in the curated set, such as `IF { DocVariable }` and a
  `FORMTEXT` whose code holds `DATE` and `TIME`). Field code is demoted to a mark: the
  `w:instrText` and any stray text are in no mode's text, so field codes never appear in text or in
  the positions an agent can name. Every other item inside a field is in a result phase and is an
  ordinary item with the `FIELD_RESULT` flag and the id of its innermost field. A result nested in a
  result (a `PAGEREF` inside a table of contents) keeps the innermost field's id, and the chain of
  `parent` ids in the field index gives the rest.
- **A field with no `separate`.** 63 curated fields have no `separate`, so no stored result. Word
  still **displays** something for 33 of them: `EQ` (11), `MACROBUTTON` (9, the text shown is in the
  field code), `LISTNUM` (4), `SYMBOL` (4), `PAGE` (2, in headers and footers), `GOTOBUTTON` (1, same as
  `MACROBUTTON`), `QUOTE` (1) and `BARCODE` (1). The model gives each of these **one read-only
  `FieldValue` atom** (U+0006) at the field's `end`, with the field's id and the `FIELD_RESULT` flag, so
  the field has a position and a presence, and `FieldInfo::display_text()` gives the shown text where it
  is in the code (`MACROBUTTON` and `GOTOBUTTON`). The keywords that do this are one named table,
  `DISPLAYED_WITHOUT_RESULT` in `text_fields.rs`, so it can grow without touching the machine. The
  rest stay empty and contribute no item: `IF` (16), `REF` (4), `XE` (4), `TA` (1) and the others. A
  `w:fldSimple` with no content and a keyword in the table gets the same atom.
  The atom's node is the `end` element (or the `w:fldSimple`), so an edit must never detach it alone:
  `ops` treats an item flagged `FIELD_RESULT` as read-only, or deletes the field whole. The edit
  agreement test of 5.2 item 8 skips ranges that hold such an atom for this reason.
- **Simple fields.** A `w:fldSimple` pushes a field already in the result phase for its content, with
  the instruction taken from `w:instr`.
- **Independent of wrappers.** The stack is driven by document order only. A `begin` inside a `w:ins`
  and an `end` outside it (three cases) work. The vis rule of 4.3 applies per item as always, so a
  field deleted whole is not in `Final` and a field whose `begin` is deleted but whose result is not
  is still a field.
- **Across paragraphs.** The state at the start of each paragraph comes from the paragraphs before
  it (19 spanning fields in 12 documents, up to 61 paragraphs). It is computed by the **field pass**:
  a walk over `w:fldChar`, `w:fldSimple` and `w:instrText` in scan order, which keeps, for each
  paragraph that has a field character or starts inside an open field, the stack it starts and ends
  with (sparse, and nearly always empty), keyed by the paragraph's `NodeId`. A unit boundary (one
  note, one comment) resets the stack, and so does a text box.
- **The pass is lazy and cached in the `Story`.** `Story::scan` does the block scan only. The field
  pass runs the first time a paragraph's text or the field list is read, once per story and tree, and
  the `Story` keeps the result in a `OnceLock`; `Story::paragraph` hands each `Paragraph` the stack it
  starts with, so `map()` is a pure function of the story and the tree. A paragraph built without a
  story (`Paragraph::new(tree, node)`) starts with an empty stack. A `Story` is valid for the tree it
  was scanned from, so the pass on the first read belongs to that tree. A `Story` records the
  `SpanTree::edit_count` of the tree it was scanned from, and `paragraph` and `fields` refuse a tree
  whose count differs (`None` and an empty list), so an index can never be built from a different
  tree. The check is `Story::is_current(tree)`, and `block`, `joined` and `field_anomalies` use it too,
  because they rest on the same entries and the same index. What it catches is a tree that was edited
  after the scan, which is the mistake of keeping a story across an edit without asking
  `Story::refresh`. It cannot tell two trees apart that have the same number of edits, since a clone has
  the count of its source. The readers that keep nothing (`num_facts`, `sections`, `anchors`,
  `revisions`, `table_site`) read the tree each time and do not check.
- **The pass is incremental across edits.** An edit that does not touch field structure must not
  pay for a new pass, or a batch of 50 operations that each write and then read pays it 50 times
  (the first design measured 659 ms on a 13 MB body, and the review 1,092 ms for 50 edits that each
  write a field). The index keeps, besides the sparse state of each paragraph, the set of paragraphs
  that hold any field markup (`w:fldChar`, `w:fldSimple`, `w:instrText` or `w:delInstrText` anywhere below
  them): only those can begin, change or end a field. A paragraph with none that sits inside an open
  field takes the state as it is, without being read, and one outside every field is not looked at.
  So the first pass walks every paragraph once to look for the markup and builds a text map only for
  those that have it (6.8 ms at 13 MB), and `Story::refresh(prev, tree, touched)` has three outcomes
  when `prev` had an index:
  - **Shared.** The new story takes the same index, no copy. It holds when:
    - every block that was added or moved leaves the fields alone. A paragraph does if it holds no
      markup now, held none and had no entry before, and no field is open where it starts, which is
      what a paragraph written into a document is: the paragraphs around it keep the state they had.
      One that goes into an open field gets an entry, one that holds markup changes the index, and
      one that was in a field loses its entry, so the pass runs again for those. The state where it
      starts is read off the entry before it that was not added or moved, in the same note or
      comment. A table or a row holds no state, and the paragraphs in it are looked at on their own;
    - no removed block was one that held markup or had an entry in the index (a removed paragraph is
      not in the new scan, but a paragraph moved where the scan does not read is still in the tree,
      and a stray `w:instrText` in it counted);
    - no touched paragraph held markup before or holds any now;
    - the `begin`, `separate` and `end` of every field are still attached to the tree. Only the paragraphs
      that hold fields and that the span tree says were edited (`NodeRef::is_dirty`) are followed up to
      the document, so this catches a field character detached from a paragraph nobody listed, at the
      cost of one read for each paragraph that holds fields.
    Each is a lookup or a walk of one edited paragraph, so a text edit costs the scan, and so does
    inserting a paragraph.
  - **Rebuilt.** Otherwise the pass runs again, once, **inside the refresh**, from what the old index
    knows: it looks at the paragraphs that held markup in the old index and at the ones that were
    touched, added or moved, and nowhere else (the contract of `touched`: a paragraph that is not in
    the old marked set and was not edited has no markup now). Field ids are renumbered by the walk, in
    document order. It runs inside the refresh because `field_starts_changed` is found by comparing
    the new index with the old one.
  - **Built.** When `prev` had no index, nothing runs; the first read builds it.
  The result must equal a scan from scratch, entries and all, and the coherence tests compare them
  after every edit: a hundred seeded sequences of 30 edits that include the shapes above, and
  800 sequences of 25 chaos edits that put any node anywhere (500,000 such edits were run once with no
  difference). **Field ids** (`Item.field`, `FieldRef.id`, `FieldInfo.id`) are story-wide ordinals in document
  order of the field's start. They are internal handles: adding or removing a field renumbers every
  later one, so nothing outside `wordml` keeps one across an edit, and no view and no ref exposes one.
  A field is named outside `wordml` by its `begin` element, a `NodeId`.
- **Recovery.** An `end` with nothing open and a `separate` with nothing open are ignored. A field
  never closed stays open to the end of its unit if it is in its result phase, which is how a table of
  contents behaves, and is closed at the end of its paragraph if it is still in its code phase, since
  field code does not cross a paragraph (an anomaly in both cases, `UnterminatedField`). Nesting deeper than 32 is clamped:
  deeper `begin`s are ignored with an anomaly. None of these changes the offsets of anything else.
- **Field information.** `story.fields(tree)` lists every field in the story in document order of its
  `begin`: `FieldInfo { id, begin, separate, end, keyword, instr, simple, paragraph_begin, paragraph_end,
  parent, dirty, locked }`, keyed by `NodeId`s so it survives edits that move paragraphs. `keyword` is
  the field type (`REF`, `HYPERLINK`, `FORMTEXT`), and `instr` the whole instruction, assembled from its
  `w:instrText` and `w:delInstrText` pieces (the code is often split over several runs). Field ids are
  story-wide, in document order of `begin`. `w:fldChar w:dirty` and `w:fldLock` are read as written.
- **What the model does not decide.** It flags a result; whether a result is editable is the caller's
  policy, and it depends on the type. Word edits the display text of a `HYPERLINK` and the result of
  a `FORMTEXT` normally, while a `PAGEREF` result is regenerated. `FieldInfo::keyword` is what a
  caller reads to apply its rule, and the architecture's "field results are read-only" is the
  default that rule starts from.

### 4.5 Other inline shapes

- **Hyperlink, smart tag, custom XML, direction, content control.** Wrappers (4.2). Their text is
  ordinary text, editable. A content control showing its placeholder (`w:showingPlcHdr`) keeps its
  text in the model, and the context says it is a placeholder, so `projection` can mark it
  read-only.
- **`mc:AlternateContent` and `mc:ProcessContent`: one rule, in `schema`.** Word reads the first
  `mc:Choice` whose `Requires` prefixes all resolve to namespaces it knows, else `mc:Fallback`, else
  nothing; and for an element in an ignorable namespace that it does not know, it reads the element's
  children if `mc:ProcessContent` lists it and skips the element if not. `schema/effective.rs`
  already applies both when it validates and when it chooses where an element goes, and the rule
  lives there once. `wordml` calls it and keeps no copy. Two functions over a `NodeRef`, in
  `schema/effective.rs`: `taken_branch(ac)` gives the branch of an `mc:AlternateContent`, and
  `disposition(node)` says how Word reads any other element: as it stands, skipped with its content (an
  ignorable namespace that `mc:ProcessContent` does not name), replaced by its children (one it does), or,
  for an `mc:AlternateContent`, by its branch or by nothing. An element of the Word namespace is answered
  without reading an attribute. All of `wordml`'s calls go through the one file `wordml/mc.rs`. The
  builder and the block scan ask `disposition` of every element that is not in the Word namespace:
  a processed element is read in place, an ignorable one that is not named is skipped silently, and an
  element that is not ignorable is skipped with its content and counted (`UnknownElement`). The block
  views, `own_parts` and the scan read rows and cells through a processed element the same way.
  None of the curated documents uses `mc:ProcessContent`, so the tests build the shapes by hand. Over
  every `mc:Choice` of the full corpus (2,089 in 762 documents), the table-based rule agrees with the
  prefix rule this module first used on 2,086; the three that differ name a pre-release Word 2010
  namespace and a Mac Office one. The comparison of 10.3 found a document (a
  `w15` Choice) whose Choice and Fallback hold different text, and pandoc reads the Fallback where
  Word reads the Choice, so the rule matters. The text model reads the **taken** branch only. The
  other branch is read by `revisions()` and `text_boxes()`, which report it marked as untaken (3.8, 7).
- **Drawings, pictures, objects.** One `Object` item, whose node is the `w:drawing`, `w:pict` or
  `w:object`. `Story::object_info(tree, node)` gives, on demand, an `ObjectInfo`: its `ObjectKind`
  (`Picture`, `Shape`, `Chart`, `Diagram`, `Ink`, `Group`, `Ole`, `Unknown`), its alt text, and the text
  boxes inside it as `TextBox` values, boxes in boxes included. It is a method of `Story` because a
  `TextBox` names the entry of the paragraph that holds it, which only the story knows. A drawing is
  classified by the `uri` of its `a:graphicData` (the content's namespace if the `uri` says nothing),
  in the Transitional and the Strict spelling. A `w:pict` is classified by its first VML shape: a
  `v:shape` with a `v:imagedata` or a `v:image` is a picture, a `v:group` a group, any other shape a
  shape; a `w:object` is an OLE object. The alt text is `descr` of the `wp:docPr`, else its `title`,
  or the `alt` of the VML shape; blank is none. Over the curated corpus the 753 objects are 518
  shapes, 100 pictures, 76 charts, 29 groups, 16 OLE objects, 11 diagrams and 3 inks, and none is
  unknown. The text of a text box is another story, not part of the paragraph's text.
- **Math.** `m:oMath` and `m:oMathPara` are one `Math` item. `math_text(tree, node)` gives
  the linear text of its `m:t` elements for `find`. Revision wrappers inside math are not read: the
  whole equation takes the revision state of the wrappers around it.
- **Ruby.** The base text is read as items under a `Ruby` wrapper, and the phonetic annotation
  `w:rt` is skipped.
- **Symbols.** `w:sym` is one `Symbol` item with a private-use code point for a symbol font. A
  literal private-use character in a `w:t` (the usual Symbol-font bullet) is plain `Text`.
- **Note references.** `NoteRef` carries the note's `w:id`. `NoteMark` is the automatic number in a
  note's own first paragraph. Both are read-only atoms.

### 4.6 Modes

`Mode` is the glossary's: `Final` (all revisions applied), `Original` (all revisions rejected),
`Redline` (both, marked). The task brief called `Final` "current"; code and docs use the glossary's
word. **`Mode` lives in `api`** as data only, `api::view::Mode { Final, Original, Redline }`, with the
glossary's names, and `lib.rs` re-exports it: it is already part of the documented interface, so it
needs no new decision record, and the rest of `ViewSpec` is built around it. `wordml` uses
it from there (section 11).

The Redline text is the XML-order concatenation of everything that is text: deleted and inserted
text together. Wrapping it in markers is syntax, and syntax belongs to the formats. The model gives
them the items, with kind, wrappers, revisions and fields, to do it with.

### 4.7 Reading decisions that could have gone another way

- **Whitespace in `w:t`.** A `w:t` without `xml:space="preserve"` and with leading or trailing
  whitespace occurs in 15 curated documents. Whether Word shows that whitespace is not settled by
  the evidence: the Open XML SDK's folklore says Word trims it, and a file in the corpus whose
  `app.xml` says Word 2007 has 9,409 such `w:t` (7,549 of them one space between words), which
  Word would have to read as spaces if it wrote them, though the producer field cannot show that
  nothing else touched the file. pandoc drops the whitespace-only ones, and python-docx returns the
  text verbatim. **Decision (settled 2026-10-10 by Word for the web): every character counts.** The
  hand-check pack's file 37 holds `<w:t>Unshown </w:t><w:t>space</w:t>` with no `xml:space` anywhere,
  and Word shows "Unshown space" when it opens the file, after Accept All and after Reject All, so
  Word keeps the white space at the ends of a `w:t` that does not say `preserve`. LibreOffice drops
  it, a documented deviation (`FIDELITY.md`, and the LibreOffice half of the Word oracle, decision [0005](../decisions/0005-word-oracle.md)). The reading of 2026-10-09, which left that
  white space out after LibreOffice did, is withdrawn, and with it the compiler's taking it out of
  the nodes an edit reaches: under Word's reading that was an untracked deletion of a space Word
  shows. It is still an anomaly, counted, and `check` flags it (`edge_space_without_preserve`),
  because other readers drop it. **The choice sits in one named function**, `text_span(raw: &str) ->
  Range<usize>` in `text_content.rs`, which feeds `Item.from` and `Item.len`.
  **Writing is separate and settled:** anything stetdoc writes or copies with leading or trailing
  whitespace carries `xml:space="preserve"`, unless an ancestor
  already says it: `xml:space` is inherited, as XML says (a Word 2007 file of the corpus says it
  once, on `w:document`), and `wordml::space_preserved` reads it so for the writer and `check`.
- **Literal tab and newline in `w:t`.** Kept as characters (4.1).
- **`w:t` text that is CDATA.** Read like any text. The item carries the `CDATA` flag, and the
  editor cannot `replace_text_range` into it, so a compiler replaces the CDATA node (two documents
  use CDATA at all).
- **A deleted paragraph mark** merges text (3.6), the model does not.

## 5. The character-to-XML map

`TextMap` is the map. There is no second structure.

```rust
impl TextMap {
    pub fn text(&self, mode: Mode) -> Cow<'_, str>;
    pub fn len(&self, mode: Mode) -> usize;                     // characters
    pub fn items(&self) -> &[Item];                             // XML order, marks included
    pub fn context(&self, ctx: u32) -> impl Iterator<Item = &Context>;   // innermost to outermost
    pub fn locate(&self, mode: Mode, range: Range<usize>) -> Option<Located>;
    pub fn gap(&self, mode: Mode, offset: usize) -> Option<Gap>;
    pub fn offset_of(&self, mode: Mode, node: NodeId, in_node: usize) -> Option<usize>;
    pub fn find_all(&self, mode: Mode, needle: &str) -> Vec<Range<usize>>;
    pub fn char_to_utf16(&self, mode: Mode, offset: usize) -> Option<usize>;
    pub fn utf16_to_char(&self, mode: Mode, offset: usize) -> Option<usize>;
    pub fn anomalies(&self) -> &[Anomaly];
}
pub struct Located { pub spans: Vec<Span>, pub items: Range<usize> }
pub struct Span { pub item: usize, pub node: NodeId, pub start: u32, pub end: u32, pub whole: bool }
pub struct Gap { pub left: Option<usize>, pub right: Option<usize>, pub between: Range<usize> }
```

### 5.1 The operations

- **`locate(mode, a..b)`** turns a text range into XML. Each `Span` is one item the range touches
  with characters in it: `node` is the text node (or the atom's element), `start..end` are
  characters of that node as `NodeRef::text` reports them, so they are exactly what
  `Editor::replace_text_range(node, start, end, with)` takes, and `whole` says the span covers the
  item. `items` is the index range from the first to the last item the range touches, so a
  caller can see the marks and the excluded items (deleted text, in `Final`) that sit inside the
  range, and the field marks. A range with `start > end` or past the end is `None`. An empty range
  has no spans.
- **`gap(mode, offset)`** is the insertion point between the characters at `offset - 1` and
  `offset`. `left` and `right` are the nearest included items with width on each side. `between` is
  the index range of **every item between them in XML order**: marks, and items excluded from the
  mode, such as a deleted run that sits at the gap. Where the new text goes (before or after a
  bookmark or a `w:del`, inside or outside a hyperlink) is the caller's decision, and it has what
  it needs: each item's context chain says which wrappers it is in.
- **`offset_of(mode, node, in_node)`** goes back: the offset in the mode's text of a position
  inside a text node, or of an atom or mark by its element. It is how a comment anchor, a
  bookmark or a hyperlink becomes an offset. For an item the mode excludes it returns the offset
  where the item would sit.
- **`find_all(mode, needle)`** returns the non-overlapping occurrences as **character** ranges.
  Byte offsets never leave the module, so the caller cannot confuse them. `ops` decides what
  `ambiguous_match` means.
- **`text(mode)`** is the mode's text, borrowed when there are no revisions.
- **Units of offsets.** Every offset is a count of Unicode scalar values, which is what
  `Editor::replace_text_range` counts. A caller on the other side of the napi boundary counts UTF-16
  code units, and an astral character (an emoji, a rare CJK ideograph) is one for us and two for it. The
  public interface takes an `OffsetUnit` (`Scalar` by default; napi passes `Utf16`), open question 3. `wordml` makes either choice cheap and
  correct: `char_to_utf16` and `utf16_to_char` convert an offset of a mode's text in both directions
  (`None` for an offset past the end, or inside a surrogate pair), so the conversion is one tested
  function and not a loop in a binding.

### 5.2 Invariants

For a paragraph P with items `I_0 .. I_n-1`:

1. **Order.** Items are in XML document order. `(kind, node)` is unique per item.
2. **Partition.** The Redline text is the concatenation of the text of every item that is not
   field code, and `at` is each item's offset in it. Marks have `len = 0`.
3. **Faithful.** A `Text` item's text is characters `[from, from + len)` of its node's decoded
   text. Every other item's text is the single character of its kind (4.1), or none for a mark.
4. **Mode text.** `text(m)` is the concatenation of the text of the items `m` includes (4.3).
   With no revision wrapper in P, the three texts are equal.
5. **Forward.** For every offset `c < len(m)` there is exactly one included item with width and one
   character index in it that produce `c`.
6. **Back.** `offset_of` of any position inside an included item gives its offset. A position on
   the boundary between two items gives the offset of that boundary from either side.
7. **Round trip.** `offset_of(at(c)) == c` for every offset of every mode, and `at(offset_of(p)) == p`
   for every position inside an included item, normalizing a boundary to the item that starts there.
   `utf16_to_char(char_to_utf16(c)) == c` for every offset of every mode.
8. **Edit agreement.** For every range `[a, b)` of `Final` over `Text` items, deleting its spans
   with `Editor::replace_text_range` on a clone gives a paragraph whose `Final` text is the old
   text with `[a, b)` removed; with atoms in the range, the same after detaching each atom's element.
   Inserting text at a gap with the same editor call gives the old text with the insertion at that
   offset. This ties the map to the one edit primitive that will really be used.
9. **Insensitivity.** A rewrite that does not change what Word shows does not change `text(m)` for
   any `m`: splitting a run at any character, merging adjacent runs that differ in nothing,
   adding or removing `w:proofErr`, bookmarks, comment range markers, `w:lastRenderedPageBreak`,
   `w:rsid*` attributes and `xml:space`, wrapping a run in a `w:smartTag`, `w:customXml` or
   `w:hyperlink`. Marks and offsets of surviving items move consistently.
10. **Purity.** The map is a function of the tree (and the story's field state). Reading never
    changes a tree. The same bytes always give the same map.
11. **Field code is invisible.** No character of a field's code is in any mode's text, however many
    fields are nested in it.
12. **Wrapper rule.** An item is in `Final` iff no wrapper is a deletion wrapper (`Del`, `MoveFrom`,
    `RowDel`, `CellDel`), in `Original` iff no wrapper is an insertion wrapper (`Ins`, `MoveTo`,
    `RowIns`, `CellIns`), whatever the element names, and the wrappers include those above the
    paragraph up to the story root.

### 5.3 Ranges across runs, fields and revision boundaries

A range is one or more spans, and the caller learns everything it needs from the spans and the
items between them. Three examples from the corpus shapes:

**Across runs.** `<w:r>Sell</w:r><w:r><w:rPr><w:b/></w:rPr>er</w:r>` has two `Text` items with
offsets `0..4` and `4..6`. `locate(Final, 2..6)` gives two spans: `ll` of the first node (start 2,
end 4, not whole) and `er` of the second (start 0, end 2, whole). The compiler deletes each, splitting runs as it
needs, and the run properties of each stay with their own text.

**Across a field.** `see section <fldChar begin/>REF _Ref1<fldChar separate/>4.2<fldChar end/> above`
has the result `4.2` at offsets 12 to 15, flagged `FIELD_RESULT`, with the marks `FieldBegin`,
`FieldCode` and `FieldSeparate` at offset 12 and `FieldEnd` at offset 15. `locate(Final, 4..16)`
gives spans for `section ` (characters 4 to 12 of its node, not whole), for the `4.2` item (whole,
flagged) and for the first space of ` above` (not whole), and `items` covers the four marks. The caller sees that the whole field is
inside the range and applies its rule: refuse as `readonly_region` when the range cuts into the
field, delete the field whole when the range covers it from `FieldBegin` to `FieldEnd`. The model
gives the facts, not the policy.

**Across a revision boundary.** `A <w:ins>B</w:ins> <w:del>C</w:del>D`: items `A `, `B` (context
`Ins`), ` `, `C` (context `Del`), `D`. In `Final` the text is `A B D`: `C` is excluded. In
`Original` it is `A  CD`. In `Redline` it is `A B CD`. `gap(Final, 4)` (between `B ` and `D`) has
`left` the space item, `right` `D`, and `between` containing the excluded `C`, so the compiler
sees the deleted run at the insertion point and decides whether to insert before or after it.
`locate(Final, 2..3)` (the `B`) returns one span whose item's context chain starts with `Ins`: the
compiler knows this is inside an insertion and applies the same-author or other-author rules
(delete outright, or nest a `w:del` inside the `w:ins`).

### 5.4 Joining paragraphs that a revised mark merges

`story.join_run(tree, i, mode)` returns the range of entries, starting at paragraph `i`, that read as
one paragraph in `mode`: `i` and each following paragraph while the previous one's `joins_next(mode)`
holds (3.6). `Joined` maps text ranges across such a run, for a `Final` search that matches across a
deleted mark, which `revisions` and `projection` would otherwise each write:

```rust
impl Joined {
    pub fn new(maps: Vec<TextMap>, mode: Mode) -> Joined;      // the maps of the run, in order
    pub fn text(&self) -> &str;                                  // the mode's texts, concatenated with no separator
    pub fn locate(&self, range: Range<usize>) -> Option<Vec<(usize, Located)>>;   // one Located for each map touched, offsets rebased
    pub fn gap(&self, offset: usize) -> Option<(usize, Gap)>;    // a gap on a join belongs to the left map
}
```

A deleted mark joins the text directly, as Word does, so offsets are sums of the maps' lengths and
there is no separator character. A range that starts in one paragraph and ends in the next gives two
`Located` values, and the caller sees which paragraph each belongs to. Which properties the merged
paragraph keeps is not decided here (3.6).

## 6. Style and numbering definitions

Definitions are read once from their part into owned structures with no borrow of the tree. They
are definitions only. The cascade is `styles`'s and clause numbers are `numbering`'s, and neither
reads XML.

### 6.1 Property containers

`w:pPr`, `w:rPr`, `w:tblPr`, `w:trPr`, `w:tcPr` and `w:sectPr` are read into one generic shape, so
no module needs its own XML reading, and the meaning of "on" and "off" is decided once. **Names keep
their namespace**, because the containers mix vocabularies: `w14:shadow` sits next to `w:shadow` in an
`w:rPr` (4 times in documents, 2 in the styles of `TextEffects_InStyle`), and `on_off("shadow")` must
not read the Word 2010 element as the Word 2007 one:

```rust
pub enum Ns { None, Word /* the w namespace, Transitional or Strict */, Ext(Box<str>) /* any other URI: w14, w15 ... */ }
pub struct Attr { pub ns: Ns, pub name: Box<str>, pub value: Box<str> }
pub struct Prop { pub ns: Ns, pub name: Box<str>, pub attrs: Vec<Attr>, pub children: Vec<Prop> }
pub struct Props(Vec<Prop>);       // document order, duplicates kept
impl Props {
    pub fn get(&self, name: &str) -> Option<&Prop>;                 // Word namespace only
    pub fn get_in(&self, ns: &Ns, name: &str) -> Option<&Prop>;     // any namespace
    pub fn on_off(&self, name: &str) -> Option<bool>;               // Word namespace only
    pub fn val(&self, name: &str) -> Option<&str>;                  // the w:val of the Word element
    pub fn attr(&self, name: &str, attr: &str) -> Option<&str>;
}
pub struct PropsRef<'a>;           // the same reads, straight over a NodeRef, no allocation; to_owned() -> Props
```

`on_off`: the element present without `w:val`, or with `true`, `1` or `on`, is `Some(true)`;
`false`, `0`, `off` is `Some(false)`; absent is `None`. Children are read to a depth of four, which
covers `w:pBdr`, `w:tabs`, `w:numPr`, `w:tblBorders`, `w:tblCellMar`, the mark's `w:rPr` inside
`w:pPr`, and the old-property snapshots inside `w:rPrChange` and `w:pPrChange`. Direct formatting in
the document (a run's `w:rPr`, a paragraph's `w:pPr`) is read through `PropsRef` with no
allocation, because hidden-text detection visits every run.

### 6.2 `StyleDefs`

```rust
pub struct StyleDef {
    pub id: Box<str>, pub kind: StyleKind /* Paragraph | Character | Table | Numbering */, pub name: Box<str>,
    pub based_on: Option<Box<str>>, pub next: Option<Box<str>>, pub link: Option<Box<str>>,
    pub default: bool, pub custom: bool, pub hidden: bool, pub semi_hidden: bool, pub q_format: bool,
    pub ui_priority: Option<i32>, pub aliases: Vec<Box<str>>,
    pub paragraph: Props, pub run: Props, pub table: Props, pub row: Props, pub cell: Props,
    pub conditional: Vec<(TableRegion, StyleProps)>,     // w:tblStylePr: firstRow, band1Horz, neCell ...
}
pub struct StyleDefs { pub doc_defaults: DocDefaults /* run, paragraph */, /* styles, indexes, issues */ }
impl StyleDefs {
    pub fn read(tree: &SpanTree) -> StyleDefs;               // infallible
    pub fn get(&self, id: &str) -> Option<&StyleDef>;
    pub fn by_name(&self, name: &str) -> Option<&StyleDef>;  // ignoring case; also aliases
    pub fn default_for(&self, kind: StyleKind) -> Option<&StyleDef>;
    pub fn chain(&self, id: &str) -> impl Iterator<Item = &StyleDef>;   // the style, then its basedOn parents
    pub fn issues(&self) -> &[StyleIssue];
}
```

- `chain` is cycle-safe and bounded: it stops at an id with no style, at an id it has already
  yielded, and after 64 steps. The corpus has chains of nine and dangling `basedOn` (48), and no
  cycle, but a hostile file can have one.
- **Duplicate style ids.** The first definition wins for `get` (30 duplicates in 5 documents). The
  rest are kept in order and reported as `StyleIssue::DuplicateId`. Which one Word uses is
  unknown, and `check` can report it.
- `issues()` reports duplicate ids, a `basedOn` or `next` or `link` that names no style, a cycle, a
  style with no id. Reading never fails: a part whose root is not `w:styles` gives empty definitions
  and an issue.
- Built-in names can be stored lower case (`heading 1`) and localized, and the id is not the
  name, so `by_name` ignores case and checks aliases. Mapping a localized name to a built-in role is
  `styles`'s.
- Theme fonts, the latent style table and the document's settings are not definitions here. The
  latent styles are skipped (up to 377 entries a document, never needed); theme fonts are
  `styles`'s, through the theme part.

### 6.3 `NumberingDefs`

```rust
pub struct AbstractNum { pub id: i64, pub multi_level: MultiLevel, pub style_link: Option<Box<str>>,
                         pub num_style_link: Option<Box<str>>, pub levels: Vec<Level> /* sorted by ilvl, at most 9 */ }
pub struct Level { pub ilvl: u8, pub start: i64 /* default 0 */, pub fmt: NumFmt, pub text: Box<str>,
                   pub restart: Option<u8>, pub legal: bool, pub suff: Suff, pub style: Option<Box<str>>,
                   pub justification: Option<Box<str>>, pub paragraph: Props, pub run: Props, pub pic_bullet: Option<i64> }
pub enum NumFmt { Decimal, UpperRoman, LowerRoman, UpperLetter, LowerLetter, Ordinal, CardinalText,
                  OrdinalText, DecimalZero, Bullet, None, Custom(Box<str>), Other(Box<str>) }   // numbering adds the renderers
pub struct Num { pub id: i64, pub abstract_id: i64, pub overrides: Vec<LevelOverride { ilvl, start: Option<i64>, level: Option<Level> }> }
impl NumberingDefs {
    pub fn read(tree: &SpanTree) -> NumberingDefs;                       // infallible
    pub fn num(&self, num_id: i64) -> Option<&Num>;
    pub fn abstract_of(&self, num_id: i64) -> Option<&AbstractNum>;
    pub fn max_num_id(&self) -> i64;                                       // 0 with no list; a new list takes the number after it
    pub fn level(&self, num_id: i64, ilvl: u8) -> Option<ResolvedLevel>; // applies lvlOverride and startOverride
    pub fn issues(&self) -> &[NumberingIssue];
}
```

- **A level's start defaults to 0, not 1.** ISO 29500 Part 1, 17.9.26 (`start`) says that when the
  element is omitted the starting value is zero. 21 counting levels in the curated set have no
  `w:start` (`alt-chunk-header.docx`, decimal, is one; 791 more are bullets, where it does not
  matter). Word's own lists always write it, and generators that omit it are the ones that get a
  list starting at 0 in Word. `numbering` computes numbers from `Level.start` as stored, so a value
  of 1 that was never in the file would be a wrong clause number.
- `level` applies the overrides: a `w:lvlOverride` with a `w:lvl` replaces the level, one with
  `w:startOverride` changes only the start. Style-linked lists (`w:numStyleLink`, `w:styleLink`)
  need both definitions, so following them is `numbering`'s, which may depend on `styles`.
- `NumFmt` has the common formats as variants and keeps any other value in `Other`. The corpus has
  about 80 distinct values, and the renderers are `numbering`'s. A lvlText such as `%1.%2(%3)` is read
  as a string.
- Duplicate `abstractNumId` or `numId` (one document each): the first wins, the rest are issues. A
  `w:num` whose abstract definition does not exist is an issue and resolves to nothing. A level
  with a repeated or out-of-range `ilvl` is an issue, and only the first of each of nine is kept.
- These definitions are built first (section 11), because `styles` and `numbering` need both before
  anything else of `wordml`.

## 7. Reading revisions

`wordml` recognizes and reads every revision element. `revisions` writes them, accepts them and
rejects them, and never has a second reader.

```rust
pub enum RevKind {
    Ins, Del, MoveFrom, MoveTo,                                   // run level
    MarkIns, MarkDel, MarkMoveFrom, MarkMoveTo,                   // the paragraph mark, in pPr/rPr
    RunProps, ParagraphProps, SectionProps, TableProps, TableExProps, TableGrid, RowProps, CellProps,
    NumberingChange,
    RowIns, RowDel, CellIns, CellDel, CellMerge,
    NumPrIns,                                                      // w:numPr/w:ins, a list number inserted
    CustomXmlInsStart, CustomXmlInsEnd, CustomXmlDelStart, CustomXmlDelEnd,        // the range markers
    CustomXmlMoveFromStart, CustomXmlMoveFromEnd, CustomXmlMoveToStart, CustomXmlMoveToEnd,
}
pub enum Branch { Outside, Taken, Untaken }     // inside an mc:AlternateContent, which branch (4.5)
pub struct Revision { pub kind: RevKind, pub node: NodeId, pub id: Option<i64>, pub author: Option<String>,
                      pub date: Option<String>, pub branch: Branch }
```

`revision_at(node)` classifies an element by its name and its parent (`w:ins` in `w:rPr` of a
`w:pPr` is a paragraph mark, in a `w:trPr` a row, in a `w:numPr` an inserted list number, anywhere else
a run-level insertion), and `story.revisions(tree)` lists every revision in the story in document
order, for `Document::revisions` and for the outline's counts. `part_revisions(tree)` does the same for
a part that has no `Story`, such as `styles.xml` and `numbering.xml`. The `w:customXml*Range*` markers
are revisions (eight kinds, start and end): they are the only record that a custom XML element, which
is a content control's wrapper, was inserted, deleted or moved, and the end has an id and no author or
date. **What a property change holds inside it is the old properties, so a revision element there is
a copy and is not listed**: Word 2010 and later copy the paragraph mark's `w:ins` or `w:del` into the
`w:rPr` of a `w:pPrChange` (114 of them in 3 curated documents), and the change itself is the revision. Attributes are read as written: `w:id` as a number when it is one,
`w:author` and `w:date` as strings, any of them absent. Move range markers are marks of the text
model and `Revision`s are not. The `w16du:dateUtc` extension is read when present.

**Both branches of an `mc:AlternateContent` are listed, each marked with its branch.** Word repeats a
text box's content in the `mc:Choice` and in the `mc:Fallback`, and a tracked change inside it is
written in both, with distinct ids (`Comment042.docx` has 6 in the Choice and 6 in the Fallback). A
caller that counts or shows revisions skips `Branch::Untaken`, so `Document::revisions` does not
count a change twice; `revisions` (the module) needs both, because accepting or rejecting a change
must resolve the copy in each branch, or the untaken one goes on holding a change that no longer
exists. `branch` is `Outside` for a revision that is not inside an `mc:AlternateContent` at all.

## 8. How `open` stays within 300 ms, and what an edit costs

`Document::open` does the container open, inflates and parses the main part, and scans it. Of
those, `wordml` owns the scan, and the scan is **the block scan only**: it visits blocks and not
paragraph content (apart from the first `w:pPr` of each paragraph), it allocates no part name, and
it does no field pass. Everything else `wordml` does is lazy and on demand: a `TextMap` is built per
paragraph when asked, the field index is built the first time a paragraph's text is read,
`StyleDefs` and `NumberingDefs` are read when first needed, and sections, text boxes, anchors,
revisions and fingerprints are computed when asked (fingerprints only at `export` and `resume`).

Measured with a throwaway release-build benchmark (not committed) over the span tree, median of
seven runs, on an Apple M-series laptop with other work running. "Scan" is a walk of the block
structure with a look at each paragraph's `w:pPr`. "Field pass" is a walk of every element looking
for `w:fldChar`, the cost of building the field index. "Text pass" is the whole text model's work:
every paragraph walked, every `w:t` decoded and counted, an item recorded. "Fingerprints" is one
FNV pass over every text node.

| Document | Blocks | Main part parse (with inflate) | Scan | Field pass | Text pass, all paragraphs | Fingerprints |
|---|---:|---:|---:|---:|---:|---:|
| EU AI Act, Council draft (main part 2.6 MB) | 3,044 | 19.0 ms | 0.24 ms | 2.2 ms | 3.0 ms | 2.6 ms |
| EU AI Act, articles 1 to 7, SLO draft | 1,441 | 9.5 ms | 0.29 ms | 1.2 ms | 1.9 ms | 1.2 ms |
| Word document with 183 fields (`content.docx`) | 757 | 3.6 ms | 0.03 ms | 0.22 ms | 0.32 ms | 0.20 ms |

The body of the first document repeated five times gives a part of the size of the largest in the
corpus (`package-and-xml.md` section 8: 12.6 MB, 15,956 blocks). Parsed in memory:

| Body | Main part | Blocks | Parse | Scan | Field pass |
|---|---:|---:|---:|---:|---:|
| EU AI Act, Council draft | 2.6 MB | 3,044 | 9.8 ms | 0.27 ms | 2.1 ms |
| the same body five times | 13.1 MB | 15,220 | 48.6 ms | 1.71 ms | 11.3 ms |

**Open.** The scan is under 2 ms at 13 MB, and the field pass is not run, so `wordml` adds under 2
ms to an `open` of the largest document, against a budget of 300 ms and a parse of 49 ms. A
300-page contract is about a third of the 872-page handbook (a 5.3 MB main part), between the two rows
above, and costs about 1 ms. `state`'s estimate for `open` stands with that 2 ms added.

**An edit.** `state` re-scans a part after it was written, and the next read of a paragraph's text
needs its field state. The first design ran the field pass in `scan`, and the review pointed out
what that costs when every operation of a batch writes and the next one reads. Measured with one
`w:t` edited through the span tree's editor, then a scan and a full field pass, 50 times, in the
first design:

| Body | Edit, scan and full field pass, 50 times | Edit and scan, 50 times |
|---|---:|---:|
| 2.6 MB, 3,044 blocks | 122 ms | 14 ms |
| 13.1 MB, 15,220 blocks | 659 ms | 86 ms |

Hence the field pass is **lazy**, the index is **shared across edits that do not touch field
structure**, and is **rebuilt from what it knows** when they do (4.4). The code review then measured the
first implementation: 299 ms for 50 text edits and 1,092 ms for 50 edits that write a field on a 13 MB
body, because the pass built a full text map of every paragraph and a refresh built two maps of the
block list. Now the pass reads only the paragraphs that hold field markup, a refresh matches the
unchanged start and end of the block list by position, and the index is shared. Measured in a
release build on this machine, 50 edits each followed by a refresh and a read of the edited paragraph:

| 50 of | 734 fields | 2,750 fields |
|---|---:|---:|
| text edits | 66 ms | 190 ms |
| edits that insert a whole field into a paragraph | 210 ms | 417 ms |
| plain paragraphs inserted | 44 ms | 54 ms |

The bodies are synthetic, 13.2 and 13.7 MB, with 11,000 blocks. The text edits are in paragraphs
picked at random, so in the second body one in four holds a field, and an edit there rebuilds the
index, which is why that column is the larger.

The scan alone is 0.9 ms and the first field pass 6.8 to 8.7 ms. A refresh after a text edit in a
paragraph with no field, or after a paragraph is written outside every field, costs the scan, about
1 ms. One after a change of field structure, or after an edit to a paragraph that holds a field, also
walks the paragraphs that hold fields and the entries once: 4 ms with 734 of them and 8 ms with 2,750,
in proportion to their number. So a batch of 50 operations that all write fields stays under 0.5 s at
13 MB even with 2,750 fields, inside the 1 s edit budget. The next step, if a document has far more
fields than that, is to keep the result of a paragraph that holds a field and whose start state and
field numbers did not change, so that only the edited ones are walked; it is not built, because no
measured case needs it.

**Cost of the rest.** Text pass: 3 ms for a 2.6 MB body and about 15 ms for 13 MB if someone asked
for every paragraph. A page view builds about 40 `TextMap`s. `StyleDefs` and `NumberingDefs`: a few
milliseconds for a typical `styles.xml` is the estimate, to be measured. Memory: a
`BlockEntry` is 24 bytes (`unit` is a `u32` index into a side table), so a 15,000-block part costs
about 370 KB; a `TextMap` is about 40 bytes an item and lives as long as its caller keeps it; the
field index is sparse; a `StyleDefs` for 164 styles is some tens of KB.

Nothing recurses on input depth. The scan, the field pass and the text builder use explicit
stacks, as `xml` does.

## 9. Strange input

The rule: **reading is total, and what was skipped is reported.** Input that parsed is never
refused by `wordml` except the one case in 3.2, the body of the document. Everything else degrades
to a defined reading and an `Anomaly`, so `check` can report problems that are already in the input
and an edit is never blocked by them (the no-new-violations rule is `check`'s).

| Input | Reading |
|---|---|
| The main part's root is not the document element, or it has no `w:body` | `scan` of the body returns `malformed_input` (`document` stage). The only error, and an edit cannot cause it |
| Any other story whose root is not the expected element (a header part with a `w:footnotes` root, say) | An empty story, with an anomaly that `check` reports. Never an error, so a re-scan after an edit cannot fail |
| A reference or relationship that does not match: a header reference whose relationship is of another type, an `r:id` with no relationship, an external or dangling target | Not a story. The reference is skipped (3.1) |
| Strict OOXML | Names are matched in both `w` namespaces, Transitional and Strict, always and by URI, so no choice is made from the root and a part that mixes the two reads the same as one that does not. The relationship namespace for `r:id` and the relationship types of 3.1 are matched in both forms |
| Unknown element where a block is expected | Skipped, and listed in `opaque()` with `w:altChunk`. Not an error, not a block, no ref |
| Unknown element inside a paragraph | Skipped, with its content, and counted as an anomaly |
| `w:p` directly in `w:p`, a table in a paragraph, a `w:r` outside a paragraph, a `w:t` outside a run | Not read as content: skipped and counted |
| Unbalanced `fldChar`: `end` or `separate` with nothing open, `begin` never closed, depth over 32 | Ignored; a field in its result phase is left open to the end of the unit, one in its code phase is closed at the end of its paragraph; depth clamped (4.4). Anomaly |
| A field nested in another field's code | Part of that code (4.4) |
| `w:t` in `w:del`, `w:delText` outside `w:del`, `w:instrText` in a deletion | Read by position (4.3). Anomaly |
| Revision with a missing `w:id`, `w:author` or `w:date`, or an id that is not a number | A revision, with the attribute `None`. Not an anomaly (the date is optional) |
| Duplicate or missing bookmark and comment ids, an end with no start | Marks are read as written. `story.anchors()` pairs them as far as it can and reports the rest with `None` ends (3.9) |
| `mc:AlternateContent` with no usable branch | Contributes nothing. Anomaly |
| `w:sym` with a missing or invalid `w:char`, or one XML cannot carry | U+FFFD, width 1. Anomaly |
| A cell with no paragraph, or not ending in one | Read as is. The cell has no entries, or fewer |
| Duplicate style ids, dangling `basedOn`, a cycle, a missing numbering definition, a repeated `ilvl` | Definitions that read, with issues (6.2, 6.3). `chain` stops at a repeat and after 64 steps |
| A `w:t` with leading or trailing whitespace and no `xml:space` | Read as written, counted as an anomaly (4.7) |
| Hostile size: 100,000 runs in a paragraph, 10,000 nested content controls, a huge `w:t` | Linear work, explicit stacks. Depth is bounded by `Limits::max_xml_depth` before `wordml` runs. The text of one paragraph is bounded by the part's size, which `Limits` bounds |

Offsets and counts are `u32` and checked: a paragraph with more than 4 billion characters cannot
exist inside a part `Limits` admits, but the arithmetic still uses checked operations and the
builder stops adding items (with an anomaly) if one would overflow. No indexing that can panic, no
`unwrap`, no arithmetic on input that can overflow, as the lints require.

An `Anomaly` is `{ kind: AnomalyKind, node: NodeId }`. The kinds are listed in `text_model.rs` and
`story.rs`. They are for `check` and for tests, and never change what a read returns.

## 10. Testing

### 10.1 Direct unit tests for the text model and the scan

`ARCHITECTURE.md` rule 5 reserved direct unit tests for `xml`, `package`, `numbering` and `schema`.
The decision is to extend it to `xml`, `package`, `numbering`, `schema`, **`wordml`'s story scan and
text model, `refs` and `state`**, because each has invariants of its own. `ARCHITECTURE.md` rule 5 says so. For `wordml` the reasons are:

1. The map has its own invariants (5.2) that are stated about the map, not about behaviour
   `Document` shows. A violation shows up through `Document` as a wrong edit offset or a missing
   word, far from its cause, and only for the shapes a law happens to exercise.
2. It was built before `Document`, which was then the only other way to test it, and revisions,
   projection and compare all build on it.
3. Fingerprints are a frozen format. A frozen format needs known-answer tests of its own.
4. The laws run through `Document`, as the rule says. The direct tests stay. They are cheap,
   they say what broke, and they test shapes no law reaches (an `ins` inside a `del` inside a
   `hyperlink`, a field spanning 61 paragraphs).

The definitions readers (`StyleDefs`, `NumberingDefs`) are tested through the corpus (counts and
issues against an independent count), through `numbering`'s own tests and through `styles`. They get
no snapshot of their own beyond that.

### 10.2 What is tested, and how

- **Known answers.** Hand-written XML for every row of 4.1 and every shape in section 1: text,
  tab, break kinds, symbols, hyphens, atoms, marks, each wrapper alone and nested to the depth
  the corpus shows, revisions in every combination (`ins`, `del`, `moveFrom`, `moveTo`, nested,
  and the row and cell kinds), complex and simple fields (spanning paragraphs, nested in the result
  and in the code, no `separate` with and without a displayed value, begin and end under different
  wrappers), `mc:AlternateContent` with each branch and `mc:ProcessContent`, text boxes, math, ruby,
  CDATA and entity references in `w:t`, the XML line-ending rules, the Strict namespaces. Each case
  states the expected text in all three modes, the items, and one `locate`, `gap` and `offset_of`.
  Fingerprints have known answers for a small document, written down once and never edited.
- **Invariants as properties.** Section 5.2, items 1 to 7 and 10 to 12, checked on every
  paragraph of the corpus and of generated paragraphs (a generator that draws nested wrappers,
  runs, fields, atoms, marks and revisions at random, with a seed). Unit conversion: for every
  offset, `utf16_to_char(char_to_utf16(c)) == c`, on text with astral characters.
- **Edit agreement and insensitivity** (5.2 items 8 and 9) as metamorphic properties through the
  real `xml::Editor`: split a run at a random character, wrap a random run range, add a mark,
  delete a random range by its spans, insert at a random gap, and compare texts. This is an oracle
  that needs no external tool. It checks that what the map says about XML is true about the one
  primitive that will write it.
- **Scan.** Order and structure on tables nested to depth four, transparent wrappers in every
  position the corpus has, `opaque()`, notes with separators, comments, a tree that the editor
  edited (clone a paragraph, move a row, delete a block) and re-scanned, which must never fail when
  the first scan succeeded, a non-body part with the wrong root (an empty story and an anomaly), and
  `stories()` with a reference whose relationship type does not match.
- **Coherence of `refresh`.** After every edit of a random sequence (insert and delete paragraphs,
  with and without field characters, edit text, split and merge, move blocks), `Story::refresh(prev,
  tree, touched)` must equal `Story::scan` on the same tree, the field index included, and an
  empty `touched` list must give the same answer when the edit changed the shape of the story. The
  shapes that fooled the first design are fixed cases: a split inside a field result (the new paragraph
  holds the field's end), an untracked delete of the paragraph that holds a field's end or separate, a
  paragraph moved into and out of the span of a field, a field begun in a new paragraph. A `touched`
  list that omits a block whose content changed makes the test fail, which is how the contract with
  `state` is checked. `state`'s operation conformance suite runs the same comparison after every
  operation. A chained variant makes the edits the way `state` does: each refresh starts from the story
  the last one returned, `touched` is the exact list of paragraphs whose own subtree changed (found by
  hashing it before and after, as `state` compares bytes), the parts are a body and a comments part,
  and in the comments part paragraphs are moved from one comment to another, which leaves the block
  list and the paragraph's bytes as they were. After each edit, every paragraph that reads differently
  in either mode, or whose items differ in width, field state or revision state, must be in the
  union of what `state` stamps: `touched`, `added`, `moved`, `context_changed` and
  `field_starts_changed`.
- **Definitions.** Properties in two namespaces side by side (`w:shadow` and `w14:shadow`), a level
  with no `w:start`, a `basedOn` cycle and a dangling `basedOn`, duplicate ids, `chain` bounds.
- **Ids.** `existing_ids` for each kind over the corpus, both branches of an `mc:AlternateContent`,
  values that are not eight hexadecimal digits, and `id_parts` against a `Rels` with and without the
  comment extension parts.
- **Anchors and joins.** Bookmarks and comment ranges that span paragraphs, an end with no start, a
  reused id, a run of paragraphs joined by deleted marks.
- **Hostile.** The same sweeps `package` and `xml` use: truncate and flip bytes of small XML parts
  that still parse, and shapes built to be slow (a field `begin` repeated 100,000 times, 10,000
  nested controls, 100,000 runs in a paragraph, 30,000 bookmarks). Each must return, never panic,
  and stay linear: a debug-build test with a generous time limit on another thread (the core may
  not read a clock, and a thread with a timeout needs only `Duration`), as `xml/tests_ns.rs` does.
- **Corpus harness.** Reads the fetched corpus, listed in `xtask/core-io-exceptions.txt` like
  the `package` and `xml` corpus laws. For every good document: `stories` and `scan` succeed or the document is an expected
  `document`-stage refusal; the number of blocks matches an independent count made by the oracle
  script (`w:p` and `w:tbl` and `w:tr` under the rules of 3.2); every paragraph's map passes the
  invariants; fingerprints are deterministic. The harness is `wordml/tests_corpus.rs` and runs from the core.

### 10.3 The oracle for paragraph text

Rule 5 and the corpus plan want an independent oracle: a way to get the right answer that does not
share our code or our reading. Two were tried and one was only checked. All numbers below are from a throwaway
comparison on the curated corpus and the LAB sample, not from the repository.

**pandoc, with `--track-changes=accept`, `reject` and `all`.** An independent reader of `.docx`
with explicit modes that mean exactly `Final`, `Original` and `Redline`. A pandoc 3.9 binary came
from the `pypandoc_binary` wheel in a private virtual environment (no system change, no network
service). I wrote the reading rules of section 4 as a short Python extractor over lxml, wrote the
document's words in each mode, and compared them with pandoc's text read through its JSON output
(the words of every paragraph, normalized for non-breaking space, U+2011 and U+00AD, with footnotes,
math and images left out).

| Population | Mode | Equal | Differ |
|---|---|---:|---:|
| LAB sample, 400 documents, 31 with revisions | `Final` | 399 | 0 (1 unreadable) |
| | `Original`, the 31 with revisions | 31 | 0 |
| Curated, not LAB, no revisions, 213 documents | `Final` | 174 | 39 |
| Curated, with revisions, 22 documents | `Final` | 17 | 5 |
| | `Original` | 18 | 4 |

On the LAB sample the reference and pandoc agree on every document in every mode. In the curated
set I sorted the first differing words of all 48 differences into the classes below. Seven classes
were then checked against the XML of a document that shows them (marked with a dagger), and the
rest are read from the words alone. Every difference I looked at is a difference of pandoc's own,
except the last class, which was a gap in my first reading.

- pandoc drops the result of some fields: a `PAGEREF` in a `w:fldSimple` (†), and by the words,
  user-name, print-date and document-variable fields. It keeps the results of others.
- pandoc puts the text of text boxes inline (†, a document with 12 `w:txbxContent`). The model
  keeps it as a separate story.
- pandoc drops the content of a cell that continues a vertical merge (†).
- pandoc maps symbol-font private-use characters to Unicode (U+F0B7 becomes a bullet). The model
  keeps the code point.
- pandoc moves paragraphs in the Title and Subtitle styles into document metadata (†).
- pandoc drops the text of a content control that shows its placeholder, and treats the glyph of a
  checkbox control differently.
- pandoc drops a whitespace-only `w:t` that lacks `xml:space`, which joins two words (†, 4.7).
- pandoc reads the `mc:Fallback` of an `mc:AlternateContent` whose `mc:Choice` requires `w15` (†),
  where Word and the model read the Choice. The two branches held different text, so the branch
  rule matters (4.5).
- pandoc treats zero-width and other invisible characters as word separators in places the
  reference does not.
- **The one gap in the reference:** a paragraph inside a body-level `w:moveFrom` (†). The revision
  wrapper is above the paragraph, not inside it, and my first extractor looked only inside. The
  model reads the wrappers above a paragraph too (4.2).

**python-docx.** Its `Paragraph.text` reads only the direct `w:r` and `w:hyperlink` children of a
paragraph. That makes it an exact oracle for the paragraphs most documents consist of, and a blind
one for everything else (`w:ins`, `w:sdt`, `w:fldSimple`, `w:smartTag`), which is where the model
is hard. On the same LAB sample, 368 of 399 documents are equal on all of their body paragraphs
under normalization (python-docx drops page breaks), over 93,955 paragraphs compared. The 31 that
differ (1,521 paragraphs) are as many as the documents with revisions in the sample, and the
first differences I looked at are text inside `w:ins`. In the curated set 187 of the 224 it could
open are equal, and the differences I looked at are the same blind spots. python-docx refuses 18
curated documents.

**LibreOffice in a container.** Useful for the things pandoc does not read the same way: field
results, text boxes, numbering labels. When this design was written nothing was run in a container. It must not bind ports: convert with
`soffice --headless --convert-to txt` after accepting or rejecting all changes through a macro,
reading no port (a named pipe if the UNO API is needed). It is the second tier, and
`numbering` needs it for clause numbers anyway.

**Recommendation.** pandoc in all three modes is the primary oracle for the text of paragraphs,
and python-docx is a second one for plain paragraphs. The two cover different ground: pandoc
covers revisions in all three modes and the block structure around them, python-docx covers
characters and breaks exactly. Neither covers fields or content controls well (pandoc drops some
field results and placeholders), so those rest on hand-written known answers and on the
LibreOffice tier (the testkit's LibreOffice reader). The arrangement:

1. An xtask command, `cargo xtask corpus oracle-text`, runs the tools, which the core may not do,
   and writes one expected-words file per document and mode into the corpus cache, next to
   `INDEX.tsv`. It skips when a tool is missing, as other oracle runs do. It would run nightly and on
   demand and never in `cargo xtask check`. **It is not implemented**, and neither is the table of item 3;
   what exists is the testkit's `oracle::pandoc_text` and its LibreOffice reader, which tests use as
   independent readers where the tools are installed.
2. The corpus harness reads those files and compares them with `TextMap` through the same
   normalization.
3. A table, `corpus/oracle-text-known.tsv`, lists each known divergence by document, mode and
   **class** (the classes above, each with a one-sentence reason, like `corpus/reasons.tsv`). A
   divergence not in the table fails the test, and a table row that no longer diverges fails too.
   So a new difference is always a decision, never noise.

### 10.4 What is not tested here

Word. No Word is available on the development machine. The behaviours the model
assumes about Word (4.7 whitespace, the join of a deleted paragraph mark, which Word style wins on a
duplicate id) are named in the open questions and checked when an oracle run on a machine with
Word is available.

## 11. Build order

`styles` and `numbering` need the definitions first, and `state` needs `Story` second. The two refs blockers
found in review are in the design of `refs` and `state`, and they do not touch `wordml`, so this
order does not wait for them.

| Step | Files | Delivers |
|---|---|---|
| 1 | `api/view.rs` (`Mode`, data only, re-exported by `lib.rs`), `names.rs`, `props.rs`, `styles_defs.rs`, `numbering_defs.rs`, `testing.rs` | `Mode`, `Props`, `StyleDefs`, `NumberingDefs`, the dialect handling, test builders. Unblocks `styles` and `numbering` |
| 2 | `story.rs`, `story_scan.rs`, `fingerprint.rs`, `rels.rs`, `ids.rs`, `refresh.rs` | `Story::scan` and `refresh`, `BlockKey`, fingerprints, `stories`, `Rels`, `existing_ids`, `id_parts`, the version constants. Unblocks `refs` and `state` |
| 3 | `text_model.rs` (the interface and contract), `text_build.rs`, `text_fields.rs`, `mc.rs`, `revision.rs` | `Paragraph::map`, items, fields, modes, revision reading |
| 4 | `text_locate.rs`, `text_join.rs`, `anchors.rs`, `object.rs`, `control.rs`, `sections.rs`, `block.rs` | `locate`, `gap`, `offset_of`, `find_all`, the UTF-16 helpers, `Joined`, anchors, views, sections, text boxes |
| 5 | corpus harness, the metamorphic and coherence tests | the evidence |
| 6 | `cargo xtask corpus oracle-text` and the known-divergence table | the independent oracle (10.3); not implemented |

The module is split along concepts so that no file passes 600 lines: `text_model.rs` holds the
interface and its invariants as the contract, and the builder, the field machine and the locate
operations are separate files. `mod.rs` keeps its contract, updated for `BlockKey`, `stories`,
`Rels` and the id readers, and "Start reading" stays `wordml/text_model.rs`.

## 12. Changes to other documents, what the review changed, and open questions

### 12.1 What the design review changed in this document

An independent review of this document and of `refs-and-state.md` asked for the changes below, and
each open question was settled. Every item is applied in this document, at the section named.
Sentences that both documents carry were agreed with `state` and are marked in section 3.

| Item | Where |
|---|---|
| `Mode` lives in `api` as data only | 4.6, 11 |
| Direct unit tests also for `refs` and `state`; rule 5 says so | 10.1 |
| The `mc:AlternateContent` rule lives once, in `schema`; `mc:ProcessContent` is part of it; `wordml` keeps no copy | 4.5, 4.2 |
| Whitespace in `w:t`: read as written, in one named function, and an open question for the Word oracle; writes always carry `xml:space="preserve"` | 4.7 |
| Text boxes: no refs, read-only inside the host placeholder, an edit is `unsupported`, recorded for `FIDELITY.md` | 3.8 |
| The merge of a deleted paragraph mark is for `projection` and `revisions`; `wordml` gives `joins_next`, `join_run` and `Joined` | 3.6, 5.4 |
| The text oracle runner runs nightly and on demand (not implemented) | 10.3, 11 |
| No `settings.xml` reader until a consumer needs one; `story.anchors()` is in scope | 3.9 |
| W1: a field nested in field code is code | 4.4 |
| W2: fields with no `separate` that Word shows become one read-only atom | 4.4, 4.1 |
| W3: row and cell revisions are contexts | 4.2, 4.3 |
| W4: property names keep their namespace | 6.1 |
| W5: a level's start defaults to 0 | 6.3 |
| W6: `stories()` requires the relationship type to match; only the body can fail `scan` | 3.1, 3.2, 9 |
| W7: sections depend on the mode (`Section.mark_revs`) | 3.7 |
| W8: revisions of both branches, each marked with its branch | 7, 3.8 |
| W9: the chain and the controls walk up to the story root | 4.2, 3.8 |
| Nits: Word's `Range.Text` claim removed, `w:sym` C0 controls, escaping of control characters, field-result policy, row fingerprint wording, interface summary, `Arc<str>` part name, annotation ids | 4.1, 4.4, 3.4, 2, 3.2, 3.5 |
| Inconsistency 1, `existing_ids` coverage, with `id_parts` | 3.5 |
| Inconsistency 2, `opaque()` lists every unknown block-level element | 3.2 |
| Inconsistency 3, the open and commit estimates include the field pass, now lazy | 4.4, 8 |
| Inconsistency 4, `Rels` targets and `Invalid` | 3.1 |
| Inconsistency 5, a re-scan cannot fail | 3.2, 9 |
| Inconsistency 6, caches keyed by part, never by ordinal | 3.3 |
| Inconsistency 7, row and table fingerprints, `wordml` owns the words | 3.4 |
| Inconsistency 8, versions: recorded, recomputed, `unsupported_version`, major releases only | 3.3 |
| Inconsistency 9, rule 5 covers both | 10.1 |
| Inconsistency 10, invalidation by generation checks on read | 3.3 |
| S8 for `wordml`: field-pass cost per edit measured; lazy and incremental; `Story::refresh` | 4.4, 8 |
| S9 for `wordml`: `BlockEntry.unit` is a `u32` index | 3.2, 8 |
| For `revisions`, `styles`, `numbering`, `comments`, `projection` and `ops`: the join helper, the UTF-16 helpers, the data-bound control note, anchors, the escaping rule | 5.4, 5.1, 3.8, 3.9, 4.1 |
| Delta review: `refresh` compares block lists by `NodeId` and finds new, removed and moved blocks itself, so `touched` only covers edits inside blocks; the carried field index needs every field's begin, separate and end still attached | 3.3, 4.4 |
| Delta review: `refresh` reports `added`, `removed`, `moved` and `field_starts_changed` (`Refreshed`) | 3.3, 4.4 |
| Delta review: a `Story` records the tree's `edit_count` and refuses another tree | 4.4 |
| Delta review: what a table or a row owns, apart from its nested blocks (`own_parts`) | 3.6 |

### 12.2 Documents to change with the code

- `ARCHITECTURE.md`: the `wordml` row of the module contracts table (`BlockKey`, `stories`, `Rels`,
  the id readers, the version constants) and the "Where do I change X" table, for a new inline
  element or a new block kind.
- `GLOSSARY.md`: `Item` (the model's unit of paragraph content), `Mark` (a zero-width item),
  `Anomaly`, `Unit` (a note or comment inside its story).
- `api`: `api/view.rs` with `Mode`, and its re-export from `lib.rs`. Data only, no decision record.
- `docs/design/package-and-xml.md` or the `xml` contract: the `NodeId` stability assumption of 3.3, with
  a test. The `xml` contract states it.
- `FIDELITY.md`: text boxes in 1.0 (3.8).

### 12.3 Open questions

1. **Whitespace in `w:t` without `xml:space`** (4.7). Settled on 2026-10-10 by Word for the web (pack file 37):
   every character counts, as Word shows it, in `text_span`. LibreOffice drops the white space at the ends.
2. **The merge of a deleted paragraph mark** (3.6). Which paragraph's properties survive, and which
   ref the merged block shows under, are for `projection` and `revisions` and need a Word oracle run.
   `wordml`'s part is the join helper.
3. **The public unit of text offsets** (5.1). Settled: scalar values inside, and the public interface takes
   an `OffsetUnit`, `Scalar` by default, which napi sets to `Utf16` because a JavaScript caller counts UTF-16
   code units. `wordml` has both conversions.
4. **The shape of `schema`'s branch function** (4.5). Settled: `taken_branch` and `disposition` over a
   `NodeRef`, `pub(crate)` in `schema`, called only through `wordml/mc.rs`. Writers that ask `schema` for a
   context in an untaken branch get one as if the branch were read, which matters to them and not to
   this reader.
5. **Is `Joined` the shape `revisions` needs** (5.4)? Settled: no. The design of `revisions` finds that every
   primitive of `revisions` works inside one paragraph, and `Joined` stays for `projection`, whose search
   matches across a deleted paragraph mark. A paragraph merged by a deleted mark takes the later
   paragraph's properties and the first paragraph's ref, as `revisions.md` 3.2.4 and 6.3 say, which
   still needs the Word oracle of question 2.
6. **The `touched` contract of `Story::refresh`** (3.3). Settled: `touched` covers edits inside existing
   blocks. New, removed and moved blocks are found by `refresh` itself, by comparing block lists. A
   wrong list fails the coherence test, so the contract is checked.
7. **The oracle runner** (10.3) is not implemented.

## References

- [`ARCHITECTURE.md`](../../ARCHITECTURE.md): module contracts, allowed dependencies, cross-cutting
  rules.
- [`GLOSSARY.md`](../../GLOSSARY.md): story, block, ref, text range, mode, revision, field.
- [`docs/design/package-and-xml.md`](package-and-xml.md): the span tree, `NodeRef`, the editor.
- ECMA-376 Part 1, WordprocessingML; Part 3, Markup Compatibility.
