# Design: `compare`

Design notes for the `compare` module: two documents, an original and a revised one, into a redline that
holds every difference as a Word tracked change. `ARCHITECTURE.md` is the map and the contract in `compare/mod.rs` is
the interface (section 2 gives its signature). This document says how blocks are aligned, how the redline
is built on the revised package, how content of the original crosses into it, what the compare quality check measures (accept-all gives the revised document, reject-all the original, under a noise threshold), what
it costs, and what it needs from the modules around it. Terms follow [`GLOSSARY.md`](../../GLOSSARY.md): revision, accept and
reject, mode, ref, block, story, stamp. It builds on [`revisions.md`](revisions.md) (the compiler, `diff.rs`, `Effect` and
origins, R8), [`styles-and-numbering.md`](styles-and-numbering.md), [`check.md`](check.md),
[`refs-and-state.md`](refs-and-state.md), [`wordml.md`](wordml.md) and [`document-and-ops.md`](document-and-ops.md).
This is a design only. No production code has been written, and the spikes behind the numbers were throwaway programs in
`/tmp` that are described in section 1.1 and have been deleted.

## The decisions that matter most

1. **Compare is four phases, and only the last one writes** (2). Prepare (accept revisions, open the revised package
   under a derived namespace), align (pure: texts and hashes in, a pairing out), plan (pure: a pairing into per-paragraph and
   per-block plans, as data), build (the plan into `revisions::Compiler` calls on one `Tx`). The pure phases are tested without
   an XML tree and snapshot their output.
2. **Alignment is hash anchors, then similarity, per container, with one pool for moves** (4). Equal blocks at the two ends are
   trimmed, blocks whose text occurs exactly once on each side anchor, the longest increasing chain of anchors is the
   spine, and the stretches between anchors are paired by word similarity (a dynamic program for small stretches, an inverted
   index for large ones). What is left over in the whole story is paired a second time for moves, across containers. On
   354 Harvey LAB turn pairs (174,750 paragraphs), as built (floor 0.35 with the locality penalty), 85.8% of paragraphs are paired by
   hash alone, 9.2% by similarity, and 4.1% are left as deleted. A pair of 20,000 paragraphs aligns in 4 to 5 ms.
3. **The redline is the revised document with the original's deleted content put back** (5, 6). Unchanged blocks are not
   touched, so they stay byte-identical (law L2). Inserted text is the revised document's own runs, wrapped in `w:ins`.
   Deleted text is cut out of the original's runs, imported with its own formatting and placed as `w:del`. A changed
   property is written as a `*PrChange` whose snapshot is the original's value. Everything goes through `revisions`, and
   `compare` never writes a revision element itself.
4. **Compare adds four entry points to `revisions`** (6.1), the three primitives that `revisions.md`
   section 12 names, in the shape they take once the cost of a story refresh is counted: `redline_paragraph` (inserted
   ranges, deleted fragments and run formatting snapshots in one call and over one text map), `mark_inserted` (a block, a row
   or a table, over a list), `place_deleted` (a list of consecutive imported blocks), `set_snapshot` (paragraph and mark
   snapshots). It also needs a read of a paragraph that does not wait for a refresh of the story, which is not implemented. Merging in the revised document is not
   a new primitive: it is an untracked split followed by a tracked merge.
5. **Import is the only cross-package code** (7). It copies detached nodes from the original's trees into the redline's trees
   with `Editor::create_element`, `set_attr` and `create_text` (no change to `xml`), imports the parts that `r:*` attributes
   name, and imports style and numbering definitions on demand: reused when equivalent, renamed when they collide. Every
   imported block is declared to `check` with its key set in the original (`Declared::foreign`).
6. **Clause numbers never reach the comparison text** (3), because `Final` text has none. Atoms (images, notes, equations)
   compare by identity, so a replaced picture is a difference, and a note reference compares by kind, so a note that was
   edited is compared inside the note and not replaced.
7. **Existing revisions in either input are accepted in memory first** (8), with a warning that carries the count, so a turn
   that came back with tracked changes compares as the text it shows. The redline is built on the accepted revised package.
8. **What is compared is the list below, and nothing else** (8): text, paragraphs inserted, deleted, split, merged and
   moved, table rows and cell text, run and paragraph formatting, headers, footers and notes. Table, row, cell and section
   properties, style and numbering definitions, comments of the original and fields' codes are not compared, and the
   warnings and `FIDELITY.md` say so. The quality check's normalization leaves out the same list.
9. **The quality check has one oracle that already exists** (9): `accept_all` and `reject_all` of the redline, read through the text
   model and the neutral model, against the two accepted inputs, on 352 Harvey LAB pairs, 469 Word-authored pairs and generated
   pairs. Noise is measured as the chars and spans of the redline against the minimal word diff. The first baseline on the
   spike gives a per-pair ceiling of 1.6 times the minimal diff plus 40 characters, and the real threshold is the
   first baseline of the built module.
10. **The cost is the writes** (10). Alignment and diffs are tens of milliseconds. Each modified paragraph costs 0.9 ms on the
    300-page contract (2.3 ms on the dense one), two thirds of it the refresh of the story after each write. At 10% of the
    blocks changed the estimate is 0.55 to 0.65 s, and at 25% it needs the read that does not wait for a refresh (0.5 to 0.7 s with it).

## 1. Evidence

### 1.1 Method

Everything was measured on pairs fetched into `/tmp/n_cmp` (shallow, pinned commits of `harvey-labs`,
`open-xml-powertools`, `docxodus` and `document-change-benchmark`, 140 MB in all, deleted afterwards) and, for the corpus
loaders, a private `STETDOC_CORPUS_DIR=/tmp/n_corpus` (the curated set, 306 documents, deleted afterwards). The scripts were
Python over `lxml` for the census and a standalone Rust program that held a copy of `revisions/diff.rs` and a
first version of the alignment of section 4, for timing and for the noise figures. Neither is committed. The module's tests
turn the findings into checks anyone can rerun, as `revisions.md` section 13 did.

### 1.2 The pairs

| Class | Source | Pairs | How a pair is found |
|---|---|---:|---|
| Harvey LAB turns | 1,713 documents of the task directories named `*redline*` and `*term-negotiation*` (the "turn" tasks) | 352 | two documents of one task directory that share at least half of the paragraphs of 20 characters or more, counted on the accepted text |
| Word-authored | OpenXmlPowerTools `TestFiles/WC` and `CA` (Word 16), Docxodus `HW`, `DB`, `LIR`, `RA` and `RPR` families | 469 | every unordered pair inside one family (`WC002-DeleteAtBeginning`, `WC002-InsertInMiddle` and so on) |
| Tool-made | `document-change-benchmark` baseline and candidate, 82 directories | 82 | the two files of a directory |
| Generated | untracked random edits applied to any corpus document through `Document` | unlimited | the generator's seed is the pair |

The census of the LAB documents: 408 of 1,713 hold `w:ins` or `w:del` (the markup documents, written by the lab's skill, so
the census of `revisions.md` of "Word revisions" does not contradict it), 1,363 hold a table, the median is 346 paragraphs and the largest
2,728. In the 352 pairs `numbering.xml` and `styles.xml` are byte-identical (the python-docx template), there are no notes,
comments or media, and 50 pairs differ in a header or footer text. The Word-authored pairs are the opposite: `styles.xml` differs in
195 of 469, 77 pairs have notes or media, and numbering is rare (22 pairs). The two classes cover each other: LAB gives size,
tables and typed numbering, Word gives definitions that differ and notes. Neither gives numbering that differs, so the cross
pairs of section 9.3 do.

### 1.3 What the pairs say

Alignment of the 352 LAB pairs, paragraphs as the unit, with the algorithm of section 4 at the constants of the first spike (floor 0.5,
no locality penalty). The figures of the module as built, at the floor of 0.35 with the locality penalty, are in the paragraph
"As built" after the noise figures:

| Of 176,840 original paragraphs | Count | Share |
|---|---:|---:|
| paired by trimming equal ends | 39,455 | 22.3% |
| paired as unique-hash anchors | 110,251 | 62.3% |
| paired by similarity (floor 0.5) | 14,671 | 8.3% |
| left over and not blank (deleted, or moved) | 9,788 | 5.5% |
| of those, an exact copy exists unpaired in the revised side (a move) | 114 | 0.06% of all |

- **The gaps between anchors are small.** There were 10,151 gaps that needed the similarity pass, 148,170 cells in all (14.6
  on average), and none above the cap of 250,000. The pass costs nothing on real documents, and the large-gap path of
  section 4.4 exists for adversarial ones.
- **The floor is not sharp.** The number of similarity pairs falls smoothly as the floor rises (17,014 at 0.2, 15,718 at 0.4,
  13,033 at 0.6, 9,510 at 0.8), so there is no bimodal gap to put it in. What decides it is the noise measure: redline
  chars over minimal diff chars is 1.189 at a floor of 0.25, 1.190 at 0.35, 1.205 at 0.5, 1.269 at 0.65 and 1.481 at 0.8. A
  paragraph rewritten past recognition costs less as one replacement than as a deletion and an insertion, which is why
  a low floor is cheaper, and a floor of 0.35 keeps pairs that a reader can still see are the same clause. Docxodus uses
  0.35 with a locality penalty, and the section keeps both.
- **Typed clause numbers are text.** 39% of LAB paragraphs start with a number or letter such as `4.2`, `(a)` or `Section`,
  because the lab's generator typed them. They stay in the text, as they are on the page, and a renumbered clause is a real
  difference. Only a computed number (numbering definitions) is excluded.
- **Splits and merges are rare in the corpus.** A search for a deleted paragraph whose words are covered (multiset Dice of 0.8
  or more) by two or three consecutive inserted ones, and the reverse, found 0 in the 469 Word pairs and 3 in the 352 LAB pairs,
  all false positives (a table row written as a paragraph). They are supported, so the generated pairs carry them (the
  generator knows which ones it made), and the constants are calibrated there.
- **Tables are common and mostly stable.** Of 1,595 tables in the originals, 1,545 (97%) pair with a table of the revised
  document by the majority of their paragraphs, and 178 of those (11.5%) differ in their number of rows. 120 tables are
  unpaired. 91 paragraph pairs of 176,840 cross between the body and a cell, so cross-container moves exist and are
  rare.
- **Formatting differences among paragraphs with equal text are rare and are not language tags.** Of 65,382 such LAB paragraphs
  924 differ in `w:pPr` (`ind` 907, `spacing` 98) and 240 in `w:rPr` (`rFonts` 416, `szCs` 208, `b` 31, `sz` 2). Of 488 Word
  ones, 40 differ in `w:pPr` (`cnfStyle` 72, `jc` 30, `pStyle` 1) and 4 in `w:rPr` (`lang` 2, `vanish` 2). `cnfStyle` is a
  cache that Word recomputes, so section 5.4 ignores it with `rsid` attributes, `w14` ids, `lang` and `noProof`.
- **Alignment time.** The 352 LAB pairs align in 1.04 s in all (3 ms a pair, 16 ms at most, for 335 to 875 paragraphs).
  Synthetic pairs made from the largest LAB document, with a unique tag on each paragraph and 5%, 10% and 25% of
  the paragraphs edited, deleted, inserted or moved: 4,620 paragraphs align in 17 ms, 20,000 in 66, 86 and 112 ms. Two
  unrelated 5,000-paragraph documents that share a vocabulary, which is the worst case for the similarity pass, take 32 ms, and
  3,000 paragraphs shuffled (all moves) take 7 ms.
