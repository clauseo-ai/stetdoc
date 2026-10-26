# Design: `revisions`

Design notes for the `revisions` module: the compiler that writes Word tracked changes, and
the engine that accepts and rejects them. `ARCHITECTURE.md` is the map and the contract in
`revisions/mod.rs` is the interface (its text is in section 10). This document says
what Word writes, what stetdoc writes in its place, how accept and reject work, which laws the module is
tested against, and what it needs from the modules around it. Terms follow [`GLOSSARY.md`](../../GLOSSARY.md):
revision, accept and reject, mode, ref, block, story, field, content control. "Span" means a byte span in
the span tree, never a range of text. The designs of `wordml` and of `refs` and `state` are in
[`wordml.md`](wordml.md) and [`refs-and-state.md`](refs-and-state.md), and this
document quotes the parts of them it builds on.

This began as a design only, and the code has since been built in the order of section 13. Section 16 lists every
place where the build differs from what is written above it: where the two disagree, section 16 is right.

## The decisions that matter most

1. **One compiler, ten primitives, one resolver** (2). `Compiler` wraps a `Tx` and a `Track` (a stamp, or
   untracked). Text, properties, blocks, rows and range markers each have one entry point, and every operation, hand
   edit, comment anchor and compare reaches the document through them. `set_props` is one generic primitive: a target
   (run range, paragraph, mark, section, table, row, cell) and a property edit. It writes the matching `*PrChange` with a
   snapshot that the schema chooses, so there is no per-property code. One **copy rule** says that every copy of
   properties, a run or a row loses its change-tracking elements and section break, so a copy never carries someone
   else's claim.
2. **Replace is word-level, cleaned, and deletion comes before insertion** (2.2, 2.5, 4). A replace trims the common
   prefix and suffix, diffs the rest by words, dissolves short unchanged islands so that dense edits become one
   replacement, and applies the edits right to left over **one** text map per call. Word writes `del` then `ins` for
   typing over a selection (139 same-author pairs in 24 documents, against 45 in 9 the other way), so the compiler does too.
3. **What gets written depends on whose text it is** (4). Plain text gets our `del`. Our own pending
   insertion is removed outright, as Word does. Another author's insertion keeps its one `w:ins` and gets
   our `w:del` nested among its runs, which is the shape Word 16 writes. An insertion inside a pending
   insertion splits it. A pre-existing revision is never edited, extended or re-attributed: it can be split,
   nested into, removed by its own author's deletion, or resolved, and nothing else.
4. **Paragraph marks follow the shapes Word writes** (3.2). Inserting after a paragraph that has a successor, and
   splitting, are Word's Enter key (Y): the first paragraph's mark becomes inserted and the new paragraph holds the
   original mark, and so is inserting after the last paragraph of a container, whose mark is the container's end and survives
   any rejection (Word's Reject All left such a paragraph empty in a cell, check C11). Inserting before a paragraph is the
   self-contained shape (X): a new paragraph with an inserted mark and inserted content (`FA/009`). A merge deletes the first
   paragraph's mark. For rejecting an inserted mark, `FA/005` shows that Word's own file restores the original only if the
   **later** paragraph's properties win. For accepting a deleted mark nothing in the corpus decides, so a merge of paragraphs
   whose properties differ also writes a `pPrChange` that makes the result the same whichever rule Word uses.
5. **Identity is the element, and a "change" is what agents see** (6.1). A revision keeps its `w:id`.
   Splitting a pending insertion gives the tail a new id, so accept and reject by id act on the **change**
   that contains the id. A change is grouped by the paragraph's items in text order, across wrappers: the pieces of a
   split insertion with another author's revision between them are still one change, and so are a paragraph deletion
   with its mark, a move with both halves, and a field with its pieces.
6. **Identifiers are derived and small** (5). Revision ids are the highest id in the document plus one,
   allocated from one counter per `Tx` so that comments cannot collide. A date is **the wall-clock time of the date the
   caller gives**, with seconds and `Z`, which is Word's convention of local time labelled `Z` (a UTC caller gets UTC, and
   the CLI and host applications pass local time). New paragraphs get a `w14:paraId` only if the paragraph they come
   from has one, and `w14:textId="77777777"`, the value Word writes on 83% of paragraphs. No `w:rsid*`, no `dateUtc`.
7. **Accept and reject are selective and structural** (6). They run by id, by author or on everything, inside
   out, with fields and moves resolved as units, and with one index per batch. A deleted paragraph mark **joins**
   paragraphs: the first paragraph's element survives, with the later paragraph's properties, so a split followed by a
   reject restores the original `w:p`. The first paragraph's ref survives. Section 6.4 lists every place this differs from
   OpenXmlPowerTools.
8. **The laws are checked with oracles that do not share the code under test** (7, 8). The text model of
   `wordml` is the oracle for the text of every mode, a string model is the oracle for what a batch means,
   pandoc is a third reader for accept and reject, and 76 small scenario documents that Word wrote are the
   oracle for shapes, with no known divergence. No Word is needed to run any of them, and section 8.4 lists what only a
   person with Word can settle.
9. **The cost is small** (9). The XML editing of a tracked replace is about 13 µs, measured on a 30 MB body. With
   one text map per call the compiler adds about 0.2 to 1 ms an operation (an estimate), and one pass over the
   document for the highest id, which the state caches. A 50-operation batch on the largest body stays inside the
   1 s budget with more than 500 ms left for `check`.
10. **Seventeen asks of other modules, and ten design decisions** (11, 12). The asks are small. Two of the decisions are deliberate differences from Word, for `FIDELITY.md`: a paragraph
    that carries a section break keeps its mark and the break when it is deleted, and `w:date` has no `dateUtc`.

## 1. What Word writes: the census

### 1.1 Method

The design rests on what Word wrote into real documents, not on the prose of the specification. The corpus
was fetched with `cargo xtask corpus fetch --full --dir /tmp/stetd-d-corpus-full` (48,666 documents, 3.0 GB,
a private directory that has been deleted), and the scans were throwaway Python scripts over `lxml` that are
not committed. Section 13 turns the findings into a catalog test that anyone can rerun, so the numbers below
can be checked without the scripts.

- **Population.** The 6,908 non-LAB documents of `expect = ok`. Harvey LAB is python-docx output and holds no
  Word revisions. 408 of the 6,908 contain revision markup in a story part.
- **Word-fingerprinted.** The manifest's `word_fp = y`: 4,415 documents, of which **257** contain revisions
  (producer Word 16 in 175, Word 15 in 43, Word 14 in 27, Word 12 in 11). 168 of the 257 have run-level
  `ins` or `del`.
- **Strict.** 84 of those 168 carry `w:rsid*` on more than 80% of the runs inside revisions, which is what
  Word writes and what a tool that edits XML usually does not. Where a finding comes only from a document
  that is not strict, the text says so, because several "Word" documents are fixtures that a tool
  produced from a Word template (the `docx-redline-js` fixtures, the Aspose gold files).
- **Scenario documents.** 76 small documents were made to show one kind of edit: the 24 `docxodus` documents
  `TestFiles/FA/RevTracking/001` to `024` (Word 16, one author) and the 52 OpenXmlPowerTools
  `TestFiles/RP/RP0xx-*.docx` that are not an `-Accepted` or `-Rejected` companion. They are the closest thing to Word
  writing a named scenario, but not every one is a single edit by one author in Word 16. All but twelve of the `RP`
  files are. The twelve are `RP001` (two files, Word 15), `RP038`, `RP049` and `RP051` (no Word producer string),
  `RP042` to `RP044` (several authors), `RP047` and `RP048` (two authors), `RP037` (its only revisions are in
  `styles.xml`, where two ids repeat) and `RP052` (the author is "Author" and there is no date). No finding below leans
  on them alone. The `-Accepted.docx` and `-Rejected.docx` companions of the `RP` files are **not** Word's
  output: the OpenXmlPowerTools test copies its own processed result over them
  (`RevisionProcessorTests.cs`, the `copy` lines), so they are regression baselines of that code and are used
  only as such (section 8.2).

### 1.2 What the corpus holds

Word-fingerprinted documents, occurrences and documents:

| Shape | Occurrences | Documents |
|---|---:|---:|
| run `w:ins`, `w:del` | 738, 610 | 120, 134 |
| paragraph mark `ins`, `del` | 510, 491 | 54, 75 |
| mark `moveTo`, `moveFrom` | 27, 27 | 14 |
| run `moveTo`, `moveFrom` | 70, 68 | 25 |
| `rPrChange`, `pPrChange` | 875, 224 | 42, 37 |
| `tcPrChange`, `tblPrChange`, `trPrChange`, `sectPrChange`, `tblPrExChange` | 135, 13, 5, 5, 7 | 18, 13, 4, 5, 6 |
| `tblGridChange` | 18 | 18 |
| row `ins`, `del` | 42, 67 | 14, 17 |
| `cellIns`, `cellDel`, `cellMerge` | 36, 12, 6 | 4, 4, 1 |
| `numPr/ins`, `numberingChange` | 3, 61 | 3, 3 |
| `ins` or `del` inside `m:r`, inside `m:ctrlPr` | 17 and 11, 8 and 3 | 4, 1 |

What the attributes look like (3,151 dated elements, 4,209 revision elements in all):

- `w:id` and `w:author` are always there. `w:date` is absent on 1,058 of 4,209 (25%): Word leaves it out
  when personal information is removed. `w16du:dateUtc` is on about 10%. `w:tblGridChange` has an id only.
- Every `w:date` of a Word document ends in `Z`. 3,108 have zero seconds, 43 do not, and those 43 are
  in two files that Aspose wrote with a Word producer string. None has a fraction or an offset. Documents
  from other producers write seconds, fractions and offsets freely (729 with an offset and a fraction in
  the non-Word population), and Word opens them.
- `w:date` is the author's local wall-clock time with a `Z`, and `w16du:dateUtc` is the real instant. In
  Word 16 documents the two differ by the author's offset (+2 h in 228 elements, +10 h in 56, -7 h in 30).
- `w:id` is unique in 256 of 257 documents. Revision ids never equal a bookmark id in 256 of 257, because
  Word numbers every annotation from one counter at save (the minimum id is 0 in 198 of 252). Comment ids
  are numbered apart.
- `people.xml` lists every revision author in 157 of the 174 documents that have the part, and 83 of the
  257 have none.
- Word writes `xml:space="preserve"` only when the text needs it (4,006 of 4,024 `w:t` that need it carry
  it, and 8,602 of 8,684 that do not need it do not).
- `w:delText` is always inside `w:del` (753 of 754). `w:t` is never inside a `w:del`. It is inside
  `w:moveFrom` (46 times): moved-from text stays `w:t`. `w:delInstrText` is inside `w:del` (32 of 32).
- `w14:paraId` is all or nothing: of 4,341 documents, 2,852 have none, 1,450 have it on every paragraph and
  39 are in between. `w14:textId` is `77777777` on 18,553 of 22,293 paragraphs.

Where each element sits (Word-fingerprinted documents, checked against the schema tables of `schema`):

| Container | Child | Where Word puts it |
|---|---|---|
| paragraph mark `w:rPr` | `ins`, `del`, `moveFrom`, `moveTo` | first (1,138 of 1,138). `ins` then `del` when both are there (19 marks in 4 documents) |
| `w:rPr` (mark or run) | `rPrChange` | last (853 of 875) |
| `w:pPr` | `pPrChange` | last, after `rPr` and `sectPr` (222 of 224) |
| `w:sectPr`, `w:tblPr`, `w:tblGrid`, `w:tblPrEx` | their `*Change` | last |
| `w:trPr` | `ins`, `del`, `trPrChange` | `trHeight` and the rest, then `ins`, `del`, `trPrChange` |
| `w:tcPr` | `cellIns`, `cellDel`, `cellMerge`, `tcPrChange` | after `tcW` and the other properties, `tcPrChange` last |
| `w:numPr` | `numberingChange`, `ins` | after `ilvl` and `numId` |

### 1.3 Word writes X for Y

Confidence is **W** when a Word-written document shows it (the fixture id is the file name in the corpus),
**T** when a tool-made fixture or the reference documents say so, **I** when inferred from the schema, and
**U** when nothing in the corpus decides it (these are the checklist of section 8.4).

| Scenario | What Word writes | Evidence | Conf. |
|---|---|---|---|
| Delete a word inside a paragraph | `w:del` around the run with `w:delText`. Neighbours untouched. The run is split, never the paragraph | `FA/001-DeletedRun` | W |
| Replace a word | `w:del` then `w:ins`, adjacent, each with its own id. Counted over the full corpus of 48,666 documents, same-author pairs are `del` then `ins` 139 times in 24 documents, and `ins` then `del` 45 times in 9 | the corpus census | W |
| Type new text | `w:ins` with a run that copies the neighbour's `rPr` | `FA/010-InsertedRun` | W |
| Delete a whole paragraph in the middle | content in `w:del`, mark `del`. The next paragraph is untouched, even when its `pPr` differs. 52 whole-paragraph deletions in the corpus, three of them with an empty-snapshot `pPrChange` that changes nothing (`RP040`) | `FA/013-DeletedParagraph`, `FA/002` | W |
| Delete from the end of one paragraph through the end of the last | the first paragraph keeps its text and its mark gets `del`. The middle paragraph's content and mark are `del`. The last paragraph's content is `del` and its mark stays live, because nothing follows it | `RP040` | W |
| Insert a whole paragraph before another | new paragraph with mark `ins` and content in `w:ins`. The paragraph before and the one after are untouched | `FA/009-InsertedParagraph` | W |
| Press Enter at the end of a paragraph (that has a successor) and type | the existing paragraph's mark becomes `ins`. The new paragraph holds the new text in `w:ins` and the original mark. When the user then makes both a list, the first paragraph has the list `pPr` and no `pPrChange`, and the second has the list `pPr` and a `pPrChange` whose snapshot is the original, empty `pPr` | `FA/005-InsertedNumberedParagraph` | W |
| Press Enter in the middle (split) | first half keeps its text and gets mark `ins`. Second half holds the rest and a plain mark | `RP006` | W |
| Delete a paragraph mark (merge) | the first paragraph's mark gets `del`. Both paragraphs' text is untouched. Of 9 merges whose paragraphs have different `pPr`, one has a `pPrChange`, and the others none | `RP005`, `RP007` | W |
| Delete a run of consecutive marks | each mark but the last gets `del` | `RP007` | W |
| Press Enter at the end of the last paragraph and type (append paragraphs) | the first paragraph is untouched. Each new paragraph has mark `ins` and content in `w:ins`, and the last one is the last paragraph, with an inserted mark. This is the self-contained shape, where Enter before a successor writes the Enter shape | `RP039` | W |
| Delete inside another author's insertion | one `w:ins` of the first author that keeps its runs and holds the second author's `w:del` among them: `ins{r del{r} r}`. 9 `ins` hold both `del` and `r` children | `docx-redline-js` `cross-author-slicing/*` (T, `word_fp = y`), `issue-319-sections` (Word 16) | W/T |
| Insert inside another author's insertion | the `w:ins` is split in two. The new `w:ins` sits between. The tail has a new id | `cross-author-slicing/insert-interior-pending` | T |
| Delete your own pending insertion | the text disappears, with no mark | Docxodus corner cases, "What Word writes" | T |
| Format text | `w:rPrChange` as last child of the run's `w:rPr`, holding the old properties. One per run, each with its own id | `FA/019`, `n830205` (589) | W |
| Format text inside your own insertion | an `rPrChange` as well (72 of 75 runs in `w:ins` with an `rPrChange` have the same author) | `issue-319-sections`, `run_property_change` | W |
| Change paragraph formatting | `w:pPrChange` last in `w:pPr`, holding the old `pPr` without `rPr` and `sectPr` | `FA/018`, `RP025` | W |
| Change the style | `pPrChange` whose snapshot has the old `pStyle` | `n830205` | W |
| Change a list level or number | `pPrChange` whose snapshot holds the old `numPr` and `ind` | `RP022`, `RP023`, `FA/017` | W |
| Turn text into a list | `pPrChange` (old `pPr` empty) on each paragraph. Older Word wrote `numPr/ins` instead | `FA/004`, `RP021` | W |
| Change paragraph mark formatting | `rPrChange` in the mark's `w:rPr` | `RP024` | W |
| Change section properties | `sectPrChange` last in the `sectPr`, holding the old one without header or footer references | `FA/020`, `RP027` | W |
| Change table, row, cell properties | `tblPrChange`, `trPrChange`, `tcPrChange`, `tblPrExChange`; the grid gets `tblGridChange` with an id only | `FA/021` to `024`, `RP028` to `RP033` | W |
| Insert a row | `trPr/ins`, and in each cell the paragraph mark `ins` and the content in `w:ins`. No `cellIns` | `FA/007`, `RP010` | W |
| Delete a row | `trPr/del`, each paragraph's mark `del`, content in `w:del` (including the last paragraph of each cell) | `FA/006`, `RP009` | W |
| Merge or split cells | `cellMerge`, `cellIns`, `cellDel` with `tcPrChange` | `RP034`, `RP035`, `RP036` | W |
| Move a paragraph | `moveFrom` and `moveTo` paragraphs, each with mark `moveFrom` or `moveTo`, content in `w:moveFrom` or `w:moveTo` (text stays `w:t`), a `moveFromRangeStart` or `moveToRangeStart` inside the paragraph and the matching `*RangeEnd` between paragraphs. The two ranges share `w:name`, each marker has an id | `FA/014`, `RP015` | W |
| Delete a field | one `w:del` around every run from `begin` to `end`, with `delInstrText` and `delText` | `RP019`, `FA/003` | W |
| Insert a field | `ins` around `begin` and the code, **the `separate` run outside any `ins`**, `ins` around the result and `end` | `RP020`, `FA/008` | W |
| Change a math character | `m:r` > `w:ins` or `w:del` > (`m:rPr`, `w:rPr`, `m:t`) (the schema does not allow it, Word writes it) | `RP013`, `RP014`, `FA/011`, `FA/012`, `tracked-changes-equations` | W |
| Insert or delete a content control | `customXmlInsRangeStart` and `End`, or `customXmlDelRange*`, around the `w:sdt`. The control's text is **not** wrapped | `RP016`, `RP017`, `FA/015`, `FA/016` | W |
| Move a paragraph that is in a content control | `customXmlMoveFromRange*` around the `w:sdt`, with `w:displacedByCustomXml` on the `moveFromRangeStart` | `RP018` | W |
| Revision inside a text box | written twice, in the `mc:Choice` and in the `mc:Fallback`, with different ids | `Comment042` | W |
| Number a `LISTNUM` field again | `w:numberingChange` with `w:original` (the old label) inside the field's `w:fldChar` | `RP026` | W |
| Which `pPr` survives when a deleted mark is accepted | not decided by any file: 9 merges with different `pPr`, no consistent shape. For **rejecting** an inserted mark `FA/005` decides it: the later paragraph's | `FA/005` | **I** (reject), **U** (accept) |
| The same author deletes inside an insertion by that author from an earlier session | not in a Word document | | **U** |
| Insert a paragraph after the last paragraph of a cell | not in a scenario document. `RP039` shows the shape for the body | | **U** |

### 1.4 What the census cannot say

The corpus holds what Word wrote, not what Word does on accept and reject. Five things follow, and each
is a risk the design handles by being correct under every answer or by refusing. They are the checklist
for a person with Word in section 8.4: which `pPr` survives when Word accepts a deleted mark that has text
on both sides (for rejecting an inserted mark `FA/005` says the later one); what Word does when a deleted mark has no paragraph after it; whether Word merges adjacent
insertions of one author and date (6 adjacent pairs have the same date, 35 have different dates);
whether Word shows a revision dated with non-zero seconds; and what Word does with the unwrapped `separate`
run of an inserted field when it rejects.

## 2. The interface

Everything in this section is `pub(crate)`, as every module below `document` is. Types from `wordml` are used
with their own names: `BlockKey`, `RevKind`, `Story`, `TextMap`, `Mode`, `NodeId`, `SpanTree`. Types from
`state` are `Tx` and `PartsView`. A text offset is a count of Unicode scalar values in the **Final** text of
one paragraph, which is what `TextMap::locate` and `TextMap::gap` take. The public unit (scalar values or
UTF-16) belongs to `api`, and `wordml` converts both ways.

### 2.1 Types

```rust
/// Who is editing and whether it is recorded. A stamp has no default, as in the public interface.
pub enum Track { Tracked(RevStamp), Untracked }

/// An author and a date that are known to be writable. The author is non-empty, at most 255 characters
/// and free of control characters. The date is validated and never converted (5.3): it is a real calendar
/// date and time of day, `YYYY-MM-DDTHH:MM:SS` with an optional fraction and an optional `Z` or offset.
/// What is written is its first 19 characters and `Z`.
pub struct RevStamp { /* author: String, date: String */ }
impl RevStamp {
    pub fn new(author: &str, date: &str) -> Result<RevStamp, StetError>;        // invalid_payload
    pub fn from_stamp(stamp: &crate::api::Stamp) -> Result<RevStamp, StetError>; // fields are api's, see asks
    pub fn author(&self) -> &str;
    pub fn date(&self) -> &str;    // as written: YYYY-MM-DDTHH:MM:SSZ, which the receipt records
}

pub enum Place { Before, After }

/// A new paragraph: its text (a tab is `\t`, a line break `\n`) and the properties that differ from
/// the paragraph it is modelled on. No properties means "the same as that paragraph".
pub struct NewParagraph { pub text: String, pub props: PropEdit }

/// What a property edit changes. Names are expanded: a namespace URI and a local name.
pub struct PropEdit { pub set: Vec<PropElement>, pub clear: Vec<(String, String)> }
/// The one description of a new element in this module: a namespace URI, a local name, attributes and children.
pub struct PropElement {
    pub ns: String, pub local: String,
    pub attrs: Vec<(String, String, String)>,      // (namespace URI or empty, local name, value)
    pub children: Vec<PropElement>,
}

/// The container of a property edit. `Run` covers the direct formatting of the runs that hold the range.
pub enum PropTarget {
    Run { block: BlockKey, range: Range<usize> },
    ParagraphMark { block: BlockKey },             // w:pPr/w:rPr
    Paragraph { block: BlockKey },                 // w:pPr
    Section { block: BlockKey },                   // the paragraph whose w:pPr holds the w:sectPr
    BodySection { part: Arc<str> },                // the w:sectPr that ends the body
    Table { table: BlockKey },                     // w:tblPr
    Row { row: BlockKey },                         // w:trPr
    Cell { row: BlockKey, cell: usize },           // w:tcPr of the cell at that index
}

/// Range markers for `place_markers`: the start element, the end element and, when the caller wants
/// one, the run that holds the reference (a comment's `w:commentReference`). The caller mints the ids.
pub struct MarkerSpec { pub start: PropElement, pub end: PropElement, pub reference: Option<PropElement> }

/// What one call did. The totals of a `Compiler` are an `Outcome`, which is the effects plus warnings.
pub struct Effect {
    pub minted: Range<u32>,                        // the annotation ids this call took from the Tx's counter
    pub split: Vec<(i64, i64)>,                    // (old id, new id): a pending revision cut in two
    pub removed_own: Vec<RemovedOwn>,              // own pending text, marks or rows removed outright
    pub folded: Vec<Folded>,                       // property edits folded into the author's pending change
    pub swallowed: Vec<i64>,                       // ids of revisions that went with an element removed outright
    pub new_blocks: Vec<BlockKey>,                 // elements created; their refs are minted at commit
    pub origins: Vec<Origin>,                      // where each new or surviving block came from
    pub survivor: Option<BlockKey>,                // merge: the element that remains
}
/// `block` is a new element, or the element that survives a merge. `from` are the blocks whose state before the
/// call it was copied or built from: the block that was split, the twin, the anchor row, or both
/// paragraphs of a merge. The no-new-violations check compares the units of `block` with those of `from`.
pub struct Origin { pub block: BlockKey, pub from: Vec<BlockKey> }
/// An own pending change that an edit went into instead of a new revision: the id of the `*PrChange`, or the id of
/// the inserted mark of a paragraph that was edited directly (3.3). `cancelled`: the edit brought the properties
/// back to the snapshot, so the change element was removed.
pub struct Folded { pub id: i64, pub cancelled: bool }
/// The revision elements a transaction wrote, found at `finish` and not kept by the compiler (2.2, Created).
pub struct Created { pub kind: RevKind, pub id: i64, pub part: Arc<str>, pub node: NodeId }
```