- **As built.** The module's own alignment, run on 354 LAB pairs (the pair finder gave two more than the first
  time: the rule is the same and the order of the directories differs) of 174,750 paragraphs: 39,542 paired by trimming (22.6%),
  110,421 as anchors (63.2%), 16,031 by similarity (9.2%) and 7,108 non-blank paragraphs left unpaired (4.1%). Alignment takes 58 ms
  for all 354 pairs (0.9 ms at most). The noise, with the paragraph edits cleaned as `Readable`, against the minimal diff of the whole text, is
  1.195 overall (1.205 at the first spike's constants), per pair median 1.20, p90 1.33, p99 1.45, max 1.84. The synthetic pairs
  of 4,620 paragraphs align in 0.7 to 1.0 ms at 5% to 25% edited, and the pairs of 20,000 in 3.7 to 4.7 ms. The two unrelated
  5,000-paragraph lists take 7.8 ms and 3,000 shuffled paragraphs less. The first spike's times were over ten times higher
  because it built a map for every stretch.
- **Noise of the spike.** Redline characters against the minimal word diff of the whole text: LAB 1.205 overall, per pair
  median 1.21, p90 1.35, p99 1.48, max 1.89; Word pairs 1.027 overall and a median of 1.04, but pairs of a few
  words give 1.6 at p90 and 3.25 at most (13 characters against 4); generated pairs 1.02. Redline spans against minimal
  spans are 0.36 on LAB (the cleaning of 2.5 of the revisions design merges the confetti of the minimal diff), and the Word
  pairs' 3.3 is an artefact of the spike counting each paragraph as its own span, where the module counts changes as `changes()` groups them.

### 1.4 Prior art, read for findings only

Both references are MIT: OpenXmlPowerTools (`WmlComparer`) and Docxodus (`IrBlockAligner`, `IrBlockSimilarity`, a descendant
of the same code, "Copyright (c) Microsoft Corporation"). No code is taken, so neither goes into `NOTICE`. What the reading
confirmed or added, with the place each is used:

- **Docxodus aligns in passes**: a key of content hash and format fingerprint that is unique on both sides, then a key of
  content alone, then a longest increasing subsequence as the spine, and moves "fall out of the anchoring by construction" as
  the anchors that are off the spine. Section 4.2 does the same with a single key (text), and decides formatting afterwards, so
  a formatting difference never moves a block.
- **A blank paragraph never anchors.** Docxodus releases a unique blank that the spine pinned between unrelated content.
  Section 4.1 does not let a blank anchor at all, and pairs blanks inside a gap.
- **Similarity is the Jaccard index of word multisets with a locality penalty** (`BlockSimilarityThreshold` 0.35, plus
  0.3 times the relative displacement of the two positions in the gap), moves need 0.8 (`MoveSimilarityThreshold`, the same
  as `WmlComparerSettings`), and a split or a merge needs 0.90 coverage (`SplitCoverageThreshold`). Section 4 starts from these
  numbers and our own measurements decide where they end up.
- **Tables are whole units at body level and are entered when two tables are similar.** Docxodus scores a table pair by the
  Jaccard index of the cell words, and recurses with the same aligner into the cell's blocks. Section 4.5 does the same.
- **`WmlComparer` stops a long common run from being kept when it is a small part of the text** (`DetailThreshold` 0.15: a
  longest common sequence under 15% of the longer side is not trusted, and the span is replaced whole). This is the same
  idea as the whole-span rule of the cleaning in `diff.rs`, which `compare` shares.
- **Both build from atoms and rebuild the paragraph structure.** `compare` builds on the revised tree instead (decision 3),
  which is the choice that keeps unchanged blocks byte-identical.

## 2. The interface

### 2.1 Public

Nothing in `api/` changes except warning codes (8.2), which `api` owns. `compare(original, revised, &CompareOptions)` is as
`document-and-ops.md` has it: bytes in, `Compared { document, receipt }` out, and `CompareOptions` has the limits, a required
stamp and the format. `Receipt` has `revisions` by author (one author, the stamp), `warnings`, and `moves`, the
`MoveEntry { from, to, text }` list that the `revisions` design reserved. The CLI is `stetdoc compare <original> <revised> -o <docx>` with
`--author`, `--date` and `--json`, and napi has `compare`. The receipt's `operations` are empty and `base` is the redline's.

### 2.2 Inside the core: a signature that keeps the transaction open

`ARCHITECTURE.md` gives `compare(&State, &State, &Stamp) -> (State, Receipt)`. That cannot work as written, for two reasons
from the other designs. `check::edit(&Tx, &Declared)` validates a transaction before it commits and needs the foreign key
sets of imported blocks, which are computed by `check::unit_keys` on the original's view, and the dependency table does not let
`compare` depend on `check`. And the redline's refs are minted at
commit, so the receipt's `moves` can name blocks only by key until then. So the interface that `document` calls is:

```rust
pub(crate) fn build(original: Vec<u8>, revised: Vec<u8>, limits: &Limits, stamp: &RevStamp)
    -> Result<Redline, StetError>;                       // opens both, prepares, aligns, plans
impl Redline {
    pub(crate) fn tx(&mut self) -> &mut Tx<'_>;          // the open transaction on the revised state: document runs check::edit on it, then commits
    pub(crate) fn declared(&self) -> Declared2;          // origins (the compiler's Effects, as ops gives them) and imports: (block in the redline, block in the original)
    pub(crate) fn original(&self) -> &State;             // for check::unit_keys of each import
    pub(crate) fn finish(self, committed: &Committed) -> CompareReceipt;   // moves (BlockKey pairs to refs), counts, warnings
}
```

`Redline` owns both states (the original accepted, the revised accepted and reopened under the derived namespace) and a
`Tx` borrowing the revised one, so it is a self-referential pair that is built in `document` with the state held in an
`Arc` or in a struct that returns the transaction by `&mut`. The exact lifetime shape belongs to `document` and is not a decision of
this design: what matters is that the order is `build`, `check::edit(tx, declared)`, `commit`, `finish`, which is the order
of an `edit` with a longer first step. `ARCHITECTURE.md`'s row and `compare/mod.rs`'s contract change to say so (section 16).

The namespace is `NamespaceId::derived("compare", [original, revised], stamp bytes)` of `refs-and-state.md` 3.2, where the two
inputs are the namespaces of the *packages as given* (`NamespaceId::of_package`), so the same two files and the same stamp
always give the same redline namespace, and a redline never shares one with either input. `State::open_in(namespace,
origin, bytes, limits)` exists for exactly this, and `build` takes bytes (and not two opened states) so that each package
is opened once. When the revised package holds revisions they are accepted by a transaction on the state that was opened
under the derived namespace, which is the cheap order (accept, then commit, with no second open).

**As built.** `document::compare` calls `compare::prepare` and `compare::redline` (re-exported by `compare/mod.rs`), turns `CompareReceipt::declared` into `check::Declared` (the origins, `foreign` for the imports and the imported definitions, `parts` for copied parts), runs `check::edit_located` on the open transaction, commits, and builds the public receipt. `compare_states` stays for the module's own tests.

**What `check` lets a redline keep.** Three kinds of foreign origin reach `check`, and they are not equally strict:

- A block of the redline that holds the original's content, and each paragraph and table inside it, gets the keys of its unit in the
  original, unit for unit, when the copy has as many paragraphs and tables as the original block. When it has fewer or more (the import
  left something out, such as a picture holding a text box), every unit of the copy gets the union of the original block's units, and
  the receipt names the block with a `compare_keys_merged` warning, so that the leniency is seen and counted (the N5 law counts it).
- A style or numbering definition that the import appends gets the keys of its element in the original.
- A part that the redline adds (a style sheet or numbering part made for imported definitions, a notes part, a part copied whole) is read
  whole by `check`, not unit by unit, because it has no base to compare with. `Origins::whole_part` gives it the union of the keys of the
  part it was copied from (`Declared::parts`, from `check::part_keys`: the schema's keys of the whole source part and, for a story, its
  Word rules and census) and the keys of every block and definition declared in it. So a problem that any of those sources had is allowed
  anywhere in the added part, not only where it was, and a problem that none of them had is new and refused
  (`document/tests_compare.rs` writes one into a style sheet that compare added).

**An exclusion names a block of the redline, or none.** `Exclusion::site` is the block itself (the revised paragraph left as it is, or a
copy placed), the block of the redline that a copy which could not be made would have stood next to, or nothing. A key of the original is
never named, since the redline's refs read another tree; an exclusion with no block of the redline is described by the first words of the
original's block.

### 2.3 Phases

```
prepare   open both under `limits`; accept every revision in each (resolve All, one transaction each), counting them;
          derived namespace for the revised side
model     per story: blocks, their comparison text, hashes and word bags (3)
align     per container: trim, anchors, spine, gap pairing (4); per story: moves, splits and merges (4.6)
plan      a Plan per story: what to do to each block of the revised side, and which original blocks to import where (5)
build     imports (7), then the Compiler calls of 6, then `finish`
```

`model`, `align` and `plan` read, and write nothing. `build` is the only phase that calls `Tx::part_mut` and `add_part`.

## 3. The comparison text

A block's comparison text is `Paragraph::text(Mode::Final)` of the accepted document. The text model puts no clause number
in it (`styles-and-numbering.md` 2.2: numbers are in `StoryNumbers` and in no text range), so a clause inserted in the middle of
a numbered list renumbers everything after it and produces no revision, and a heading whose typed number changed
(`4.2` to `4.3` in the text) does. Three further rules, none of which changes what an edit's offsets mean, because they replace
characters one for one:

1. **Atoms compare by identity.** `ItemKind::Object` (a drawing, picture or object), `Math` and `FieldValue` are one
   character each in the text. Compare gives each a character of the private use area (U+E000 and up) from a dictionary of
   the pair, keyed by the atom's identity: for an object, the hash of its extent, its alt text and the bytes of the media it
   names (not its `r:embed`, which differs between packages); for a text box, a hash of its text and its drawing's shape;
   for math, a hash of its canonical XML; for a field value, its cached result. Two different pictures are two different
   characters, so replacing a picture is a deletion and an insertion. `NoteRef` stays one character, because the note is compared
   in its own story (4.7), and `Tab`, `LineBreak`, `PageBreak`, `ColumnBreak`, `Hyphen`, `SoftHyphen` and `Symbol` are
   their own characters already. The substitution is only in the strings the aligner and the diff see.
2. **Fields compare by cached result.** The text model reads a field's result, and a change of a field's code that leaves
   its result as it was is not a difference. `FIDELITY.md` says so, and the quality check's normalization has the same rule. A field is a
   unit for editing: section 5.3 widens an edit to the whole field.
3. **The key for alignment is looser than the text.** A block's alignment key is its text with runs of white space
   collapsed, the ends trimmed, curly quotes straight and the non-breaking space a space, hashed to 64 bits by a small
   in-process hash with a fixed seed (`refs-and-state.md` 4 says `compare` does not use the identity hash, and a seed that
   varied would break determinism). A match of keys is verified
   against the strings, so a collision cannot pair two blocks. The word diff that follows works on the exact text, so a
   difference of quotes or spaces is shown.

A paragraph of only white space (a blank) has key 0 and never anchors.

## 4. Alignment

### 4.1 What is aligned

`model` turns a story into a tree: a container (the story body, a cell) holds blocks, a block is a paragraph, a table or
nothing else, and a table holds rows, and a row holds cells that hold a container. `wordml`'s flat pre-order list of
`BlockEntry` already has this shape (`parent` and `kind`). The body, each header and footer part that both
packages have, and each note are separate stories (4.7), and each container of a story is aligned on its own, top down:
a paragraph pairs only with a paragraph of a container that its parent pairs with, except in the moves pass. A paragraph
has the comparison text and a bag of word tokens (a sorted vector of 64-bit hashes of the letters-and-digits runs and
the other characters, lowercased, so a case change does not make a paragraph unrecognizable, while the diff stays exact). A
table has the bag of all the words in its cells and the key of all its text. A blank paragraph has an empty bag.

### 4.2 Algorithm

For two block lists A (original) and B (revised) of one container:

1. **Trim.** Pair equal keys from the front and from the back, as long as they are equal (including blanks, which pair by
   position here: a blank is equal to a blank).
2. **Anchors.** In what is left, a key that occurs exactly once in A's range and once in B's, and is not blank, is an
   anchor pair. The key is tagged with the block's kind (paragraph, table, row), so a table never anchors a paragraph. The longest increasing subsequence of the pairs by B index (patience sorting, O(n log n)) is the spine, kept
   as paired. Anchor pairs that are not on the spine are not discarded: they are move candidates (4.6), and the pairs of
   a tie are resolved in favour of the spine that keeps tables and rows anchored, since a table is the heavier block to
   move (Docxodus's finding).
3. **Recurse.** Each stretch between two spine pairs is aligned again from step 1, with the occurrence counts of the
   stretch only, until a stretch has no anchor.
4. **Pair by similarity.** A stretch with no anchor, m blocks against n, is paired by the dynamic program of 4.3 if m times n
   is at most 250,000 (the cap of `diff.rs`, `DP_CELLS_MAX`), and by the candidate path of 4.4 above that. Whatever remains
   unpaired is deleted (A) or inserted (B), and goes to the moves pass.

The result is a list of pairs in order, with the unpaired blocks between them. It is deterministic: no map is iterated for
output, ties go to the lower index of A and then of B, and the hash is a function of the text alone.

### 4.3 The dynamic program

For a stretch the score of pairing block i of A with block j of B is the multiset Dice coefficient of their bags, `2 |A ∩ B|
/ (|A| + |B|)`, plus nothing else. Docxodus uses Jaccard, which orders pairs the same way, and the Dice score is what
the spike measured. The program maximizes the sum of the scores over order-preserving pairings in which each pair scores at
least the floor, which is the longest common subsequence with weights. A pair that clears the floor by a little but sits far
from the diagonal of the stretch is held to a stricter floor, by Docxodus's penalty: the floor is 0.35 plus 0.3 times the
difference of the two relative positions in the stretch (so a full displacement asks 0.65). Two blanks score 1.0, so blanks
pair inside a stretch by position. A table scores against a table only, a paragraph against a paragraph only.

Cost: the score of a pair is a merge of two sorted vectors, and a stretch of the LAB pairs has 15 cells on average. The
cap bounds one stretch to 250,000 cells, each a merge of two short sorted vectors.

### 4.4 Large stretches

Over 250,000 cells (a new section of 500 paragraphs on each side with no anchor), the similarity is found through an inverted index
of the words of B's stretch that occur in at most 8 of its blocks. Each A block gets the best-scoring B block among those
that share at least one such word, if the score is at least the floor, and the pairs are reduced to their longest increasing
subsequence. The stretches between those pairs are aligned again from step 1. A stretch with no pair after that is left
unpaired. This path is also taken by the 5,000-paragraph unrelated pair of the spike, and it costs 32 ms.

### 4.5 Tables

A table is a block. In a stretch it keys on its text and scores by the Dice coefficient of all its cell words, with the same
floor. Two paired tables are then aligned row by row by the same algorithm (a row is a block with the key of its cells'
texts joined by U+001F, and the bag of all its words, blanks included so that an empty row can pair), and two paired rows
are aligned cell by cell **by position**, only when the rows have the same number of cells and the same `gridSpan` and
`vMerge` sequence. **A table replaced by another**, the last unpaired block of a gap in the original and the first in the
revised document, would stand directly before it in the redline, since a gap's deletions go before its insertions. Word does not
keep two tables with nothing between them apart: it showed them separated by untracked space that stayed after both Accept All
and Reject All (hand check `17-old`). So the two are always paired, and the redline is **one table** of deleted rows and then
inserted rows, with the revised table's properties, as every paired table has. (An empty paragraph with a hidden mark between two
tables was tried and dropped: Word for the web shows it as an empty line, hand check `r2-17`.) The deleted rows count their
columns in the original's grid, so once the story is written `Compiler::fit_grid` gives the rows one grid. When the original's
grid starts with the same widths as the revised one, the deleted rows fit as they are, ending short of the grid if the revised
table is wider, as Word itself leaves rows when it rejects inserted cells (`RP035`). Otherwise the table takes the grid of every
column edge of both grids, and each cell of each row spans the columns between its own edges, so every reader sees each row at
its own widths, and none drops a cell past the grid's end, as LibreOffice does. A row that ends short has no `w:gridAfter`:
LibreOffice draws one as a border past the row's end. The corpus has no Word-authored table whose deleted and inserted rows differ
in cell count to copy; of 384 Word-authored tables whose rows do, the rows span a grid of every column edge, and a row that ends
short has a `w:gridAfter` and a `w:wAfter` (350 of 366). LibreOffice shows a row that ends short with its last cell stretched to
the table's width. Column operations are not in this release, so a row whose cells changed shape is not one that
a reader can compare cell by cell: such a pair is treated as a deleted row and an inserted row, and a warning counts them
(`compare_table_replaced`). The blocks of two paired cells are aligned as a container, by the algorithm above, so a
table in a cell is handled by the same code. Of the 1,545 paired tables of the LAB pairs, 178 differ in row count, so row
insertion and deletion are the common case and shape changes are the exception that the corpus does not show.

### 4.6 Moves, splits and merges

After every container of a story is aligned, the unpaired, non-blank blocks of the whole story, from any container, form
two pools, deleted (A) and inserted (B). **A block inside an unpaired table or row stays out of the pools**: the table or the row is
the unit (a deleted table is deleted whole, and may itself be a move candidate), because a cell paragraph that left with its
table and re-appeared as a body paragraph would split one table's deletion into pieces.

- **Moves.** A deleted block and an inserted block whose Dice score is at least **0.8** (WmlComparer's and Docxodus's
  value, and exact copies score 1.0) and that have at least 5 words (a short line is a coincidence, not a move) are paired,
  best score first, ties by the nearer position. Each pair is a **move**: the block is deleted where it was and inserted where
  it is, as two whole blocks (no word diff inside the pair: the block is a deletion and an insertion),
  and the pair is listed in `Receipt.moves` with the refs of the two and the text. A table moves by the same rule on its
  words. In the 352 LAB pairs there are 114 exact moves (0.06% of paragraphs). The pass finds candidates through an inverted index over
  the words of the pools, with the **word-frequency cap of 4.4** (a word in more than 8 blocks of a pool is not an index key), so
  it is bounded the same way as the large stretch and cannot go quadratic on boilerplate.
- **Splits.** A paired paragraph pair (a, b1) with unpaired inserted paragraphs b2 to bk following b1 immediately, where the
  words of b1 to bk together (the multiset union) cover at least 0.8 of a's words and each of b2 to bk contributes at least
  10% of them, becomes a **split** of a into b1 to bk. The text of the group is diffed as one string (5.5).
- **Merges** are the mirror: a paired (a1, b) with unpaired a2 to ak following a1 immediately.
- A group takes only blocks that are adjacent, unpaired, in one container and not already in a move, and a block is in at most
  one group. Splits are scanned before merges.

The constants (0.8, 10%, the length of 3 paragraphs, 5 words) are starting values taken from Docxodus. Because the real
corpus has no splits or merges (1.3), they are calibrated on generated pairs where the generator knows the truth: a split or a merge that the generator made must be found, and an unrelated pair of neighbours must not be.

### 4.7 Stories

- **Body.** One story, aligned as above.
- **Headers and footers.** A header or footer is matched by its slot, the section and the type (default, first, even), where
  section k of one package corresponds to the section of the other whose ending paragraph was paired in the body (the last
  section to the last), and an inherited slot (a section with no reference for a type) reads through to the nearest earlier
  section that has one. Each effective part is compared once, however many sections inherit it, and **a part pairs with at most one part of the other
  package, and the pairing is one to one** (two sections that inherit one header on one side and have two on the other pair the
  first effective part and skip the second, with the warning). A slot present in one
  package and absent from the other cannot be shown as a revision in the document (a `w:headerReference` is not tracked by
  Word, and `sectPrChange` omits it), so **it is skipped, with a warning** (`compare_story_unmatched`, in the receipt), and the quality check's
  normalization leaves the same slots out (decision 3 of section 13).
- **Notes.** A note is a story of its own whose blocks are aligned as a container. Two notes are paired through their
  references: a `NoteRef` atom of the original and one of the revised paragraph that are in an unchanged stretch of a
  paired paragraph (the characters on both sides correspond one for one there) pair their notes. **A note follows its
  reference, and no reference is ever left without its note or the reverse.** The note of a reference that is in inserted
  text (including a reference that an edit's range covers) is new: its paragraphs are `mark_inserted`, as is the reference with
  the text around it. The note of a reference in deleted text is imported with the deleted reference (7.5). A reference in an
  unchanged stretch pairs the two notes, which are compared in their own story. Footnotes and endnotes are separate parts and
  pair separately.
- **Comments** are not compared. The revised document keeps its own, the original's are dropped (7.6).
- **Text boxes** have no refs and are atoms (3), so a changed text box is a deleted object and an inserted object.

## 5. From pairing to plan

The plan is data: for each story, the list of what `build` does, in document order of the revised side. It has these
entries, and a snapshot test pins the plan of every fixture.

| Entry | Meaning |
|---|---|
| `Keep(b)` | the pair is equal in text and formatting: nothing is written |
| `Edit(b, ParaPlan)` | the pair differs: the edits, deleted fragments and formatting snapshots (5.3 to 5.5) |
| `Insert(b)` | an unpaired revised block: it is marked inserted (6.2) |
| `Delete(a, at)` | an unpaired original block: it is imported and placed deleted (6.3) at `at`, a position among the revised blocks |
| `Split(a, [b1..bk])`, `Merge([a1..ak], b)` | 5.6 |
| `Move(a, b)` | a `Delete` and an `Insert` that the receipt lists |

### 5.1 Placement of deleted blocks

A deleted block `a` goes before the first revised block that follows it in the pairing, in the same container, and after the
last revised block of the container when none follows (`Place::Before` and `Place::After` of `revisions`). Consecutive deleted
blocks go in one call in their order. In a stretch that holds both deleted and inserted blocks, the deletions come before
the insertions, as Word writes a replacement (`del` then `ins`).

### 5.2 Equal text, formatting only

A pair with equal exact text has no text edit. Its formatting is compared (5.4), and a difference is a `FmtChange` in a
`ParaPlan`, and the pair is `Keep` only when there is none. Equal comparison key and different exact text (a quote or a
space) is an ordinary edit.

### 5.3 The text edits

For a pair with different text, `diff::edits(a_text, b_text, Cleanup::Readable)` of `revisions` gives replacements, each a range of
the original's text and the text that replaces it. A replacement is a deletion of the range from the original (when it is not
empty) and an insertion of the new text (when it is not empty). The range of the new text in the revised paragraph is the
range of the old one moved by the sum of the earlier replacements' length differences. The plan turns each replacement into a
`PlanEdit { insert: Range<usize>, deleted: Option<Fragment> }` on the revised paragraph, where `insert` is the revised
characters to mark inserted (empty for a pure deletion) and `deleted` is the original's range, as a fragment to import
(7.2), to be placed at `insert.start`. Two rules:

- **A replacement never cuts a field or an atom.** A field (`begin` to `end`) that an edit's old range or new range cuts is
  covered whole: the edit is widened on both sides to the field, and the other side's range is mapped through the unchanged
  stretches (the characters correspond one for one there), then the edits that now touch are one. Deleting a field
  whole deletes `w:instrText` as `w:delInstrText` (`revisions`' shape for a deleted field, `RP019`), and inserting one marks its
  runs inserted with the `separate` run outside the `w:ins` (`RP020`), both of which the primitives do.
- **A field that spans paragraphs is one unit** (a TOC, with its entries and their page numbers, in several paragraphs). The
  field index of both sides (`wordml::FieldInfo`) says which paragraphs a field covers. For a pair of such fields, if any of
  their paragraphs differs in text or formatting, **the whole group is unpaired**: every original paragraph of the field
  is a deleted block and every revised one an inserted block, so neither side has half a field, and it is not listed as a
  move. If every paragraph is equal the group is kept. The fixture is a TOC whose page numbers changed.
- **The cleaning is `Readable`.** The noise measure counts against `Minimal` (9.4), and the same function does both.

### 5.4 Formatting

Paragraph properties are compared as `Paragraph::base_props()` of the two paragraphs (the `w:pPr` children except `rPr`,
`sectPr` and `pPrChange`), and the mark's `w:rPr` separately. Run properties are compared per character: in the stretches
where the two texts are equal, the sequence of `Paragraph::run_props(item)` of the revised and of the original paragraph (the
direct `w:rPr`, with `rStyle` as one of its children) is compared along the common characters, and each maximal range where they
differ is a `FmtChange { range, snapshot }` whose snapshot is the original's properties, with ids mapped by the import
(7.3, 7.4). Text that is inserted or deleted has the formatting of the side it is on, and compares nothing.

**What is not a difference.** Before comparing, both sides drop `w:rsid*` attributes, `w14:paraId` and `w14:textId`,
`w:cnfStyle` (a cache of conditional formatting), `w:lang`, `w:noProof`, `w:proofErr`, and `w:rFonts/@w:hint`. They are
properties that no reader sees and that Word rewrites on save, so a redline that showed them would be pure noise (the
corpus: `cnfStyle` is 72 of the 112 formatting differences of the Word pairs). The list is one `const` in `compare/fmt.rs`
and `FIDELITY.md` repeats it. The quality check's normalization drops the same.

Where only the style's definition differs and not the style id, nothing is a difference: a style definition is not compared
(8.3). Where the `pStyle` or `numPr` of two paragraphs differ after mapping (7.3, 7.4: an original `numId` that means the
revised document's list is the same list), a `pPrChange` is written.

### 5.5 Splits and merges as plans

- **Split** (a, b1..bk): `a`'s text is diffed against the concatenation of the revised texts, and each replacement goes to the
  paragraph that holds its start (an edit across a paragraph boundary is cut at the boundary into one piece per paragraph,
  and a deleted fragment at a boundary offset goes to the earlier part). b1 to b(k-1) have their **marks alone** marked inserted (`ParaPlan.mark_inserted`, a flag of 6.1: `w:ins` in the mark's `w:rPr`
  and no wrapper on the content), and bk keeps its mark. That is `revisions`' Y shape from the other side, and rejecting joins them with the later paragraph's properties, so
  `bk` gets `set_snapshot` with a's properties when they differ.
- **Merge** (a1..ak, b): the concatenation of the original texts is diffed against `b`, with the **replacements cut at the
  original paragraph boundaries** (a replacement whose old range crosses a boundary of a1..ak is cut there into one piece per
  original paragraph, so each deleted fragment comes from one original paragraph). The offsets of the boundaries are mapped
  to `b`, `b` is split untracked at them (`Compiler` with `Track::Untracked`, which is `revisions`' `split_block`), and the first k-1
  paragraphs are merged with `merge_blocks` tracked, which marks their paragraph marks deleted and leaves both
  paragraphs' text as it is. Each of the k parts gets `set_snapshot` with its own original paragraph's properties when they
  differ, so that rejecting restores each, and since all k copies of `b`'s properties are equal, accepting gives
  `b`'s properties whichever paragraph's survive (the open question U of the census of `revisions.md`).

## 6. Building the redline

### 6.1 The five entry points

`revisions.md` section 12 left three primitives to this design. They are one new family of `Compiler` methods, written in new
files of `revisions/` (decision 1 of section 13: compare writes them), reusing the carve, the wrapper rules, the gap rule and the
stamp, and obeying the compiler's promises of 2.2 (atomic, ordered, touches, schema order, ids, the copy rule, origins). Each
takes lists, because every call that reads a paragraph waits for a refresh of the story (10).

```rust
impl Compiler<'_, '_> {
    /// A paragraph of the revised document, with edits, one text map and right to left.
    pub fn redline_paragraph(&mut self, block: &BlockKey, plan: &ParaPlan) -> Result<Effect, StetError>;
    /// Marks existing blocks inserted: a paragraph (its mark and all its content, `revisions`' shape X), a row (trPr/ins, and the
    /// paragraphs of its cells), or a table (all its rows). A list of consecutive blocks, in one call.
    pub fn mark_inserted(&mut self, blocks: &[BlockKey]) -> Result<Effect, StetError>;
    /// Places imported detached blocks (paragraphs, rows, tables, a note's paragraphs) as one tracked deletion each:
    /// content in w:del, the mark deleted, a row's trPr/del. A list of consecutive blocks.
    pub fn place_deleted(&mut self, anchor: &BlockKey, place: Place, blocks: &[Imported]) -> Result<Effect, StetError>;
    /// Writes a pPrChange (and the mark's rPrChange) whose snapshot is given, and changes nothing else.
    pub fn set_snapshot(&mut self, block: &BlockKey, snapshot: &Snapshot) -> Result<Effect, StetError>;
}
pub struct ParaPlan {
    pub edits: Vec<PlanEdit>, pub fmt: Vec<FmtChange>, pub snapshot: Option<Snapshot>,
    pub mark_inserted: bool,           // the paragraph's mark alone is inserted (the first parts of a split)
    pub source: Option<BlockKey>,      // the original paragraph the fragments come from, for the foreign origin
}
pub struct PlanEdit { pub insert: Range<usize>, pub deleted: Option<Imported> }   // offsets in the paragraph's Final text
pub struct FmtChange { pub range: Range<usize>, pub snapshot: Vec<PropElement> }   // an rPrChange per run, this snapshot
pub struct Imported { pub node: NodeId, /* detached, in the redline's tree */ pub origin: BlockKey /* in the original */ }
```

- **`redline_paragraph`** builds the paragraph's `TextMap` once, applies every `PlanEdit` and `FmtChange` from the last to the
  first, and for each edit: carves the revised runs at the ends of `insert` and wraps them in our `w:ins` (the same wrap
  as the compiler's insertion, which puts the `ins` inside a hyperlink and never around one, and keeps range markers in
  place), then places the deleted fragment immediately before the `ins` (or, for a pure deletion, at the gap that
  `insert.start` names, by the gap rule of `revisions.md` 4.2), as `w:del` runs in which `w:t` is renamed `w:delText` and `w:instrText`
  `w:delInstrText`, with the text of the fragment's own formatting. A `FmtChange` carves the covered runs, takes each run's `w:rPr`
  (creating it) and writes an `rPrChange` as its last child with the snapshot. All revision elements take their
  ids from `Tx::next_annotation_id` and carry the stamp. With `mark_inserted` it also writes `w:ins` in the mark's `w:rPr`
  and nothing on the content. The call touches the paragraph. **A paragraph that receives fragments is declared as built from
  nothing in the revised document and from `source` in the original**: `Effect::imported` lists `(block, source)`, so that the
  paired paragraph is a foreign origin as much as an imported block is (7.7).
- **`mark_inserted`** needs no text offsets: a paragraph that **carries a section break** gets its content wrapped and not its
  mark (the mark and the break stay, as in `revisions`' deletion of such a paragraph, so rejecting leaves an empty paragraph with the break:
  one of the exclusions of 9.1, with a test). Any other paragraph gets `w:ins` in its mark's `w:rPr` (first child, `ins` before `del`)
  and every run of its content wrapped (consecutive runs in one `w:ins`; the `separate` run of a field outside it), a row gets
  `w:ins` in `w:trPr` and the same for each paragraph of its cells (Word writes no `cellIns` for a new row, `FA/007`), and a
  table does that for each row. A block in a deleted container is `readonly_region`, as for any edit.
- **`place_deleted`** puts each imported block in the container of `anchor`, before or after it, in schema order. A paragraph
  is a copy of the original's with the **copy rule** applied (it loses every revision element and its section break), its mark
  gets `w:del` and its content is wrapped in `w:del` (text renamed). A row gets `w:del` in `w:trPr` and its cells' paragraphs
  are deleted the same way, including each cell's last paragraph (`FA/006`). A table is its rows. **A run of deleted blocks that ends its
  container** (nothing live follows in the cell or the body) uses `revisions`' rule for a deleted last paragraph (`RP040`) for the run as a
  whole: every imported paragraph but the last has its mark deleted, **the last live revised paragraph before the run takes a
  deleted mark** too, and the last imported paragraph keeps a live mark with its content deleted. That last paragraph gets a copy
  of the live paragraph's `w:pPr` and of its mark's `w:rPr`, **with the original's own in a `pPrChange` and an `rPrChange`**, so
  that accepting joins the live paragraph with the empty ones and leaves it with its own properties, and rejecting restores the
  original paragraphs and their marks. Because the live paragraph is written to, the plan lists it as an `Edit` (flagged
  `trailing`), and N4 does not call it kept. N1 and N2 do **not** inherit R8's exclusion of trailing paragraphs: the redline has no
  paragraph more or less than the two inputs. The call declares an `Origin` with an empty `from` for each block, and
  `Effect::imported` lists `(block, source)` for each (an addition to `Effect`, for `document`'s `Declared`).
- **`set_snapshot`** is `set_props` with the old value given and the new one left alone: it checks the container for a
  pending `*PrChange` (none will exist, because revisions were accepted first and this transaction writes one per
  container), writes `w:pPrChange` last in `w:pPr` with the snapshot (the paragraph's, minus `rPr`, `sectPr` and `pPrChange`, by
  `Ctx::allows_child`) and an `rPrChange` in the mark's `w:rPr` when `Snapshot` has one, and creates the containers with
  `insert_in_order` when they are missing. It is a no-op when the snapshot equals the properties, and `finish` refuses a change element whose snapshot is equal
  (W11).

The split and merge of 5.5 use `split_block` untracked and `merge_blocks` tracked, over two `Compiler`s in one transaction,
which `revisions` supports (several compilers over one `Tx`, section 16.15 of the revisions design). The ask is that a test pins the
shape: an untracked split followed by a tracked merge has the same XML as `revisions`' tracked merge of two paragraphs that were
there before (a deleted mark on the first, nothing else).

### 6.2 Inserted blocks

An unpaired revised paragraph is `mark_inserted`, which is `revisions`' shape X: mark and content inserted, self-contained, so that
rejecting removes it whatever its neighbours. It is correct in the middle of a story and before another block. **A container
that ends with inserted blocks** gets a live last mark instead (`revisions.md` 3.2.1, check C11), since Word cannot remove the container's
last mark: once the whole story is written, `settle_end_marks` finds each container whose last paragraph has an inserted mark,
walks back over the paragraphs whose marks are inserted and the tables whose rows all are (rejecting removes those before it
joins the marks), and gives the paragraph it reaches the inserted mark, next to a deleted one if it is a deleted copy of the
original (one paragraph mark both inserted and deleted, as two authors leave it). The container's last paragraph keeps a live
mark, with its own properties and that paragraph's in a `pPrChange` and an `rPrChange` in its mark, so that rejecting joins
them into a paragraph with the earlier one's properties, as it was. It mirrors the `RP040` shape of 6.1. A join does not reach
across a table that rejecting removes (`revisions.md`, finding 28), so in the gap that ends a container the deleted blocks go
after the last inserted table, right before the paragraphs that end the container, and not before the gap's first insertion:
the paragraph that takes the mark is then next to the run it joins (`DB010`). When the gap after a
container's last pair holds both deleted and inserted blocks and its last blocks on both sides are paragraphs with no section
break, no atom and no markup-compatibility content, `tables::pair_the_last_paragraphs` pairs those two, so they share the last
mark as one paragraph whose text is replaced, which is what Word writes for text typed over a cell's. When nothing before the run
can take the mark (a section break, a container whose content is all new) the receipt names the last paragraph `not_restored`:
Word keeps it, emptied, when its insertion is rejected, and so does stetdoc. A note or comment that is all new goes with its
reference and is left alone. An inserted table is `mark_inserted` over the table. Consecutive inserted blocks are one call.

### 6.3 Deleted blocks

An unpaired original block is imported (7), then `place_deleted` as in 5.1. A paragraph that carries a section break loses
the break (the copy rule), so the redline does not reproduce the original's sections: section structure is not compared (8.3).

### 6.4 Order of the writes and the state they leave

Per story: first the imports (they create detached nodes and parts, and do not change the story), then `place_deleted` and
`mark_inserted` for blocks (a call per run of consecutive blocks), then `redline_paragraph` and `set_snapshot` for the paired
paragraphs, last to first, then the splits and merges. The order is only an order of execution: the revised block keys never
change, because the compiler keeps the original node for everything it does not create, so a plan made before the
build is valid throughout. `Compiler::finish` checks the compiler's own post-conditions (W1 to W11) for everything that
was written, and `document` runs `check::edit` with the foreign origins, so a redline that breaks a rule is refused and not
saved.

### 6.5 What the redline looks like for the two accepts

Accepting everything removes the `w:del` content and unwraps the `w:ins`, drops deleted marks (joining nothing, because a
deleted paragraph is whole), and keeps the revised properties: that is the revised document. Rejecting everything removes
the inserted blocks and runs, unwraps the `w:del`, restores the marks and the snapshots: that is the original. These are
`revisions`' laws R8 and R2, which is why the quality check needs no new resolver (9).

## 7. Import

`compare/import/` is the one place that copies from one package into the other. It never writes a revision. It produces
**detached nodes in the redline's trees** and the parts and definitions they need, and `place_deleted` and
`redline_paragraph` write the revisions around them. An `Importer` holds the maps of one comparison (style ids, numbering ids,
relationship ids, note ids, bookmark ids, content hashes of the parts it copied), all in vectors or ordered maps, so
that two runs of the same inputs write the same bytes.

### 7.1 Nodes across trees

`xml` has `clone_subtree` within one tree and nothing across two. The copy is made with the existing `Editor`: for each
element, `create_element(ns, local)` by namespace URI, `set_attr` for each attribute by URI, `create_text` for text, and
`append_child`. The prefixes are chosen by the editor at insertion from what is in scope (`xml/late.rs`), so a prefix the
original used and the revised part lacks (`w14`, `mc`) is declared where needed, and an attribute in `mc:Ignorable` terms is
handled by the tree's own rule. The cost is one editor call a node, which is constant, and was not on the 13 µs a
tracked replace of `revisions`' measure: an imported paragraph of ten nodes is tens of microseconds. An ask to `xml` (`Editor::import_subtree`
over two trees) is an optimization and not a need.

### 7.2 Fragments: runs of a paragraph

A deleted range of a paragraph (a `PlanEdit.deleted`) is carved from the original's runs **in the copy**, not in the original: the
importer builds the runs of the range from the original's `TextMap` (`locate` gives the spans and the items between them)
by copying each covered run with its `w:rPr` and the part of its text that the range covers (a `w:t` is split by character
offset, a tab, break, symbol, object or field character is copied whole), keeping the wrappers that the original had
(`w:hyperlink` and `w:smartTag` get a copy, with the relationship of a hyperlink imported (7.5), as `w:hyperlink > w:del > w:r`
by the compiler's rule), and writing `w:t` as it was, for `place_deleted` to rename. The ranges of fields are whole (5.3), so
no fragment has half a field. A field that the range covers is copied from `begin` to `end` with its `w:instrText`. A content
control inline is not copied as a control (its text is, as runs), and a comment range or reference in the range is dropped,
as 7.6 says.

### 7.3 Styles

The cascade of the original (`state`'s `PartsView::cascade`) says which style ids the imported content uses: the `pStyle` of
a paragraph, the `rStyle` of a run, a table style of a table, and the styles those are based on. For each:

- an id that the revised package has, **with an equivalent definition** (the effective properties of the style in both
  cascades are equal after the ignore list of 5.4), maps to itself: nothing is imported;
- an id that the revised package lacks is imported as it is, with its `basedOn` chain (the chain's members by the same rules)
  and its `next` and `link` dropped, because they are not needed for how it looks and a dangling one is a violation;
- an id that exists with a different definition is imported **renamed**: the id gets a suffix (`Heading1` becomes
  `Heading1Original`, made unique) and the `w:name` gets ` (original)`, so that Word sees two styles, and the imported content
  looks as it did.

Definitions are appended to `word/styles.xml` through `part_mut` and `insert_in_order`. The quality check's normalization compares styles by
`w:name` with a trailing ` (original)` removed. In the 352 LAB pairs the two `styles.xml` are byte-identical so nothing is
imported, and in 195 of the 469 Word pairs they differ, which is where the rule is exercised. A paired paragraph keeps the
revised document's style id (5.4), and a style definition's own difference is not a revision (8.3).

### 7.4 Numbering

A paragraph's list is its `numId` (direct or from its style) and `ilvl`. For each `numId` of the imported content (and of the
snapshots of 5.4): if the revised package has a `w:num` with the **same definition** (the abstract numbering's levels, formats and
texts, starts, the style links, and the overrides, all compared with the ids, `w:nsid` and `w:tmpl` left out), the original's
id maps to the revised one's (when several revised `w:num` share the definition, **the one with the same `numId` is preferred**, and
otherwise the lowest), so that a deleted clause is counted in its own list and shows the **ghost number** it had, as
`styles-and-numbering.md` says a deleted paragraph does in a redline (the number the counters hold at its position). Otherwise the
abstract numbering and the `w:num` are imported with **fresh ids** (the highest `abstractNumId` and `numId` plus one,
in the order of first use), and the deleted paragraphs show their numbers as a list of their own. The test of "same definition"
is on the XML of `numbering.xml`, canonicalized (the unit of `numbering_defs`' levels). The 352 LAB pairs share their
`numbering.xml` byte for byte, so every id maps to itself there. A style that carries a `numPr` is imported together with
the numbering it names.

### 7.5 Relationships and parts

Every attribute in the `r` namespace of imported content (`r:id`, `r:embed`, `r:link`, `r:pict`, `r:dm`, `r:lo`, `r:qs`,
`r:cs`, `r:href` and the like) names a relationship of the source part. The importer reads it in the original's relationships
(`Rels`), and: an **external** target (a hyperlink, a linked picture) becomes a new external relationship on the revised
part with a new id; an **internal** target (an image, a chart and its embedded workbook, an OLE object, a diagram) is copied
with `Tx::add_part`, with the relationships of the copied part imported the same way, recursively, and the parts are
deduplicated within one comparison by the SHA-256 of their bytes. The new relationship id replaces the old in the copy. A
relationship type that the importer does not know is copied the same way, since the rule is the closure of what the
attributes name. A closure larger than the limits (`Limits` of the options) is `limit_exceeded`. Footnote and endnote
references need the note copied:

- **Notes.** A `NoteRef` in imported content copies the note body into the revised `footnotes.xml` (or `endnotes.xml`) with a
  new `w:id` (the highest plus one) and its paragraphs as detached imported blocks that `place_deleted` marks deleted, with
  the last paragraph's mark too (`delete_block_with_mark`, the shape of `RP050`), and the reference run is in the `w:del`.
  Accepting removes the note once no reference names it (the unreferenced-note rule, `revisions.md` 3.6.4). A note that
  belongs to a reference in **inserted** text is not imported: it is the revised document's own, and its paragraphs are
  `mark_inserted` (4.7), so a note and its reference are always written together.
- **Bookmarks.** A `bookmarkStart` and its end in the imported content get fresh ids from `Tx::next_annotation_id` (bookmark ids share the counter with revisions and comments) and keep their name, unless the revised
  document has a bookmark of that name, in which case both ends are dropped (a count in `compare_bookmarks_dropped`), since two
  bookmarks may not share a name. `_GoBack` is always dropped.
- **Content controls and ids.** The `w:id` of an imported control is made unique in the revised part, and `wp:docPr` ids are
  unique **across the whole package** (a drawing in a header and one in the body may not share one), found by one walk of every
  story part and a counter. An imported paragraph gets a new `w14:paraId` (by `revisions`' rule for new ids, so that two imported
  paragraphs never collide and none collides with the revised document's) only when the revised part uses paraIds, and
  `w14:textId="77777777"`.
- **Package-level parts that need adding** (`[Content_Types].xml`, relationships) are written by `add_part`, which is atomic.
  The one thing it does not do, an external relationship with no part, is an ask (12).

### 7.6 What is dropped, with a warning

Comments and their ranges in deleted content are dropped (`compare_comments_dropped`, with the count), because a comment has an
author and a thread in a part of its own that a deleted paragraph cannot carry into the redline without inventing a thread.
The original's other comments are not carried either: the redline has the revised document's comments. Content controls'
wrappers, section breaks, text box fallbacks (`mc:Fallback` of a deleted drawing is copied whole with its `Choice`, so there
is no loss), and the `w:id`s of everything are the other differences, and none is a text difference.

### 7.7 Origins and `check`

Every imported block is declared as `Origin { block: new, from: [] }` and, for `check`, as a foreign origin: `document` computes
`check::unit_keys(original.parts(), source)` for each entry of `Effect::imported`, **which lists the paired paragraphs that
received fragments as well as the blocks placed whole** (a fragment can carry a problem that the original paragraph already has,
and a paired paragraph's own base would not know it), and passes them as `Declared::foreign` (`check.md` 2.3, the ask to
`compare`). **The definitions and parts that import adds are foreign too**: a style or a numbering definition appended to
`styles.xml` or `numbering.xml`, and a part copied with `add_part` (a picture, a chart), is a new top-level element or part
that `check` would treat as everything new, so `Redline` also lists them with their node in the original, and `document` passes their key
sets (an ask to `check`, 12: `unit_keys` for an element of a non-story part and for a whole part). Without it a problem in the original's paragraph would be new in the redline, and the edit would be refused
for it. The renames (`w:t` to `w:delText`) are in `check`'s rename table already. The paired paragraphs of the revised document are
checked against themselves in the base, as for any edit, so a problem that was in the revised input is not blamed on the
redline.

## 8. Existing revisions, warnings and what is not compared

### 8.1 Accepting first

Both inputs are accepted by `revisions::resolve(tx, index, [Step { Accept, All }])` in one transaction each, before the model is
built: moves, tracked formatting, rows and marks, all of it, so the comparison reads the document as Word shows it with
all changes accepted (`Final`), which is what the numbering design expects of compare too. The count of each side goes in the
`compare_revisions_accepted` warning. `revisions`' R8 says the result has the text of `Final`, so accepting is not a second
interpretation. The redline is built on the accepted revised package, and `Compared.document`'s original bytes are the revised
input's as given, so `save` writes the redline and the working copy keeps both. A revised document with revisions
therefore costs one accept (23 to 45 ms in the `revisions_accept_all` benchmark), and one without costs nothing.

### 8.2 Warnings

The codes are in `api`'s registry (`finding_codes!`'s sibling for warnings), and these are the ones compare raises, each once with a
count, as `Receipt.warnings` deduplicates:

| Code | When |
|---|---|
| `compare_revisions_accepted` | a side had revisions that were accepted first (side, count) |
| `compare_comments_dropped` | comments of deleted content were not carried (count) |
| `compare_bookmarks_dropped` | an imported bookmark's name was taken (count) |
| `compare_story_unmatched` | a header or footer slot that exists on one side only, or has no one-to-one partner, was skipped (count) |
| `compare_table_replaced` | rows whose cells changed shape were written as a deleted row and an inserted row (count) |
| `compare_not_compared` | styles, numbering, table, row, cell or section properties differ and were left as the revised document has them (a count of each kind) |

### 8.3 Not compared, said once

Compare does what is listed here and nothing more: **not compared** are style and numbering definitions (their properties are the
next release's "editing numbering and style definitions"), table, row, cell and section properties (`w:tblPr`, `w:trPr`, `w:tcPr`,
`w:tblGrid`, `w:sectPr`, and so headers' references and page setup), document and package properties and settings, a
field's code, the original's comments, and the wrappers of content controls. The redline carries the revised document's value for each of
them, and `FIDELITY.md` lists them. The quality check's normalization leaves them out of both sides, so the law is a law about what compare
promises (9.1). A reader who needs one of them compared should tell us, and the answer is a property writer of `revisions`
(`Extend compare to a new property` in the where-do-I-change table), not a change of the structure here.

## 9. The quality check

### 9.1 The law

For every pair (O, R) of the corpus, with `Ô` and `R̂` the inputs with their revisions accepted, and `D = compare(O, R)`:

- **N1 (accepting).** `normalize(accept_all(D)) = normalize(R̂)`.
- **N2 (rejecting).** `normalize(reject_all(D)) = normalize(Ô)`.
- **N3 Identity.** `compare(R, R)` has no revision, and saves as `R`'s bytes when `R` has no revision.
- **N4 Locality.** The bytes of every block of the revised document that the plan keeps (`Keep`) are unchanged, and
  so are the parts that the redline did not stage (the same law as `revisions`' R6, with the touched set the plan's). The last live
  paragraph before a run of trailing deletions is planned as an `Edit` (6.1), because its mark is written, and so is not a kept block.
- **N5 Validity.** `check::edit` of the redline passes with the foreign origins, and `Document::check` on the saved redline has no
  finding that neither `Ô` nor `R̂` has (the no-new-violations rule).
- **N6 Determinism.** The same bytes, options and stamp give the same output bytes and the same receipt (no map is iterated for
  output, and ids come from the transaction's counter).
- **N7 Moves are real.** Each `MoveEntry` names a deleted block and an inserted block of the redline, and their comparison text
  has a Dice score of at least 0.8.
- **N8 Accepting first.** `compare(O, R)` and `compare(Ô, R̂)` give the same redline apart from the warnings.
- **N9 Noise** (9.4) stays within the threshold.

N1 and N2 are `revisions`' R8 applied to the redline and read through `normalize`. The oracle for R8 is the text model of `wordml` and
pandoc where installed, and neither shares code with `compare`. `normalize` extends `revisions`' `laws_norm::normalize` (ids
renumbered by document order, white space between elements dropped, adjacent equal runs merged, namespaces compared by URI)
with the neutral model of `projection` read in the mode (`Final` for N1, `Original` for N2, in the mode names of the
glossary): per story, per block, the block kind and the structure of tables (rows, cells), the **text including the clause number**
(so a wrong list mapping or a wrong ghost number fails), the style by name (with ` (original)` removed, 7.3), the
**direct** paragraph and run properties minus the ignore list of 5.4 (and not the effective ones, which would make a style
definition's difference a difference of every paragraph that uses it), notes by content, headers and footers of the slots both
sides have (paired one to one, 4.7). Clause numbers are compared **only where the two numbering definitions are equal** (7.4):
where an imported list got fresh ids its numbers are those of a list of its own, and each such paragraph is counted as an
exclusion and not compared. Notes are not excluded: a note follows its reference. It leaves out the list of 8.3 and the slots
skipped by `compare_story_unmatched`, and the number of
exclusions per pair is printed and has a ceiling, so the list cannot grow quietly (the discipline of `numbering`'s golden test).

### 9.2 The pairs

The pair manifest is `corpus/pairs.tsv` (the testkit's, decision 6 of section 13, a fetch manifest with pinned sources and hashes): class, the manifest ids of the two documents, the rule's score. The
classes are those of 1.2. The curated set (PR gate) holds **40 LAB pairs** chosen by size and by the presence of tables and
revisions in the markup document (about 80 documents and 5 MB; the curated set has one LAB turn document today), the Word
families of `WC001` to `WC067` (they are small, a few KB), and 20 of the tool-made pairs. The full corpora (nightly) hold all
352 + 469 + 82. A pair whose input does not open is out of the set, as `expect = ok` says elsewhere. Section 21 has the
manifest as built.

### 9.3 Generated pairs

A generated pair is (D, E) where E is D with a batch of **untracked** edits applied through `Document::edit` (so the engine's
own operations make it, and the generator is `revisions`' `laws_gen`, which makes the batches for the laws): text replacements,
paragraph inserts and deletes, splits, merges, formatting, row inserts and deletes. The generator knows which operations it
applied, so it is also the ground truth of the alignment: the **alignment recovery** test counts the untouched blocks the
aligner paired with the right partner (a ceiling of one wrong pair in 1,000) and the splits and merges it found (4.6, the
calibration). Three kinds:

- **Same-package edits** (D with E): every curated document, at least 10 batches per document on a pull request and 50 nightly.
- **Cross pairs**: two different documents of the corpus. The reject side must restore all of one document, so these exercise the
  import of styles, numbering, notes, relationships, media and bookmarks in bulk, which no real pair in the corpus does
  together. Reject-all must give the original, and accept-all the revised.
- **Pairs with revisions in an input**: a corpus document that holds revisions as one side (the 31 curated ones), which tests
  that accepting first is what N1 and N2 mean.

### 9.4 The noise measure and its threshold

For a pair, `min_chars` and `min_spans` are the characters and the edits of `diff::edits(old, new, Cleanup::Minimal)` over the
text of each story (the paragraphs of the comparison text joined by a line break, and the stories summed). The redline's
`red_chars` is the characters of all its inserted and deleted text (a deleted or inserted paragraph counts its mark as one
character), and `red_spans` is the number of **changes** that `revisions::changes` finds, so adjacent paragraph deletions are one
span, which is what a reader sees as one change. Formatting changes are **counted separately**: `fmt_spans` is the number of formatting changes (`rPrChange`, `pPrChange`),
which the minimal text diff has no counterpart for, and has its own ceiling (a gate of its own in the baseline, set the same way
from the first baseline, so that a formatting flood cannot hide in the text figure or push it over). The ratios are
`red_chars / min_chars` and `red_spans / min_spans`, with `red_spans` counting text changes only.

**The first baseline** is taken with the built module and `Cleanup::Readable`, on all three classes, and written to
`stetdoc-testkit/bench/noise_baseline.tsv` with the same machinery as the benchmark baseline (rows by pair, a commit
adds or changes one). The spike's figures are the provisional ones: LAB per-pair p99 1.45 and maximum 1.84, aggregate 1.195 (as built),
and spans 0.36; Word pairs aggregate 1.027, generated 1.02. The gate is:

- **per pair**: `red_chars <= C * min_chars + 40` where `C` is the p99 of the first baseline's per-pair ratio of the class
  times 1.10 (1.6 on the spike), and the 40 characters are the slack for small documents, where one added paragraph mark is
  a quarter of the minimal diff (a pair of 13 against 4 characters is 3.25 and is within it);
- **per pair**: `red_spans <= min_spans + 2`, because compare may merge but must not fragment more than the minimal diff;
- **per class**: the aggregate char ratio may not exceed its baseline by more than 5%, so a regression in alignment that
  makes many pairs a little worse is caught where no single pair is;
- **a change to the cleaning constants or the alignment floor** re-takes the baseline in the same commit, with the table of
  before and after pairs, as `revisions`' calibration did for the cleaning. The review by a lawyer in a host application judges the same figure by eye,
  and a figure that the reviewers dislike is a reason to lower `C`, not to raise it.

**Outliers are recorded, never silent.** A pair above the per-pair gate is an outlier. Each one is listed in the pairs manifest
with its reason (a short pair, a rewritten section, a table replaced) after a person has looked at the redline, and the gate
fails on **any outlier that is not recorded**, and on a recorded one whose reason no longer holds (its figure fell under the gate).
The baseline is never written by a test. `cargo xtask check` reads it and fails on a pair above the gate.

### 9.5 What this cannot prove

That Word accepts the redline without a repair prompt (the stratified sample of 500 outputs for the Word check, decision 0005, includes at least 50
redlines), and that a lawyer finds it readable (the review in a host application). LibreOffice, which the
testkit's oracle runner has, opens every redline of the pair set and converts it to PDF as a smoke test; it reads
tracked changes differently from Word and decides nothing. Which `pPr` Word keeps when it accepts a deleted mark is still
the open question of the census of `revisions.md`, and compare's shapes were chosen to be correct under either answer (5.5, 6.1).

## 10. Performance and memory

The budget given for compare is **1 s on the benchmark inputs**. The speed budgets in `ARCHITECTURE.md#performance` have no row for it, so the inputs are the five of `cargo xtask
bench` and the pairs are made from them: the contract and the dense contract each against a copy of itself with 5%, 10% and 25%
of the blocks edited, deleted, inserted or moved, by the same generator as the synthetic pairs above (made when the benchmark
is prepared, so the clock does not see it).

What each step costs, from the baselines of `stetdoc-testkit/bench/baseline.tsv` on the development Mac and from the spike:

| Step | contract-300p (about 4,600 blocks) | dense-contract-300p | Source |
|---|---:|---:|---|
| open, original and revised | 15 + 15 ms | 42 + 42 ms | `state_open` |
| accept revisions, if present | 24 ms each | 45 ms each | `revisions_accept_all` |
| comparison text and bags | 3 + 3 ms | 10 + 10 ms | `wordml_text_pass` |
| alignment | about 1 ms | not measured | the module's `align` on 4,620 blocks at 5% to 25% (the scoring is in the model step) |
| word diffs of the changed pairs | a few ms | | spike: 8.6 ms a pair on average for the 352 LAB pairs, which includes the whole-text minimal diff of the noise measure |
| writes, per modified paragraph | 0.93 ms (0.62 refresh + 0.31 compile) | 2.3 ms (1.0 + 1.3) | `revisions_compile_50` over 50 |
| `check::edit`, per 1,000 dirty units | 40 to 230 ms | | `check.md` 2.2 |
| commit audit | about 13 ms | about 32 ms | `state_edit` less `state_writes` |

For the contract with 10% of the blocks modified (460 calls), the estimate is 30 + 6 + 25 + 8 + 428 + 20 to 105 + 13, which is
**0.53 to 0.62 s**. With 25% (1,150) the writes alone are 1.07 s, so the budget is missed, and that is the case that
**the read that does not wait for a refresh of the story** (6.1, an ask to `revisions` and `wordml`) is for. Of the 0.93 ms a paragraph takes, 0.62 ms is the refresh of the
story after the previous write, and the compile part alone is 0.31 ms, which gives 357 ms of writes, about 80 ms of the other steps and 46 to 265 ms of
`check::edit` for 1,150 dirty units, so 0.5 to 0.7 s at 25%. A paragraph of the
dense contract costs 2.3 ms with the refresh and 1.3 without. The writes of whole blocks (deleted and inserted blocks) are
one call to a run of blocks, so a complete rewrite (two unrelated documents) is a handful of calls and a copy of the original's
nodes, and its cost is the import (7.1), tens of microseconds a paragraph, plus the refresh of a few stretches: the adversarial pair
benchmarks it.

**The target** is, then: on the contract, 1 s at 10% modified blocks as built, and 1 s at 25% once the refresh-free read exists (it is not implemented); on the dense
contract, the same two figures reported and held against 1 s at 5% and 10% (the benchmark's baseline fixes what the
numbers are, as `cargo xtask bench --update-baseline` does for the rest). The step that does not scale with the modified
blocks is the imports of media: a closure with a 50 MiB picture copies it.

**Memory.** Two states are open at once (the original is read while the redline is built, for the import), plus the redline's staged
trees, which copy only the chunks that edits write (`ARCHITECTURE.md`, Performance). **Compare's memory budget is
the sum of the memory budgets of its two inputs** (each 4 MiB, the file and eight times its XML, as `ARCHITECTURE.md#performance` sets them), because it holds two states. On the
benchmark inputs at 10% modified blocks that is 74.1 MiB for the contract (two files of 0.38 MB and 4.3 MB of XML each) and 128.6 MiB
for the dense contract (0.66 MB and 7.8 MB). The measured peaks are 58.4 MiB and 137.4 MiB: the contract fits, with 15.7 MiB to spare,
and **the dense contract is 8.8 MiB (7%) over**. The rows of `compare_*` hold the budget now (`Limit::TwoInputs`, section 21), with the dense contract a
known gap; the alignment's texts and bags of every block and the redline's staged trees are where to look first.

## 11. Module layout, laws and tests

```
compare/
  mod.rs        the contract; build, Redline, the phases of 2.3                                   ~200
  prepare.rs    open both, derived namespace, accept first, the counts                            ~170
  model.rs      stories into container trees: blocks, comparison text, keys, bags                 ~420
  text.rs       the atom identities, the key normalization, the small hash                        ~230
  similarity.rs bags, Dice, the locality floor                                                    ~120
  align.rs      trim, anchors, spine, recursion (pure)                                            ~380
  align_gap.rs  the dynamic program, the candidate path for large stretches                       ~330
  tables.rs     tables, rows and cells (4.5)                                                      ~280
  regroup.rs    moves, splits and merges (4.6)                                                    ~340
  stories.rs    which stories pair: slots, notes by reference (4.7)                               ~300
  plan.rs       Plan, ParaPlan; pairing into entries (5)                                          ~420
  edits.rs      the word edits into PlanEdits: offsets, field widening (5.3)                      ~300
  fmt.rs        paragraph and run formatting: the ignore list, per-character comparison (5.4)     ~330
  build.rs      the Compiler calls in the order of 6.4                                            ~420
  receipt.rs    moves, counts and warnings into the receipt                                       ~160
  import/
    mod.rs      Importer: the maps, the entry points                                              ~220
    nodes.rs    detached copy across trees; fragments of a paragraph (7.1, 7.2)                   ~420
    styles.rs   (7.3)                                                                             ~330
    numbering.rs (7.4)                                                                            ~330
    parts.rs    relationships and the part closure (7.5)                                          ~300
    notes.rs, markers.rs   notes, bookmarks, ids (7.5)                                            ~330
revisions/      (new files, 6.1)
  redline_text.rs   redline_paragraph                                                             ~420
  redline_blocks.rs mark_inserted, place_deleted                                                  ~480
  snapshot.rs       set_snapshot                                                                  ~180
```

Every file is under 600 lines. The module keeps the `//!` contract of its row. The dependency table needs no change for
`compare` itself (`api`, `package`, `xml`, `schema`, `wordml`, `styles`, `numbering`, `state`, `revisions` is what the files above use) and
`document` converts `Origin` and the imports to `Declared`.

**Laws and tests.** The laws N1 to N9 are functions of `stetdoc-testkit`'s `laws` module over the public interface (`compare`,
`Document::view`, `Document::edit` for accept and reject, `save`), as AGENTS.md says laws run, and the pair loaders are
the testkit's. Direct unit tests, since the module has pure invariants: the alignment (order-preserving; deterministic;
a pair is never paired twice; the spine is the longest; a table pairs with a table only; recovery on generated pairs,
the large-stretch path, the adversarial pair under 100 ms), the plan (snapshots per fixture), the diff of formatting (the
ignore list, one case each), the import (a fixture per row of section 7: a collided style, an equivalent one, a missing one, a numbering that maps and one
that does not, a hyperlink, an image, a chart with its workbook, a note, a bookmark with a taken name, a comment), and the shapes: a table
of Word's shapes for compare (the scenario table of `revisions.md` 8.2a, one row per case: inserted paragraph, deleted paragraph, replaced
word, split, merge, inserted row, deleted row, moved paragraph, formatting only, field replaced, picture replaced,
deleted note), written by hand from what Word writes, and compared with `normalize` of the result. Every
test that makes a document builds it with `stetdoc_testkit::fixtures`. The tests the review added: a run of trailing deleted blocks (body and cell, a table before them), the numbering tie (two revised
`w:num` that share a definition), a TOC whose page numbers changed, unique `w14:paraId` over several imported paragraphs, a fragment of
an original paragraph that has a schema problem passing `check::edit`, blocks of a deleted table staying out of the moves pool, a
note whose reference is inserted and one whose reference is deleted (never a dangling reference), a paragraph with a section break
inserted, and a split whose first parts have only their marks inserted. Seeded bugs, as `revisions` did: a one-line change to the
alignment floor, the placement of a deleted block, a snapshot that is not the original's, and an import that does not
rename a collided style must each be caught by a named test or law.

## 12. What compare needs from other modules

They are numbered so other sections can name them. Each says whether it is in the code.

**From `revisions`.**

- **RV1.** The five entry points of 6.1, which compare writes as new files of `revisions/` (decision 1 of section 13). They are
  built. The decisions of the shapes are made here and can be corrected: the self-contained inserted paragraph (shape X) for
  every unpaired revised block, the RP040 shape for a deleted block that nothing follows, and the Y shape from the other side
  for splits.
- **RV2.** A read of a paragraph that does not wait for a refresh of the story. `Compiler::paragraph` builds a `ParaView` through
  `story.index_of` and the entry's flags, which makes every call after a write wait for `Story::refresh` (0.62 ms on the
  contract, 1.8 on the dense one). The compare edit is known whole in advance and never reads the story between writes, so
  the paragraph's flags can be read from its own element and `Tx::touch` does not need an index. This is the item that
  decides 25% (10). It is not implemented.
- **RV3.** `Effect::imported: Vec<(BlockKey, BlockKey)>`: each block that received content from another package, with its source block in
  that package. It lists the blocks placed whole and the **paired paragraphs that received fragments**, so that `document` can build
  `Declared::foreign` for all of them without a second walk (`check.md` 2.3: every unit that receives copied content declares its
  origin). `Origin { block, from: [] }` for each, which `revisions` already uses for a block made from nothing. It is built, with
  `Effect::imported_defs` beside it (20.2).
- **RV4.** An untracked split followed by a tracked merge has the shape of 6.1. `split_block_untracked`
  (`revisions/redline_split.rs`) does the split (20.1).
- **RV5.** `Change` grouping for the noise measure: `changes()` over a whole story with one change per run of adjacent paragraph
  deletions, which section 22 of `revisions.md` says it does.

**From `wordml`, `state`, `package` and `xml`.**

- **ST1.** `State::open_in` is what `build` uses, and accepting revisions of the revised input is done by a transaction
  after `open_in`. If a `State` could be re-keyed (`State::renamespaced(namespace, origin)`, which
  rebuilds the ref table through `PartsView::layout` and shares the parts), `build` could open the revised input
  once with `of_package` and reuse that open. That is an optimization, not a need, and it is not implemented.
- **ST2.** An external relationship with no part: `Tx::add_relationship(source, type, target, external)`, which `add_part` does
  not provide (hyperlinks, linked pictures). `Tx::add_relationship` exists in `state/tx.rs`, and compare's import does not call it yet (18.5).
- **ST3.** Nothing for run properties: `Paragraph::run_props(item)`, `run_style`, `base_props` and `mark_props` exist, `Item`
  has the kind and the node of an atom, and `StoryNumbers` is read by the cascade accessors on `PartsView`.
- **X1 (optional).** `Editor::import_subtree(&SpanTree, NodeId)`, which would copy a subtree across trees in one call. It is not
  implemented, and the import copies through `compare/import/snip.rs` instead (18.1).

**From `check`.**

- **CK1.** `unit_keys` for an element of a part that is not a story, and `part_keys` for a whole part: the key set of a style or a
  numbering definition in the original's `styles.xml` and `numbering.xml`, and of a part that import copies with `add_part`. Without it a
  definition or a picture part that carried a problem in the original would be new in the redline (7.7). Both are in `check/origins.rs`.

**From `styles` and `numbering`.**

- **SN1.** An `equivalent(style_id, other_cascade)` on the cascade (or the effective properties of a style as a comparable
  value), so that 7.3's test of an equivalent definition is not written twice. It is not implemented, so compare compares the
  definitions itself (18.3).
- **SN2.** The ghost number of an imported paragraph whose `numId` was mapped to the revised document's list is computed
  in `Redline` mode as `styles-and-numbering.md` says. A fixture in the suite of `numbering` (a deleted clause between two live
  ones of one list shows its own number) is what compare's test relies on.

**From `api`, `document` and `ops`.**

- **DOC1.** The warning codes of 8.2 in the registry, and their entries in `capabilities()` and the reference.
- **DOC2.** `document/compare.rs` follows 2.2: it takes `Redline`, converts the origins and the imports to `Declared`
  (`check::unit_keys` on the original for each import), runs `check::edit`, commits, wraps the state in `Document::from_state`
  (with the revised input's bytes as the original), and builds the public `Receipt` (revisions by author from `changes`, `moves`
  from `finish`, with the keys mapped to refs after the commit).
- **DOC3.** `CompareOptions` has no knob in 1.0. A knob for formatting (compare text only) is cheap to add later, because
  `ParaPlan.fmt` is a list that the plan may leave empty.

**From the testkit (`stetdoc-testkit`).**

- **TK1.** `corpus/pairs.tsv`, the loader, the rule of 1.2, and the curated pairs and their documents (section 21).
- **TK2.** The noise baseline file and the check that reads it (9.4), the compare benchmarks (10) on pairs made from the five inputs,
  and the generator of pairs (9.3) as a function of a document and a seed.
- **TK3.** A `compare` fuzz target: one input split into two documents (or an input that is one package and a seed), through
  `document`. It is not implemented.

**From the CLI and the napi binding.** Both are thin: `compare` and `Receipt`, with `--author`, `--date` and `--json`. The
receipt's `text` for a compare says the counts and the moves, and the CLI owns its wording.

## 13. Design decisions

1. **Compare writes its primitives (6.1) as new files of `revisions/`**, reviewed as part of `revisions`. The five names are fixed
   by this design, with the corrections of the review (`ParaPlan.mark_inserted` and `source`, `Effect::imported` as pairs).
2. **`build(...) -> Redline` with an open transaction** (2.2), so that `document` runs `check::edit` with the foreign origins.
3. **A header or footer on one side only is skipped, with a warning in the receipt, and the quality check's normalization leaves it out** (4.7).
   **A note is not skipped**: it follows its reference. A new note's content is marked inserted, a deleted note is imported with
   its deleted reference, and a reference is never left dangling.
4. **The list of 8.3 stands**, and it goes into `FIDELITY.md`.
5. **Compare has no row in the speed budgets of `ARCHITECTURE.md#performance`.** The target is 1 s at 10% modified blocks on the
   300-page contract as built, and at 25% once the refresh-free read (RV2) exists.
6. **The pairs join the curated corpus as a fetch manifest** with pinned sources and hashes (TK1), with no document vendored into
   the repository.
7. **No options on `compare`** beyond the stamp, the limits and the format.

## 14. Build order

Each step was tested alone and ended with its laws. Steps 1 to 9 are in the code, except the fuzz target (TK3) and RV2.

| # | Step | Tests that end it |
|---|---|---|
| 1 | `text.rs`, `similarity.rs`, `align.rs`, `align_gap.rs` (pure). The pair finder rule as a test over the fetched corpus | the unit properties of 4 (order, determinism, no pair twice, the spine), the large-stretch path and the adversarial pair under 100 ms, the alignment figures of 1.3 reproduced as printed counts with floors, **rerun at the floor of 0.35 with the locality penalty** (the figures of 1.3 are at 0.5 without it) |
| 2 | `model.rs`, `tables.rs`, `regroup.rs`, `stories.rs`, over `State` | tables (row counts differ, cells by position, a changed cell shape), moves across containers, notes paired by reference, slots paired by section; recovery on generated pairs (the generator of TK2), the calibration of the split and merge constants |
| 3 | `edits.rs`, `fmt.rs`, `plan.rs` (pure): the plan of every fixture | snapshot of each plan; field widening; the ignore list; formatting on equal-text stretches |
| 4 | The primitives of 6.1 in `revisions/` (RV1), with the laws R1 to R5 and R13 of `revisions.md` on each: `redline_paragraph`, `mark_inserted`, `place_deleted`, `set_snapshot` | R2 and R8 on the result, W11 for the snapshots, the shape table of `revisions.md` 8.2a extended by the compare rows |
| 5 | `import/` (nodes, styles, numbering, parts, notes, markers) | a fixture per row of section 7, the check's foreign origins, a cross pair opens and validates |
| 6 | `prepare.rs`, `build.rs`, `receipt.rs`, `compare()`; the shape table of 11 | N1, N2, N3, N4, N6 on the fixtures; the seeded bugs |
| 7 | `document/compare.rs`, the CLI and napi, the pair corpus (TK1) | the quality check on the curated pairs; N5 and N7 through `Document`; the noise baseline taken, the gate set from it |
| 8 | Performance: the benchmarks (TK2), the adversarial pair, the cross pair, RV2 if the numbers need it | the budget of 10 at 10% and 25%; memory against the formula; the fuzz target (TK3) |
| 9 | The docs of 16, `FIDELITY.md` entries, `CHANGELOG.md`, the Word checklist for compare (50 redlines in the sample of 500 outputs, and the case "a deleted table next to an inserted table") | `cargo xtask check` |

## 15. Risks

| Risk | Mitigation |
|---|---|
| The self-contained inserted paragraph (X) or the RP040 shape for a trailing deletion differs from what Word does on accept or reject (the census of `revisions.md` cannot say) | the laws are correct under either answer by construction (6.1), the Word checklist of `revisions.md` 8.4 gets the compare rows, and the sample of 500 outputs for the Word check includes redlines |
| Alignment pairs the wrong clauses where a contract repeats boilerplate (indemnities, notices) | non-unique keys never anchor, blocks pair by position only inside a stretch and by similarity with a locality penalty, the recovery test on generated pairs counts wrong pairs, and the review by a lawyer reads them |
| Splits and merges are untested by the corpus | the generator knows the truth, the constants are calibrated there, and a miss degrades to a deleted and an inserted paragraph, which is correct and noisy, never wrong |
| A style or numbering that collides makes deleted content look different from the original | renamed imports (7.3), equivalence before import (7.4), and cross pairs in the quality check whose reject-all must restore clause numbers |
| Import copies a large part (a picture, a chart workbook) | deduplicated by hash, bounded by `Limits`, and costed in the benchmarks |
| The writes miss the budget at a heavy turn | RV2, and the fallback that compare writes the plan with one `Tx` and one `Compiler` per story in a batch with the story refresh deferred, which is the same ask in another shape |
| The redline's validation (`check`) costs more than 230 ms per 1,000 dirty units | `check` already validates only dirty units, and the cost is what the benchmark of `check_edit` bounds |
| `compare` grows into a second path for edits | it writes no XML revision itself, every revision comes from a `Compiler` call, and a test greps the module for `"w:ins"` and `"w:del"` the way the dependency check greps paths |

## 16. Changes to other documents

- **`ARCHITECTURE.md`**: the `compare` row's interface becomes `build(Vec<u8>, Vec<u8>, &Limits, &RevStamp) -> Redline` (2.2) and the
  "Compare" paragraph of how the pieces work together gains the open transaction and the foreign origins; the "Where do I change X"
  row for compare names `compare/fmt.rs` and the `revisions` writer.
- **`compare/mod.rs`**: the contract's interface line and the invariants N1 to N9.
- **`GLOSSARY.md`**: *comparison text*, *alignment* (anchor, spine, stretch), *import* (imported block), *noise*, *move* in compare's sense
  (a block deleted in one place and inserted in another, listed in the receipt), and *original* and *revised* if not defined
  precisely.
- **`docs/design/revisions.md`**: section 12's open item 1 points here, and section 2.3's `compare` row names the five entry points.
- **`docs/design/check.md`**: the ask to `compare` is answered by 7.7.
- **`docs/design/refs-and-state.md`**: the rule that compare uses for its namespace is 2.2.
- **`docs/design/document-and-ops.md`**: section 6's `compare` paragraph follows 2.2, and the warning codes of 8.2 join the registry.
- **`FIDELITY.md`**: the list of 8.3, the ignore list of 5.4, and the field-code rule of 3.
- **`corpus/SOURCES.md`**: nothing for the sources, which are already listed. The pairs manifest is the testkit's.
- **`CHANGELOG.md`**: one line under Unreleased for the module.

## 17. What the build changed

Steps 1 to 4 of section 14 (the pure alignment, the model and the pairing, the plan, and the four primitives in
`revisions/`) changed the design as follows. Where this section and the text above disagree, this section is right.

1. **Layout.** The pairing of containers and tables is `tables.rs` (`pair_containers`), and the pairing of stories is `stories.rs`. The
   replacements and the widening to fields are `edits.rs`, the formatting comparison is `fmt.rs`, and `plan.rs` holds the entries
   while `plan_para.rs` holds the plan of one paragraph, a split and a merge. `similarity.rs` and `text.rs` are as designed. The
   primitives are `redline_types.rs`, `redline_wrap.rs` (the node-level wrapping), `redline_text.rs` (`redline_paragraph`),
   `redline_blocks.rs` (`mark_inserted` and `place_deleted`) and `snapshot.rs` (`set_snapshot`), under 600 lines each.
2. **The figures of 1.3 at the constants of the build** (floor 0.35 with the locality penalty): 85.8% of 174,750 paragraphs of 354 LAB pairs
   by hash, 9.2% by similarity and 4.1% unpaired, 58 ms to align all of them, and noise 1.195 (p99 1.45, max 1.84). The alignment is more
   than ten times faster than the first spike's, which built a map for every stretch.
3. **Field spans.** A field that spans paragraphs is found from the story's field index (the paragraph of its `begin` and of its `end`),
   because a paragraph read alone does not know the fields that were open before it. A group is the paragraphs from one to the other, and
   overlapping spans are one group.
4. **`place_deleted` takes the trailing paragraph as a fourth argument**, `trailing: Option<&BlockKey>`, and the entry points keep the names of
   6.1. `ParaRedline` is the input of `redline_paragraph`, and it has the two flags (`mark_inserted`, `mark_deleted`) and `source`.
5. **Right to left, with a tie rule.** `redline_paragraph` sorts its edits and its formatting changes by start, last first, and where a
   formatting change starts at the offset of an edit, the formatting goes first: it lies to the right of a deletion at that offset. The
   other order cut a text node twice at one offset from one map. A deletion at an offset that the formatting to its left has already cut
   goes after the run that ends there (`gap_boundary`).
6. **The law test.** `tests_redline.rs` builds 300 random pairs of formatted paragraphs, makes the plan with `diff::edits`, writes the redline
   and checks that accepting gives the revised body and rejecting the original, after `normalize`, with `finish` run on each. It found the
   two cuts at one offset above, and nothing else. The primitives touch nothing else in `revisions` but `Effect::imported` and the visibility of
   `rename_deleted_text`.
7. **The coverage law on the pair set.** Pairing and plan were run over the 354 LAB pairs and 469 Word pairs of 1.2 (823, none skipped), each
   side accepted first. For every one of them each original block is paired, in a group or deleted, and each revised block is paired, in a
   group or inserted, exactly once (`tests_plan::assert_covered`, also run over the edited corpus documents). The plans hold 21,690 edits, 4,364
   insertions, 3,917 deletions, 83 moves and 161 splits or merges (the corpus search of 1.3 found none at its stricter test: the build's test is
   containment, 0.8 of the words covered and 0.6 of each part's own words in the paragraph), and 70 s of pairing and planning in a debug
   build, 1.1 s for the slowest pair.
8. **A paragraph the primitives refuse.** A paragraph that `check_items` refuses (a locked control, ruby text, alternate content) makes
   `redline_paragraph` fail, and the build has a fallback (19.2) so that one such paragraph does not fail a whole comparison.

## 18. The import as built

`compare/import/` has `snip.rs` (a subtree as plain data, read from one tree and written into another with the editor's calls),
`transform.rs` (the rewrite on the way across), `fragment.rs` (a range of a paragraph as runs), `styles.rs`, `numbering.rs`,
`parts.rs`, `notes.rs`, `markers.rs` and `canon.rs`, each under 400 lines. Where this section and 7 disagree, this one is right.

1. **One path.** A block (paragraph, row or table) or a fragment is read as a `Snip`, rewritten, and written detached into the revised
   tree. The rewrite drops what is not content (`proofErr`, range markers of comments and permissions, section breaks, revision
   marks, `rsid` and `paraId` attributes), unwraps content controls and `customXml`, turns `fldSimple` into the runs of a complex
   field, and maps style ids, list ids, relationship ids, note ids, bookmark ids and drawing ids. The namespaces that the copy
   uses are bound on the target's root (ignorable where the source says so), and the prefixes of an `mc:Requires` are rebound.
2. **Fragments** are the original paragraph pruned to what a range covers, by text node and by atom, with the marks of a whole
   field. A range that cuts a field is refused, and the build then replaces the paragraph whole.
3. **Styles** are compared by their definition (the body of the style without `next`, `link`, `uiPriority` and the like, the list
   ids mapped, and `basedOn` by what it maps to). Equal: the id maps to itself. Missing: imported with its base. Different:
   imported as `<id>Original` named `<name> (original)`, without `w:default`.
4. **Lists** are compared by the canonical abstract numbering (without `nsid`, `tmpl`, `name` and the style links of levels) and the
   overrides. Equal: the same id if the revised document has one, else the lowest. New: fresh `abstractNumId` and `numId`, the
   abstract numbering before the first `w:num`.
5. **What is not carried, counted in `Stats` and warned**: a hyperlink to an address that the revised part has no relationship for
   (the text stays, the link goes: the import does not call `Tx::add_relationship` yet, ST2), a picture or object whose part has
   relationships of its own (a chart, a diagram, an OLE object), a note when the revised document has no notes part, a list when it
   has no numbering part, comments, and bookmarks whose name the revised document has. Pictures and other parts without
   relationships are copied once for the same bytes, under a name the package does not have.
6. **Notes** are copied into the revised notes part under a new id as a `w:footnote` whose paragraphs the build marks deleted
   (19).
7. **Tests**: `tests_import.rs` has a fixture for each of the above.

## 19. The build and the quality check as built

`compare/prepare.rs` opens both packages (the revised one under the derived namespace) and accepts every revision. `build.rs` and
`build_edit.rs` write the plan with **one tracked compiler** on the revised state's transaction and return the open `Redline` (the
transaction and a `CompareReceipt`); `compare_states` commits it. Where this section and 2.2, 6 and 8 disagree, this one is right.

1. **The writer.** Entries are written in the order of the plan. `Compiler::tx_mut` and `Compiler::swap_track` are the two additions to
   `revisions/compiler.rs` (the import writes detached nodes and definitions between two calls, and a merge cuts the paragraph untracked and deletes
   the mark between the parts tracked). Every call is atomic: `redline_paragraph` finds what it would refuse (a locked control, a range
   that splits a field, a text node that cannot be cut) in `precheck_redline` before it writes.
2. **The fallback.** A paragraph that `precheck_redline` refuses, or whose fragments cannot be copied, is written as
   a deleted copy of the original and the revised paragraph marked inserted (`replace_whole`, warning `compare_paragraph_replaced`). If that is refused too the
   revised paragraph is kept as it is and listed as an `Exclusion` with a warning (`compare_not_redlined`). A call that fails after it has
   written something is a defect, counted in `CompareReceipt::broken` (zero on every pair of the sets below).
3. **Exclusions** are the blocks that the quality check cannot hold, each with its reason: an equation (`revisions` never writes the form Word writes for a
   changed equation), a picture or object whose part has parts of its own (charts and diagrams), a note when the revised document has
   no notes part and none could be added, a paragraph that ends a section and is inserted (rejecting leaves it empty with its break), a
   trailing inserted paragraph that holds a bookmark or a comment range (`revisions` keeps ranges when an insertion is rejected), and a paragraph that
   nests another. A pair that fails a law and has no exclusion fails the test.
4. **What the build added to the import**: a missing styles, numbering or notes part is added (empty, or with the original's separator notes)
   so that styles, lists and notes of deleted content have somewhere to go; notes with tables are copied whole; a footnote and an endnote are
   not the same mark in the comparison text.
5. **What the build added to the primitives**: `mark_deleted` (a note's paragraphs), `precheck_redline`, `precheck_split`, `ParaRedline::shifted`,
   markers in the wrapper (a paragraph of markers alone gets one, so that rejecting an insertion or accepting a deletion does not leave
   it), `_GoBack` removed from an inserted paragraph, spell-check marks removed, `xml:space` given to inserted text that needs it, text renamed
   deep inside a deleted run (ruby, fallbacks) and the trailing rule with a mark that is not copied twice.
6. **Splits and merges, calibrated.** The groups of the first build (161 over the 823 pairs) were classified by reading the text of 26 of
   128 that were valid: **16 of 26 were not splits or merges** (a new paragraph inserted next to the paired one, with words in common). Two
   rules were added: a part that alone covers 0.8 of the paragraph (`GROUP_SOLO`) is that paragraph with something next to it, and the
   replacements of the diff must leave 0.5 of the paragraph's characters alone (`GROUP_KEPT`). 29 groups are left (20 splits, 9 merges),
   all read: 24 are real (a heading and its body, a list collapsed into a sentence, a sentence cut in two), 5 are doubtful (a rewritten
   paragraph taken for a merge). A group whose parts cannot be planned (a part with no text of its own) is dissolved: its paragraphs are
   paired, deleted and inserted one by one, and a deleted block that comes after a group in the original is placed after the group.
7. **The quality check on the 823 pairs** (354 LAB, 187 `oxpt`, 200 `dox`, 82 `dcb`; each side accepted first): **802 hold N1, N2, N4 and N7 outright**, 21 fail
   a law and have an exclusion with its reason (equations, diagrams, linked pictures), 0 fail without one, **0 errors and 0 broken calls**.
   Generated: 40 same-package pairs (untracked edits through the compiler) and 40 cross pairs of the curated corpus hold the laws, and a
   document compared with itself has no revision and saves as its own bytes (N3, 80 documents), two runs give the same bytes (N6), and a
   document with revisions compares as its accepted form does (N8).
8. **Noise** (N9, `Cleanup::Readable` against `Cleanup::Minimal` over the text of each story; formatting counted apart): LAB aggregate
   **1.202** (the spike had 1.195), per-pair p99 **1.59**, maximum **2.15** (spike 1.45 and 1.84); `red_spans` over `min_spans` 0.50. Word
   `oxpt` 1.377 and `dox` 1.185 aggregate. 191 pairs exceed the gate `1.6 * min + 40` or `min_spans + 2`, 3 of them LAB pairs of different
   documents of one family that were rewritten (the noise baseline of section 21 sets the gate and lists them).
9. **Known gaps.** The import does not call `Tx::add_relationship` (ST2), which would keep the address of a hyperlink in deleted text, and the closure of a part with
   relationships (charts, diagrams) is not imported; both are counted in the receipt.

## 20. Round two: one writer, laws that compare everything, failing closed

Where this section and the sections above disagree, this one is right.

1. **One writer of the stories.** `Compiler::tx_mut` and `Compiler::swap_track` are gone. The import writes through
   `Compiler::import_writer` (`revisions/import_writer.rs`): the style sheet, the numbering part and copied pictures (never a story
   and never the comments part), `add_notes_part` for a notes part the revised document lacks (only the separators of the original's are kept), and in
   a story a `Detached` handle that can only create detached nodes and bind namespaces on the story's root. Placing what it made is
   left to `place_deleted`, `redline_paragraph` and the new `add_deleted_note` (`revisions/redline_notes.rs`), which adds a note with
   its paragraphs marked deleted and replaces `mark_deleted`. A merge cuts the paragraph with `split_block_untracked`
   (`revisions/redline_split.rs`). The other files of `revisions` changed only by the field `Effect::imported_defs` and the visibility of
   `Compiler::effects`.
2. **Foreign origins of definitions.** Every style and numbering definition the import appends and every part it copies is
   declared as an `ImportedDef` (the element or part in the redline, and the one in the original) in `Effect::imported_defs`, and
   the receipt gathers them in `Declared::definitions`, for `check` to key (`document/compare.rs` keys them with `check::unit_keys` and `check::part_keys`).
3. **Notes follow a reference that exists.** The import copies a note's paragraphs but does not add the note. After each entry the
   build adds, with `add_deleted_note`, each copied note one of whose referring copies is in the story; a note whose copies were
   abandoned (a paragraph replaced whole after its fragments were made) is forgotten and copied again if another reference needs it.
4. **Failing closed.** `place_deleted` checks before it writes (the anchor is in its container, each block is a detached block that
   fits it, the trailing paragraph is beside the anchor), and `replace_whole` places the deletion before it marks the insertion, so
   a refusal leaves the revised paragraph as it is. `build::redline` returns `internal` when a call failed after it had written, so no
   caller can commit such a transaction; `add_deleted_note` is checked with `check_deleted_note` first, so a refusal leaves the note
   out (an exclusion) and a failure after writing is broken.
   An inserted paragraph with an equation has its content marked inserted and keeps its mark (`mark_content_inserted`), so that
   the equation, which stays when the insertion is rejected, is not joined to the next paragraph; it is excluded.
5. **The laws compare everything.** An `Exclusion` lists the blocks of the redline and of the original it is about, and the quality check leaves
   exactly those out of both dumps (a table or row with what is in it); every other difference fails the pair. The dump has the
   headers and footers: N1 compares every header and footer part of the revised package, and N2 the slots both inputs have (section
   k with section k when both have as many, else the last with the last, one part with one part), which leaves out what
   `compare_story_unmatched` counts. Where both inputs have a list's definition the dump has the computed clause number. A note
   that no reference names is not in the dump (Word does not show it and a redline does not carry it), and a property listed twice
   in one container counts once. An exclusion also lists the copies placed for its blocks and the inserted paragraphs that a
   rejection joins to an excluded one, which only the rejected dump leaves out (accepting removes a copy, and the one that survives
   a trailing join is the revised paragraph).
6. **The trailing exclusion, narrowed.** The exclusion of a trailing inserted paragraph that holds a bookmark or a comment range
   fired on 134 LAB pairs where N2 held: the resolver hoists range markers and they do not keep the paragraph. It now fires only
   for a comment's reference, which rejecting does leave (a curated cross pair shows it), and no pair of the 823 has one.
7. **Fields and alternate content.** An insertion wraps a field's `separate` run too when the field does not begin and end in the
   paragraph, because the resolver takes a field whole only within one paragraph (a table of contents left its `separate`
   behind). An `mc:AlternateContent` outside a run in imported content is replaced by the branch the original reads, so that the
   deleted copy shows the original's text in runs a tracked change wraps. A copy leaves out an empty `w:rPr` or `w:pPr`, which says
   nothing and which the Word rules that `Compiler::finish` now runs count as a defect of the revision around it.
8. **Noise per paired story.** The minimal diff of N9 is taken story by story: body with body, notes with the notes of their kind,
   and a header or footer with the one of its slot.
9. **VML shape ids** (`_x0000_s<n>` in `id` and `o:spid`) of imported content are renumbered past every shape id of the revised
   document, one new id per old id within a copy.
10. **Benchmarks.** `compare_10pct_*` and `compare_25pct_*` on `contract-300p` and `dense-contract-300p` time `compare_states` in the
   testkit's benchmark harness. On the first baseline (macos-aarch64) they take about 1.0 s and 2.5 s at 10%, and 2.2 s and 4.2 s at 25% (a second run of
   the dense contract at 10% took 5 s). The target of decision 5 (1 s at 10%) is not met on the dense contract, so the rows are held
   to the baseline alone for now.

## 21. The pairs in the corpus and the noise gate, as built

Where this section and 9.2, 9.4 and 10 disagree, this one is right.

1. **The manifest.** `corpus/pairs.tsv` has one pair a line: the class, the set (`curated` or `full`), the SHA-256 of the
   original and of the revised document, a label and the outlier reason (or `-`). Documents are named by hash, because the
   corpus lists a file once under the first source that has it (a Docxodus pair can name a PowerTools file). `cargo xtask
   check` checks that each hash is a document of the manifest that must open, that the two differ, and that no pair is listed
   twice in either order. The documents of the curated pairs are fetched with the curated corpus into a set of their own
   (`curatedpairs`), so that the documents the other corpus tests sample do not change, and `cargo xtask corpus fetch --pairs`
   builds a `pairs` set of every pair's documents (785, 43 MB), so the full run does not need the whole corpus. The testkit's
   `pairs` module resolves the pairs against the set the tests read: the curated pairs on the curated set, every pair on the
   `pairs` or `full` set, and a pair whose documents are missing fails the test.
2. **The pairs.** The 823 of section 19 are reproduced from the pinned sources: 354 LAB pairs by the rule of 1.2 (two
   documents of one task directory, the directory above `documents/`, of the task directories in the `*redline*` and
   `*term-negotiation*` tasks, that share at least half of the distinct paragraphs of 20 characters or more of the smaller,
   on the accepted text; the original is the one without revisions when only one has them, else the first by path), 187
   `oxpt`, 200 `dox` and 82 `dcb`. Of those, 44 were the same pair twice: the 41 `dcb` pairs are in the benchmark twice
   (`fixtures/` and `src/dcab/fixtures/`, the same bytes), and of the `oxpt` pairs one compared a file with an identical
   copy and two were listed twice. The manifest has **779 pairs: 354 `lab`, 184 `oxpt`, 200 `dox`, 41 `dcb`**. The curated
   ones are 241: the 39 smallest LAB pairs with a table, one per task directory, and one pair that is over the gate on
   characters (80 documents, 5.0 MB; 4 of the 40 are recorded outliers), 181 `oxpt` pairs of `WC001` to `WC067`, and 20
   `dcb` pairs. They take one to three minutes in a debug build on a loaded machine, and all 779 four in a release build.
3. **The gate's factor.** 9.4 takes `C` as the p99 of the per-pair ratio times 1.10. On the Word classes the p99 of
   `red_chars / min_chars` is a ratio of a few characters to none (a text box adds 124 characters against a minimal diff of 0),
   which made `C` 180 for `oxpt`. So the p99 is taken over the pairs whose minimal diff has 40 characters or more, where the
   slack of the gate does not decide: **`C` is 1.72 for `lab` (p99 1.561), 2.93 for `oxpt` (2.659), 4.55 for `dox`
   (4.130) and 1.10 for `dcb`**, which has no text change at all (its pairs change settings, fields and links).
4. **The baseline.** `crates/stetdoc-testkit/bench/noise_baseline.tsv` has one row per set and class (pairs, the characters
   of the minimal diffs and of the redlines, the formatting changes, the pairs with exclusions, the p99 and `C`), taken on
   2026-10-06; the figures are counts, so they do not depend on the machine. The test holds each class to its row: the
   aggregate character ratio and the formatting changes may grow by 5%, and the pairs with exclusions not at all (this takes
   the place of the 5% ceiling on pairs with exclusions, which the curated pairs, rich in equations, would break at 7%). The
   test prints the rows a run would make and never writes them.
5. **Outliers.** 125 of the 779 pairs are over the gate (32 `lab`, 36 `oxpt`, 57 `dox`, none `dcb`), and each has its
   reason in the manifest. The test fails on an outlier that is not recorded and on a recorded one that is within the gate.
   The reasons were assigned by family, from the figures, the file names and the way the measure counts, not by reading every
   redline: a text box (the minimal diff reads the story's paragraphs and not a text box's, so the redline's 124 characters
   are against 0), tables (each inserted or deleted row or cell is a change of its own), notes, equations and drawings (a
   deleted and an inserted object against one edit), different documents of one family (`HW010`, `DB002`, `DB007`, `DB010`,
   `DB013b`, `RPR`), and for 30 LAB pairs many replacements (a replaced word is a deletion and an insertion, two changes
   against one edit of the minimal diff). Two LAB pairs are over on characters (2.15 and 1.91 against 1.72): rewritten turns.
   The review by a lawyer in a host application is where a person reads them.
6. **Compare's memory.** The `compare_*` rows take `Limit::TwoInputs`: twice the memory budget of the input (`ARCHITECTURE.md#performance`), 74.1 MiB on the
   contract and 128.6 MiB on the dense contract. The contract fits; the dense contract is over (140 MiB at 10%, 143 MiB at
   25%) and is a known gap in `MEMORY_GAPS`, which ends by itself when it fits.

## References

- [`ARCHITECTURE.md`](../../ARCHITECTURE.md): the `compare` row, the dependencies, "Compare" in how the pieces work together, rule 2
  (one path for edits), performance.
- [`docs/design/revisions.md`](revisions.md): `diff.rs` and the cleaning (2.5), the carve, `Effect` and origins (2.1, 2.2), moves (3.5),
  notes (3.6.4), R8 (7.2), the open item for compare (12), and what the build changed (16).
- [`docs/design/styles-and-numbering.md`](styles-and-numbering.md): clause numbers and the ghost number, the cascade accessors.
- [`docs/design/check.md`](check.md): the origins, `Declared::foreign` and `unit_keys`.
- [`docs/design/refs-and-state.md`](refs-and-state.md): `derived` namespaces, `open_in`, `add_part`.
- [`docs/design/wordml.md`](wordml.md): the text model, `Item`, `Paragraph::run_props`.
- [`docs/design/document-and-ops.md`](document-and-ops.md): `compare`, `CompareOptions`, `Receipt.moves`.
- Prior art, read for findings (MIT): OpenXmlPowerTools `WmlComparer`, Docxodus `IrBlockAligner` and `IrBlockSimilarity`.