`RemovedOwn`, `Folded` and `swallowed` exist for two reasons. They are the receipt's way of telling an agent
that an operation removed its own earlier insertion, changed its own earlier formatting change, or took one of
its own revisions away with an element it removed (a paragraph that it inserted and then formatted loses its
`pPrChange` too). The laws need to know as well (7). They are the only departures from "an operation only adds
revisions".

### 2.2 The primitives

```rust
pub struct Compiler<'t, 's> { /* tx, track, the Tx's id range at the start */ }

impl<'t, 's> Compiler<'t, 's> {
    pub fn new(tx: &'t mut Tx<'s>, track: Track) -> Result<Compiler<'t, 's>, StetError>;
    pub fn tx(&self) -> &Tx<'s>;                   // for Tx::resolve between two calls of one batch
    pub fn finish(self) -> Result<Outcome, StetError>;   // derives `created`, checks the post-conditions of section 7.3

    // text, in one paragraph. An insertion is a range `at..at`, and a deletion replaces with "".
    pub fn replace_text(&mut self, block: &BlockKey, range: Range<usize>, with: &str) -> Result<Effect, StetError>;

    // properties: run, paragraph, mark, section, table, row, cell; style and list level are paragraph properties
    pub fn set_props(&mut self, target: &PropTarget, edit: &PropEdit) -> Result<Effect, StetError>;

    // blocks
    pub fn insert_block(&mut self, anchor: &BlockKey, place: Place, spec: &NewParagraph) -> Result<Effect, StetError>;
    pub fn delete_block(&mut self, block: &BlockKey) -> Result<Effect, StetError>;   // a paragraph, a row or a table
    pub fn delete_block_with_mark(&mut self, block: &BlockKey) -> Result<Effect, StetError>;   // tracked: the last paragraph's mark too (a note's whole body, 3.6.4)
    pub fn split_block(&mut self, block: &BlockKey, at: usize) -> Result<Effect, StetError>;
    pub fn merge_blocks(&mut self, first: &BlockKey, second: &BlockKey) -> Result<Effect, StetError>;

    // table rows (a row is deleted with delete_block)
    pub fn insert_row(&mut self, anchor: &BlockKey, place: Place, cells: &[String]) -> Result<Effect, StetError>;

    // list instances: a new w:num that restarts an existing list at a value (numbering.xml, not tracked)
    pub fn new_list_instance(&mut self, num_id: i64, restart_at: i64) -> Result<i64, StetError>;

    // range markers (comment anchors and the like). Never tracked, whatever the Track is.
    pub fn place_markers(&mut self, block: &BlockKey, range: Range<usize>, spec: &MarkerSpec) -> Result<Effect, StetError>;
    pub fn remove_markers(&mut self, block: &BlockKey, ids: &[i64]) -> Result<Effect, StetError>;
    pub fn remove_markers_in_part(&mut self, part: &Arc<str>, ids: &[i64]) -> Result<Effect, StetError>;   // wherever they are (3.6.5)
    pub fn place_markers_beside(&mut self, part: &Arc<str>, comment_id: i64, spec: &MarkerSpec) -> Result<Effect, StetError>;   // a reply (3.6.5)
}

pub enum Action { Accept, Reject }
pub enum Selector { All, Author(String), Ids(Vec<i64>), Elements(Vec<i64>) }   // Elements: by element, for the tests (R2)
pub struct Step { pub action: Action, pub selector: Selector }

/// The id to node index of one batch. It is empty until the first `resolve`, which builds it from the `Tx`'s current trees.
pub struct ChangeIndex { /* id to nodes */ }
impl ChangeIndex {
    pub fn new() -> ChangeIndex;
    pub fn note(&mut self, tx: &Tx<'_>, effect: &Effect);    // after a compiler call: add what it minted, drop what it removed
}
pub fn resolve(tx: &mut Tx<'_>, index: &mut ChangeIndex, steps: &[Step]) -> Result<Vec<Resolution>, StetError>;   // one Resolution per step
pub fn changes(story: &Story, tree: &SpanTree, range: Option<Range<usize>>) -> Vec<Change>;     // the grouping of 6.1, read only, a window when there is a range
pub fn remove_unreferenced_notes(tx: &mut Tx<'_>, rejected: bool) -> Result<Vec<RemovedNote>, StetError>;   // 3.6.4: for what takes a reference out untracked
```

There are ten primitives. Insertion and deletion of text are not separate entry points, and neither is the deletion
of a row: they would be aliases, and the module would have two names for one rule. `resolve` takes a list of steps
that are **consecutive** `accept` and `reject` operations (6.1): `ops` groups the operations that follow one another and
no others, so a batch is still applied in order, and `resolve` never needs a stamp or an id. `changes` is for
`Document::revisions` and the receipts, so `projection` and `ops` do not each group revision elements differently; a
receipt asks for the blocks it touched, and the range is extended to whole changes.

What each call promises, once, so that the sections that follow can say "the compiler" without repeating it:

- **Atomic and ordered.** A call that fails has changed nothing a caller can see (the `Tx` is discarded by
  `document`). Calls are applied in order, and a call sees the effect of the earlier ones through `Tx`. A `resolve` is a
  call like the others: it sees the effects of the calls before it, and the calls after it see its.
- **Touches.** Every existing block whose own bytes change is `Tx::touch`ed, by what a block owns
  (`Story::own_parts`): a paragraph owns all its XML, a row owns its `w:trPr`, `w:tblPrEx` and the `w:tcPr` of
  its cells, a table owns `w:tblPr` and `w:tblGrid`. An edit inside a cell touches the paragraphs and not the row.
  Three touches are not obvious, and each is there because the scope guard reads `touched()` before the audit
  stamps anything: a tracked merge touches the second paragraph as well as the first, since it writes the
  alignment into it (3.2.3); `insert_row` and the removal of a row touch the table, whose list of rows changed
  (ask S5); a split and a merge touch the paragraphs they changed even though the audit would stamp the one after
  a changed mark. New blocks and removed blocks need no call: the audit finds them.
- **Declarations.** `split_block` keeps the original element for the first part, so it declares nothing. A merge
  that really joins two paragraphs (`merge_blocks` over our own pending mark or untracked, and every merge that a
  resolution performs) is the **join** of 6.2 and declares `merge(first, second, into: the first element)` in the
  forward chain that `refs-and-state` section 7 describes. A pending tracked merge declares nothing. A rejected
  insertion declares `retire(.., Rejected)` for every block it removes. A tracked deletion declares nothing: the
  element stays and the ref stays live.
- **Schema order.** Every child that is added to a container goes where `schema::insert_in_order` says
  and every removal from a property container is followed by `schema::reorder`. The compiler never picks an
  index. The `Ctx` for the call is `schema::ctx_at`.
- **Ids.** Every revision element it writes gets `w:id`, `w:author` and `w:date` in that order, and an id
  from `Tx::next_annotation_id` (5.1). Untracked calls write none of them.
- **Own pending, and created by this transaction.** The two are not the same. An element is **own pending** when its
  `w:author` is the stamp's author: the author's earlier insertions and formatting changes, from this transaction or from
  any earlier one. Own pending text may be removed outright (4.1), and an own pending property change may be folded into
  (4.3). An element is **created by this transaction** when its author is the stamp's, its `w:id` is in the range that the
  `Tx` minted, **and** the id is not the new id of a pair in `Effect::split`. A split's tail and a carve's copy of a pending
  change take minted ids too, but they are pieces of a revision that already existed, whoever wrote it, and the author test
  alone would still call the pieces of our own earlier revision ours. Only what a transaction created may be extended
  (coalescing, 4.2) or edited in place (a second `set_props`), and nothing that it did not create is. An inserted run joins
  only an `ins` that this transaction created, and a deleted run only a `del` that it created.
- **Created is derived.** The compiler keeps no list of what it wrote. Every revision id comes from the `Tx`'s
  counter, so `finish` takes `Tx::minted_annotation_ids()` (ask S4) and walks the subtrees of the touched and new
  blocks, and the body's `w:sectPr` when a `BodySection` was edited, for revision elements that were created by this
  transaction in the sense above. Each call's `Effect::minted` is the part of the range it took. The range includes the
  tails of splits and the copies that a carve makes, which are not changes of that call, so a receipt attributes a revision
  to an operation only when it passes the strict test above (our author, a minted id, not the new id of a split) and its id
  is in the call's range. The split pairs are reported as splits. This is what makes two compilers in one `Tx`, or a compiler and a `resolve`,
  safe.
- **The copy rule.** Whenever the compiler copies properties, a run or a row, the copy is stripped of every
  **change-tracking element** and of the section break: `w:ins`, `w:del`, `w:moveFrom` and `w:moveTo` (in a
  mark's `w:rPr`, in `w:trPr` and in `w:numPr`), `w:rPrChange`, `w:pPrChange`, `w:sectPrChange`, `w:tblPrChange`,
  `w:tblPrExChange`, `w:tblGridChange`, `w:trPrChange`, `w:tcPrChange`, `w:numberingChange`, `w:cellIns`,
  `w:cellDel`, `w:cellMerge`, and a `w:sectPr` inside a copied `w:pPr`. A copy never carries someone else's claim,
  and the compiler never writes a revision in someone's name. This covers the formatting borrowed for a new run,
  the twin's properties in a new paragraph, the first paragraph of a split, and a cloned row. One copy keeps its
  change on purpose: the halves of a **carve**. A run that is cut in two stays in one revision, so the clone keeps
  a pending `w:rPrChange` under a **new id**, stays in the same revision wrapper as the original (a wrapper is split only
  when an insertion goes inside it, 4.2), and the pair of ids is reported in `Effect::split`. Section 3.3 step 3 uses the same list to decide what a snapshot leaves out.
- **Origins.** Every call that makes a block out of another one says so in `Effect::origins`: the second paragraph
  of a split or of a Y insertion has the original paragraph as its source, an X paragraph has its twin (or R, when it has no
  twin), a new row has the anchor row, and the survivor of a merge has both paragraphs. The no-new-violations check needs
  this because a new element has no earlier unit of its own to compare with (`validate_unit`: the keys after are a
  subset of the keys of the origin).
- **One map per call.** A call builds the `TextMap` of its paragraph once and applies all its edits to it from
  right to left (see `replace_text` step 3). A write between two calls needs state's refresh of the story, which is
  counted in state's figures (section 9); nothing inside a call refreshes.

#### `replace_text`

An insertion is `replace_text(b, at..at, t)` and a deletion is `replace_text(b, r, "")`. One algorithm covers all
three.

1. **Check.** The block must be a paragraph that is not a deleted block (3.2.5). Offsets must lie in the
   paragraph's `Final` text (`invalid_payload`). The text must hold no character that XML cannot carry or
   that stands for an atom (U+0001 to U+0006, U+000C, U+000E): `invalid_payload`, with the name of the atom.
2. **Diff.** The old text of the range and `with` are cut into tokens: a run of letters and digits, a run of spaces, or
   one other character (punctuation, a tab, a line break, an atom). A combining mark, a joiner or a variation selector
   stays with the character before it, and so does the character after a zero width joiner, so an edit never cuts a
   grapheme and every edit begins and ends between tokens. The common prefix and suffix are trimmed. The rest is matched
   on the tokens that occur exactly once on each side, kept in the longest order that increases in both (the anchors),
   and the stretches between the anchors are trimmed and matched again. A stretch with no anchor is matched by the
   longest common subsequence if it has at most 250,000 cells (old tokens times new tokens) and the 4,000,000 cells that
   one diff may use in all are not yet spent. A stretch over either limit is one edit. The raw edit script is then
   **cleaned** by the rules of 2.5, so that dense edits become one replacement. The result is a list of
   `(range, replacement)` pairs, with the range counted in scalar values of the old text. A replace of a whole paragraph is
   therefore a handful of word edits, or one, and never a deletion of every run followed by an insertion. The diff is a
   pure function in `revisions/diff.rs`, and `compare` reuses it.
3. **Apply right to left on one map.** The call builds **one** `TextMap` for the paragraph before the first pair and
   asks it for every `locate` and `gap`. The pairs are applied from the last to the first. This is safe, and the
   rebuilding of the map for each pair that an earlier draft did (about 4.5 ms a pair, because each rebuild
   waits for state's refresh) is not needed, for two reasons. First, an edit changes nothing to its left: a carve
   keeps the **original node as the left part** (the original run and its text node keep the beginning of the
   text and every offset into it, and the right part is the new clone), the deletion wraps only the carved
   middle, and the insertion goes after the deleted piece. Second, the cleaning rules always leave an
   unchanged island between two pairs, so the neighbours of a pair's gap are items of the island that separates it
   from the next pair, which no pair to the right has moved. A test pins the claim: for every generated edit, the
   bytes with one map equal the bytes of the version that rebuilds the map before each pair (kept in the tests only).
   For each pair:
   a. **Locate** the range with `TextMap::locate(Final, range)`. The spans, and the items between them, say which
      runs, atoms, marks and fields are covered. A field that is only partly covered is `readonly_region`; a field that is
      covered from `FieldBegin` to `FieldEnd` is one unit.
   b. **Classify each span** by the revision wrappers around its item (4.1): plain, own pending insertion,
      another author's pending insertion, a moved-in range, or inside an inserted row or cell.
   c. **Carve.** Split each partly covered run so that the covered characters have runs of their own. A run
      is split by cloning it, so formatting and any child the compiler does not understand are kept, and
      every child goes to exactly one side (`revisions/carve.rs`). The clone follows the copy rule.
      **As built.** A `w:t` may hold several text nodes and CDATA sections (`a]]&gt;b<![CDATA[c]]>d`): a cut inside
      one moves the nodes after it into the new `w:t` with the tail, and a cut between two nodes cuts the `w:t` there,
      so the text keeps its order. Only a cut inside a CDATA section is `unsupported` (`xml-cdata-end-written-in-text`).
   d. **Delete** each carved group by the rule of its class: wrap in our `w:del` and rename `w:t` to
      `w:delText` and `w:instrText` to `w:delInstrText`; or remove outright; or nest our `w:del` among the runs
      of the other author's `w:ins`. Range markers between the first and last carved run go inside the
      `w:del`, as Word does (`w:del{commentRangeStart, r, commentRangeEnd}` occurs 7 times); markers
      outside them stay.
   e. **Insert** `with` as one new `w:ins` placed after the last piece that was just deleted. For a pure
      insertion the place is chosen by the gap rule of 4.2. The run's `rPr` is a copy, by the copy rule, of the
      first deleted run's, or of the nearest text on the left (then on the right, then the paragraph mark's),
      because atoms do not donate formatting: text typed after a footnote reference must not become superscript.
      `xml:space="preserve"` is written exactly when the text has leading or trailing white space.
4. **Touch** the paragraph. Nothing is declared.

Untracked, steps 3c to 3e become: remove the covered characters from their text nodes with
`Editor::replace_text_range`, remove runs and wrappers that are left empty (marks stay), and put the new text in
the nearest run to the left, or in a new run with that run's `rPr`. Revisions in the paragraph stay as they
are, with their changed text.

#### `set_props`

1. **Resolve the container**: for a run range, carve the covered runs and take each run's `w:rPr`, creating it
   as the first child of the run when the run has none. Other targets are the element named above, created
   with `insert_in_order` when it is missing.
2. **Check** the container for a pending `*PrChange` (4.3): none; one **created by this transaction**; an **own pending**
   one from before (the author's own, including a carve's copy of it); or another author's, including a carve's copy of
   theirs. Another author's is `readonly_region`. A paragraph whose **mark is our own pending insertion** is edited directly
   and no `pPrChange` is written: Word does this (`FA/005` has the list formatting on the first paragraph of an Enter pair
   with an inserted mark and no change), because rejecting the mark makes the paragraph disappear into the next one. When
   that mark was inserted by an earlier transaction, the call reports the direct edit as a fold into the mark's id
   (`Folded`), so that law R2 knows the paragraph's properties changed under that mark.
3. **Snapshot**, tracked only and only when no `*PrChange` is there yet: a copy of the container's children
   that the snapshot's declaration allows (`Ctx::allows_child` on the declaration under the `*PrChange`, so
   `pPrChange/pPr` takes everything except `rPr`, `sectPr` and `pPrChange`, and `sectPrChange/sectPr` takes
   no header or footer reference), minus every change-tracking element of the copy rule, even where the schema
   would allow one. The schema decides, so there is no list of properties in the compiler. A container with no
   properties has an empty snapshot, which is legal (`FA/019`, `FA/004`).
4. **Edit**: for each element of `edit.clear`, remove the child of that name; for each of `edit.set`, replace
   the child of that name or insert it with `insert_in_order`. Where the result is a property container that
   was out of order, apply the `Fit::Reorder` order. A `Fit::NoValidSlot` is `invalid_payload` (the edit would
   put a property in twice or against another one).
5. **Write the change element** last, when a snapshot was taken: `w:id`, `w:author`, `w:date`, and the snapshot as
   its child. When the container already holds a change created by this transaction, nothing is added. When it holds an
   own pending one from before, the edit is applied, the change is left as it is, and the call reports `folded`. If the
   edit brings the properties back to the snapshot, the change element is removed and the fold is reported as cancelled,
   because a change of ours whose snapshot equals its properties is a no-op, and `finish` refuses one (W11, 7.3).
6. **Touch** the block that owns the container.

**As built.** Where a new child goes is asked of the element's own content model when the walk from the part root finds
no context for its parent (a paragraph in Word's body-level `w:moveTo` or `w:moveFrom`, a row that is built before it
is placed), and a new `w:pPr`, `w:rPr` or `w:tcPr` always goes first in its paragraph, run or cell, whatever unknown
element comes before the content (`props_edit::place`). The snapshot of a container that was out of order is written in
schema order too.

A table grid change is not written (no operation changes the grid in this release), and `tblGridChange` has
no author attribute in the schema, so it is read, accepted and rejected only.

#### Blocks, rows and markers

`insert_block`, `delete_block`, `split_block`, `merge_blocks` and `insert_row` are specified with the
paragraph-mark rules in 3.2 and 3.4, because the shape is the contract.

`place_markers(block, range, spec)` is the route for comment anchors, so that `comments` does not edit the
document part by hand. It writes no revision whatever the `Track` is. It carves the paragraph at the two ends of the
range exactly as `replace_text` does (the same code, step 3c), puts `spec.start` immediately before the first
covered item and `spec.end` immediately after the last covered item, and the reference run, when there is one,
immediately after the end marker. A marker at a position inside a revision wrapper is a sibling of the runs inside
that wrapper (`w:ins` and `w:del` take range markers), so no wrapper is split. A range may be empty. The block must
not be a deleted block, and a range that cuts into a field is `readonly_region`, as for text. A range that lies in
a tracked deletion places its markers around the deleted runs. `remove_markers(block, ids)` removes the markers
and reference runs of the given annotation ids from the paragraph, and drops a run that the removal leaves empty.
Both touch the paragraph. The ids are minted by the caller from `Tx::next_annotation_id`.

### 2.3 How `ops` maps every operation

| Operation | Compiler calls |
|---|---|
| `replace` a text range, or the whole paragraph | `replace_text(block, range, with)`. A whole paragraph is `range = 0..len`, and the diff keeps it word-level or one replacement |
| `insert` text at an offset | `replace_text(block, at..at, text)` |
| `insert` a paragraph before or after a reference | `insert_block(anchor, place, spec)`, once per new paragraph. The next paragraph of the same `insert` takes the key in `Effect::new_blocks` as its anchor with `Place::After`, so a run of lines lands in order. Refs are minted at commit, so no later operation of the batch can name them |
| `delete` a text range | `replace_text(block, range, "")` |
| `delete` a range that spans paragraphs, from offset `a` in the first to offset `b` in the last | the text first, then at most one merge. **When `a` is 0** the first paragraph is deleted whole by `delete_block(first)`, which is Word's whole-paragraph shape, and there is no merge. Otherwise `replace_text(.., "")` deletes the tail of the first paragraph, and `merge_blocks(first, last)` comes last. For the rest of the range: `delete_block` for each paragraph in the middle, and `replace_text(.., "")` for the head of the last paragraph, or `delete_block` instead of it when the range ends at the end of the last paragraph of a container (it keeps its mark). The merge skips the paragraphs between its two, which are deleted blocks (3.2.3), and its alignment lands on the last paragraph, the one whose properties survive a join. This is the shape Word writes for the same selection (`RP040`). Merging pair by pair first would align the middle paragraph, which is about to disappear, and leave the last one unaligned |
| `delete` paragraphs | `delete_block(block)` for each. A table reference is deleted with one call that walks its rows |
| `split` | `split_block(block, at)` |
| `merge` | `merge_blocks(first, second)` |
| `format` runs (with `find`) | `set_props(Run { block, range }, edit)` |
| `paragraph` properties | `set_props(Paragraph { block }, edit)` |
| `style` | `set_props(Paragraph { block }, { set: pStyle })`. `ops` checks that the style exists |
| `list` level | `set_props(Paragraph { block }, { set: numPr })` |
| `list` restart | `new_list_instance(num_id, value)`, then `set_props` with the new `numId` |
| `list` remove | `set_props(Paragraph { block }, { set: numPr with numId 0 })` |
| `set` on paragraph, run, section, table, row or cell properties | `set_props` with the target of the reference's kind. `ops` turns human units into attribute values, and `check` validates the result |
| `row` insert, `row` delete | `insert_row(anchor, place, cells)`, `delete_block(row)` |
| `accept`, `reject` by id, by author, all | one `resolve(tx, index, steps)` for each run of **consecutive** accept and reject operations. The operations stay in the order of the batch, so `reject` of everything followed by a `replace` does not reject the replacement |
| `comment`, `reply`, `resolve`, `delete-comment` | `comments` calls `Tx::add_part` and `Tx::touch` for the comments part, and `place_markers` and `remove_markers` for the anchors in the document. When `comments` deletes a comment anchor inside tracked text, the markers stay paired because the carve and the hoisting are the same code |
| `Edit::files` | `projection::files` first turns the edited files into the operations above |
| `compare` | `compare` reuses `diff.rs`, `set_props` and the carve, and adds the primitives listed in section 12 (designed in `compare.md`) |
| untracked editing | the same calls with `Track::Untracked` |

### 2.4 What is refused, and with which code

| Code | When |
|---|---|
| `readonly_region` | text or property edit in a deleted block; a range that cuts into a field, or a gap strictly inside a field result, or a delete, merge or split at a paragraph that begins or ends a field that spans paragraphs; text in a locked, data-bound or placeholder content control; a property edit on a container that holds another author's pending `*PrChange`; `merge_blocks` where either paragraph carries a section break, or where either holds another author's pending `pPrChange`; `split_block` inside an inline content control or a `w:fldSimple`; `delete_block` on a row that another revision already deletes |
| `cross_block` | `merge_blocks` of two blocks that are not neighbouring paragraphs of one container (neighbouring in `Final`: only deleted blocks may lie between them); a table range for a paragraph primitive |
| `invalid_payload` | an offset outside the text or inside an atom or surrogate pair, a character that cannot be written, an empty or over-long author, a date that is not a real ISO 8601 date and time, a `PropEdit` that cannot be placed |
| `unsupported` | an edit whose target is inside a text box (the text boxes of `Story::text_boxes` have no refs, so `ops` refuses before the compiler), and a primitive that is not built in 1.0 |
| `limit_exceeded` | an annotation id that would pass 2,147,483,647 |
| `internal` | a post-condition of section 7.3 failed. It is a bug, and the batch is discarded |

The message of each says what to do, as `ARCHITECTURE.md` requires. For a deleted block it is, for example,
"p5 is deleted by Eric White. Reject that change first, or edit another paragraph." For a merge across a section
break it is "p5 ends a section, and merging it would move or drop the break. Edit the two paragraphs without merging
them."

### 2.5 Cleaning the word diff

A minimal word diff is correct and often unreadable: two paragraphs that share a few common words come out as a
confetti of one-word deletions and insertions, and a reader, a lawyer in particular, wants to see one clause
replaced by another. The cleaning rules turn dense edits into one replacement, as a careful human would write
them. They follow the semantic cleanup of diff-match-patch, which is the standard answer to the same problem, and
run over the edit script, the unchanged islands between edits and the token kinds of step 2. Repeat 1 to 3 until
nothing changes:

1. An island with **no word token** (only spaces and punctuation) between two edits is dissolved into them. An island is
   the unchanged tokens between two edits, and there is always one, because two edits that touch are one.
2. An island of **at most two word tokens** is dissolved when it is no longer, in characters, than the larger of
   the two edits around it (the deleted and the inserted text of an edit counted together, in scalar values).
3. Two edits that touch after a dissolve are one edit, and the outer ends are not moved.
4. At the end, if **more than half** of the word tokens between the first and the last edit are inside edits, the
   span from the first edit's start to the last edit's end becomes one replacement. The words are counted on both sides:
   those of the old span and those of the new span.

`diff::edits(old, new, Cleanup)` takes `Cleanup::Readable`, which `ops` and `compare` use, or `Cleanup::Minimal`,
which compare's noise measure and the tests use. `Minimal` is the matching with no cleaning rule applied. It keeps as many
tokens as the longest common subsequence when the words are distinct and in the same order, and in the rare case of
anchors that cross it can keep fewer, which the tests pin. The cleaning adds noise on purpose: compare's quality check sets its noise threshold from
the first baseline, so the baseline is taken with `Readable`, and the minimal diff stays the denominator. The
constants (two tokens, one half) are starting values, checked with a table of before and
after pairs written by hand (the cases a reviewer would call choppy), with the 139 Word-written pairs of `del`
then `ins` (1.3) as the check that Word's own replacements are not coarser than ours, and with the noise figure
over the Harvey LAB pairs.

## 3. Revision shapes: reading, writing, accepting, rejecting

Reading is `wordml`'s (`revision_at`, `Story::revisions`, the wrapper rule of the text model) and `revisions`
never has a second reader. Table 3A says what each shape is and what stetdoc writes. Table 3B says what
accepting and rejecting do. The notes after them are the parts that need an argument.

### 3.1 Tables

**3A. Where each shape sits, how it is read, and whether stetdoc writes it**

| Shape | Sits | `wordml` reads it as | stetdoc writes it |
|---|---|---|---|
| `w:ins` around runs | child of `w:p`, `w:hyperlink`, another `w:ins` or `w:del` or `w:moveTo`, an inline `w:sdtContent`, `w:smartTag`, `w:customXml`, or of the body, a cell or a row | `Ins` wrapper, `RevKind::Ins` | yes: new text |
| `w:del` around runs, with `w:delText` and `w:delInstrText` | the same places | `Del` wrapper, `RevKind::Del` | yes: deleted text and fields |
| paragraph mark `ins`, `del` | first child of `w:pPr/w:rPr` | `MarkIns`, `MarkDel` | yes: insert, split, merge, delete |
| `w:moveFrom`, `w:moveTo`, their range markers and the mark forms | run wrappers; `moveFromRangeStart` in a paragraph; `moveFromRangeEnd` in a paragraph or between blocks; first child of the mark's `w:rPr` | `MoveFrom`, `MoveTo`, `MarkMoveFrom`, `MarkMoveTo`, `MoveRangeStart`, `MoveRangeEnd` marks | no: a move is written as a deletion and an insertion (3.5) |
| `w:rPrChange` in a run | last in the run's `w:rPr` | `RunProps` | yes: `set_props(Run)` |
| `w:rPrChange` in the mark | last in `w:pPr/w:rPr` | `RunProps` | yes: `set_props(ParagraphMark)` |
| `w:pPrChange` | last in `w:pPr` | `ParagraphProps` | yes: paragraph, style and list changes |
| `w:sectPrChange` | last in `w:sectPr` | `SectionProps` | yes: `set_props(Section)` |
| `w:tblPrChange`, `w:trPrChange`, `w:tcPrChange` | last in `w:tblPr`, `w:trPr`, `w:tcPr` | `TableProps`, `RowProps`, `CellProps` | yes: `set_props(Table, Row, Cell)` |
| `w:tblPrExChange` | last in `w:tblPrEx` | `TableExProps` | no |
| `w:tblGridChange` | last in `w:tblGrid`, id only | `TableGrid` | no: no operation changes the grid in this release |
| `w:numberingChange` | in `w:numPr` or in a `w:fldChar` | `NumberingChange` | no |
| `w:numPr/w:ins` | in `w:numPr` | `NumPrIns` | no: a list change is a `pPrChange` |
| `w:trPr/w:ins`, `w:trPr/w:del` | in `w:trPr` | `RowIns`, `RowDel` wrappers | yes: `insert_row`, `delete_block` |
| `w:cellIns`, `w:cellDel`, `w:cellMerge` | in `w:tcPr` | `CellIns`, `CellDel`, `CellMerge` | no |
| `w:ins`, `w:del` in a math run or in `m:ctrlPr` | child of `m:r` or `m:ctrlPr` | listed by `revision_at`; the text model treats a whole equation as one atom | no |
| `w:customXml{Ins,Del,MoveFrom,MoveTo}Range{Start,End}` around a `w:sdt` | between blocks or inline, and as the first and last child of `w:sdtContent` | `CustomXmlInsStart`, `CustomXmlInsEnd`, and the same for `Del`, `MoveFrom` and `MoveTo` | no |
| revisions in `word/styles.xml` and `word/numbering.xml`: `pPrChange`, `rPrChange` inside a style or a level | as in a paragraph | `part_revisions(tree)`, since `Story` does not cover those parts | no |

**3B. Accept and reject** (`resolve`; `Del` and the like are the elements of 3A)

| Shape | Accept | Reject |
|---|---|---|
| `ins` | unwrap: the children take its place | remove it and its content. Range markers and bookmarks in it are hoisted out, so no start loses its end |
| `del` | remove it and its content, markers hoisted | unwrap, `delText` to `t`, `delInstrText` to `instrText` |
| mark `ins` | remove the element | remove the mark: join with the next paragraph (6.2) |
| mark `del` | remove the mark: join with the next paragraph (6.2) | remove the element |
| `moveTo`, `moveFrom`, mark forms, range markers | resolved as one move (3.5): `moveFrom` content removed, `moveTo` unwrapped, the marks of the moved paragraphs as `del` and `ins` | the reverse |
| `rPrChange`, `pPrChange`, `sectPrChange`, `tblPrChange`, `trPrChange`, `tcPrChange`, `tblPrExChange` | remove the element | put the snapshot's children back in the container, and remove the element. What the snapshot does not hold stays: the `rPr` and `sectPr` of a `pPr`, the header and footer references of a `sectPr`, the live `ins`, `del`, `cellIns`, `cellDel`, `cellMerge` and mark revisions |
| `tblGridChange` | remove the element | put the snapshot's `gridCol` children back |
| `numberingChange` | remove the element | remove the element |
| `numPr/ins` | remove the element | remove the whole `numPr` |
| row `ins` | remove the marker | remove the row |
| row `del` | remove the row | remove the marker |
| `cellIns`, `cellDel` | `cellIns`: the marker is removed. `cellDel`: the cell is removed, and the `gridSpan` of the nearest live cell before it grows by the deleted cell's own span, so that the row still covers the grid (with no live cell before it, the next live cell grows). OpenXmlPowerTools does the same, but adds one per deleted cell and notes in its source that the sum is the right amount | `cellIns`: the cell is removed. `cellDel`: the marker is removed |
| `cellMerge` | becomes `w:vMerge` (`rest` is `restart`, `cont` is `continue`) | remove the marker (the cell's `tcPrChange`, if there is one, is a change of its own) |
| `ins` in `m:r` | unwrap | remove the `m:r` |
| `del` in `m:r` | remove the `m:r` | unwrap |
| `ins`, `del` in `m:ctrlPr` | `ins`: remove the element. `del`: remove the `m:f` that holds it | `ins`: remove the `m:f`. `del`: remove the element |
| `customXmlInsRange*` around a `w:sdt` | remove the markers | unwrap the `w:sdt` (its content stays) and remove the markers |
| `customXmlDelRange*` | unwrap the `w:sdt` and remove the markers | remove the markers |
| a field with a revision around its code | as `ins` or `del` for **every** piece of the field at once (3.6.3) | as above |
| styles and numbering `*PrChange` | as for a paragraph or a run | as for a paragraph or a run |

After any removal the containers it emptied are pruned: an empty `w:rPr`, then an empty `w:pPr`, `w:trPr`,
`w:tcPr`, `w:numPr`; a `w:ins` or `w:del` with no child; a `w:tc` with no block gets an empty `w:p`; a
`w:tbl` with no row is removed. Nothing else in the part is touched.

### 3.2 Paragraph marks

A paragraph mark is the pilcrow. It belongs to the paragraph it ends, and it holds that paragraph's
properties. `w:pPr/w:rPr/w:ins` says the mark was inserted, `w:del` says it was deleted (so the paragraph is
joined with the next one), and a mark can carry both, `ins` then `del`, when one author inserted it and
another deleted it (19 marks in 4 documents).

#### 3.2.1 The two shapes of an inserted paragraph

Word writes both, depending on where the cursor was (counts in 1.3):

- **Y: Enter in a paragraph that has a successor, and a split.** The paragraph the cursor was in keeps its text and
  gets an inserted mark. The new paragraph holds the new text in a `w:ins`, and it holds the **original** mark.
  49 paragraphs have an inserted mark and plain content (47 more have both kinds), and 27 of them are followed
  directly by a paragraph that holds only inserted content (`FA/005`, `RP006`).
- **X: a paragraph that stands alone.** The new paragraph has an inserted mark and all of its content is
  inserted. Its neighbours are untouched (235 paragraphs). It is what Word writes for a paragraph inserted before
  another (`FA/009`) **and for Enter at the end of the last paragraph** (`RP039`: the first paragraph is
  plain, and each new paragraph that follows it has an inserted mark and inserted content).

The reason both exist is the question of what rejecting the mark does: it removes the mark, and the
paragraph is joined with the next one. In Y the join puts the two halves together again. In X the paragraph is
empty once its content is rejected, so it disappears and the next paragraph is unchanged. **With nothing after it in its
container, an inserted mark has nothing to join into, and rejecting it keeps the paragraph, emptied**: the last paragraph of
a cell, a note, a comment or the body holds the container's end, which Word cannot remove. Word for the web left an empty
paragraph so in check C11 (`15-table-cell-edits`), LibreOffice does the same in a cell and at the end of the body, and so
does stetdoc's resolver. The one exception is a run of such paragraphs that holds nothing and follows a table (or
another block that is not a paragraph): it goes, and the container ends with the table as it did before, where Word gives a
story a last paragraph of its own anyway. `RP039`'s shape, Word's own for Enter at the end of the body, therefore rejects to
an empty last paragraph, which is what Word shows.

stetdoc writes the same shapes and chooses by position:

- `split_block(P, k)` writes Y, except that a cut before the first character of P's `Final` text that ends no field
  writes an empty X before P, as `insert_block(P, Before, ..)` does. A Y there would leave P's element empty with the
  inserted mark, and rejecting the mark would remove that element (6.2) and keep the new one, losing P's attributes and
  its ref. The X keeps P's element, attributes, text and ref, so a reject gives back the very `w:p`.
- `insert_block(R, Before, ..)` writes X before R.
- `insert_block(R, After, ..)` writes Y when a paragraph follows R in the same container and R's mark holds no
  revision of any kind, and **when nothing follows R in its container** (`wordml::ends_container`, which reads through
  content controls and block wrappers as the join rule does) and R's mark is clean or only inserted: the inserted mark
  moves with the original `w:pPr` to the new paragraph, which then ends the container, and rejecting joins R back into it.
  Otherwise it writes X before the paragraph that follows R. Two `ins` cannot be on one mark, and a pending deleted mark is
  left alone. **At the end of a container where Y cannot go** (R's mark is deleted or moved, or the edit needs a
  `pPrChange` where another author's is pending) the insertion is `readonly_region`: a paragraph of its own there would be
  the one that Reject All leaves empty. After a table that ends its container X stays, and rejecting removes it (above).
  `insert_block(R, After)` is `split_block(R, len)` followed by putting the text in the second paragraph.

**As built.** After the last block of a content control the next block of the story is in another container, so
`insert_block(R, After)` writes X right after R in R's own container, with no twin. An insertion before a paragraph that
a field's result runs into, or after one that it runs out of, would be inside the field and is `readonly_region`, as a
split there is.

**Y in detail.** The first paragraph is the **original element**, as the ref design wants, and it keeps its
`w14:paraId`. Its `w:pPr` is replaced by a new one: a copy of the original's by the copy rule (2.2), so with no
`pPrChange`, no revision on the mark and no `sectPr`, and with our `ins` first in the mark's `w:rPr`. The original
`w:pPr` moves whole to the second paragraph: its `sectPr`, its pending `pPrChange` and the revisions on its mark come
with it, because the second paragraph holds the original mark. So a section break stays at the end of the section,
now on the new paragraph. The second paragraph is new, with a new `w14:paraId` where the first has one (5.4). A
properties edit in `NewParagraph::props` is applied to the second paragraph by `set_props`, which writes a `pPrChange`
whose snapshot is the original properties (3.3). That is what `FA/005` has: the second paragraph's `pPrChange`
holds the original, empty `pPr`, and the first paragraph carries the same list formatting with **no** change,
because its mark is inserted. If the second paragraph already holds another author's `pPrChange` and the edit
needs one, the shape is X instead. The first paragraph does not get a copy of the pending `pPrChange`: Word does not write one (1 of 27
Enter pairs has a `pPrChange` on the first paragraph, against 4 on the second), and a copy would be another
author's revision written under a new id.

**X in detail.** The new paragraph stands before its **twin**, the paragraph that rejecting its mark joins it
into (the paragraph after it). Its `w:pPr` is a copy of the twin's by the copy rule, with our `ins` first in the
mark's `w:rPr`, and its text is in one `w:ins`. When `NewParagraph::props` makes it differ from the twin, the edit is
applied and the paragraph gets a `pPrChange` whose snapshot is the twin's base properties, so that rejecting gives the
twin's properties whichever answer 3.2.4 turns out to have. With **no twin** (X after the last paragraph, `RP039`)
there is nothing to join into, the paragraph is copied from R, and edits are applied with no `pPrChange`. That is now
only the paragraph after a table that ends its container, or after the last block of a content control that blocks follow.

**Run formatting of a new paragraph.** The run's `rPr` is a copy, by the copy rule, of the **mark's** `rPr` of the
paragraph the new `pPr` was copied from (the original in Y, the twin or R in X), and no `rPr` when the mark has
none. Direct formatting of the neighbouring runs does not leak in: a bold lead-in does not make the next paragraph
bold. `RP039` has the mark's `lang` on the new run, and `FA/005` has no `rPr` on either.

#### 3.2.2 Deleting a paragraph

`delete_block(P)` deletes P's content as `replace_text` does over its whole text, and then deletes the mark:

- The mark gets `del` when a paragraph follows P in the same container and P carries no section break.
  This is what Word writes (52 whole-paragraph deletions in the corpus), and the next paragraph's properties
  are left alone.
- Otherwise the mark is left as it is: **the last paragraph of a container keeps its mark** (a `w:tc` must
  end with a paragraph, and Word writes it that way: `RP040` deletes the content of its last paragraph and leaves the
  mark plain), and **a paragraph that carries a section break keeps its mark**, so that accepting leaves an
  empty paragraph that still ends the section. Word would delete the section break with the mark (it did in
  `n830205`, 7 times). stetdoc does not, on purpose: a lost section drops its headers and footers silently. This is a
  deliberate difference from Word, and `FIDELITY.md` will say so (decision 8).
- If the mark is our own pending `ins`, it is removed with the paragraph's content, and when all the
  content was ours too the element is removed outright (`removed_own`, 4.1), and whatever revision of ours was inside it
  is listed in `swallowed`. A mark that is another author's `ins` gets our `del` after it. A mark that is a `moveTo`
  gets our `del` as well (4.4).

A range of text that spans paragraphs is not one primitive. `ops` writes it as the text deletions and then one merge of the
first and the last paragraph (2.3), which is how Word's selection comes out in `RP040`: the first paragraph keeps its text and loses its
mark, the middle paragraph loses content and mark, the last loses its content and keeps its mark.

#### 3.2.3 Merging

`merge_blocks(P, Q)` makes P's text and Q's one paragraph. P and Q must be neighbouring paragraphs of one
container in `Final` (`cross_block`): the blocks between them, if any, are deleted blocks, which is what a range of text
across three paragraphs leaves (2.3). Neither may be a deleted block itself (`readonly_region`). **Neither may carry a
section break** (`readonly_region`, with the message of 2.4): the break would move or vanish with the merge. And
**neither may hold another author's pending `pPrChange`** (`readonly_region`): the merged paragraph's properties would
depend on properties that are still pending, so the alignment below could not be right once they are resolved.

1. If P's mark is clean, add `del` to it. If it is another author's `ins`, add our `del` after it. If it is
   our own pending `ins` (a split or insert of ours in this or an earlier transaction), do the merge for real
   (below). If it already holds a `del`, the merge is already pending: `readonly_region`.
2. **Alignment.** If P has live text and the base properties of P and Q differ (3.2.4), write a `pPrChange` on Q that
   makes Q's base properties P's, with Q's old base properties as the snapshot. If Q already holds a `pPrChange` of ours,
   the edit is folded into it. If Q's own mark is our pending insertion (two new paragraphs of ours), the edit is direct, with
   no change element, by the own-mark rule of 3.3, which `set_props` applies here as everywhere. The alignment covers base
   properties only: the mark's own `rPr` (the pilcrow's formatting)
   is not aligned. **If P has no live text** (its `Final` text is empty and it holds no atom) the alignment is skipped:
   accepting joins nothing into Q, P simply goes (6.2), and Q keeps its properties and its element, which is what Word's
   52 whole-paragraph deletions leave. `ops` does not merge a range that starts at offset 0, it deletes the first
   paragraph whole (2.3).
3. Touch P and Q. Nothing is declared: both paragraphs stay in the tree with their refs, and the join is declared
   when the deleted mark is accepted (6.2).

A **real merge** (our own pending mark, or untracked) writes the same alignment, and **in tracked mode it is the
`pPrChange` of step 2**, folded into ours if Q has one: writing Q's base properties directly would reformat Q's content with
no revision, which rejecting everything could not undo (law R1). Only **untracked** mode, and a Q whose own mark is our pending insertion (the rule above), write the base properties
directly into Q's `w:pPr`. Then comes the **join** of 6.2: P's element survives, Q's `w:pPr` (with its alignment change)
moves into it, Q's content moves to the end of P, Q's element goes, and `merge(P, Q, into: P)` is declared. A merge is
therefore always a merge into the first element, and the merged paragraph has P's properties in every mode.

**As built.** A merge into a paragraph that begins a field spanning paragraphs (a table of contents) is
`readonly_region`, as one out of such a field was: the paragraphs the field spans read as one read-only block, and the
merge would move P's text into it. A real merge with deleted paragraphs between P and Q (a range deleted across a
paragraph the batch inserted) joins P with the paragraph right after it, not Q, so nothing moves across them and
`Original` keeps its order, while their deleted marks still read P and Q as one in `Final`.

#### 3.2.4 Which properties survive a join

When a deleted mark is accepted, or an inserted mark is rejected, two paragraphs become one and one paragraph's
properties must go.

- OpenXmlPowerTools keeps the **following** paragraph's mark, so its properties survive. It chose the last
  paragraph's `pPr` for a run of deleted marks after a test lost list numbering with the first one (`RP052`, the
  comment in `RevisionProcessor.cs`, which also wonders whether a reject should take the first paragraph's instead).
- docx-cli does the same and cites ECMA-376 section 17.13.5.4: the merged paragraph adopts the next
  paragraph's `pPr` "since its mark is the one that survives".
- **`FA/005` is Word's own evidence for rejecting an inserted mark.** Its first paragraph has `ListParagraph` and `numPr`, an
  inserted mark and no `pPrChange`. Its second paragraph has the same `pPr` and a `pPrChange` whose snapshot is empty.
  Rejecting everything gives the original, a plain paragraph, only if the join keeps the **later** paragraph's
  properties: the second paragraph's `pPrChange` is rejected to its empty snapshot, and that paragraph must be the one
  that survives. If the first paragraph's properties survived, the original would come back as a list item.
- For **accepting a deleted mark** nothing in the corpus decides. The design review counted 9 merges whose two paragraphs have
  different `pPr`, and only one of them has a `pPrChange`. A whole-paragraph deletion leaves the next paragraph
  alone (52 cases, three of them with an empty-snapshot `pPrChange` such as `RP040`'s, which changes nothing).

The decision: **resolution keeps the later paragraph's properties, in the first paragraph's element, and the compiler never
relies on it for a merge.** Whenever stetdoc writes a merge, or an X paragraph whose properties differ from its twin's,
the alignment `pPrChange` makes both paragraphs equal on accept and reject alike, so Word gets the same answer under
either rule. Y needs no alignment: its first paragraph is a copy of the original, so it is right under either rule
already. The alignment costs one extra revision, and only when the base properties differ. Resolving revisions
that others wrote has no such protection, and accepting a deleted mark is the first item of the Word checklist (8.4).
The alignment is kept (decision 1, section 12).

The comparison is of **base properties**: the paragraph's `w:pPr` children without `rPr`, `sectPr` and
`pPrChange` (`Paragraph::base_props`, which is `PartialEq`).

#### 3.2.5 Edges

- A **deleted block** is a paragraph whose mark is deleted (`del` or `moveFrom`), or that sits in a deleted row or
  cell, **and** whose `Final` text is empty (no text and no atom). Text and property edits in a deleted block are
  `readonly_region`. A paragraph whose mark is deleted but that still has live text is a pending merge, and it can be
  edited. A paragraph whose content is all deleted but whose **mark is live**, such as the last paragraph of a
  container or a paragraph that keeps its section break (3.2.2), is live and editable: new text goes after the
  deleted text. This is the definition `refs-and-state` needs where it says that a block "whose whole content is
  deleted" is read-only ("Tracked deletion keeps the ref live").
- A deleted mark with no paragraph after it in its container (a table, the end of a cell, the end of the
  story) is accepted by removing the paragraph when it is empty, and by dropping the revision when it is
  not. Word writes this shape inside deleted rows (164 of 176 such marks) and for the last paragraph of a
  deleted note.
- A paragraph's `w14:paraId` and `w14:textId` follow 5.4. `w:rsid*` attributes are never added or removed.

### 3.3 Property changes

- **Snapshot.** The snapshot is a copy of the container's children as `set_props` step 3 describes, taken
  once, before the first change. It is the whole old state of the properties that the change element can
  describe, so rejecting restores them exactly. A run with no `w:rPr` gets one, and its snapshot is an
  empty `w:rPr` (`FA/019`).
- **One change per container.** The schema allows one `*PrChange` per container. A second edit of a
  container whose change was created by this transaction (2.2) edits the properties and leaves the change. An own pending
  change from an earlier transaction, a carve's copy of it included, is folded the same way and reported
  (`Effect::folded`), because Word keeps one change per run. One of another author, a carve's copy of it included, is
  `readonly_region`: Word's own rule is not known (checklist), and writing a second change is impossible.
- **A paragraph whose mark is our own pending insertion** is edited directly, with no `pPrChange`. Rejecting
  the mark joins it into the next paragraph, so the change would be noise, and Word writes it this way (`FA/005`).
  An alignment `pPrChange` that the paragraph already holds (3.2.1, X) stays as it is. When the mark was inserted by an
  earlier transaction, the direct edit is reported as a fold into the mark's id (2.2, `set_props` step 2).
- **Per run.** A formatted range becomes one `rPrChange` per run it covers, each with its own id, as Word
  writes it (589 in `n830205`). Formatting text inside an `ins` that this transaction created is applied to the run
  directly with no `rPrChange`: rejecting the insertion removes the run, so the change would only be noise. Formatting
  text of an earlier insertion writes the `rPrChange`, as Word does (72 of 75 runs in an `ins` with an
  `rPrChange` are the same author).
- **Style and list.** A style change is a `pPrChange` whose snapshot has the old `pStyle`. A list change is a
  `pPrChange` whose snapshot has the old `numPr` and `ind` (`RP022`, `FA/017`). Newer Word writes a
  `pPrChange` with an empty snapshot to number a plain paragraph (`FA/004`), and older Word wrote `numPr/ins`
  (`RP021`). stetdoc writes the first form.
- **List restart** adds a `w:num` that points at the same `abstractNum` with a `lvlOverride/startOverride`
  to `numbering.xml`, with the next free `numId`, placed by `insert_in_order` after the last `w:num`. Word does not
  track definition edits, and neither does stetdoc. The paragraph's `numPr` is the tracked part.
- **The mark's `rPrChange`.** The mark's snapshot never holds `ins` or `del`, though the schema allows them
  and Word 14 copied them there (105 in `n830205`). Restoring such a snapshot leaves the live mark revisions
  alone and drops the stale copies (6.4).
- **Sections.** A `sectPrChange` is written by `set_props(Section)` and `set_props(BodySection)`. The body's
  `w:sectPr` belongs to no block, so a change to it touches no block and the scope guard cannot name it
  (ask DOC3).

### 3.4 Tables

- **Insert a row** is a copy of the anchor row with the contents emptied, made by the copy rule (2.2). Its
  `w:tblPrEx`, `w:trPr` and the `w:tcPr` of each cell are kept, but without `trPr/ins`, `trPr/del`, `trPrChange`,
  `tcPrChange`, `cellIns`, `cellDel`, `cellMerge` and `tblPrExChange`, so a row inserted beside a row that another author
  inserted, deleted or formatted does not borrow their revisions. Each cell holds one paragraph with the text from
  `cells` (empty when there is none). The row gets `trPr/ins`. Each paragraph gets the mark `ins` and its content in a
  `w:ins`. There is no `cellIns` (`FA/007`, `RP010`). `gridSpan` is kept. A **vertical merge** needs care, because a copied
  `restart` would begin a second merge in the middle of the first. After the anchor, the new cell is `w:vMerge` with no
  value (continue), whether the anchor's cell is `restart` or `continue`. Before the anchor, a new cell above a `restart`
  has no `vMerge` and the anchor keeps its `restart`, and above a `continue` it is `continue`. A nested table in the
  anchor is not copied. `w14:paraId` on the row and its paragraphs is new if the anchor has one. The table is touched,
  because its list of rows changed.
- **Delete a row** is `delete_block(row)`. It gives `trPr/del` to the row, deletes each cell's paragraphs as in 3.2.2
  but **with the mark deleted for the last paragraph of each cell too** (`FA/006`, `RP009`: the row goes away as a whole,
  so there is nothing to keep a cell valid), and recurses into nested tables. A row that is our own pending
  insertion and whose content is ours is removed outright. A row that another author inserted gets our `del` on
  `trPr` after their `ins`. `delete_block(table)` does this for every row.
- **Cells.** `cellIns`, `cellDel` and `cellMerge` are read, accepted and rejected. They are not written, since
  no operation inserts or deletes a cell. `set_props(Cell)` writes `tcPrChange`.
- **Accept and reject of rows** remove the row, so the audit retires its refs. A table left with no row is removed.

**As built.** A row may stand in a row-level content control or custom XML element: its table is the nearest one up, a
table whose other rows stand in such wrappers is not left with no row, and a wrapper that held only the removed row goes
with it (`Strict01`). An untracked removal of a row or a table hoists its range markers and comment anchors as an
accepted deletion does (3.6.5), a story or a cell that a removed table was all of keeps one empty paragraph, and a new row
does not copy the range markers that stand between the model row's cells.

### 3.5 Moves

stetdoc writes a move as a deletion and an insertion. It reads, accepts and rejects Word's:

- A move is **two revisions that share a name**: the range markers `moveFromRangeStart` and `moveToRangeStart`
  have the same `w:name`, and each marker and element has its own id. A paragraph moved whole has the mark
  `moveFrom` or `moveTo`, the content in a wrapper, the start marker inside the paragraph and the end marker
  inside it or between paragraphs (inside the paragraph 23 times, between blocks of the body 22 times, between rows 9 times). Inside a `w:sdt` the markers
  gain `w:displacedByCustomXml`.
- **Resolution is by move.** Resolving any element of a move (an id, or the author) resolves every `moveFrom`,
  `moveTo`, marker and mark of the same name, found as the elements that lie between the start and end
  markers (`Story::anchors().moves` pairs them). Accept removes the `moveFrom` side and unwraps the `moveTo`
  side, and reject does the opposite. Moved-from text stays `w:t` (46 times), so unwrapping it needs no rename.
- **Edits next to moves** are in 4.1. Text inside a `moveTo` is edited by nesting a `del` in it, never by removing
  it, whoever moved it. Moved-from text is not in `Final`, so no edit reaches it.

### 3.6 Other places

**3.6.1 Math.** Word puts the wrapper inside the math run: `m:r > w:ins > (m:rPr, w:rPr, m:t)` (28 occurrences,
five documents, Word 16 included) and `m:f/m:fPr/m:ctrlPr/w:ins`. The schema does not allow it and the SDK validator
reports it, which is why Docxodus leaves it alone. stetdoc does the same when reading, never writes it, and
resolves it as in 3B (an accepted deletion removes the whole `m:r`, where OpenXmlPowerTools leaves an empty one).

**3.6.2 Content controls and custom XML.** A control is inserted or deleted by markers **around** the `w:sdt`,
and its text is not wrapped. Edits to the text of an unlocked control are ordinary text edits, with the
control as a wrapper kept around the new runs.

**3.6.3 Fields.** Word writes a deleted field as one `w:del` around `begin`, the code, `separate`, the result
and `end`, with `delInstrText` and `delText`. It writes an inserted field as `ins{begin, code}`, **then the
`separate` run with no `ins`**, then `ins{result, end}` (`RP020`, `FA/008`). Rules:

- A revision removes what it holds and no more. A field is one unit when it is selected and when it is
  deleted (`changes()` groups the revisions around its pieces into one change, and a range that covers it is
  deleted whole), but resolving takes out the pieces the revision wraps and not the result it does not wrap:
  a field whose code was replaced (`ins{code}`, `del{delInstrText}`, as `tdf124472` has it) keeps its result and
  its structure, and an inserted field whose result is outside the insertion (`stress023`, `Academic_Regulations`)
  keeps the result as text. What the removal leaves of a field that lost its `begin` or its `end` is the pieces
  of structure that no field holds together now, the separator, the other end and the instruction, and those go
  with the revision, so the unwrapped `separate` of an inserted field goes on reject as before. The result stays.
  The other two cases unwrap every piece.
- Deleting a field is allowed only when the range covers it from `begin` to `end` in one paragraph. A
  field that spans paragraphs (a table of contents) can be deleted only by deleting every paragraph that holds
  a piece, one `delete_block` each, and a paragraph that holds `begin` or the closing `end` of a field that goes
  beyond it is `readonly_region`.
- `w:fldSimple` cannot sit inside a `w:del` (schema), so a range that covers a `w:fldSimple` whole is
  `readonly_region`. A range that is inside the result of a field is also refused, as the text model flags
  results read-only.
- Rejecting an inserted field and accepting a deleted one remove the field. Everything else leaves it.

**3.6.4 Notes.** A deleted footnote is the reference run in a `w:del` in the body and the note's own paragraphs
deleted in `footnotes.xml` (`RP050`). Accepting leaves a note with no content, which the schema forbids and
Word writes anyway (Docxodus, corner cases). stetdoc removes the note when no reference to it remains. A
rejected insertion of a note removes the reference and the note. `comments` owns notes, and the cleanup is a function
that its operations can call: `notes::remove_unreferenced_notes(tx, rejected)` removes the footnotes and endnotes that a
reference named when the transaction began and none in any story names now (not the separators, and not a note that
nothing named before, which is not this transaction's to remove), retires the paragraphs that had a ref as rejected when
`rejected` is set, and returns what it removed. The resolver calls it after a step that resolved an element that holds a
note reference (step 5), with what the references named before that step, and reports the notes in
`Resolution::notes`. To delete the whole body of a note tracked,
`delete_block_with_mark` is `delete_block` that also deletes the mark of a paragraph that nothing follows, which is the
last paragraph of the note in `RP050`, and that deletes the note's own number (`w:footnoteRef`, `w:endnoteRef`) with its text,
as `RP050` has it. A text edit keeps the number (a note goes only with its reference), but a whole note deleted with its
reference does not: with the number left live, Word for the web kept an empty note "1" after Accept All (hand check 26,
2026-10-10). LibreOffice resolves both shapes the same.

**3.6.5 Comments.** A range marker inside a removed region is **hoisted**, never deleted: it moves to where the
region was, so every `commentRangeStart`, `bookmarkStart` and the like still has its end. A comment whose text
is all deleted survives with an empty range. `comments` decides whether to mention it. This holds for a tracked removal, an
untracked one (`delete_block` of a paragraph, `replace_text`) and an accepted deletion alike.
**The edge rule:** a range covers the items that have characters in it, so a comment reference run and the range
markers at the edge of a range are not covered. A replacement over all the text of a paragraph leaves them where
they are, and the comment keeps its anchor (`a_comment_reference_at_the_edge_of_a_range_is_not_covered_by_a_replace_of_the_whole_paragraph`).
`place_markers_beside(part, comment_id, spec)` puts the markers of a reply where Word puts them, beside those of the comment
it answers, whatever that comment's range (it may span paragraphs, and then the start and the end are in two of
them): the start right after the comment's start marker, the end right after its reference run (its end marker if it has no
reference), and the reply's reference run after that. `remove_markers_in_part(part, ids)` takes the markers and the
reference runs of the ids out of a whole part: a comment's two ends can be in two paragraphs, or between blocks.
**As built.** The range markers a removed paragraph leaves stand between blocks where it was, so the next paragraph is
untouched (6.2), and a comment reference run, which cannot stand there, goes into the neighbouring paragraph with them in
order, or into an empty paragraph made for it when the story has no other. An untracked removal of a row or a table
hoists them the same way (`comments-and-notes.md` asks for it), and a reply's markers go after the run that holds the root's
reference even when that run holds more (a `w:lastRenderedPageBreak`).

**3.6.6 Hyperlinks, controls, smart tags.** The wrapper stays and the revision goes inside it:
`w:hyperlink > w:del > w:r`. `w:ins > w:hyperlink` is invalid (the schema tables say `w:ins` takes no
`w:hyperlink`), and an insertion at the end of a hyperlink goes outside it. An insertion in the middle of
one goes inside, where both neighbours are in it.

**3.6.7 Text boxes.** A text box is written twice, in `mc:Choice` and `mc:Fallback`, with different ids on the
revisions in each (`Comment042`: 6 and 6). Edits inside a text box are `unsupported` (they have no refs). Accept
and reject must resolve both copies. `Story::revisions` lists them marked `Taken` and `Untaken`. A revision in the
taken copy is paired with the revision at the same position of the other copy, found by walking the same
child path inside the `mc:Fallback` and requiring the same kind, author and date. A revision whose twin
cannot be found is resolved alone, with a warning.

## 4. Other authors, nesting, coalescing and refusals

The rule that organises this section: **a revision that already exists is evidence, and the compiler leaves
it alone.** An operation may split a pending insertion, nest a deletion among its runs, or remove text
that its own author inserted. It never changes an id, an author or a date, never adds a child to a revision
element that existed before the transaction, and never extends one. The only other way an existing revision
changes is that someone resolves it. This keeps the laws of section 7 simple: everything an operation did is in
`Effect`.

### 4.1 Deleting text, by whose text it is

The **owner** of an item is the innermost wrapper of `Ins`, `MoveTo`, `RowIns` or `CellIns` kind around it
(`TextMap::context`). The wrapper's element gives its author through `revision_at`. An item inside a
`Del` or `MoveFrom` wrapper is not in `Final`, so no range reaches it.

| The text is | Written |
|---|---|
| plain | `w:del` around the carved runs, inside the hyperlink, control or smart tag that holds them |
| in our own pending `w:ins` (this transaction or an earlier one) | removed outright, with no mark, as Word does. The `w:ins` goes when it is empty. Reported as `removed_own` |
| in another author's `w:ins` | our `w:del` placed **among the runs** of their `w:ins`, which stays one element: `ins{r del{r} r}`. It is the shape Word 16 writes (9 `w:ins` hold `w:del` and `w:r` children, 19 `ins` > `del` nestings across authors) |
| in a `w:moveTo`, either author | the same nested `w:del`, never outright removal: the text is half of a move |
| in a row or cell inserted by a revision, with its runs unwrapped | by our own row: removed outright. By another author's: a plain `w:del` |
| in a field result | `readonly_region` (3.6.3) |

Two cases are not in the corpus and are decided here: our own pending insertion from an **earlier session**
(removed outright, as if from this transaction, because the Docxodus note describes the rule by author and not by
date or session) and nested
`w:del` inside `w:moveTo` (checklist C8).

### 4.2 Inserting text: the gap rule

`TextMap::gap(Final, offset)` gives the nearest included items on the left and right and **every item
between them in XML order**: marks, and items excluded from `Final` such as a deleted run. The compiler chooses
the place from that.

1. **Container.** If both neighbours are inside the same hyperlink, control or smart tag, the new run goes inside the
   innermost one that they share. Otherwise it goes at the level of the paragraph: text typed at the end of a
   hyperlink is not part of the link. A gap inside a field's result is `readonly_region`.
2. **Position.** In that container the run goes after the left neighbour, after every range-**end** marker
   (`bookmarkEnd`, `commentRangeEnd`, `permEnd`, `moveFromRangeEnd`, `moveToRangeEnd`) and after every excluded
   item that follows it, and before the first range-**start** marker or the right neighbour, whichever
   comes first. So new text never joins a bookmark or a comment range at its edge, and it lands after a deleted
   word, which is where Word puts text typed over a selection.
3. **Wrapper.** If the position is inside a revision wrapper, whether it is ours, another author's or a
   `moveTo`, the wrapper is split there: the children after the position move into a new wrapper that
   copies author and date and gets a new id, and our `w:ins` sits between the two. (Word does the same:
   `insert-interior-pending` shows `ins(John){..} ins(Anson){MASTER } ins(John){..}`.) Nothing is added to the old wrapper.
4. **Coalescing, within the transaction.** If the sibling just before the position, ignoring zero-width
   marks, is an `ins` (for an inserted run) or a `del` (for a deleted run) that **this transaction created** (2.2: our
   author, a minted id that is not the new id of a split), with the same parent, the run is appended to it instead of making
   a new element. An inserted run never joins a `del`, a deleted run never joins an `ins`, and neither joins the tail of a
   split or any other element that existed before the transaction, **even of our own author**. The zero-width marks
   between the wrapper and the position (a `bookmarkEnd`, a `commentRangeEnd`) **move into the wrapper together with the
   run**, ahead of it, so that each keeps its place between the old text and the new text (`w:ins` and `w:del` take range
   markers). A replace and the insertions around it therefore stay one change, and a 50-operation batch does not write 50
   elements where Word would write a few. Elements that existed before the transaction are never appended to, even when
   author and date are the same: Word keeps them apart (35 adjacent `ins` of one author have different dates, 6 have the
   same date, and no adjacent `del` pair has the same date).

A replace is a deletion followed by an insertion at the position **after the last deleted piece**, so
the result reads `del` then `ins`, and the insertion is found by the same rule.

### 4.3 Formatting

| The container holds | Tracked `set_props` |
|---|---|
| no change | snapshot, edit, write the change element |
| a change created by this transaction (2.2) | edit only |
| an own pending change from an earlier transaction, or a carve's copy of it | edit only, folded, reported (`Effect::folded`) |
| another author's change, or a carve's copy of it | `readonly_region` |

Text inside another author's insertion can be formatted: it gets its own `rPrChange` inside their `w:ins`
(3 such runs in the corpus). Text deleted by another author is not in `Final`. Untracked, the same edit has no record, so
it is `readonly_region` (5.5), and so is an untracked edit of properties under another author's `*PrChange`.

### 4.4 Marks, rows and blocks

| The target | Written |
|---|---|
| a paragraph mark with no revision | `ins` or `del`, first in `w:rPr` |
| a mark with another author's `ins` | our `del` after it: `ins` then `del` (19 in the corpus) |
| a mark with a `moveTo` (a paragraph moved in, by anyone) | our `del`, placed by `insert_in_order`, which puts it **before** the `moveTo` because the schema's order of the mark's revisions is `ins`, `del`, `moveFrom`, `moveTo`. The paragraph's content gets the nested `del` of 4.1, and nothing is removed outright |
| a mark with a `moveFrom` | not reached: the paragraph is a deleted block, and edits in it are `readonly_region` |
| a mark with our own pending `ins`, and we delete or merge it | removed, and the merge is done for real (3.2.3) |
| a mark that already has `del` | `readonly_region` for merge and delete, and a no-op warning for `delete_block` of a whole paragraph already deleted by anyone (nothing is written) |
| a row with another author's `trPr/ins` | our `del` after it in `w:trPr`, and nested `del` in every cell's content |
| a row with our own pending `trPr/ins` | removed outright when its content is ours |
| a row already `del` | `readonly_region` |

### 4.5 What stetdoc refuses, in one place

`readonly_region` and the other codes are listed in 2.4. The principle is: text and
formatting that another author has a pending claim on is not rewritten under them. An agent that wants
to change it resolves the revision first, with `accept` or `reject`, which are operations of their own. This holds in
untracked mode too (decision 4, 5.5): untracked means the author's own changes go untracked, never that someone else's
change is rewritten without a record.

## 5. Ids, authors, dates, rsids and untracked mode

### 5.1 Revision ids

- **Rule.** A new revision id is the highest decimal `w:id` in the document plus one, and plus one more for
  each element after it. It comes from `Tx::next_annotation_id` (ask S1). The first call
  of a `Tx` returns the highest `w:id` of the base plus one (zero when none is in use), each later call one more, and
  an id above 2,147,483,647 is `limit_exceeded`. The counter belongs to the `Tx`, so a dry run and the real edit at one
  base give the same ids, and a part brought in with `Tx::add_part` that already holds ids (a comments part imported
  with its own) counts too.
- **The maximum is the base's, and it is read once.** It comes from `PartsView::ids(AnnotationId)?.max()`, a derived
  cache of the base that is keyed by the part generations (state has confirmed that it never reads a staged tree).
  `wordml::existing_ids` reads the `w:id` of every element of every story part, and now of `styles.xml` and
  `numbering.xml` too, with no allocation per element. Only the first transaction after a commit that changed a story pays
  for the pass (about 10 to 30 ms on the largest body, of the order of the 11 ms that `wordml.md` measures for a walk
  of every element at 13 MB). Nothing inside a transaction reads it again, because every edit
  makes a cache that is keyed by generations stale, and 50 operations would be 50 passes.
- **One counter.** Comments call the same function, so two allocators cannot hand out the same number.
  `mint_id(AnnotationId)` stays `internal`.
- **Why one space.** Word numbers revisions and bookmarks from one counter at save, and a revision id equals a bookmark
  id in 1 of 257 documents. The highest id plus one can collide with nothing. Ids are not renumbered into document
  order, because that would change every other element (law L2), and valid documents do not need it (54
  of 252 documents with ids do not start at 0).
- **Determinism.** Ids depend on the base and on the order of the operations, never on a clock or a hash.
- **What the maximum includes.** The `w:id` of any element, including a revision element inside a `*PrChange`
  snapshot (Word 14 copies the mark's `ins` or `del` there, and `wordml` no longer reports those as revisions), so a
  copied id is never reused.

### 5.2 Authors

The author is written as given, after `RevStamp::new` has checked it (non-empty, at most 255 characters, no
control character; the schema type is a string of at most 255). It is never defaulted. "The same author" means
byte-for-byte equality of `w:author`, because that is all the file holds (Word's names include trailing commas
and initials, `Alvarez,`), so `Jane Doe` and `jane doe` are two authors.

### 5.3 Dates

`w:date` is `YYYY-MM-DDTHH:MM:SSZ`, where the digits are **the wall-clock time of the date the caller gives**. This is
Word's own convention. Its `w:date` is the author's local clock labelled `Z`, and `w16du:dateUtc` carries the instant (in
Word 16 documents the two differ by the author's offset: +2 h in 228 elements, +10 h in 56, -7 h in 30). A caller that
passes a UTC instant gets UTC. A caller that passes local time, `2026-10-05T14:03:27` or the same with the offset it was
taken at, gets that local time, labelled `Z` as Word does. The CLI defaults to the machine's local time. A host application whose sandbox clock is UTC must pass
the user's local wall-clock time, because a redline stamped in UTC would show its
reviewers a time that is hours off. The receipt records the date that was written.

`RevStamp::new` is a **validator, not a converter**. It accepts `YYYY-MM-DDTHH:MM:SS` with an optional fraction and an
optional `Z` or `+HH:MM` or `-HH:MM`. It checks that this is a real date and time (the month is 1 to 12, the day is
within the month with leap years, the hour is below 24, the minute and the second are below 60), and writes the first 19
characters followed by `Z`. A fraction or an offset is dropped, with no conversion. Anything else is `invalid_payload`,
including a date with no time. It needs no calendar library, only the days in each month.

- **Why seconds.** Word writes minutes (3,108 of 3,151 dated elements end in `:00Z`), but 43 Word-producer
  documents have seconds, and producers such as LibreOffice and Aspose write seconds, fractions and offsets
  that Word reads. Keeping the seconds keeps two batches in one minute apart.
- **No `w16du:dateUtc` in 1.0.** Writing it would add a namespace to the root of every part that is edited, with the
  `mc:Ignorable` change, for a display detail, and it needs the offset, which the caller may not know. `FIDELITY.md` will
  say so, and checklist C4 is the Word check of how Word shows a date that has no `dateUtc`.
- The date is always written for tracked edits: "every edit becomes a Word revision with an author and a date".
  Word omits it when personal information was removed (25% of elements), and a reader never needs it.

### 5.4 `w:rsid*`, `w14:paraId` and `w14:textId`

- **`w:rsid*`**: never added, never removed or rewritten. Word does not need them: only 84 of 168 Word
  documents that have run revisions carry them on most revision runs, and tools that write none produce documents Word opens.
- **`w14:paraId`** on a new paragraph, only if the paragraph it is modelled on has one. Real documents are
  all or nothing (2,852 documents have none, 1,450 have it on every paragraph, 39 are in between), so the
  rule matches the document. The value comes from `Tx::mint_id(IdKind::ParaId, part)`. The prefix `w14` is
  declared whenever a paragraph has one, so no root edit is needed. Both ids are written only in the Word 2010
  namespace: a model whose `paraId` is in the beta namespace of Word 2007 (`.../office/word/2007/5/30/wordml`,
  where `textId` does not exist) gives the new paragraph neither. Split keeps the id on the first half,
  and a cloned row or paragraph always gets new ids, never a copy. `check` makes sure ids stay unique and
  ignores `mc:Fallback` copies.
- **`w14:textId`**: `77777777` on every new paragraph that gets a `paraId`. Word writes that value on 83%
  of paragraphs (18,553 of 22,293). It needs no minting and no uniqueness, and the sentence in `ARCHITECTURE.md`
  that listed it among the hashed identifiers is changed to match (decision 6). An **edited** paragraph keeps its
  `paraId` and its `textId` as they were, and so does the first half of a split. Word changes the `textId` when it
  saves a paragraph whose text changed, but a stale value is legal, nothing in the package depends on it, and
  rewriting it would change bytes that the operation had no need to touch (law L2).

### 5.5 Untracked mode

`Track::UntrackedBy(author)` applies each primitive directly. It takes no id from the `Tx`, so `Compiler::new` has nothing to read.
The author says whose pending changes are the edit's own: those are edited in place, and **another author's pending change is
never rewritten or hidden** (decision 4). `Track::Untracked`, with no author, treats every pending change as another author's.
The checks are in `foreign.rs` and run before anything changes, so a refused call has changed nothing: text inside another
author's insertion, deletion, move or inserted or deleted row or cell, or in a run with their formatting change, is not deleted
or formatted; new text is not put inside their wrapper (at its edge it goes beside it, as nobody's insertion); properties under
their `*PrChange`, or of a paragraph whose mark they inserted, deleted or moved, or of a row or cell they inserted or deleted, are
not set; and a paragraph, a row or a merge that would take their changes with it is refused. Each is `readonly_region`, and the
message says to accept or reject that change first.

| Primitive | Untracked |
|---|---|
| text | the characters are replaced in place (`replace_text_range`), runs and wrappers left empty are removed, new text goes in the nearest run to the left, and stays in an insertion only if it is the author's own. Text inside another author's pending insertion is `readonly_region` (decision 4) |
| properties | the properties are set on the container. The author's own pending `*PrChange` there stays, and its snapshot is still the baseline. Another author's is `readonly_region` (decision 4) |
| `insert_block`, `split_block` | a new paragraph with no marks of ours |
| `merge_blocks` | Q's base properties are aligned to P's, then the join of 6.2: P's element survives, Q's `w:pPr` moves into it, Q's content is appended and Q is removed. Declares `merge(first, second, into: P)` |
| `delete_block` | the paragraph is removed. A paragraph with a section break, or the last one of a container, is emptied and kept |
| rows | a row is copied by the copy rule and filled, or removed |

The refusals that do not depend on tracking (a deleted block, a field, a locked control) still apply.
Untracked edits keep guarantee 1: they change the edited elements only (law L2), and the laws of
section 7 that say "reject gives the original" do not apply to them.

## 6. Accept and reject

### 6.1 Changes, selection and closure

An agent does not think in revision elements. It thinks "the deletion of that clause" or "Eric's move". A
**change** is the unit that `changes()` returns, `Document::revisions` lists, receipts report, and that accept and
reject by id act on. `changes(story, tree, blocks)` groups the revision elements of the blocks it is given, in
document order (the whole story for `Document::revisions`, only the touched and new blocks for a receipt). A range of
blocks is **extended to whole changes**, so a receipt never cuts an Enter pair, a split insertion, or a change that
straddles the edge of the range.

The grouping works on each paragraph's **items in text order** (the items of the `TextMap`, including the ones that
`Final` leaves out) and not on XML siblings, because a pending revision that someone split or nested into is no longer
made of adjacent siblings. A pending insertion that a second author wrote into the middle of is
`ins(A, id 1){x} ins(B){new} ins(A, id 9){y}`, and it is still one change of A's.

1. **Pieces.** A *piece* is a maximal run of consecutive items under one revision element of kind `ins`, `del`,
   `moveFrom` or `moveTo`. An item can be in the pieces of several elements: the text of `ins(A){r del(B){r} r}` that
   B deleted is in A's piece and in B's.
2. **Changes of text.** Two pieces of the same kind, author and date are one change when everything between them in
   item order is zero-width or is itself under another revision element: the `del` of another author nested in the
   `ins`, our `ins` between the two halves of a split insertion, the tail of a split that has a new id. Plain items
   between them separate them. The wrappers do not matter: a deletion that straddles the edge of a hyperlink, a content
   control or a smart tag is one change.
3. **Property changes.** The `rPrChange` of consecutive text items with the same author, date and snapshot (compared
   by name, attributes and children) are one `RunFormat` change, so that a carve, which copies a pending change under a
   new id, does not split it. The same holds for the `pPrChange` of consecutive paragraphs. Any other property change is
   a change on its own.
4. **A paragraph.** A mark revision and every run-level revision of the same kind, author and date in the same paragraph
   are one change of kind `ParagraphInserted` or `ParagraphDeleted` when nothing in the paragraph is outside it. A mark
   alone, with plain content, is a `MarkInserted` or `MarkDeleted` (a split, or a merge).
5. **An Enter pair.** An inserted mark, and the inserted content that starts the next paragraph, with the same author
   and date, are one `ParagraphInserted` change (the Y shape of 3.2.1).
6. **A row.** A row revision and the revisions of the same kind, author and date in its cells are a `RowInserted` or
   `RowDeleted`.
7. **A move.** Every element of one move name, from and to, is one `Move`.
8. **A field.** The revision elements that wrap the pieces of one field, of one kind, author and date.
9. **A text box.** A revision of the taken copy and its twin of the untaken copy.

```rust
pub struct Change {
    pub kind: ChangeKind,
    pub ids: Vec<i64>,            // every id of the change, in document order
    pub nodes: Vec<NodeId>,
    pub author: Option<String>,
    pub date: Option<String>,
    pub branch: Branch,           // Taken or Outside; the ids of the untaken twins are in `ids` too
    pub blocks: Range<usize>,     // the entries of the story that it spans
    pub touches: Vec<usize>,      // the entries that hold an element of it, which `changes(range)` filters by
    pub inserted: String,         // the text it puts in, at most CHANGE_TEXT_LIMIT (1,000) characters
    pub deleted: String,          // the text it takes out, in the same way
    pub text_cut: bool,           // true if either was cut at the limit
}
pub enum ChangeKind {
    Insertion, Deletion, Move,
    ParagraphInserted, ParagraphDeleted, MarkInserted, MarkDeleted,
    RowInserted, RowDeleted, CellInserted, CellDeleted, CellMerged,
    RunFormat, ParagraphFormat, SectionFormat, TableFormat, RowFormat, CellFormat, GridChange, NumberingChange,
}
```

`inserted` is what the change's insertions and moved-in text hold, in document order, with a line break for each
paragraph mark it inserts (after the text of its paragraph), and `deleted` is what its deletions and moved-out text
hold in the same way. Nothing is read from a field's code, from a text box (its paragraphs are blocks with their own
changes), or from a deletion that another author made inside an insertion, which is not text the insertion has any
more. A receipt carries both, and a host application's check on what an agent wrote can read them.

Why grouping, and not one id per element: splitting a pending insertion gives the tail a new id, so an
agent that listed id 17 before an edit would otherwise accept half of what it meant. Word's own user
interface accepts a contiguous change as one. The receipt of an operation lists all the ids of each change it wrote.

**Selection.** `Selector::All` selects every revision in every story part, in both copies of a text
box, and in `styles.xml` and `numbering.xml`. `Selector::Author(a)` selects every revision whose `w:author` is `a`
(an empty string selects the missing and the empty author). `Selector::Ids(v)` selects the change that contains
each id (an id that is on no revision is `not_found`, and an id that is on several elements, which the
input can have, selects them all). `Selector::Elements(v)` selects exactly the elements with those ids and does not group them
into changes; it exists for the laws (R2), and `ops` never emits it. **Closure** then adds the twin copies in text boxes, every element of a move
whose element was selected, and every piece of a field that one of whose pieces was selected.

**The index.** `resolve` finds an id in a `ChangeIndex` (id to nodes). The index lives for the batch. It is built from the
`Tx`'s **current** trees at the first `resolve`, not from the committed base, because the batch may already have written
revisions that only the staged trees hold, by one walk of the parts that hold revisions (the cost of `Story::revisions`, about 11 ms at
13 MB), and after that it is kept up to date and never rebuilt: after each compiler call `ChangeIndex::note` adds the
revision elements with ids in that call's `Effect::minted` (a walk of the call's touched and new blocks, which also finds the
tails of splits) and drops the ids in `removed_own` and `swallowed`, and after each resolved step the index drops the nodes
that step removed (resolution creates no revision element). `ops` hands `resolve` only **consecutive** accept and reject
operations as one list of steps, and merges consecutive steps of one action with `Ids` selectors into one. It never
moves an operation across an edit: a batch of `reject` of everything and then `replace` on p5 applies the replacement
after the rejection and does not reject it, and a `reject` after an edit sees the revisions that edit wrote.

**What else goes.** Rejecting an insertion removes the revisions nested in it, and the
`Resolution` lists them as swallowed, so the receipt can say that a pending deletion of another author went
with it. Accepting an insertion that holds another author's deletion leaves that deletion pending.

### 6.2 The algorithm

For each part with selected elements, over its `SpanTree` from `Tx::part_mut` (the styles and numbering parts too):

1. **Elements, inside out.** In reverse document order, which visits a nested element before its parent,
   apply the rule of 3B to each non-structural element: unwrap, remove, restore a snapshot, remove a marker. Node
   ids are never reused, so the selection can be a set of `NodeId`s, and a removal never invalidates another.
   Removal hoists range markers (3.6.5), and a table that loses its last row this way is removed with the same
   hoisting, so the markers its rows hoisted leave it too. `delText` and `delInstrText` are renamed back when a `del` is unwrapped.
   A restored snapshot is applied with `schema::insert_in_order` and `reorder`, and the container's out-of-snapshot
   children stay (3B).
2. **Marks, last to first.** The paragraphs whose mark is to be removed, in reverse order within each container.
   Each is **joined** with the paragraph after it. With nothing after it, an empty paragraph is removed and a paragraph
   with content keeps its place and loses the revision, except that a rejected inserted mark that ends its container keeps
   its paragraph, emptied, unless the inserted paragraphs before it follow a table or another block that is not a paragraph,
   or its whole container goes (an inserted row, a note or comment that is all new) (3.2.1). An empty paragraph that holds
   range markers and has no paragraph before it sends them to the start of the paragraph after it, past a removed table,
   rather than stay for them. When the first of two joined paragraphs is removed because it is empty and the second was emptied
   by the rejection (the new half of a Y), the empty runs and the attributes of the first go to the second, so that the
   paragraph reads and is written as it was before the split. Walking from the end makes a run of consecutive marks collapse
   into the first paragraph of the run, which ends with the properties of the last, as OpenXmlPowerTools' grouping does.
3. **Rows and cells.** Rows and cells that are removed go, and an accepted `cellDel` widens the cell before it (3B).
   A cell left with no block gets `<w:p/>`, a table with no row is removed, and a `w:vMerge` is converted for an accepted
   `cellMerge`. **As built.** A cell gets the paragraph only when what it held was removed: a paragraph, row or table
   that changed inside it leaves its last block as it was, so a cell that already ended with a table (LibreOffice writes
   them, `tdf153891`) gets none on accept, while a reject of a paragraph inserted after such a table gives the cell the
   paragraph W9 asks for. A story (the body, a note, a comment, a header or a footer) that an accept leaves with no block
   keeps one empty paragraph, which takes the markers left between blocks there. A reject keeps the last paragraph of a
   note or a comment whose blocks were all inserted, emptied, as it keeps the last paragraph of any container (3.2.1): the
   note goes only with its reference (3.6.4), and while the reference stays it must hold a block (`check` W9). A story that a
   reject leaves with no block, because its last block was an inserted table, gets one empty paragraph as on accept. So the
   compare law N2 takes one emptied paragraph for the empty note of `WC063-Footnote-Mod`, an original that breaks W9.
4. **Prune** the containers that became empty (3B).
5. **Notes.** A note whose last reference was removed by an accepted deletion or a rejected insertion is removed
   (3.6.4).
6. **Touch and declare.** Every existing block whose own bytes changed is touched, and so is a table that lost a row.
   A block that a **rejected** insertion removes declares `retire(.., Rejected)` (a row, and every paragraph in it, too).
   A block removed by an **accepted** deletion declares nothing, and the audit records `Deleted`.

**The join of `first` and `second`.** `second` is the paragraph after `first` in the same container, and `first`'s mark
is the one being removed.

- If `first` has no content left (a whole-paragraph deletion that is accepted, or an X paragraph whose content was
  rejected), its element is removed and `second` is untouched: it keeps its properties, its element and its ref.
  **As built.** A `w:proofErr` and a run with nothing but its properties and a `w:lastRenderedPageBreak` are no content
  here, and the range markers `first` leaves stand between blocks where it was, so `second` keeps its XML; only when a
  comment reference run is among them do they go to the start of `second` (3.6.5).
- Otherwise `second`'s content moves to the end of `first`, `first`'s `w:pPr` is replaced by `second`'s (the later
  paragraph's properties, mark and section break win, and with no `w:pPr` there `first` ends with none), and `second`'s
  element is removed. **`first` keeps its element**, its attributes (`w14:paraId`, the rsids) and its ref. A split followed by
  a reject therefore gives the original `w:p` back, and a real merge is the same operation. **As built.** An empty
  `w:pPr` of `second`, or an empty mark `w:rPr` in it, is dropped rather than moved into `first`.
- Touch `first`, and declare `merge(first, second, into: first's element)`. When `second` was emptied by the rejection
  of its inserted content (the second half of a Y paragraph), declare `retire(second, Rejected)` instead, which gives
  the reason an agent expects. `refs-and-state` shows the survivor's element as the later one for two consecutive marks
  (`merge(p5, p6, into: e6)`); `Tx::merge` accepts either element as `into`, and this design names the first.

Cost: one index walk per batch, one walk of each selected part, and then work proportional to the number of selected
elements.

### 6.3 Which properties survive, and which element and ref

- The merged paragraph has the **later** paragraph's properties, numbering and section break (3.2.4), in the **first**
  paragraph's element. The first paragraph's `sectPr` goes with its mark.
- The merged paragraph's ref is the **first** paragraph's. `projection` shows it under that ref, and the other
  refs report `merged into pN`. This is the part of "which ref the merged block shows under" that
  `docs/design/wordml.md` (3.6) leaves to `revisions`; `projection` decides how to show it.
- A paragraph whose mark is deleted and whose content is all deleted (the whole-paragraph deletion) is removed
  and the next paragraph is untouched. Word writes this shape without a `pPrChange` (52 cases, three of them with
  an empty one), so it cannot be a join into the next paragraph's properties.

### 6.4 Where this differs from OpenXmlPowerTools

The semantics are a port of `RevisionProcessor` (MIT), and the algorithms are re-implemented in Rust with
attribution in `NOTICE`. The places where they differ, and why:

| # | OpenXmlPowerTools | stetdoc | Why |
|---|---|---|---|
| 1 | Reject is "swap every `ins` and `del`, then accept". The flag that marks inserted text is never set | each element is resolved by its own rule | by-id and by-author need partial resolution, and a swap cannot do it |
| 2 | accepts or rejects everything | by id, author or all, with changes, moves and fields as units | the `accept` and `reject` operations |
| 3 | rejecting a `pPrChange` replaces the `pPr` with the snapshot and loses an inline `sectPr` | keeps `rPr` and `sectPr` | the snapshot type (`CT_PPrBase` in the standard) holds neither. Docxodus found and fixed the same loss |
| 4 | rejecting a `sectPrChange` loses the header and footer references | keeps them | the snapshot is `CT_SectPrBase` (Docxodus fix) |
| 5 | restores a mark's `rPrChange` snapshot whole, copied `del` included, so the mark stays deleted | keeps the live mark revisions and drops the stale copies from the snapshot | Word 14 copies them (105 in `n830205`) |
| 6 | removes range markers, bookmarks and comment anchors inside removed content | hoists them | a start must keep its end |
| 7 | an inserted field rejected: the unwrapped `separate` run stays | it goes with the `begin` and the `end` that the revision removed, and the result that no revision wraps stays | `RP020`, `FA/008`, `stress023` |
| 8 | a deleted math run leaves an empty `m:r`. Rejecting an inserted `m:ctrlPr` revision does nothing | the `m:r` is removed. The `m:f` is removed | tidy, and the reverse of accept |
| 9 | a deleted note stays as a childless `w:footnote` | the note is removed once nothing refers to it | the schema forbids a note with no block (Docxodus issue 631 chose the same) |
| 10 | removes every `w:rsid*` of the part and every empty `numPr` anywhere | touches only what the resolution changed | law L2 |
| 11 | merges adjacent tables that the removal of rows or paragraphs brought together, and rewrites the cell widths (`tcW`) of the merged table from its grid | does neither | no resolution changes `tblW` or `tcW`, merging two tables is an edit that nothing asked for, and Word does not require it |
| 12 | rebuilds block-level content controls by run ids | edits in place | the rebuild rewrites paragraphs it did not need to |
| 13 | pairs moves by scanning tags | pairs them by `w:name` and the range markers | partial resolution needs the pairs |
| 14 | does not reorder what it leaves | `schema::reorder` after a restore | no new violations |
| 15 | processes headers, footers, notes and styles, not comments | also comments, and `numbering.xml` | stories are `wordml`'s list |
| 16 | builds one new `w:p` with no attributes from a group of paragraphs joined by deleted marks, with the last paragraph's `pPr` | the first element survives, with its attributes, and takes the later paragraph's `pPr` | a split followed by a reject restores the original `w:p`, its `w14:paraId` and its ref |
| 17 | accepting a `cellDel` widens the cell before by one per deleted cell, and its source notes that the sum of the spans is the right amount | widens it by the sum of the deleted cells' spans | the row must still cover the grid |
| 18 | does not resolve a `w:moveFrom` or `w:moveTo` in a paragraph mark's `w:rPr`, so a paragraph whose whole text was moved stays as an empty paragraph where the text was moved from (accept) or to (reject) | resolves the mark with the text, and the paragraph goes | the mark moved with the text, and the text model reads the join (`RP015`) |

Nothing in the table changes what an accept of all revisions yields, for the shapes where OpenXmlPowerTools is
right. The differential test of section 8.2 b is `tests_oxpt.rs`: over the 52 `RP` documents of its test files and
their 102 baselines (`-Accepted` and `-Rejected`), the text of every story after `accept_all` or `reject_all` is the
baseline's in all 102, and the paragraphs with no text differ in two (`RP015`, row 18). Each such fixture is in the
table `DIVERGENCES` with its row, and the test fails for a difference that has none and for a row that no longer
differs.

## 7. The laws

The round-trip laws L1 to L6 (`GLOSSARY.md`) say what must hold, with three release checks: the edit laws L2 to L5 on property tests, no new schema or Word-rule violation in any output, and compare's accept-all and reject-all giving back its two inputs. This section says what they mean for
`revisions`, and how each is checked with something other than the code it checks.

### 7.1 Vocabulary

For a document D, a batch B (a list of compiler calls with one stamp) and the document N after B:

- `Final(X)` and `Original(X)` are the texts and block lists of X in `Mode::Final` and `Mode::Original`, as
  `wordml` reads them. The text model derives them from the wrapper rule alone, so it shares no code with the
  compiler or the resolver. In `Original` mode a paragraph whose mark is inserted reads as joined with the next one
  (`Story::join_run`), so a **join run** is a list of one or more paragraphs that read as one.
- `Outcome` is what B reports: for each call the annotation ids it took, the pairs it split, the own text,
  marks and formatting it removed or folded, the revisions that went with them (`swallowed`) and where each new block
  came from (`origins`). Its `created` list is derived at `finish` from the ids the `Tx` minted (2.2): the revision
  elements whose author is the stamp's, whose id was minted, and that are not the new id of a split.
- A **trailing inserted paragraph** is a paragraph whose mark is inserted and that has nothing after it in its container,
  such as the new paragraphs of `RP039`. `wordml`'s `Original` mode ignores the join at the end of a container, so it reads
  as a paragraph of its own, emptied of its inserted content, and rejecting keeps it so (3.2.1): R1 and R8 compare it like
  any other paragraph. Only a run of them that is empty and follows a table, which rejecting removes, is left out. stetdoc
  writes none after a paragraph: it uses Y there.
- A **trailing deleted paragraph** is the mirror: an empty paragraph (no live text, no atom) whose mark is deleted and that
  has nothing after it in its container, such as the last paragraph of each cell of a deleted row (164 such marks in the
  corpus) or of a deleted note. `wordml`'s `Final` mode keeps it, and accept-all removes it (3.2.5). R8 leaves it out of the
  accept side.
- **`reject_elements(X, ids)`** rejects exactly the revision elements with those ids, closed under moves, fields and
  text-box twins (6.1) and **not** grouped into changes (`Selector::Elements`). Grouping would reject an earlier batch's
  change of the same author and date as well.
- The **string model** is a model of B that knows no XML: the `Final` text of each paragraph as a vector of
  characters, the block list as a list, and each call as a splice, a split, a merge, an insert or a delete on those.
  It says what the `Final` result must be.
- **`normalize(X)`** is the comparison form of a package, used wherever two documents must be "the same":
  1. drop `w:rsid*`, `w14:paraId`, `w14:textId`, `w:proofErr`, `w:lastRenderedPageBreak` and a bookmark named `_GoBack`;
  2. merge adjacent runs that have equal `rPr` (and equal text kind), where equal includes an equal `rPrChange`
     **up to its `w:id`** (a carve copies a pending change under a new id), merge adjacent `w:ins` or `w:del` with equal
     author and date, and unwrap a wrapper with no child;
  3. drop an empty `w:rPr`, `w:pPr` and `w:numPr`, and the attribute `xml:space` where it is not needed;
  4. drop a `*PrChange` whose snapshot equals the properties it sits in, a **no-op change**: Word writes an empty-snapshot
     `pPrChange` on paragraphs that have no properties (`RP040`, in 3 of 52 whole-paragraph deletions), and accepting or
     rejecting one changes nothing. Because `normalize` hides them, `finish` refuses a no-op change that the transaction
     created (W11);
  5. renumber `w:id` by document order and compare author and date as written;
  6. sort attributes, drop white space between elements, and compare namespaces by URI;
  7. leave the order of elements alone (it carries meaning).
  `normalize` is a function of the XML of the story parts that a batch can touch. It is written once, in the
  testkit, and the laws call it.

### 7.2 The laws

| Law | Statement | Checked against |
|---|---|---|
| **R1 Original is preserved** (L5) | for every story, the `Original`-mode join runs of N and of D are the same list, **leaving out trailing inserted paragraphs**: the same runs in the same order, each run **keyed by its first paragraph that existed in D** and with the same text. A paragraph that the batch inserted before another (shape X) is joined into its twin in `Original`, so the run is keyed by the twin's ref and not by the new paragraph's. And `normalize(reject_all(N)) = normalize(reject_all(D))`, which adds formatting and structure | the `wordml` text model for the text, for the rest the resolver itself on both sides |
| **R2 Rejecting what was written restores the original** (L5) | `normalize(reject_elements(N, created ∪ A_N)) = normalize(reject_elements(D, A_D))`. A is the set of the author's own pending revisions that the batch removed, folded or split: the ids in `removed_own`, in `folded` (the id of a change, or of an inserted mark that was edited directly), and both ids of every pair in `split` (the old ids in D, the old and the new in N). A_D keeps only the ids that exist in D. `created` is what 2.2 defines: our author, a minted id, not the new id of a split. The rejection is **by element** (7.1): an earlier batch of ours with the same author and date must not be rejected with it, and the tail of another author's split insertion must not be either. Rejecting A on both sides takes away exactly what the carve-outs changed, so the two sides are equal. With no carve-out A is empty and the law reads "rejecting what was written gives D" | the resolver, restricted to the ids in `Outcome`, against the input |
| **R3 Accepting gives the intended result** (L3, L5) | the `Final` text of every block of N is what the string model gives, and the block list is the model's. `accept_all(N)` has the same text as `Final(N)` | the string model and the `wordml` text model |
| **R4 Existing revisions are untouched** (L2) | each revision element of D is still in N with the same id, author, date and kind, unless `Outcome` lists it (`split`, `removed_own`, `folded`, `swallowed`) or the batch's own `accept` or `reject` resolved it. Its subtree is the old one once the new revision elements nested in it are taken out and the pieces of a split are rejoined. No old revision gains or loses an attribute | the revision census of D and N (`Story::revisions` on both) |
| **R5 No new violations** (L6) | every dirty unit passes the rule of `schema` (`validate_unit`: the keys after are a subset of the keys of the origin, where the origin of a new block is the units of `Effect::origins`), and the Word rules W1 to W10 of 7.3, and W11 for what was created, hold on what was written | `schema`, `check`, and the post-conditions of `Compiler::finish` |
| **R6 Locality** (L2) | the bytes of every element outside the touched blocks are unchanged. The touched blocks are exactly `Tx::touched()` plus the new blocks. Parts that were not staged are the same `Arc`. The part's root start tag may gain namespace declarations and nothing else | state's touch oracle, and the corpus laws of `xml` |
| **R7 Refs are right** | after commit, `resolve` of every ref ever issued gives the block the identity model says, or an error with the right reason: a tracked deletion keeps its ref, a split keeps the first, a merge and a join keep the first, a rejected insertion is `Rejected`, an accepted deletion is `Deleted` | the identity oracle of `refs-and-state` section 16, with the declarations of this module |
| **R8 Resolution is sound** | on every corpus document with revisions, `text(accept_all(D)) = Final(D)` (leaving out trailing deleted paragraphs) and `text(reject_all(D)) = Original(D)` (leaving out trailing inserted paragraphs); resolving twice changes nothing; after `accept_all` or `reject_all` there is no revision element left; `accept` by author A then by author B equals `accept_all` when A and B are all the authors; and the text equals pandoc's `--track-changes=accept` and `reject` where the oracle runner exists | the `wordml` text model and pandoc. These are also the oracle of compare's quality check: `compare` is correct when this engine is |
| **R9 Determinism and atomicity** | the same bytes, batch and stamp give identical output bytes and `Outcome`. A call that fails leaves `export` and the state version unchanged | the harness of `state`'s atomicity test |
| **R10 A window is the whole story's answer** | `changes(story, tree, Some(range))` is, for every range, the changes of the whole story that touch it, each whole | `changes(story, tree, None)` |
| **R11 Ids select whole changes** | `Selector::Ids` takes exactly the revisions of the changes that hold the ids, through the index and by windows alike | the changes of the whole story |
| **R12 The index is complete** | after every call of a batch with resolves between its compilers, `ChangeIndex` knows every revision of the document by id | `part_revisions` |
| **R13 Origins are declared** | every block a call makes has an `Origin` in its `Effect`, and what `Outcome::origins` hands on names only blocks of the document before the batch | the document before |

**As built.** R6 leaves out the range markers that were hoisted on purpose: those that were inside a block that went or
was touched, and those the batch made in a block it made or touched, which the law notes after every call and accept
(`laws_util::MarkerHomes`). A marker that stood outside such blocks, or a new one written anywhere else, is compared.

For a sequence of batches by different authors (A, then B on the output, then A again) R1 to R4 hold for each
step against the one before it. This is the case that makes nested revisions and splits happen, and the
generator produces it (7.4).

R2 has carve-outs, and they are on purpose. A batch that removes the author's own pending insertion, or folds an
edit into its own pending formatting change, or splits a pending revision, changes something that rejecting only
the new revisions cannot bring back. `Outcome` says when that happened, so the test compensates by rejecting the same
own revisions on both sides, and the receipt tells the agent.

### 7.3 Word rules and post-conditions

These are the rules a revision must follow to open in Word without a repair prompt, and that the schema alone
does not say. `check` (`check.md` 3.1) owns the one implementation of W1 to W10 and runs it on any
document, so `Document::check` reports them for input too, and `Compiler::finish` calls `check::scan_unit` on the
subtrees the transaction created and expects nothing in them. W11 is `finish`'s alone, since only it knows what the
transaction created (a failure of either is `internal`). `finish` keeps only the findings at or below a revision it
created whose key the part did not have before the transaction, since the rest is the input's, which `check` carries. Where each rule is read, as `check.md` has it: W1, W2, W5, W9 and W10 in
one pass over a unit; W3 and W7 as a census over a story part; W8 from the field index; W4 and W6, and the hyperlink half
of W5, are the schema's (`invalid_attribute_value`, `missing_attribute`, `non_canonical_order`, `out_of_order`,
`unexpected_child`).

| Rule | Says |
|---|---|
| W1 | `w:delText` and `w:delInstrText` have a `w:del` ancestor, and `w:t` and `w:instrText` do not, within one paragraph. (Moved-from text stays `w:t`.) |
| W2 | text with leading or trailing white space has `xml:space="preserve"` (`w:t`, `w:delText`, `w:instrText` and `w:delInstrText`) |
| W3 | every revision has a `w:id`, and the ids of the revisions in the story parts are unique across the parts. Bookmark ids are unique **within a part**, because Word repeats them across parts (1 of 4,121 documents), and comment ids are the comment audit's. A revision and a bookmark may share a number, as they do in 1 of 257 documents, and the compiler's counter never produces such a pair. In `styles.xml` and `numbering.xml` Word repeats ids (`RP037` has a `pPrChange` and an `rPrChange` that both have id 0), so those two parts are not checked, and neither are the snapshots inside a `*PrChange` |
| W4 | (the schema) `w:author` is present and has at most 255 characters, and `w:date`, when present, is an `xsd:dateTime` |
| W5 | within one paragraph, no `w:ins` inside a `w:ins` and no `w:del` inside a `w:del` (the schema allows it, Word does not write it). A `w:ins` that holds a `w:hyperlink` or a `w:fldSimple` is the schema's |
| W6 | (the schema) in a paragraph mark's `w:rPr` the revision children come first, `ins` before `del`, and `rPrChange` is last; `pPrChange` is last in `w:pPr` |
| W7 | range markers are paired by id within a part, and every move name has one `moveFrom` side and one `moveTo` side. Comment ranges are the audit's |
| W8 | a field is whole in its revision: if any piece of its structure is in a `w:del`, all of them are (an inserted field may leave `separate` outside) |
| W9 | the last block of a `w:tc`, looking through a block-level `w:sdt` or `w:customXml` and ignoring range markers, is a `w:p`; a `w:tbl` has a `w:tr` and a `w:tr` a `w:tc`; a note or a comment holds a block |
| W10 | no empty `w:ins`, `w:del`, `w:moveFrom` or `w:moveTo` at run level, and no empty `w:rPr` or `w:pPr`, left by an edit. The snapshot inside a `*PrChange` is exempt: an empty one is legal and Word writes it (`FA/019`, `FA/004`, `RP040`). This is hygiene and not a promise about Word, and as a no-new-violations rule it never blocks an edit for an empty property that was already there |
| W11 | **checked by `finish` only**, since only it knows what the transaction created: no `*PrChange` created by this transaction has a snapshot that equals the properties it sits in. `normalize` hides such a no-op change, so the laws cannot see it. A fold that brings the properties back to the snapshot removes the change instead (2.2, `set_props` step 5) |

**Origins.** `check` accepts a unit that has keys its base node did not have only if a block it was built from had them,
so every call declares, in its `Effect`, an `Origin` for each block it makes: the new paragraph of `insert_block` from the
paragraph it is modelled on, the tail of `split_block` and the second paragraph of a shape Y Enter from the original, each
paragraph of a new row from the paragraph it copies, and the survivor of a `merge_blocks`, a `delete_block` that joins and
a resolver join (`Resolution::origins`) from both paragraphs. A block made from nothing declares an empty list. `Outcome::origins()`
hands them on in order with each `from` followed back to blocks the document had before the compiler began (a tail that a
merge joined back is the paragraph it was cut from, and a block made from nothing is dropped), which is what `ops` puts in
`check`'s `Declared`. Law R13 holds every call to it.

### 7.4 How the laws are exercised

- **Generator.** From the public `view` of a document (so it never reads XML) it draws: a paragraph, a word or a
  character range inside it (on word boundaries and not), a replacement text from the paragraph's own vocabulary
  and from a fixed set (accents, a tab, a long word), a property edit from a short list, a split offset, a
  neighbouring pair for a merge, a row. A batch is one to eight calls. At least 50 batches per curated document on every pull request and the full corpora nightly.
- **Sequences.** A second author edits the output of the first at a later date, sometimes inside the first author's
  insertions, then the first author edits again.
- **Documents.** The curated corpus (`word-authored` with revisions, `lab` for scale, `edge`), the synthetic
  fixtures with nested wrappers, and a large body built by repeating real ones for the budget.
- **Failures are minimised** by deleting calls from the batch until it passes, and the minimal batch is recorded as a
  fixture. A failing law is never fixed by accepting a snapshot.
- **Direct tests.** `revisions` has invariants of its own that no law reaches, so it has unit tests beside the laws, as
  `ARCHITECTURE.md` rule 5 allows for modules with their own invariants (decision 9 adds `revisions` to the list):
  a split run keeps every child; a carve at every character of a run with a tab, a field, an atom and nested
  wrappers three deep; the hoisting of a marker; the one-map claim of 2.2 (the bytes with one map equal the bytes of a version
  that rebuilds the map before each pair, kept in the tests only); the grouping of 6.1 on a split and nested insertion; the
  cleaning rules of 2.5 on a table of hand-written before and after pairs.

## 8. The oracle without Word

Word is not installed on the development machine. The Word check is a hand check in Word for the web plus 500 seeded
outputs through headless LibreOffice ([decision 0005](../decisions/0005-word-oracle.md)), and most of what it asks can
be tested without Word, because Word's own output is in the corpus.

### 8.1 What stands in for Word

1. **Word's writes.** The 76 small scenario documents and the 257 Word-fingerprinted documents of section 1 are Word's
   answer to "what do you write for this edit".
2. **The text model of `wordml`**, for what each mode shows, independent of any XML rewriting.
3. **pandoc** in the three `--track-changes` modes, as the independent reader of accept and reject, through the testkit's
   oracle runner.
4. **OpenXmlPowerTools' `RevisionProcessor`**, as the reference for accept and reject (section 6.4 lists where
   stetdoc differs), through its `-Accepted` and `-Rejected` baselines.
5. **A string model** of the batch (7.1).

### 8.2 The tests

**a. The Word-written scenario test** (the Word check without Word). For each row of the table, take the Word-written document, make
the original with `reject_all` (checked by R8 against `Original`), run the operations of the row through the
compiler under a fixed stamp, and compare `normalize` of the result with `normalize` of Word's document, ids, dates and
authors renamed. They must be equal. **There are no known divergences**: a row that differs is a bug, or a decision
to change this design, and it is not entered in a list of exceptions. The one thing the comparison hides is the no-op
`*PrChange` of 7.1, which Word writes in 3 of 52 whole-paragraph deletions and which carries no meaning.

| Scenario | Document | Operations |
|---|---|---|
| delete a word | `FA/001-DeletedRun`, `RP002-Deleted-Text` | `replace_text(.., "")` |
| insert a run | `FA/010-InsertedRun`, `RP003-Inserted-Text` | `replace_text(at..at, text)` |
| delete a paragraph | `FA/013-DeletedParagraph` | `delete_block` |
| delete from the end of one paragraph through the end of the last | `RP040-Deleted-Paras-at-End` | `delete_block(P2)`, `delete_block(P3)`, `merge_blocks(P1, P3)`. Word's file has P1's text kept and its mark deleted, P2 deleted whole, and P3's content deleted with its mark kept |
| delete numbered paragraphs | `FA/002-DeletedNumberedParagraphs` | `delete_block` five times |
| insert a paragraph before | `FA/009-InsertedParagraph` | `insert_block(Before)`: shape X before the twin |
| append paragraphs | `RP039-Inserted-Paras-at-End` | `insert_block(P1, After)` and then `insert_block(P2, After)`: shape Y at the end, twice, since check C11 (3.2.1). Word's file has the first paragraph plain and an inserted mark and inserted content on each new one, which Word's Reject All resolves to an empty last paragraph; stetdoc's keeps the last mark live, so rejecting gives the original back |
| Enter at the end of a paragraph, then number the paragraphs | `FA/005-InsertedNumberedParagraph` | `insert_block(P1, After, props: list)` (shape Y), `set_props(P1, list)` (the mark of P1 is our own pending insertion, so no `pPrChange`), `set_props(P3, list)` (the paragraph that was second before). Word's file has exactly these three |
| split | `RP006-Inserted-Paragraph-Mark` | `split_block` |
| merge, equal properties | `RP005-Deleted-Paragraph-Mark`, `RP007-Multiple-Deleted-Para-Mark` | `merge_blocks`, three times for `RP007` |
| paragraph, style, list properties | `FA/018`, `RP025`, `RP037`, `FA/017`, `RP022`, `RP023`, `FA/004` | `set_props(Paragraph)` |
| mark properties | `RP024-ParagraphMark-rPr-Change` | `set_props(ParagraphMark)` |
| run properties | `FA/019-RunPropertiesChange` | `set_props(Run)` |
| section, table, row, cell | `FA/020` to `024`, `RP027`, `RP029` to `RP033` | `set_props(Section ..)` |
| insert a row, delete a row | `FA/007`, `RP010`, `FA/006`, `RP009` | `insert_row`, `delete_block(row)` |
| delete a field | `FA/003-DeletedFieldCode`, `RP019` | `replace_text(.., "")` over the field |
| a second author | `cross-author-slicing/*-pending` (tool made, T) | `replace_text` by author B on the document with A's insertion |

A replacement has no row, because no scenario document holds one edit that replaces text. What the corpus does hold is the
order. Counted over the full corpus of 48,666 documents, same-author pairs are `del` then `ins` 139 times in 24 documents
and `ins` then `del` 45 times in 9, and the catalog (d) asserts those counts as ranges.

The accept and reject shapes of moves, math, content controls, fields inserted by Word, `cellMerge`, `numberingChange` and
`numPr/ins` have documents and no compiler row: they are covered by **b** and **c**.

**b. Differential test against OpenXmlPowerTools.** For each of the 52 `RP` documents and the 24 `FA` documents,
`accept_all` and `reject_all` are compared, after `normalize`, with the `-Accepted` and `-Rejected` baselines
where they exist (the baselines come from that code, not from Word, so they find disagreements and do not prove
correctness). Every fixture where they differ is listed with the row of 6.4 that explains it. A difference
that no row explains is a bug in one of the two.

**c. Engine against the text model.** R8 on every document with revision markup: the text after `accept_all` and
`reject_all`, per paragraph and per story, equals what `wordml` reads in `Final` and `Original` mode. On the full
corpus (`STETDOC_CORPUS_SET=full`) that is 1,174 documents, and none fails (with R10 and R11 on each, and no schema
violation added to the main part); `tests_corpus.rs` runs it on whatever set is named. Pandoc is a third reader where it
is installed (`tests_pandoc.rs`, with the runner in `stetdoc-testkit`'s `oracle`): it reads each document with
`--track-changes=accept` and `=reject`, and the words of its text are compared with ours and printed. Pandoc was not
installed where this was built, so the divergence classes of that comparison are not known yet.

**d. The catalog of shapes.** Section 1.3 is turned into a corpus-gated test: each row is an assertion about a
named document (the `w:del` of `FA/001` has `delText`, `RP006` has an inserted mark on the first of two
paragraphs, and so on), and the counts of 1.2 are asserted as ranges. When the corpus changes or Word's behaviour is learned
from the first oracle run, a row changes in one place.

### 8.3 What this cannot prove

It proves that stetdoc writes what Word wrote, and that its accept and reject agree with the text model and with
two other implementations. It cannot prove that Word accepts what stetdoc writes where Word wrote nothing to compare
with (an alignment `pPrChange` on a merge, an `ins` around a `del` of another author's text inside a hyperlink), or that
Word resolves a revision as stetdoc does when the corpus does not say. Those are the checklist.

### 8.4 Checklist for a person with Word

Each item says what to do and what it decides. A "pass" is "Word opens the file with no repair prompt and
does what is described".

| # | Do | Decides |
|---|---|---|
| C1 | Tracked-delete the paragraph mark between a Heading 1 paragraph and a Normal paragraph that both have text. Accept all | which `pPr` the merged paragraph has when a deleted mark is accepted (3.2.4, 6.3). If Word keeps the first paragraph's, resolution changes to "first" and the alignment `pPrChange` becomes unnecessary. For rejecting an inserted mark `FA/005` already says "later" |
| C2 | Press Enter inside a Heading 1 paragraph, set the second half to Normal, with tracking on. Reject all | the same, for an inserted mark, where the two halves differ (`FA/005` has equal ones in effect) |
| C3 | Tracked-delete a mark that has no paragraph after it: the last paragraph of a cell, of the body, before a table. Accept all | the "nothing after" rule of 3.2.5 |
| C4 | Open a file with `w:date="...T14:00:00Z"` and no `dateUtc`, and one with seconds (`14:03:27Z`) | decision 5 and 5.3: how Word shows a date with no `dateUtc`, and one with seconds |
| C5 | Insert text, save, reopen after a minute, delete part of it with the same author | own pending text removed outright (4.1) |
| C6 | Insert text in the middle of another author's insertion. Look at the Reviewing pane | the split in 4.2: three changes |
| C7 | Open and resolve `ins{r del{r} r}` made by stetdoc: accept all, reject all, accept only the deletion, reject only it | the nested shape of 4.1 |
| C8 | Delete text inside another author's moved-to text; insert inside it | the moveTo rules of 3.5 and 4.1 |
| C9 | Open the three paragraph shapes of 3.2.1 (insert after, insert before, split). Reject all, accept all | R2 and R3 under Word |
| C10 | Open a stetdoc merge of two paragraphs with different properties, with its alignment `pPrChange`. Accept all, reject all | the net of 3.2.4 |
| C11 | Open a stetdoc paragraph inserted after the last paragraph of a table cell and after the last paragraph of the body. Reject all, accept all | **Settled (2026-10-10)**: with shape X, Word for the web left the cell an empty paragraph after Reject All (`15-old`), so stetdoc writes Y there (3.2.1) and its resolver keeps a rejected trailing paragraph, emptied, as Word does; `15` and `04` have the new shape |
| C12 | Reject all on a field that Word inserted (`ins` around `begin` and the code, `separate`, `ins` around the result) | no piece of structure is left (3.6.3) |
| C13 | Open a field deleted by stetdoc and reject it | the code survives as `instrText` |
| C14 | Open a stetdoc deletion of a footnote reference. Accept all | note removal (3.6.4) |
| C15 | Open an inserted and a deleted row from stetdoc, and a deleted row with a nested table | 3.4 |
| C16 | Delete a paragraph that carries a section break, in stetdoc. Accept all | the kept mark (3.2.2, decision 8): the section survives, where Word would drop the break |
| C17 | Format text twice, with the same author, over two sessions | one `rPrChange` or two, and which snapshot (3.3) |
| C18 | Resolve one half of a move by id in the Reviewing pane | closure by move name (6.1) |
| C19 | Accept and reject the math and content-control revisions of the corpus fixtures (`RP013`, `RP016`) | 3B |
| C20 | Open a list restart from stetdoc (a new `w:num` with `startOverride`) | 3.3 |
| C21 | Open 500 stetdoc outputs, stratified by shape, in Word | no repair prompt |

## 9. Performance

The budget is a 50-operation `edit` on the largest body in under 1 s. State and `wordml` use 265 to 405 ms of it
(`refs-and-state` section 15). That figure already contains one refresh of the story after each write (about 4.5 ms
on the worst body), so a batch of 50 calls pays for 50 refreshes there and not in this module. The cost of `revisions`,
and the reasons:

| Step | Cost | Where the estimate comes from |
|---|---|---|
| The first `Tx::next_annotation_id` of a transaction: the highest id | 0 after the first transaction of a state (`PartsView::ids` is cached on the base). The first pass is a walk over the elements of every story part: about 10 to 30 ms at 13 MB | `wordml.md` section 8: the field pass, a similar walk, is 11 ms at 13 MB |
| Each later id | one increment | state's design |
| Resolve a ref and check attachment | 10 ns and at most 256 parent steps | state's design |
| The paragraph's `TextMap`, **once per call** | tens of microseconds (all paragraphs of a 2.6 MB body take 3 ms) | `wordml.md` section 8. Building it again for each pair of the diff would cost a story refresh each time, 6 pairs a replace and 50 replaces, about 1.35 s: this is why the call keeps one map (2.2, step 3) |
| The diff | prefix and suffix trim, then work on the middle only, then the cleaning of 2.5, which is linear in the number of edits. At most 250,000 cells in a stretch and 4,000,000 in all, then one edit per stretch over the limit. Measured: the 35 diff tests, with about 27,000 random pairs and the cases at the cell limits, take 1.5 s unoptimized and 0.4 s optimized | `revisions/tests_diff.rs` |
| XML edits for one pair: carve (a clone and a few moves), wrap, build a run, up to 20 editor calls | each constant or linear in the paragraph's children. Moves are constant time (a previous-sibling link). The dirty flags mark O(depth) once per path. **Measured with a throwaway spike on `xml`'s `Editor`** (release build, a body of 15,935 paragraphs): 50 tracked replaces (two `clone_subtree`, `set_text`, `replace_text_range`, a `w:del` and a `w:ins` with attributes, the `w:t` to `w:delText` rename, one `rPr` clone) took 0.68 ms at 30.7 MB and 0.59 ms at 10.2 MB, and the output serialized in 7.3 and 2.9 ms. So 12 to 14 µs a pair | `package-and-xml.md` 2.2, 2.3, and the spike |
| `schema::insert_in_order` into a `w:pPr` or `w:rPr` | the work is linear in the children times the states of the model (`schema/order.rs`). About ten children: tens of microseconds | the `schema` contract |
| `touch`, declarations | constant | state's design |
| `finish`: derive `created`, check W1 to W11 | a walk of the touched and new blocks only, a few microseconds each | design |
| `changes()` for a receipt | the blocks of the range and a window around them (two blocks on each side, grown by four while a change reaches the edge or the first block of the window holds a revision), not the story. **Measured: twenty single-block receipts take 0.8 to 1.1 ms on every input** | 6.1, section 16 item 23, `bench/revisions.rs` |
| `resolve` | one index walk per batch, at the first `resolve` (8 ms at 12 MB), extended after each edit by `ChangeIndex::note`, which walks only the blocks that call touched and made. Then O(selected) edits, each under 1 ms. **Measured: accepting every revision of a document that holds 80 takes 24 to 84 ms; twenty accepts and rejects by id one after the other take 29 to 81 ms; ten rounds of resolving one change and then editing take 34 to 99 ms.** Most of it is the refresh of the story after each write (2 to 5 ms), which `state` owns | `bench/revisions.rs` |

For the 50-replace batch of the budget, with 6 pairs each, the estimate was a compiler part **under 40 ms** over
state's 265 to 405 ms. **Measured** (`cargo xtask bench --filter revisions`, release build, 50 tracked replacements of
one word, one in each of fifty paragraphs spread through the body, through one `Compiler`, without and with the commit):

| Input | bare writes (`state_writes`) | `revisions_compile_50` | `state_edit` | `revisions_edit_50` |
|---|---|---|---|---|
| contract-300p | 31 ms | 48 ms | 44 ms | 61 ms |
| dense-contract-300p | 49 ms | 116 ms | 81 ms | 143 ms |
| worst-blocks-12mb | 102 ms | 139 ms | 144 ms | 181 ms |
| node-dense-18mb | 111 ms | 170 ms | 182 ms | 229 ms |
| fields-13mb | 100 ms | 144 ms | 161 ms | 192 ms |

So what the compiler adds to the writes and the refreshes of the story that `state` pays for any edit is 17 to 67 ms for
fifty replacements (0.3 to 1.3 ms each: the map, the diff, the carve, the wrapper and the checks of W1 to W11), and the
whole batch, with the commit, is under a quarter of the budget on the largest body. A paragraph costs most in
`dense-contract`, whose paragraphs hold many runs of one space, where the map and the carve work over many runs.
**`check` is the hot spot, not `revisions`**, as estimated. Memory: the peak of a batch is within 5% of `state_edit`'s on
four of the inputs and 26% over on the contract (23.0 against 18.2 MiB, with the old and the new state both held by the
benchmark), where the budget (4 MiB plus the file plus eight times the parsed XML) is 30.8 MiB. The closest any of them
comes to its budget is accepting every revision of the contract, 27.5 of 30.8 MiB.

**A cost that is not this module's.** A transaction passes every block it has touched to each later refresh of the
story (`Tx::touched_nodes`), and `Story::refresh` shares the field index of the previous story only if no touched
paragraph holds field markup. So after one write to a paragraph with a field, every later refresh in that transaction
rebuilds the field index, whatever paragraph it wrote (a refresh costs 8.5 ms then on the 23,700 paragraphs of the fields
input, where one that shares the index costs 2 to 3 ms). Measured on that input: `revisions_compile_50` took 445 to 543
ms when the paragraphs it edited included ones with fields (the first of them was one), and 144 ms when none did, and
`revisions_resolve_then_edit_10` 170 ms against 70 ms. The benchmarks above edit paragraphs with no field for that
reason (`holds_field` in `bench/revisions.rs`), so that they measure the compiler and not that. Passing the refresh only the
blocks touched since the previous story was built would remove it. The fix belongs to `state`.

The two pathological cases are a paragraph of 100,000 runs (linear, with the explicit stacks of `xml`) and a diff of
two huge texts (bounded by the cell limits of 2.2).

## 10. Module layout and the contract

`revisions` is deep: a small interface (the `Compiler`, `resolve`, `changes`, and the types they use) over the
algorithms of sections 3 to 6. Nothing in it is a seam: there is one implementation of everything, and the
tests cross the same interface the callers do. No file passes 600 lines, and each owns one concept. This is the layout
as built (the design had an `untracked.rs`; each primitive carries its untracked form instead, so there is none):

| File | Owns | Lines |
|---|---|---|
| `mod.rs` | the contract and the re-exports | 160 |
| `types.rs` | the plain data: `Place`, `Action`, `Selector`, `Step`, `PropEdit`, `PropElement`, `NewParagraph`, `MarkerSpec`, `ChangeKind` | 200 |
| `stamp.rs` | `RevStamp`, `Track`, the author check, the date validator, writing the three attributes | 200 |
| `outcome.rs` | `Effect`, `Origin`, `Outcome`, `Created`, `RemovedOwn`, `Folded` | 130 |
| `errors.rs`, `names.rs`, `policy.rs` | the error constructors; the `w` namespace of either dialect and the small reads every file shares; own pending and created by this transaction | 250 |
| `compiler.rs` | `Compiler`, `new`, `finish`, the story and tree reads that every primitive uses | 210 |
| `verify.rs` | deriving `created` from the minted ids; the post-conditions, `check::scan_unit` on each unit that holds a created revision | 200 |
| `tree_ops.rs` | the small edits of a span tree that everything is built from | 200 |
| `carve.rs` | splitting a run at a character, carving a range into whole runs (the original node stays the left part), the copy rule | 430 |
| `gap.rs` | the gap rule (4.2): where new text goes, splitting a wrapper or a container | 520 |
| `insert_run.rs`, `wrap.rs` | building the run for new text and placing it; deleting what a carve covered, by whose text it is | 560 |
| `text.rs`, `text_apply.rs`, `diff.rs` | `replace_text`: locate on one map, classify, delete, insert; the word-level difference and its cleaning (pure, shared with `compare`) | 940 |
| `props.rs`, `props_edit.rs`, `lists.rs` | `set_props`: containers, snapshots, edit, the change element, fold and refuse; `new_list_instance` | 840 |
| `markers.rs` | `place_markers`, `place_markers_beside`, `remove_markers`, `remove_markers_in_part` | 400 |
| `blocks.rs`, `blocks_insert.rs`, `blocks_delete.rs`, `join.rs` | what the block primitives share; `insert_block` and `split_block` (shapes X and Y); `delete_block`, `delete_block_with_mark` and `merge_blocks`; the join of two paragraphs | 1,440 |
| `tables.rs` | `insert_row` (the copy and the vertical merge), the row side of `delete_block`, nested tables | 460 |
| `index.rs` | `ChangeIndex`: id to nodes, built at the first `resolve` of a batch and extended after each compiler call | 120 |
| `changes.rs`, `changes_text.rs`, `changes_rows.rs`, `changes_marks.rs`, `moves.rs` | `changes()` and `Change`: the grouping of 6.1, over a window of entries; the atoms and text groups; rows; paragraphs and moves | 1,350 |
| `resolve.rs`, `resolve_select.rs` | `resolve`: steps, selection (by id through the index, by window), closure, the driver, the report | 420 |
| `resolve_run.rs`, `resolve_props.rs`, `resolve_mark.rs`, `resolve_table.rs`, `resolve_control.rs` | accepting and rejecting run-level revisions, math, fields, moves; snapshots and numbering; the join at a paragraph mark; rows and cells; custom XML ranges | 920 |
| `cleanup.rs`, `notes.rs` | pruning and hoisting range markers; removing a note that no reference names | 380 |
| `laws*.rs`, `tests_*.rs` | the laws and their generator, normalizer, reference checks and selection checks; the direct tests, the scenario rows, the corpus tests | not counted |

Dependencies are as the Allowed dependencies table says: `api`, `xml`, `schema`, `wordml`, `state`. `revisions` reads
numbering definitions through `wordml::NumberingDefs`, so it does not need `styles` or `numbering`. It reaches refs
only through `Tx` (`resolve`, `touch`, `merge`, `retire`). `ops` and `compare` depend on it, as the table says.

The contract for `revisions/mod.rs`:

```text
//! Every revision shape Word writes, plus accept and reject.
//!
//! **Owns.** The compiler that turns edits into tracked changes, or applies them directly: the revision
//! shapes it writes (insertions, deletions, paragraph marks, the `*PrChange` family, rows), stamping, the
//! word-level difference of a text edit, list instances, and the placing of range markers. Also the grouping
//! of revision elements into changes, and the engine that accepts and rejects them.
//!
//! **Interface.** `Compiler::{new, replace_text, set_props, insert_block, delete_block,
//! delete_block_with_mark, split_block, merge_blocks, insert_row, new_list_instance, place_markers,
//! place_markers_beside, remove_markers, remove_markers_in_part, finish}` over a `Tx`, with
//! `Track`, `RevStamp`, `Effect` and `Outcome`; `resolve(tx, &mut ChangeIndex, &[Step])`;
//! `changes(story, tree, blocks)`; and `diff::edits`.
//!
//! **Invariants.**
//! - Operations, `Edit::files`, comment anchors, the editor and compare all end up as `Compiler` calls on a
//!   `Tx`. There is never a second route.
//! - A revision that existed before the transaction is never edited, extended or re-attributed. It can be
//!   split, nested into, have its own author's text removed from it, or be resolved.
//! - New revision ids come from `Tx::next_annotation_id`. A date is `YYYY-MM-DDTHH:MM:SSZ`, the wall-clock
//!   time of the date the caller gave, validated and never converted. No `w:rsid*` attribute is added,
//!   removed or rewritten.
//! - Every copy of properties, a run or a row loses its change-tracking elements and its section break. The
//!   only exception is the halves of a carve, which keep a pending change under a new id.
//! - Every child written into a property container goes through `schema::insert_in_order`, and every
//!   removal is followed by `schema::reorder`.
//! - A replace is the word-level difference, cleaned, deletion before insertion, applied right to left over
//!   one text map. A tracked edit never deletes a paragraph's runs to write the same text again.
//! - Accept and reject are selective and structural. A change is the unit, a move and a field are resolved
//!   whole, a range marker is hoisted and never deleted, and a join keeps the first paragraph's element and
//!   ref with the later paragraph's properties.
//! - What a call minted, split, removed, folded or swallowed is in its `Effect`. What a transaction created
//!   is derived from the ids it minted, and only an element it created is ever extended or edited in place.
//!
//! **Depends on.** `api`, `xml`, `schema`, `wordml`, `state`.
//!
//! **Start reading.** `revisions/compiler.rs`, then `revisions/text.rs`.
//!
//! **Design.** `docs/design/revisions.md`.
```

## 11. What revisions needs from other modules

There are 17 asks. They are numbered so other sections can name them.

**From `wordml`. All done.**

- **W1. Read what was not read, and stop reading what is not a revision. Done.**
  (a) Nothing inside the old properties of a property change is a revision any more (`rPrChange`, `pPrChange`,
  `sectPrChange`, `tblPrChange`, `tblPrExChange`, `tblGridChange`, `trPrChange`, `tcPrChange` and `numberingChange`), which
  fixes the copy of the mark's `ins` and `del` that Word 14 writes into a snapshot (114 in 3 documents). The change element
  itself is listed. (b) `RevKind::TableExProps` for `w:tblPrExChange`. (c) `RevKind::NumPrIns` for `w:numPr/w:ins`.
  (d) The eight custom XML range markers are revisions (`CustomXmlInsStart`, `CustomXmlInsEnd`, and the same for `Del`,
  `MoveFrom` and `MoveTo`), each with an id, and the author and date where the element has them. Every `match` on
  `RevKind` in `revisions` needs arms for the new variants.
- **W2. Annotation ids and the other parts. Done.** `id_parts(IdKind::AnnotationId, ..)` returns the stories, then the
  styles part, then the numbering part, each once. `part_revisions(tree: &SpanTree) -> Vec<Revision>` reads any part, and
  `Story::revisions` is now `part_revisions`.
- **W3. Comparing paragraph properties. Done.** `Paragraph::base_props(&self) -> Props` (the `pPr` children without `rPr`,
  `sectPr` and `pPrChange`, and an empty `Props` with no `pPr`), `Props: PartialEq`, and `Props::without`.
- **W4. Numbering. Done.** `NumberingDefs::max_num_id(&self) -> i64`.
- **W5. Answered.** `Joined` is not needed by `revisions` (open question 5 of `docs/design/wordml.md`): every primitive works
  in one paragraph, and a range never crosses a paragraph. `Joined` stays for `projection`'s search across a deleted mark.

**From `state`.**

- **S1. One annotation counter. Done.** `Tx::next_annotation_id(&mut self) -> Result<u32, StetError>`: the first call of a
  `Tx` returns the base's highest `w:id` plus one (zero when none), each later call one more, `limit_exceeded` past
  2,147,483,647, and a part brought in with `Tx::add_part` that already holds ids counts. Revisions and comments both call it, and `mint_id(AnnotationId)` stays `internal`.
- **S2. Confirmed.** `tx.base().parts().ids(IdKind::AnnotationId)?.max()` is the base's cached set, built once per distinct
  set of part generations, and never read from a staged tree. `IdSet::max` is `pub(crate)` and gives `Option<u32>`.
- **S3. Confirmed.** `Tx::touch` dedups per key and never errors, and the audit's own stamps set `changed = max(changed, at)`, so a
  block that is touched and also stamped is stamped once. A key with no live ref is ignored. `merge(survivor, absorbed, into)`
  accepts either element as `into` and requires the other to be gone from the tree by commit.
- **S4. Done.** `Tx::minted_annotation_ids(&self) -> Range<u32>`: the ids this `Tx` took from the counter, so that
  `Compiler::finish` can find what a transaction wrote without the compiler keeping a list (2.2, "Created is derived").
- **S5.** Touching a **table** when a row is inserted or removed: `Tx::touch` of a table key is for the table's own parts,
  and a table's rows are not among them (`Story::own_parts`). The scope guard reads `touched()` before the audit, so `revisions`
  needs the contract to say that a changed list of rows is a change of the table, or that the guard accepts it.
- **S6.** `refs-and-state` showed two consecutive deleted marks as `merge(p5, p6, into: e6)`, the later element.
  This design joins into the first element (6.2). `Tx::merge` accepts either, so only the example and the line that defines a
  block "whose whole content is deleted" (to match 3.2.5, a deleted block) need to change. `refs-and-state.md` now gives the join into the first element.

**From `schema` and `check`.**

- **SC1. `schema::ctx_at(part, tree, node) -> Option<Ctx>`. Done** (`schema/ctx.rs`). It works with
  `SpanTree` and passes through `mc:AlternateContent`, so every `insert_in_order` the compiler makes has its `Ctx`.
- **SC2. Keep** the declarations under the `*PrChange` elements as the spike found them.
  `pPrChange/pPr` has no `rPr`, `sectPr` or `pPrChange`; `sectPrChange/sectPr` is `CT_SectPrBase` with no header or footer
  reference; `rPrChange/rPr` is `CT_RPrOriginal`, and the mark's is `CT_ParaRPrOriginal`; `tcPrChange/tcPr` is `CT_TcPrInner`;
  `trPrChange/trPr` is `CT_TrPrBase`; `tblGridChange/tblGrid` is `CT_TblGridBase`. Snapshots are chosen with
  `Ctx::allows_child` on them.
- **SC3. `check` implements the Word rules W1 to W10** of section 7.3. Done. They are not schema
  violations. W3, W5 and W10 changed in review: ids are unique per kind, W5 is read within one paragraph, and W10
  exempts a snapshot.

**From `api`, `document` and `ops`.**

- **DOC1. `Stamp`** with public `author: String` and `date: String` (ISO 8601). `RevStamp::from_stamp` validates. The date is the
  wall-clock time of the caller (5.3): the CLI defaults to the machine's local time, and a host application passes the user's.
- **DOC2. The receipt** carries, for each revision an operation wrote: the `ChangeKind`, every id of the change, author, the date
  as written, and the ref of the block, and for `accept` and `reject` a `Resolution`. It also reports `removed_own`, `folded`
  and `swallowed`, which tell an agent what an operation took away.
- **DOC3. Sections.** The body's `w:sectPr` belongs to no block, so a `set` on it touches nothing the scope guard can name.
  `ops` needs a rule (a ref for the last section, or an exemption).
- **DOC4. `ops` mapping.** Pass each run of **consecutive** `accept` and `reject` operations to one `resolve(tx, index, steps)`
  and never move an operation across an edit (6.1), call `ChangeIndex::note` after each compiler call, and map a delete
  range that spans paragraphs as in 2.3 (text first, then one merge, or `delete_block` when it starts at offset 0).

## 12. Decisions and open items

These ten decisions settled the questions of the first version of this design. Each is applied where the right-hand column says.

| # | Decision | Applied in |
|---|---|---|
| 1 | A merge of paragraphs whose properties differ writes the alignment `pPrChange` | 3.2.3, 3.2.4 |
| 2 | Insert-after uses Word's Enter shape (Y), insert-before the self-contained shape (X) | 3.2.1. Position decides: Y when a paragraph follows and the mark is clean, X otherwise (`RP039`) |
| 3 | Deleting your own pending insertion removes it outright, and the receipt says what was removed | 4.1, `Effect::removed_own` and `swallowed` |
| 4 | Formatting under another author's pending `*PrChange` is `readonly_region`. **Extended in review: untracked mode does not rewrite or hide another author's pending change either**, for text and properties alike: `readonly_region`, with a message saying to accept or reject that change first. Untracked still means the author's own changes go untracked | 4.3, 4.5, 5.5, `foreign.rs` |
| 5 | `w:date` is the wall-clock time of the date the caller gives, with seconds and `Z`: Word's local time labelled `Z`. A UTC caller gets UTC. The CLI defaults to local time and the receipt records the date written. No `dateUtc` in 1.0 | 5.3, `RevStamp`. `FIDELITY.md` and checklist C4 |
| 6 | `w14:textId` is `77777777`, and the determinism sentence of `ARCHITECTURE.md` changes to match | 5.4, `ARCHITECTURE.md` |
| 7 | Accept and reject act on the whole change | 6.1 |
| 8 | A deleted paragraph with a section break keeps its mark and the break. A deliberate deviation from Word | 3.2.2. `FIDELITY.md` and checklist C16 |
| 9 | `revisions` is added to the modules with direct unit tests (rule 5 of `ARCHITECTURE.md`) | 7.4, `ARCHITECTURE.md` |
| 10 | The diff lives in `revisions/diff.rs` | 2.2, 2.5, 10 |

**Open items.**

1. **Compare's primitives are designed in `compare.md`.** `compare` reuses `diff.rs` (with `Cleanup::Minimal` where its quality check measures noise),
   `set_props` and the carve. It needs three primitives that this design does not specify: **mark existing content as
   inserted** (a tracked insertion over runs that are already in the revised package), **insert content that comes from
   another package as a tracked deletion** at a place in the revised document, with its own formatting (`ARCHITECTURE.md`, the
   import steps), and **set properties with an explicit snapshot**, where `set_props` reads the snapshot from the container
   and compare knows the original's properties. They follow the shape and copy rules of this design, and they are built
   (`redline_*.rs`, `snapshot.rs`).
2. **Straight quotes.** Agents type straight quotes into documents that use curly ones, so a replacement can leave a mixed
   paragraph. The compiler writes the text it is given, and `ops` curls straight quotes to match the paragraph (`ops/text.rs`).
3. **The dates convention** is in `FIDELITY.md` and on the Word checklist (C4). A host application whose sandbox clock is
   UTC must pass the user's local time.
4. **Alignment of `refs-and-state`** with 3.2.5 (a deleted block) and 6.2 (the join keeps the first element) (ask S6).

## 13. Build order

The module was built in this order, and every step is in the code. The order let every
step be tested alone, and it ends with the laws. Steps 1 to 8 use `Tx::next_annotation_id` and `Tx::minted_annotation_ids` (S1,
S4), `ctx_at` (SC1) and the `wordml` reads of W1 to W4.

| Step | Files | Delivers | Tests |
|---|---|---|---|
| 1 | `mod.rs` (the new contract), `stamp.rs`, `outcome.rs`, `policy.rs` | `RevStamp` (the date validator), `Track`, `Effect`, owner analysis. The measure of the id pass | dates (leap years, offsets, fractions, refusals) and authors; the owner of an item for every wrapper chain, written by hand; the pass over the largest body |
| 2 | `carve.rs` | split a run at a character (the original node stays the left part), carve a range, wrap in `w:del`, the renames, the copy rule | metamorphic: split at every character of runs with a tab, a field, an atom, three wrappers deep. `Final` and `Original` texts never change, and no child is lost or copied. A pending `rPrChange` in a cloned run gets a new id and is reported |
| 3 | `diff.rs` | tokenizer, minimal edits, the cleaning rules of 2.5, `Cleanup::{Minimal, Readable}` | applying the edits gives the new text; against a brute-force LCS on small inputs; the cap; a table of hand-written before and after pairs for the cleaning; the 139 Word-written `del`/`ins` pairs as a check that Word's own replacements are not coarser than ours; noise over the Harvey LAB pairs. **This step checks the constants of 2.5 and the figure compare's noise baseline starts from** |
| 4 | `insert_run.rs`, `text.rs`, `markers.rs`, `cleanup.rs` (no `untracked.rs`, see section 10) | `replace_text` over one map, tracked and untracked, and the marker primitives | the string model against `Final` for random edits; the one-map test of 2.2; R1 to R4 on fixtures with nested wrappers and two authors; scenario rows of 8.2 for text and the second author |
| 5 | `props.rs`, `lists.rs` | `set_props`, list instances | a snapshot for each container, fold and refuse, the schema order after every edit; the own-mark rule; the scenario rows for properties |
| 6 | `blocks_insert.rs`, `blocks_delete.rs`, `tables.rs` | insert, delete, split, merge, rows | the scenario rows for paragraph marks and rows (`RP039`, `RP040`, `FA/005`); R7 with state's identity oracle; the edges of 3.2.5; a cloned vertical merge |
| 7 | `index.rs`, `changes.rs` | `ChangeIndex`, `changes()` | the grouping on the corpus: counts per kind equal the counts of 1.2; a split and nested insertion is one change; `note` after a split and after a removal of own text keeps the index equal to a fresh build |
| 8 | `resolve*.rs` | `resolve` | a known answer for every row of 3B (hand-written XML); a batch of `reject` all and then `replace` keeps the replacement; R8 on the 408 documents; the differential test of 8.2b with the divergence table; pandoc where available |
| 9 | testkit: generator, string model, `normalize`, laws R1 to R9, sequences of authors | the edit laws: 50 batches per curated document on every pull request | the laws themselves, and tests that each law fails when it should (a compiler with one bug seeded per law) |
| 10 | testkit: the catalog of 1.3 and the scenario table | executable evidence of Word's shapes | corpus-gated |
| 11 | the testkit's benchmark harness | the 50-operation batch with compile on the largest body, resolve of everything | the budget of section 9 |

`NOTICE` has the OpenXmlPowerTools entry (MIT), for the resolver of step 8. Docxodus, docx-cli and the ooxmlsdk notes were read for
findings only, and no code is taken from them. No decision record is needed: nothing here changes the public interface.

## 14. Risks

| Risk | What contains it |
|---|---|
| Word's behaviour is unknown in five places (1.4) | Each is handled by being correct under both answers (the alignment `pPrChange`), by refusing (`readonly_region`), or by keeping the safe state (the kept mark of a section break). The checklist of 8.4 settles them, and each answer is one rule or one function in this design |
| The census is biased: Word 16 is 175 of 257 documents, no Word for Mac or Online can be told apart, several "Word" fixtures were produced by tools, and twelve of the `RP` documents have several authors, Word 15, no Word producer or revisions only in `styles.xml` | strict subset (84), a confidence mark on every row of 1.3, and the other `RP` documents and the 24 `FA` ones as the main evidence |
| The text model has edges that the compiler meets first (an offset in a field result, a `w:sym`, CDATA) | those ranges are refused or handled by replacing the text node. The metamorphic tests of `carve.rs` run the compiler over the shapes of the corpus |
| The one-map claim (2.2, step 3) is wrong for some shape of run | the test that compares one map with a map rebuilt before each pair, over every generated edit. The fallback is the rebuild, which costs a story refresh a pair and would break the budget, so the test is the guard |
| Choppy or over-merged word diffs | the cleaning rules of 2.5, with constants checked by hand-written pairs, Word's 139 pairs and compare's noise baseline |
| An agent holds the id of half of a split revision | changes (6.1) group across the split, and the receipt lists every id |
| The wall-clock date is read as UTC by a consumer | the receipt records the date as written, `FIDELITY.md` states the convention, and a host application passes the user's local time |
| Comments and compare have needs this design only anticipates | the hoisting rule, `place_markers` and `Effect` are the contract with `comments`, and the three primitives are named for `compare` |
| The check dominates the 1 s budget | `check`'s per-unit validation is much cheaper than whole-part validation (state's estimate). The compiler leaves 500 ms |
| Memory on the largest body is at 95% of its budget before this module | the compiler holds a few hundred bytes per operation. The chunked arena in `xml` is the agreed remedy |
| A hostile paragraph (100,000 runs, 10,000 nested controls) | every walk is iterative, a carve scans one paragraph, the diff is capped. Hostile fixtures are among the carve tests |
| Licensing | the port of `RevisionProcessor` is MIT and attributed. Nothing of a GPL or AGPL project is read for code |

## 15. Changes to other documents

- `ARCHITECTURE.md`: the `revisions` row of the module contracts table lists the settled interface (`resolve(tx, &[Step])`),
  the determinism list says that a new paragraph's `w14:textId` is `77777777` and only `w14:paraId` and the durable ids
  are hashed (decision 6), and rule 5 lists `revisions` among the modules with direct unit tests (decision 9).
- `GLOSSARY.md`: **Change**, **Pending revision**, **Snapshot**, and the revision kinds that were missing (table grid, numbering,
  cell insertion, deletion and merge).
- `revisions/mod.rs`: the contract of section 10. `NOTICE`: the OpenXmlPowerTools entry.
- `FIDELITY.md`: `w:date` is the caller's wall-clock time labelled `Z`, with no `dateUtc`; a deleted
  paragraph that carries a section break keeps its mark and the break, where Word drops the break; a tracked replacement is
  a cleaned word-level diff and not the character-minimal one; a move is written as a deletion and an insertion.
- `refs-and-state`: the two alignments of open item 4.

## 16. What the build changed

The sections above are the design that was reviewed. The code follows them, and these are the places where it does
not, or where it settled something the design left open. Each was found by a test that the design asked for.

**Compiler.**

1. **The gap rule** (4.2) passes a run that holds only a `w:commentReference`: a comment anchor is not text, so new text
   may go on either side of it.
2. **`cover()`** (2.2, step 3a): a range that covers all the visible characters of a field's result covers the field
   whole, and `begin` and `end`, which have no width, widen the items that a range names.
3. **Insertion next to an insertion that the transaction made.** Before it, the new text is put into it, at its start. At
   its end, when an end marker follows (a bookmark or comment end), the new text goes after the marker and coalescing
   moves the marker into the insertion.
4. **`NewParagraph.style: Option<String>`** (the style of a new paragraph, `projection-and-index.md`): the new paragraph's base properties are
   exactly `{pStyle}`. The Y shape writes a `pPrChange` with the original snapshot when the style differs from the
   paragraph it splits. The X shape writes none when it has no twin, and aligns to the twin when it has one.
5. **`new_list_instance(num_id, ilvl, restart_at)`** takes the level, which a restart override names.
6. **Untracked edits** that stay inside one run are made in place and write no wrapper (`untracked_in_place`).
7. **Carve** leaves a run in fragments when it removes the author's own text. `normalize` merges them, and so does
   the next edit that touches them.
8. **Merging.** A tracked `merge_blocks` declares nothing, so `ops` resolves refs per call: untracked, the second merge
   call of three paragraphs is `(P1, P3)` because `P2` is gone. Merging goes pair by pair from the first. A real merge
   (an own pending mark, or untracked) is the join of 6.2 and declares `merge`.
9. **Untracked `insert_block` after** a paragraph takes the anchor as its model and has no twin.
10. **The properties of a paragraph whose mark is the author's earlier insertion** are edited in place (3.2, 4.4) only
    when a later paragraph follows whose mark is not an insertion by the author, which is what makes the properties go
    when the author's insertions are rejected. Where nothing like that follows, the paragraph can stay after a
    rejection with the properties it has then, so the edit is a `pPrChange` like any other. Law R2 found this.
11. **`Effect::swallowed`** lists every revision of another author that goes with the author's own text when that text is
    removed: a deletion nested in it and a formatting change on its run.
12. **The mark of a deleted paragraph** is not removed when nothing would end the container: an own inserted paragraph
    goes only if a paragraph before it is left, so a cell and a body still end with a paragraph (W9).
13. **A row can hold an insertion and a deletion** (two authors). `delete_row` and `in_deleted_row_or_cell` read every
    child of `w:trPr`, and `Row::revisions()` and `Row::revs()` give every row revision (see the end of this section).
14. **Deleting a paragraph whose mark is the author's own insertion** is the join of 6.2, as a merge over that mark is,
    and not a removal of the element: the paragraph keeps its element and its ref, takes the later paragraph's
    properties and content, and the later ref is merged into it. A paragraph with nothing but range markers is the one
    exception, and goes (its ref is retired as deleted). `merge_blocks` always keeps the first element, even when it
    holds nothing, because the caller named it as the one that remains (`join::Why::Keep`). Deleting the rows of a
    table goes last to first, so that no paragraph is joined into one that is already deleted.
15. **Copies of a revision that existed** (the tail of a split `w:ins` or `w:del`, the copy of a pending `w:rPrChange`
    that a carve makes) are noted on the transaction (`Tx::note_copied_annotation_id`), and `is_created` reads that
    list, so a second `Compiler` over the same transaction does not take a copy for a revision of its own. They were
    on the `Compiler`. `Effect::touched` lists the blocks a call changed, whether or not an earlier call had, and
    `ChangeIndex::note` indexes those and the new blocks: the transaction's own list holds a block once, and a
    revision written into a block that was already touched was missed. A transaction with several compilers has
    `Outcome::created` in each of them as what that compiler can see (the touched blocks and its own new ones), so
    the receipt of a batch takes the union.
16. **Text that a tool wrote without `xml:space`** (`<w:t> </w:t>`, which PDF converters write for the space between
    words) gets `xml:space="preserve"` when a deletion wraps it, because W2 does not allow a `w:delText` with white
    space at an end without it. A deleted run that holds a text box is not checked for W1: the text of the box is
    the text of another story.
17. **A new row** gets `w14:textId="77777777"` and a `w14:paraId` of its own when the row it is modelled on has ids
    (FA/007, as Word writes it). A paragraph at the end of a content control or a revision wrapper is not merged with
    the one after it, and deleting it leaves its mark: the resolver joins siblings only, and `wordml` reads a deleted
    mark there as joining the paragraph after the control. A caller's properties and markers in the Word namespace
    are written in the namespace the part uses, so a Strict document does not get Transitional elements.

**Resolver and changes.**

18. **`Selector::Ids`** takes the whole change that holds the id. `Elements` takes exactly the elements, with no closure,
    and exists in test builds only. The closure adds the other side of a move for `Ids` and `Author`, and a resolved
    move loses its range markers.
19. **Moves** are paired by position: a wrapper belongs to the nearest range start of its side before it whose end is
    after it (`moves.rs`). That is how a move over several paragraphs is one change, and how the two sides meet.
20. **Custom XML range markers** (3B) are resolved: an inserted control keeps its `w:sdt` on accept and loses it on
    reject, a deleted control the other way round, and the content stays. A start and the end with the same id are one
    change.
21. **A cell that a resolution leaves with no block, or ending with a table**, gets an empty paragraph, when the
    resolution removed what the cell held (6.2). A join of two
    paragraphs that a split cut inside a hyperlink or a smart tag makes the two halves one element again.
22. **`changes()`** chains an Enter, the text typed, and the next Enter into one `ParagraphInserted` change, takes the
    copy of a text-box revision in the untaken branch into the change of the taken one, and groups the `rPrChange` of
    consecutive runs and the `pPrChange` of consecutive paragraphs by their old properties and not by their ids.
23. **`changes(range)` costs what the blocks of the range cost.** It reads a window of entries around the range, widened
    to whole tables and grown by four while a change reaches its edge, and reads the whole story when the window holds
    a move or a custom XML range, which are grouped by a name or an id that can be anywhere, or a wrapper around blocks
    or a marker between them, which belong to a block by position (the first block at or after it). `Change::touches`
    lists the entries that hold an element of the change, and a range is touched by the changes that have one of them
    in it: a move from the third entry to the four hundredth touches two, while its `blocks` is 3..401. A step by
    `Ids` finds the elements through the index (one walk of the document, at the first step of a batch) and groups only
    the blocks that hold them: twenty steps on a 10 MB body went from 1.08 s to 63 ms, and ten rounds of resolving and
    editing from 580 ms to 42 ms.
24. **The block of a node that a resolution prunes is touched.** The node marked as changed is the one that is left in the
    tree where the pruning stopped (`prune_from` returns it), not the empty wrapper that went. A block whose only
    change was the loss of such a wrapper was not touched.
25. **The end of a container has no defined original.** A last paragraph whose mark is an inserted one cannot lose it.
    The resolver keeps one that has content and removes an empty one when a paragraph is before it, and so a merge
    over an own mark into such a paragraph can leave the later paragraph's properties after reject-all, where the
    document before the batch would have left the earlier one's. Law R1 leaves the properties of the last paragraph of
    the body out for a document that ends in such a paragraph.

26. **Origins** (`check.md` 2.3 and section 10, asks RV1 and RV4). Every block a call makes declares an `Origin` in its `Effect`, so that
    `check` can tell a problem a copy inherited from one the edit added: an empty list for a block made from nothing, the
    paragraph it was modelled on for a new paragraph, the original for the tail of a split, the paragraph it copies for
    each paragraph of a new row. A tracked merge joins nothing and declares the second paragraph as built from both only when
    it gave it the first one's properties. `Outcome::origins()` follows each `from` back to blocks the document had
    before the compiler began, and `Resolution::origins()` gives the joins of a step. Law R13.

27. **A revision removes what it holds of a field** (3.6.3). The rule that a revision around any piece of a field removes
    the whole field lost the result of three documents of the full corpus (`tdf124472`, `stress023`,
    `Academic_Regulations`); what is left of a field that lost its start or its end goes, and the result stays.
28. **A join does not reach across a table the step removes** (`RP051-Arabic`): the text model reads the paragraph before a
    table and the one after it as two, and the resolver agrees (`Engine::gap_nexts`). The range markers that
    stand between two blocks when a paragraph goes move with its leftovers and stay on the same side of them, so no
    range gets its end before its start (`RP015`, `014-MovedParagraph`, `tdf149711`).
29. **A new row copies the cells that Word puts in a content control** (`coverpage2`): the cells are found through the
    wrappers as the text model finds them, and the copied control gets an id above every id in the part.
30. **A violation that an element carries through a rename keeps its key** (`verify::RENAMES`,
    `key_through_renames`). The compiler and the resolver rename `w:t` to `w:delText` and `w:instrText` to
    `w:delInstrText` and back, and no other element. `check` compares keys through the same list (`check.md` 2.3), so python-docx's `w:space` on a `w:t` is not a new violation when the text is deleted.
31. **A text box is a boundary in every namespace** (`strict-smartart`): `txbxContent` is `wne:txbxContent` in a Strict
    document, and W1 and the container of a block read it by its local name.
32. **A change carries the text it puts in and takes out** (`Change::inserted`, `deleted`, `text_cut`), at most
    `CHANGE_TEXT_LIMIT` (1,000) characters each, with a line break for each paragraph mark and nothing from a field
    code, a text box or a deletion by another author inside an insertion. A receipt shows it, and a gate on what an
    agent wrote reads it.
33. **`Outcome` lists what its calls removed, folded and swallowed as ids** (`removed_own()`, `folded()`, `swallowed()`),
    each call's own `Effect` still has them in order.
34. **The entry point of `comments` for notes is the precise one**: `notes::remove_unreferenced_notes` removes the notes that a
    reference named when the transaction began and none names now, and the resolver does the same step by step.
**Not done.** The constants of 2.5
were checked against Word's 133 same-author replacements in 23 documents (`tests_calibrate.rs`: the cleaning never
makes more edits than Word did, and merges more in a third of the 75 paragraphs), and they were not recalibrated against the
Harvey LAB noise figure. The pandoc test (`tests_pandoc.rs`) runs only where pandoc is installed and prints what it
finds, and it was not installed where this was built. Word is not installed, so the Word check here stays on the scenario table, and the hand check in Word for the web is section 8.4.
`Tx` passes every touched block to each story refresh (section 9).

**Tests.**

35. **The laws live in `revisions/laws*.rs`**, in `stetdoc-core`, and not in the testkit crate: the primitives are
    `pub(crate)`. `laws_checks.rs` holds the laws, `laws.rs` makes the batches, `laws_norm.rs` is `normalize`, the
    generator and the string model are `laws_gen.rs`. R6 compares the part with every touched, new and removed block
    left out. R7 is read from the commit's report (`Committed`), because `refs` is not reachable from `revisions` under
    the dependency table, and it holds each ref to the text it names: a ref retired as deleted may leave no text in a
    block that was there before, and the survivor of a merge, at the end of the chain, holds the text of the block it
    absorbed. R10 is that a window of `changes` is what the whole story gives for it, R11 that `Ids` takes what the
    whole story's changes say, and R12 that the index finds every revision of the document. R2 carves out only the
    author's own splits (a revision of another author that a batch cut is put back together by `normalize`), and R4
    compares what a revision holds as well as its id, kind, author and date. R8 reads with `Story::join_run` and drops
    only the last paragraph of a run that is gone and has no text, unless it holds range markers and nothing else of its
    group would be left.
36. **The batches of the laws are of four kinds**: a tracked batch with one compiler, one with several compilers over
    the transaction, one with accepts by id between the compilers (changes of other authors that were whole in the
    document, of kinds that join nothing: the string model and the original reading do not move), and an untracked one
    (R3, R5, R6, R7, R8, R9, and that it writes no revision). The long run is `cargo test -p stetdoc-core -- --ignored
    soak`. It also runs on an even sample of the corpus (`the_laws_hold_for_batches_on_corpus_documents`: 135 documents
    of 120 KB or less, about 500 batches).
37. **The scenario table of 8.2a** is hand-transcribed from 1.3: `tests_scenarios.rs` compares `normalize` of the result
    with `normalize` of the shape Word writes, for 22 rows. The Word-written files themselves are in the full corpus
    (`cargo xtask corpus fetch --full`), and the curated set holds three of the `RP` ones. `tests_corpus.rs` runs R8,
    the single-change law of `changes()`, R10 and R11, and the schema law over every document of the set that holds
    revisions (31 in the curated set), and `tests_catalog.rs` pins what `changes()` finds in ten of them.
38. **R8 on the corpus, read strictly.** With no paragraph left out for where it sits, the reading of a story after
    resolving everything differs from the mode it should read as in eight readings of six documents, and only in
    paragraphs with no text: those of rows and cells that a tracked change removes (`Final`) or inserts (`Original`),
    and those inside block-level `w:moveFrom` wrappers, which `wordml` lists as empty entries and the resolution
    removes. Three footers of `n830205` stop being stories after accepting, because the deleted paragraphs that
    carried the section breaks referenced them. The text of every story, with nothing left out, is the same, and
    `tests_corpus.rs` holds it to that. The one exemption is the notes that the resolution removed
    (`Resolution::notes`): three documents have a footnote whose reference is an insertion and whose own paragraphs are
    not tracked, so `Original` reads them from before and rejecting removes them with their reference (3.6.4). Over the
    full corpus the same test checks 1,174 documents with revisions and none fails; the four that did at the review
    (`tdf124472`, `stress023`, `Academic_Regulations`, `RP051-Arabic`) are items 27 and 28. The batch laws on its sample of
    101 documents (394 batches) break none, and a census of 219,000 compiler calls (seven shapes of edit, tracked and
    untracked) over 5,214 documents of it finds no refusal but those by design (fields, locked controls, alternate
    content): no `internal`, no `invalid_payload`, no panic.
39. **Seeded bugs.** One-line changes to the compiler and the resolver were each caught by the laws or by a
    hand-written test. Two were asked for by the review: the tail of a split foreign `w:ins` written with another
    author (`set_w_attr(tail, "author", ..)` in `split_wrapper`) is caught by R2, and by three hand tests; and a deletion
    inside another author's insertion that removes the text and declares nothing (`remove_group` for the nested
    class) is caught by R4's content element (`R4 broken: the revision (Some(1), Ins, ..) changed or vanished`). The
    same removal that declares itself as the author's own is caught by R2 instead. `tests_laws.rs` holds R4 to its word on
    pairs of documents that differ the way a bug would make them differ: text dropped from a foreign insertion, an
    insertion with another author, a deletion shortened or gone, and the exemptions (a revision cut in two, what the
    outcome declares).

**Where `wordml` was first-only.** `Paragraph::mark()` and `Row::revision()` read the first revision on a mark or a row.
A mark that holds an insertion and a deletion (19 in the corpus census) was read as inserted only. `Paragraph::mark_revs()`
with `MarkRevs::gone_in(Mode)` and `Story::join_run` are on main, and `laws_util.rs` reads with them. Rows now
read the same way: `Row::revisions()` gives every `RowIns` and `RowDel` in order, and `Row::revs()` gives them as `MarkRevs`,
so a row inserted by one author and deleted by another is gone in both modes.

**Content that is not a block.** The text is read across a `w:altChunk` or another element of `Story::opaque`, as if it
were not there, so a paragraph whose mark is deleted right before one reads as joined with the paragraph after it. Accepting
that mark cannot join them, and law R8 broke on docx-js `word-imported-styles`. So `merge_blocks` refuses two paragraphs
with such content between them, and `delete_paragraph` leaves the mark of a paragraph that such content follows
(`revisions::blocks::opaque_after`, `tests_blocks_delete.rs`).

## References

- [`ARCHITECTURE.md`](../../ARCHITECTURE.md): the `revisions` and `ops` rows, rule 2 ("one path for edits"), the allowed
  dependencies, performance.
- [`GLOSSARY.md`](../../GLOSSARY.md): revision, accept and reject, mode, ref.
- [`docs/design/package-and-xml.md`](package-and-xml.md): the span tree, the editor, `NodeId` stability.
- [`docs/design/wordml.md`](wordml.md) and [`docs/design/refs-and-state.md`](refs-and-state.md): the text model, `Story::refresh`,
  `Tx`, declarations, the audit and the performance figures this document builds on.
- ECMA-376 Part 1, section 17.13 (tracked changes), and the OpenXmlPowerTools `RevisionProcessor` (MIT), `RP` test files.
- Docxodus `docs/ooxml_corner_cases.md` and docx-cli `src/core/track-changes/CLAUDE.md`, read for what Word does.