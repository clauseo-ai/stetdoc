# Design: `check`

Design notes for the `check` module: the validity check that runs before every commit, and the report behind `stetdoc check`. `ARCHITECTURE.md` is the map, and the contract in `check/mod.rs` is replaced by the one in section 14. This document says how the units a batch changed are found, how the no-new-violations rule is applied to them, which Word rules there are and what the corpus says about each, how the comment audit and the receipt fit in, what it costs, and what the other modules must provide. Terms follow [`GLOSSARY.md`](../../GLOSSARY.md). The schema half of the rule (keys, units, origins) belongs to `schema` and is not repeated. The designs of `revisions` ([`revisions.md`](revisions.md)) and of `projection` are quoted where this one builds on them, and the comment audit is described in [`comments-and-notes.md`](comments-and-notes.md) section 3.2.

Evidence comes from the full corpus (48,595 documents that are expected to open, fetched with `cargo xtask corpus fetch --full`), from a throwaway census program that reads every story part with a different XML parser (`roxmltree`, not kept), and from benchmarks on the five inputs of the speed and memory budgets in `ARCHITECTURE.md#performance` (release build, development Mac, other agents running). A figure moves by up to a factor of two between runs, and the smallest is quoted. **Word-saved** below means a document whose manifest `producer` is `Microsoft Word` and whose `word_fp` is `y`, which is how the corpus tests of `state` define a file that Word saved. The corpus has 4,121 of them. No Word was available, so every claim about what Word writes is a claim about files that Word, or another named producer, wrote.

## The decisions that matter most

1. **Two entry points, one rule set** (1). `check::edit(&Tx, &Declared)` is step 3 of the commit pipeline: only the parts the transaction changed, only the units that differ from the base, `validation_failed` when a finding is new. `check::document(&PartsView)` is `Document::check`: every part, every unit, every finding, and nothing blocked. Both run the same rules: the schema's keys, the Word rules, the comment audit and the package rules.
2. **The units a batch changed are found from the span tree, not from the operations' declarations** (2.2). A walk through the dirty flags finds every paragraph, table and frame the transaction changed, and an equality test against the base drops what only earlier commits dirtied. A compiler that forgets to declare a change cannot make `check` skip it. The declarations are used for one thing: saying where a new block came from.
3. **The origin of a unit is the same node in the base, plus what was declared** (2.3). The staged tree is a clone of the base, so a node id names one element in both. A merge's survivor carries the keys of both paragraphs (and of any paragraph the join removed whose elements it now holds), declared or not. A new block with no declaration has no origin, so every finding in it is new, which is the safe direction. `compare` brings origins from the other package as key sets.
4. **A table or a frame is validated only when an element of its own shell changed** (2.2). Editing text in a cell dirties the table and the body without changing either, and re-checking a 12,000-block body for it costs 4 to 100 ms. The test is a comparison of names, attributes and child ids with the base, about 1 ms.
5. **The schema already covers W4, W6 and half of W5, and `check` adds the rest** (3.1). W4, W6 and the hyperlink half of W5 are schema violations, shown by probes (`invalid_attribute_value`, `non_canonical_order`, `unexpected_child`). W1, W2, W5 (nesting), W9 and W10 are read inside a unit. W3 and W7 are read over a whole part, cheaply, and W8 over the story's field index. Three package rules (P1 to P3) cover the parts and relationships that `add_part` writes.
6. **The corpus bears the rules out, and says what they cannot be** (3.2). Of 4,121 Word-saved documents, 37 (0.9%) have a pre-existing problem under these rules, and the ones I read are test fixtures and the outputs of other tools (the accept, reject and compare outputs of OpenXmlPowerTools, LibreOffice's test data), none shown to be a problem that Word wrote. That is why every rule is a no-new-violations rule and none is absolute. Two candidates failed the evidence and are not rules: unique `w14:paraId` (duplicates in 35 of 4,415 Word-fingerprint documents) and a header or footer with a block (26 documents have an empty one).
7. **The comment audit is folded in by key** (4). `threads::audit` runs on the base and on the transaction. A new error finding blocks, a new note is reported, and what was already there is neither (it is about the comments of the document, not about the blocks the edit changed, and `Document::check` lists it).
8. **`check` supplies the validation part of the receipt, and `document` builds the receipt** (5). Revisions by author come from `revisions::changes`, which `check` may not depend on. `check` gives the receipt its `Verified` value: what was checked, the problems that were already in the blocks the edit touched, and new notes.
9. **One implementation of the Word rules, shared with the compiler** (7.1). `check::scan_unit` is what `check` runs and what the compiler's `Compiler::finish` runs on the subtrees it created, so there is no second copy of W1 to W10 to keep equal. It costs a dependency edge, `revisions` to `check`, which is decision 1 of section 11.
10. **No new error code** (7.3). A refused edit is `validation_failed`, with the first findings in the message and the operation's index when it can be told. Findings have their own stable vocabulary of codes, in `api`.
11. **The cost is small** (6). A 50-edit batch costs 3 to 13 ms to examine on the benchmark inputs plus one pass over each changed story part (5 to 60 ms), against the 500 ms the `revisions` design asked to be left for it. The whole-document check costs 0.1 to 0.5 s.

## 1. Where check sits

| Caller | Call | Scope | Blocks |
|---|---|---|---|
| `document::edit`, after `ops::apply` and before the scope guard (`ARCHITECTURE.md`, editing step 3) | `check::edit(&Tx, &Declared)` | the parts the `Tx` changed, and the units that differ from the base | yes |
| `Document::check`, `stetdoc check` | `check::document(&PartsView)` | every part that has a schema, every unit | never, it reports |
| `revisions::Compiler::finish` | `check::scan_unit` | the subtrees the transaction created | by the compiler's rule |
| `compare` | `check::edit` with `Declared::foreign` | as `edit` | yes |

`stetdoc check <dir>` is `edit(Edit::files(pending).dry_run())` and `Document::check()`. The dry run answers whether the pending edit would add anything, with the code a real edit runs, and `Document::check()` lists what was already wrong. Opening a document validates nothing, because `open` has a budget of 300 ms and `check` is on demand.

## 2. The no-new-violations rule

### 2.1 Units

The rule is applied per unit, never to a whole part (a key names a declaration and a subject, not a place, so one bad paragraph would hide a new one anywhere else). The units are the ones `schema::validate_unit` checks.

| Unit | Root | What it covers | In |
|---|---|---|---|
| paragraph | a `w:p` | its properties, runs and attributes, and everything below it except the paragraphs and tables of a text box | a story part |
| table | a `w:tbl` | its own properties, grid, rows and cells, and the order of what each cell holds, a paragraph among it | a story part |
| note | a `w:footnote`, `w:endnote` or `w:comment`, a child of the root | the element itself, except its paragraphs and tables | the notes and comments parts |
| frame | the root of the part | what is left: the root, `w:body`, block-level `w:sdt`, body-level markers. In the notes and comments parts, only the root's own attributes and the order of its children | a story part |
| top-level element | a child of the root | the element and all below it | any other part (styles, numbering, settings, `people`, the comment extension parts) |

A story part is one of `Document`, `Header`, `Footer`, `Footnotes`, `Endnotes`, `Comments` and `Glossary`. The notes and comments parts are laid out like a part that is not a story: each note or comment is a unit of its own, as each top-level element is, so that a problem in one note does not hide a new one in another, and its paragraphs and tables are units below it. In a part that is not a story the root is the frame, and the unit of an element is the top-level element that holds it.

### 2.2 Finding the units a batch changed

The transaction's parts are compared with the base's: a part whose `PartsView::generation` is the same in `tx.parts()` and in `tx.base().parts()` was not changed, one with the same epoch and another edit count is a staged clone that changed, and one with another epoch was made or rewritten by `add_part`. The parts are found through the main part's relationships (`schema::PartKind::from_relationship_type`). For each part that changed, in part order:

1. **A part whose tree is not a clone of the base's** (one that `add_part` made, such as a new `comments.xml`, found by its epoch) is validated whole by `validate_part` and the Word rules, with no origin, so every finding in it is new. A part that was just parsed has no dirty flags, and a walk would find nothing in it.
2. **Walk the staged tree** from the root with a stack, entering only elements whose `is_dirty()` is true. The flag is true for an element that was edited or has an edited element anywhere below it, so the walk follows the edits and visits the children of each dirty element and no others. It tracks whether it is inside a paragraph, a table, or neither.
   - A dirty `w:p` is a **candidate paragraph unit**. The walk goes on below it, because a text box holds paragraphs and tables of its own.
   - A dirty element whose nearest ancestor among `w:p` and `w:tbl` is a `w:tbl`, or that has none, is a **shell element** of that table, or of the frame when there is none, and so is the `w:tbl` itself. It marks that unit for validation if it **changed itself**: it is not in the base, or its name, its attributes or the list of its child ids differ from the same node of the base.
   - In a part that is not a story, and in the notes and comments parts, a dirty top-level element is a candidate unit, and the root marks the frame if it changed itself. In the notes and comments parts the walk goes on below a note, to its paragraphs and tables. The frame of such a part is validated with `validate_unit` on the root, keeping the violations found at the root or at one of its children: the order and number of the top-level elements, and the root's attributes.
3. **Drop what this transaction did not change.** Dirty flags only grow: an element edited in an earlier commit is dirty for ever, because the span tree writes against the bytes it was opened from. A candidate paragraph (or note) that is element for element the same as the same node of the base (names, attributes, text, the ids of nested blocks, and **the id of its parent**) is dropped. A table or frame whose shell did not change itself is dropped without being read. So the cost of a long session does not grow with its history.
4. **Validate what is left** with `validate_unit(ctx_at(part, tree, root), tree, root)`, on the staged tree and, for the origin, on the base tree (2.3). A unit with no context (`ctx_at` is `None`, inside an element the schema skips) is not checked, before or after. A paragraph, a run, a table, a row or a cell has its own context wherever it stands (inside a body-level `w:moveFrom`, under an element of an unknown namespace, a table inside a paragraph), so its children are checked by its own content model, by `Document::check` and by an edit alike, and a misplaced `w:pPr` there cannot pass unseen. On the curated set this adds findings to one document, `tdf111550` (a table inside a paragraph): they were in the input, so an edit carries them.

Everything above uses `NodeId` equality across two trees. That is sound because `Tx::part_mut` clones the base's tree, a clone keeps the ids of the original, and the base is never written. A part that `add_part` rewrote (the content types and relationships) is not a clone, and is the package rules' (3.3).

What a walk costs is the number of children of the dirty elements: a text edit in a 12,000-block body visits the body's children (about 0.1 ms) and not their insides. Measured, with 1,000 dirty paragraphs of which 50 changed in this batch: the walk takes 0.4 to 1.9 ms, the equality test 2 to 19 ms (10 to 19 µs a paragraph, with allocation that a real version avoids) and validating the 50 that changed 1 to 11 ms. Validating all 1,000 would take 40 to 230 ms.

An edit that moves a block changes the shells of its old and new parent, and the block's own unit is checked as well, because its parent id is not the base's: the keys of a unit do not depend on where it stands, but `mc:Ignorable` and `mc:ProcessContent` do, and a moved block is rare enough to cost one more check. The edit marks the old and the new parent and not the block, and `clone_subtree` copies the insides of a subtree clean (the paragraphs of a cloned row are new elements that no flag marks), so the walk also meets the children of a marked element that were not its children in the base, and enters a new element whole. Built and measured by `tests_units` (a moved paragraph is validated with the frame it left and the table it entered) and `tests_origins` (the paragraphs of a cloned row).

The keys of the base side of a frame, a note or a table are cached per generation of the part (`PartsView::derived` on the base's view, and on the transaction's for the after side, which commit promotes), so a run of structural batches validates a frame once for each state it passes through and not twice for each batch.

### 2.3 Origins

The keys after the edit of a unit must be a subset of the keys of its **origin**: the union of the keys of

- the same node in the base tree, if the node exists there (a node exists in the base when `base.node_ref(id).is_element()`, since the arena only grows and the base is never edited);
- every block that the edit declared the unit came from (`Declared::origins`: the block, and the blocks it was copied or built from), validated in the base. A declared block that is not a unit (a row, a cell) stands for the unit that holds it (`unit_root`);
- every unit of the base that the unit absorbed: a unit that is no longer in the document, one of whose elements now stands in this unit. That is the second paragraph of a join (`merge`, or an accepted deletion of a paragraph mark, whose survivor is the first paragraph), and a join carries the keys of both blocks whether or not the compiler declared it, through any paragraphs in between that it removed. A unit inside the unit (a table inside a paragraph, which the schema does not allow) is checked as itself and is not looked into: it brings the unit whose root held it, so a rejected split that had left such a table alone in its second paragraph carries that paragraph's keys. A block that is still there (the first half of a split) gives its keys by declaration only;
- for a block from another package (`compare`), the key set given with it (`Declared::foreign`). `check::unit_keys(parts, block)` computes one.

| Edit | Units checked | Origin |
|---|---|---|
| text, properties or markers inside a paragraph | the paragraph | itself in the base |
| split (the second half is a new element, the first keeps the element; before the text, an empty new element before the paragraph, as for an insertion) | both halves | the paragraph before the split, for both |
| Enter after a paragraph with a successor (`revisions.md`'s shape Y) | both | the original paragraph, for both |
| a paragraph inserted before another (shape X) | the new paragraph | its twin, declared |
| merge, or an accepted paragraph-mark deletion (the compiler joins into the first element) | the survivor | the first paragraph and every paragraph it absorbed, union, declared or not |
| a row inserted | the table; each paragraph of the new cells | the table in the base; the anchor row's paragraphs, declared per paragraph |
| a block moved, or removed | the parents' shells | the parents in the base |
| a new part, or a new top-level element in a part that is not a story | everything new | none |
| a cell paragraph copied from another cell | the new paragraph | the paragraph it was copied from, declared |
| a tracked move, or a run copied into another paragraph | the receiving paragraph | itself in the base, and the paragraph the content came from, declared |
| a block that `compare` imports from the original | the block | its key set in the other package (`foreign`) |
| a style or numbering definition that `compare`'s import appends to a part that is not a story | the element, which is its own unit | the key set of its element in the other package (`foreign`, by `unit_keys`) |
| a part that `compare` copies whole (a notes part, a style sheet the redline lacked) | the part | the key set of the whole part in the other package (`Declared::parts`, by `part_keys`): its schema keys, and for a story its Word rules and census |

A row is not a unit, and nothing is declared for it. **Every unit that receives content copied from another unit declares that unit as an origin** (ask RV1 of section 10): the second half of a split, a twin, each paragraph of a cloned row, the target of a tracked move, a run copied into another paragraph, and the blocks `compare` imports. Without the declaration, a copy of text that already had a problem would be refused for it, and the rule is that an edit is never refused for a problem that was in the input.

**A key survives a rename.** `revisions` renames `w:t` to `w:delText` and `w:instrText` to `w:delInstrText` when it deletes a run, by making a new element that has the old one's attributes and text, and back when it rejects a deletion. The schema key of a problem names the declaration of the element (`w:CT_Text/w:t`), so a problem that `w:t` had (python-docx's `w:space`, in 1,274 of 41,688 lab documents) would be a new key on `w:delText`, and every tracked deletion in those documents would be refused. `keys.rs` names such an element by the name it had before the rename, in the keys of the schema and in the key of W2, and has one table of the renames to extend when `revisions` lists another. `tests_edit` renames an element that has a problem and one that has none, and shows the first refused without the table.

**A property container is keyed by its name.** The compiler also copies a container from one place to another: a paragraph mark's `w:rPr` onto a new run (`w:CT_ParaRPr` to `w:CT_RPr`), and a paragraph's `w:pPr` into the snapshot of a `w:pPrChange` (`w:CT_PPr` to `w:CT_PPrExtended`). Keyed by its declaration, a copy of a repeated or out-of-order child would be a new key, and that refused about 1% of sampled documents (`tdf91095`, pandoc `trim_last_inline`, docxplate `issue.48`, officeimo Hamlet). So the key of a violation in a `w:rPr` or a `w:pPr` names the element and not its declaration, and the copy is carried. Another property twice is still a new key, because its subject differs. `tests_props_keys` shows both.

**Missing declarations fail safe.** A unit with no origin treats every finding as new, so a compiler that forgets `origins` is refused on documents that already had a problem and passes on clean ones. Law V6 (9) runs edits on exactly those documents, which is how a missing declaration is found.

### 2.4 Comparing, refusing, and what a refusal says

A key is a `schema::ViolationKey` for a schema violation, and a `WordKey { code, subject }` for a Word rule or a package rule (3.1 gives the subject of each). A set holds both kinds. `added = after.added_since(&before)` is the test, and counts play no part, for the reason in `08-schema.md` 5.2: splitting a run clones its properties, and counting would refuse the clone of a problem the run already had. One kind of key is counted: `non_canonical_order`, a child of a property container out of the order Word writes. Word reads any order, but the Open XML SDK validator counts every such `w:rPr` as a schema error, so a copy of one is a new error to it. The compiler never copies one (`revisions::canonical` writes every property container a transaction created in schema order, and leaves the ones it did not create as they were), so a unit with more of such a key than its origin has an instance the edit added, and the instances past the origin's count are new (`Key::is_counted`). Origins add their counts when a unit has several.

- **Located.** The findings of the unit's `Report` whose key is in `added`, in document order, with the part, the block (the paragraph or table that is the unit, as a `BlockKey`, turned into a ref with `PartsView::ref_of`, or none for a new block) and a sentence.
- **Refused.** If anything is added, in any unit of any part, `edit` returns `validation_failed` and the transaction is discarded by `document`. The message names the first three findings with their refs, then the number of the others: `The edit would add 4 violations. p42: w:rPr has w:b before w:ins. p57: w:delText has no w:del around it. And 2 more.` `document` finds the operation by asking which `Effect` touched or created the block first, and attaches `at_op`.
- **Carried.** A key in both sets is a problem that was already in the unit. `Verified::carried` lists up to 10 of them, located, with the total, so that the receipt can say that the block the agent edited already had a problem before it did.
- **A refusal is nearly always a bug in stetdoc.** The compiler is the only writer, and it writes the order, the attributes and the revision shapes itself. The check is what keeps the validity guarantee (no new schema or Word-rule violation) when it is wrong.

Nothing is read in units that were not changed, in parts the transaction did not change, or in parts with no schema (custom XML, drawings, charts, media). A `CheckReport` counts those parts and does not list them.

## 3. The Word rules

Section 7.3 of `revisions.md` lists W1 to W11 as the rules a revision must follow to open in Word without a repair prompt and that the schema does not say. This section gives each a code and a place to run, with what the corpus says. W11 (a no-op `*PrChange` created by this transaction) is for `finish` alone, which is the only code that knows what the transaction created.

### 3.1 The rules

**Where a rule is read.** A rule is read inside a unit, or over a whole part, or with a reader that already exists.

- **Unit scan.** `check::scan_unit` walks the elements of one unit with `schema::walk_unit` (7.4), which is the walk the validator makes: it stops at nested paragraphs and tables, skips the untaken branch of an `mc:AlternateContent`, and never enters an element the schema skips. A rule therefore cannot see something Word does not read, and the schema and the Word rules cover the same elements. One pass applies every unit rule, because each is a test on an element and its ancestors up to the paragraph.
- **Census.** One walk over a story part, cached per part generation, collects sorted vectors of ids (four bytes an id) and compares starts and ends by a merge. It skips untaken branches too: 3 Word-saved documents hold 18 revision elements inside `mc:Fallback` copies of text boxes.
- **Schema.** Already a key.

| Rule | Code | Read in | Says | Key |
|---|---|---|---|---|
| W1 | `text_in_deletion`, `deltext_outside_deletion` | unit scan | Within one paragraph, `w:t` and `w:instrText` have no `w:del` ancestor, and `w:delText` and `w:delInstrText` have one. Text moved away stays `w:t`: `RA001-Tracked-Revisions-02`, a Word file, has it, and `wordml`'s `TextInDeletion` anomaly agrees, raised only for a `w:t` in a `w:del` | code, element |
| W2 | `edge_space_without_preserve` | unit scan | A `w:t`, `w:delText`, `w:instrText` or `w:delInstrText` whose text starts or ends with a space, tab, line feed or carriage return has `xml:space="preserve"`. `revisions.md`'s list names the first two. Only 1 of 4,121 Word-saved documents lacks it on the other two | code, element, as it was before the compiler renamed it (`delText` is keyed as `t`) |
| W3 | `revision_id_duplicate`, `bookmark_id_duplicate` | census | Revision ids are unique across the story parts. Bookmark ids are unique within a part, because Word repeats them across parts (1 of 4,121). A revision and a bookmark may share a number. Comment ids are the audit's. The `*PrChange` snapshots are not read, and neither are `styles.xml` and `numbering.xml`, which repeat ids | code, `id` |
| W4 | schema | schema | `invalid_attribute_value` for an author over 255 characters or a date that is not `xsd:dateTime`, `missing_attribute` for a missing author or id | schema key |
| W5 | `revision_nested_in_itself` | unit scan | Within one paragraph, no run-level `w:ins` inside a `w:ins` and no `w:del` inside a `w:del`. A `w:hyperlink` or `w:fldSimple` inside a `w:ins` is the schema's `unexpected_child` | code, element |
| W6 | schema | schema | The revision children come first in a mark's `w:rPr`, `ins` before `del`, `w:rPrChange` last, `w:pPrChange` last in `w:pPr`: `non_canonical_order`, `out_of_order` | schema key |
| W7 | `bookmark_unpaired`, `move_range_unpaired`, `move_name_unpaired` | census | A start without an end or an end without a start by id, per part. A move name with one side only. Comment ranges are the audit's | code, id or name |
| W8 | `field_split_by_deletion` | field index | The `begin`, `separate` and `end` of one field (`wordml::FieldInfo`) are all in a `w:del` of their own paragraph or none is. `fldSimple` has one piece and is not read. A field belongs to the unit that holds its `begin`, so one old split field cannot hide a new one in another paragraph: a cut field that is not in the base, or in a paragraph that a declared origin names, is new, although the paragraph of its `begin` may be one the edit did not change. The part is scanned again for the fields, because the story that a transaction would give is refreshed from the nodes the compiler declared it touched, and a declaration is not what `check` trusts. A part with no `w:fldChar` (a count the census keeps) is not scanned. The instruction text is not compared: a field code whose first part one author deleted and whose `\* MERGEFORMAT` was kept is something Word saves (`RP043`, `RP044`, `tdf124472`), with the three characters of the field all deleted, so a reading that compared each `w:instrText` with the `begin` would call Word's own files wrong | code, the paragraph of the `begin` |
| W9 | `cell_without_final_paragraph`, `table_without_row`, `row_without_cell`, `container_without_block` | unit scan | The last block of a `w:tc`, looking through a block-level `w:sdt` or `w:customXml` and ignoring range markers, is a `w:p`. A `w:tbl` holds a `w:tr`, a `w:tr` a `w:tc`, and a `w:footnote`, `w:endnote` or `w:comment` a paragraph, table or `w:sdt`. The schema accepts all of these when empty (probes) | code, element |
| W10 | `empty_revision`, `empty_properties` | unit scan | A run-level `w:ins`, `w:del`, `w:moveFrom` or `w:moveTo` with no child element, and a `w:rPr` or `w:pPr` with none, except a snapshot inside a `*PrChange`, which Word writes empty | code, element |
| W11 | | `finish` | | |

W5, W6 and W4 were probed against the validator on synthetic paragraphs: an `ins` after `b` in a mark's `rPr`, an `rPrChange` first, a `del` before an `ins`, a `pPrChange` before the `rPr`, a hyperlink in an `ins`, an author of 300 characters, a date of `yesterday`, and a missing author or id are all reported. A table with no row, a cell with only `w:tcPr`, a cell ending in a table, a row with no cell, a `w:footnote` or `w:comment` or `w:hdr` with no block, a body with no block, an `ins` inside an `ins`, a `delText` in a plain run, a `w:t` in a `w:del`, a bookmark with no end and two `ins` with one id are all accepted by the schema, and are what the rules above add.

**W10 is a rule of hygiene, not a promise about Word.** Word reads an empty `w:rPr` and an empty `w:ins`. The rule is there because an edit that leaves one has written bytes it did not need, and the compiler promises it never does. It is checked everywhere because it costs nothing, and being a no-new-violations rule it never blocks an edit for an empty property that was already there.

### 3.2 What the corpus says

The census applies the rules as defined above to every story part of every document expected to open (48,595 documents, 4,121 of them Word-saved) and counts the documents that violate each. A document with a rule broken once counts once. The figures are those of `check::document`, which `tests_corpus.rs` asserts on the full set (`STETDOC_CORPUS_SET=full`). A throwaway census program, which read every part with another XML parser, gave the first version of them, and the two agree on every rule and every document but the four listed after the table, and for W8, where the program read a stricter rule (its row says which).

| Rule | Word-saved, of 4,121 | All, of 48,595 | The Word-saved ones |
|---|---|---|---|
| W1 | 1 | 4 | a LibreOffice test file with a `delText` outside a deletion |
| W2 | 9 | 80 | test files of docx4j, docxbox, LibreOffice, pandoc, POI, the SDK and OpenXmlPowerTools |
| W3 | 2 | 24 | a docx-rs fixture (a revision id twice) and a sablon fixture (a bookmark id twice in a part) |
| W5 | 0 | 0 | none in any document |
| W7 | 8 | 35 | OpenXmlPowerTools outputs `RP015`, `RP018`, `RP023`, `RP024`, and two LibreOffice test files |
| W8 | 0 | 1 | `tdf115212` (LibreOffice), which Word did not save. The weaker rule, `begin` against `end`, is broken by 0 of 4,121. The census program read a stricter rule, which also compares each `w:instrText` and `w:delInstrText` of the field code with the `begin`, and found 4 documents, 3 of them Word-saved: `RP043` and `RP044` (OpenXmlPowerTools) and `tdf124472` (LibreOffice), which is why the rule does not read the instruction text |
| W9 | 6 | 13 | `RP001-*-Accepted` (a table with no row), `WC063` and `WC064` (WmlComparer outputs, a note with no block), `tdf153891` |
| W10 | 15 | 679 | OpenXmlPowerTools and LibreOffice outputs, and the SDK's `5Errors.docx`. None has an empty `w:ins` or `w:del` (41 documents of the lab tier do) |

Any of W1 to W3, W5 and W7 to W10: 37 of 4,121 Word-saved documents (0.9%), 753 of 48,595 (1.5%), and 25 (0.6%) and 147 (0.3%) without W10.

The census program read the parts by name and every element, and `check` reads the parts the main part's relationships name and the elements Word reads. They differ in four documents. `tdf104713` has its main part at `word/trial.xml`, and `check` finds 2 tables with no row and 8 empty property elements there (one more document for W9 and W10). `tdf168070_2` has a `footnotes` relationship to `word/footnotes.xml`, a part that is not there, and the footnotes it holds are in an unrelated part named `word/footnotes1.xml` that `check` does not read (3 empty `w:rPr` that the program counted). `tdf135144` and `tdf91095` hold their empty `w:rPr`, and for `tdf135144` two `w:instrText` with a space at the edge, only in the `mc:Fallback` of an `mc:AlternateContent` whose `mc:Choice` is taken, which Word does not read (one fewer document for W2 and W10; `tdf91095` has other empty elements and loses none). The documents behind each Word-saved count above are test fixtures and tool outputs (the accept, reject and compare outputs of OpenXmlPowerTools, LibreOffice's test data, the fixtures of other libraries and of the SDK). I read the whole list for W1, W3, W7, W8 and W9 and part of the lists for W2 and W10, and none is shown to be a file in which Word wrote the problem. Three observations shaped the rules.

- **Rules cannot be absolute.** One document in a hundred already breaks one, so pre-existing problems must be reported and never blamed on an edit. That is the premise of the validity guarantee, and these numbers are its evidence.
- **W9 must look through a block-level content control.** 33 Word-saved documents (132 cells) have a cell whose last child is not a `w:p`: 131 of the cells end with a block-level `w:sdt` (32 documents, Office's own templates among them) and 1 with a table. With the look-through, 1 document breaks the rule. No cell of a Word-saved document ends with a range marker, so ignoring markers (which the rule does, to be safe) changes none of these figures.
- **W3 and W7 skip the untaken branch.** 3 Word-saved documents hold 18 revision elements inside `mc:Fallback` copies of text boxes, which Word does not read.

### 3.3 Package rules

`add_part` is atomic and registers what it adds, so these three cannot fail unless it has a bug. They are cheap, they run only when the transaction called `add_part`, and they are the defence of L6 for the parts that `comments` creates.

| Code | Says | Word-saved, of 4,121 | Tier `word-authored`, of 4,403 |
|---|---|---|---|
| `part_without_content_type` | every part has an `Override`, or a `Default` for its extension | 33 (mostly LibreOffice test files, and `missing_content_type.docx`) | 32 |
| `relationship_target_missing` | an internal relationship names a part of the package | 20 (all of tier `edge`) | 0 |
| `relationship_id_duplicate` | the ids of one relationships part are unique | 1 | 1 |

Keys are the code and the part or relationship id. P1 reads `[Content_Types].xml` as a tree (an `Override` for the part's name, or a `Default` for its extension, case-insensitively), P2 and P3 read `PartsView::rels` of each part the main part names, and the rules compare the transaction's findings with the base's: a finding the base had is not new, and is not listed in the receipt of an edit that is about something else. `document` does not run them: they defend `add_part`, and a report of the 33 Word-saved documents that lack a content type would be a finding about the package that no edit of stetdoc can cause or repair. A third check was tried and dropped: an `Override` for a part that is not there. No edit removes a part, so none can add one (51 documents have it, all of tier `edge`).

### 3.4 What is not a rule

- **Unique `w14:paraId`.** 35 of 4,415 Word-fingerprint documents (131 ids) and 19 of 4,121 Word-saved ones have duplicates in the taken branch, among them outputs of Aspose's reporting engine and of docx-templates. It is not something Word enforces. The compiler mints ids and tests uniqueness as a law of its own. The audit does report the cases that break threads: a comment paragraph and another paragraph with one id.
- **A header, footer or body with no block.** 26 documents have an empty `w:hdr` or `w:ftr` (3 Word-saved, and the fixtures of docx-js and docx-rs among the rest), and 12 have a `w:body` with no block, python-docx's default template among them. A rule that called the normal output of those libraries a violation would be a guess about Word, so W9 does not name them.
- **`w14` in `mc:Ignorable`.** Not a rule here. The span tree adds the `mc:Ignorable` tokens that new content needs, and the schema reports an unknown namespace that is not ignorable. Whether Word 2010 and later need the token for `w14` is item 4 of the Word checklist in `comments-and-notes.md` section 13.
- **Dangling references**: a `w:pStyle` or `w:numId` that names nothing, an `r:id` with no relationship, a note reference with no note, a bookmark referred to by no field. Word repairs most of them silently, and `projection` already warns about the ones that change what a reader sees (`unreferenced_note`, `numbering_approximate`). They are a candidate for notes after the first release, and are not counted in the figures here.

### 3.5 Severity

A finding is a **violation** or a **note**. Every rule above and every schema key is a violation, and so is an error of the audit, except `empty_properties`. A note is legal and worth saying: the audit's notes (`unanchored`, `row_missing`, `para_id_missing`, `para_id_range`, `parent_missing`), `part_unreadable` (an encrypted part, or one stored with a method other than stored or deflate, which `Document::check` cannot read and so does not validate), and `empty_properties`. The schema allows an empty `w:rPr` or `w:pPr`: every child of `CT_RPr` and `CT_PPr` is optional, and the schema half of the check reports nothing for one (only W10 did). 57 documents of the corpus have them (`check/expected_findings.tsv`), and counting them as violations made the valid outputs of other tools look invalid. A new violation blocks an edit. A new note goes to `Verified::notes` and the receipt's warnings and blocks nothing, and a note the input had is not carried, since it is no problem (`Verdict::add_new` and `Verdict::carry` route a finding by its code's class). The corpus shows which rules Word-saved files keep, not which ones Word enforces, and no Word is available to try (risk 1).

## 4. The comment audit

`threads::audit(parts)` returns findings with a stable code, a subject (a comment id, a `paraId` or a part) and a class, error or note (`comments-and-notes.md` 3.2). `check` folds it in by key:

- **When.** `edit` runs it on `tx.base().parts()` and on `tx.parts()` when the transaction changed a story part or a comment part or added a part, and when either side has a comments part (without one it is a relationship lookup). `Document::check` runs it once.
- **Keys.** The code, the comment the finding is about and the part. `Finding` has no subject string, and its message carries numbers that move when a row is added. A key in the after set and not in the before set is new. A new error is a violation and blocks like any other. A new note goes to `Verified::notes`. A key in both is neither new nor carried: the audit speaks of the comments of the document and not of the blocks the edit changed, so listing it in the receipt of every edit would be noise, and `Document::check` lists every finding. The package rules (3.3) are the same.
- **One reader.** The comment halves of W3 and W7 are the audit's `comment_id_duplicate`, `marker_unpaired`, `marker_without_comment`, `reference_missing` and `reference_duplicate`. The census does not read comment markers, so there is one reader of comment anchors.
- **Cost and caching.** The audit reads the comment parts and walks the anchors of every story (9.5 ms at 13 MB, measured as `Story::anchors`). Each side is cached by `PartsView::derived` on the generations of the parts it reads, and the after set of one batch is the before set of the next.

The law C3 of `comments-and-notes.md` (after any batch of comment operations the audit has no finding the input lacked) is then a statement about the verdicts of `check::edit`, and C12 (the output validates) is `Document::check` of the output.

## 5. The receipt

`stetdoc check` validates and prints a receipt: every revision about to be written, by author. `GLOSSARY.md` calls the receipt of a check the revisions that `pack` would write. The shape belongs to `api`, and this is what `check` supplies.

```rust
pub struct Receipt {
    pub base: BaseToken,                  // the new base token; the unchanged one for a dry run
    pub dry_run: bool,
    pub operations: Vec<OpResult>,        // one per operation: index, status, the blocks it touched, notes
    pub revisions: Vec<AuthorRevisions>,  // what the batch wrote, by author
    pub comments: Vec<CommentEffect>,     // action, c3, thread, author, anchored block, warnings (comments-and-notes 7)
    pub blocks: Vec<RenderedBlock>,       // the post-edit text of every block touched or added (projection::render_blocks)
    pub warnings: Vec<Warning>,           // projection's codes, the notes of `check`, deduplicated
    pub validation: Validation,           // the part check supplies
}
pub struct AuthorRevisions { pub author: String, pub counts: Vec<(ChangeKind, u32)>, pub changes: Vec<ChangeEntry> }
pub struct ChangeEntry { pub kind: ChangeKind, pub ids: Vec<i64>, pub date: String, pub block: Option<String> }  // as written; the ref, or none for a new block
pub struct Validation { pub parts: u32, pub units: u32, pub carried: Vec<Finding>, pub carried_total: u32 }
```

| Part | Built by | From |
|---|---|---|
| `revisions` | `document` | `revisions::changes(story, tree, blocks)` over the touched and new blocks of the staged state, kept to the changes the compiler's `Outcome::created` lists (the stamp's author, a minted id, not the new id of a split), with `removed_own`, `folded` and `swallowed` shown per operation (D's H2) |
| `comments` | `ops` | the `CommentEffect` of each comment operation |
| `blocks`, block `warnings` | `document` | `projection::render_blocks(parts, base, blocks, spec)` over `tx.parts()`, so a dry run shows the staged text and a new block has no ref |
| notes, `validation` | `document` | `Verified` from `check::edit` |

A refused batch has no receipt. The error carries the findings (2.4). For `stetdoc check <docx>` with nothing pending the receipt is empty, and `document` adds the revisions already in the document by author (`Document::revisions`, `projection`'s `doc_facts`) beside the `CheckReport`. `check` builds no revision list because it may not depend on `revisions`, and a revision list built from `wordml::part_revisions` would count elements where agents think in changes (`revisions.md` 6.1).

The exit status of `stetdoc check` is 0 when the pending edit would add nothing, whatever the document already holds; 5 when it would; 6 when the input cannot be read. A `--strict` flag makes any violation, new or old, exit 5, for use in a pipeline.

## 6. Performance

The budget is 1 s for 50 operations including validation of the dirty units (`ARCHITECTURE.md`), and the `revisions` design asked for more than 500 ms of it to be left for `check`. Measured in a release build on the five benchmark inputs of `ARCHITECTURE.md#performance`, in milliseconds:

| Input | Walk, 50 edits | 50 units and the frame, before and after | Of that, the frame | 1,000 dirty, 50 changed: walk, equality test and validation | One pass over the part (scan, anchors, ids and a Word pass) |
|---|---|---|---|---|---|
| `contract-300p` | 0.1 | 11 | 4 | 14 | 9 |
| `dense-contract-300p` | 0.4 | 21 | 10 | 32 | 34 |
| `worst-blocks-12mb` | 0.5 | 19 | 14 | 10 | 14 |
| `node-dense-18mb` | 1.0 | 39 | 40 to 100 | 7 | 58 |
| `fields-13mb` | 1.1 | 29 | 28 | 5 | 21 |

- **A text batch.** Nothing needs the frame or a table, so the cost is the walk, the shell comparison (0.1 to 1.2 ms) and the paragraphs: about 3 to 13 ms. `schema_units` in the baseline (50 paragraphs, one validation each) is 0.3 to 1.6 ms.
- **A structural batch** validates the frame or table that changed: 4 to 100 ms, before and after, for the frame of a body of up to 18 MB, and about 20 µs a row for a table (22 rows take 0.47 ms), so a 1,000-row table takes 20 ms.
- **The census** of a changed story part is one pass: 4.0 ms for the ids and 9.5 for `Story::anchors` at 13 MB, and the census is one walk of that size. `check` reads the base's census from the cache and builds the after one, which the next batch reuses.
- **The total** for a structural batch of 50 on the largest input is about 100 ms for the frame, 10 for the units, 60 for the census and 10 for the audit: under 200 ms, against 500 ms. A text batch skips the frame and costs under 100 ms.
- **`Document::check`** is `schema_part` (77 to 378 ms in the baseline) plus a Word scan and the census (10 to 60 ms): 0.1 to 0.5 s. It is cached per generation of the parts it read.
- **As built.** The benchmarks `check_edit` (fifty text edits in blocks spread through the main part, checked once), `check_frame` (the same and a paragraph inserted in the body, so that the frame is validated before and after) and `check_document` (`Document::check`) give, in milliseconds on the five inputs in the order of the table: `check_edit` 23, 69, 29, 114 and 57; `check_frame` 28, 81, 47, 165 and 92; `check_document` 100, 276, 118, 392 and 165. They are the best of several runs of a release build on a machine that was in use. The transaction in them is never committed, so each run builds the census of the base as well as the census after the edit; in a session the commit promotes the census after to the next base, and an edit costs one pass less. Most of `check_edit` is that census, a pass over the whole changed part with `schema::walk_unit`, which follows what Word reads (the branch of an `mc:AlternateContent` that is taken, no ignorable element). A first version of the walk took 0.16 to 0.9 s on the same benchmark, and a profile put 60% of that in the namespace and name tables, which `skip_of` consulted for every element, twice. It now asks the tables only for an element that an ancestor or the element itself makes ignorable, `walk_unit` collects the attributes of an element only if it has one in the markup compatibility namespace, and a parent's children are read through `effective_children` without a work list for the usual child. `schema_part` went from 378 to 263 ms on the largest input with it. The census could be refreshed from the base's census and the dirty flags instead of built by a pass, which would bring a text batch on the largest input to about 10 ms. That is later work, and it is exact only if a test compares it with a pass on every edit of V4 to V8.

Memory is the vectors of ids (four bytes per id-bearing element, at most 20 MB for the 5,000,000 nodes the limits allow), the key sets, which are bounded by the number of distinct keys, and the findings, which are capped. Nothing is copied from a tree.

## 7. The interface

### 7.1 Rust

```rust
// check/mod.rs
pub(crate) fn edit(tx: &Tx<'_>, declared: &Declared) -> Result<Verified, StetError>;
pub(crate) fn document(parts: &PartsView<'_>) -> CheckReport;                 // infallible, as ARCHITECTURE says
pub(crate) fn scan_unit(tree: &SpanTree, root: NodeId) -> Vec<Found>;         // the Word rules of one unit (scan_part: of a whole part)
pub(crate) fn unit_keys(parts: &PartsView<'_>, block: &BlockKey) -> Result<KeySet, StetError>;  // for compare's foreign origins

pub(crate) struct Declared {
    pub origins: Vec<Origin>,                    // from the Effects of the compiler calls, converted by `ops`
    pub foreign: Vec<(BlockKey, KeySet)>,        // compare only: origins in another package (a block, or an element of a part that is not a story)
    pub parts: Vec<(Arc<str>, KeySet)>,          // compare only: parts copied whole from another package (part_keys)
}
pub(crate) struct Origin { pub block: BlockKey, pub from: Vec<BlockKey> }   // the shape of revisions' Origin
pub(crate) struct Verified {
    pub checked: Checked,                        // parts, dirty units met, dropped as the base's, shells skipped, units validated
    pub carried: Vec<Finding>, pub carried_total: usize,
    pub notes: Vec<Finding>,
    pub changed: Vec<BlockKey>,                  // the paragraphs and tables found changed, for the scope guard's second pass
}
pub(crate) struct Found { pub key: Key, pub code: FindingCode, pub node: Option<NodeId>, pub message: String }  // `node` lets the compiler keep the ones in its created subtrees
```

The errors `edit` returns are `validation_failed`, and `internal` for a `Declared` that names a block that is not in the base, which is a bug of the converter. `document` cannot fail, because a part it cannot read becomes a note (3.5). It lists the parts in the order of the main part's relationships, and in each the schema's findings, then the unit rules' in document order, then the census's and the field rule's, and the comment audit's after the parts, and never iterates a hash map, so the same bytes give the same report (V1).

`Origin` repeats the shape of `revisions`' type, field for field, because `check` may not depend on `revisions`. `document` converts with a map of three lines (`ops` may not depend on `check`). The alternative was to record origins on the `Tx` like `touch` and `merge`, so that `check(&State, &Tx)` keeps its signature. It would put a validity concept in `state`, and `compare`'s foreign origins are key sets that `state` cannot name.

### 7.2 Public types

`Document::check` is public, so its types are `api` types that derive `serde` and `schemars` like the rest, and a code is stable once released, as an error code is.

```rust
pub struct CheckReport {
    pub ok: bool,                        // no violation, notes allowed
    pub violations: usize, pub notes: usize,
    pub findings: Vec<Finding>,          // violations first; at most 200
    pub by_code: Vec<(FindingCode, usize)>,   // every code that occurred, complete
    pub parts: usize,                    // parts examined
    pub unchecked: usize,                // parts with no schema, or unreadable
}
pub struct Finding { pub code: FindingCode, pub class: Class, pub part: String, pub block: Option<String>, pub message: String }
pub enum Class { Violation, Note }
pub enum FindingCode { /* schema_<kind> (12), the Word and package codes of 3.1 and 3.3, the audit codes of threads, part_unreadable */ }
```

Schema codes are `schema_` and the name of the `ViolationKind` (`schema_out_of_order`, `schema_invalid_attribute_value`), and the audit's codes are those of `threads`, unchanged. The wire names and a sentence for each live in one `finding_codes!` table, as `error_codes!` does for errors, and `cargo xtask docs` generates `docs/reference/findings.md` from it with a drift check (ask H3).

### 7.3 Errors

`check` adds no error code. `validation_failed` (exit 5) is the refusal of 2.4, with the default hint ("Change the edit so it does not add the violation the message describes"), and `at_op` set by `document`. `internal` is a `Declared` that does not match the base. A part that cannot be read is a note, because `Document::check` has no `Result`, and a staged part always parsed when it was staged.

### 7.4 What schema adds

Three small things, owned by `schema`, so that `check` holds no copy of what the validator knows:

- **`walk_unit(tree, root, stop_at_boundaries, visit)`** calls `visit(node, depth)` for each element of a unit in document order, using `effective_children` (4.3 of `08-schema.md`), so mc wrappers, untaken branches and skipped elements are read in one place. The Word rules and the census are written on it.
- **`is_unit_boundary(tree, node)`** exposes the test of `unit.rs` (`w:p`, `w:tbl`, in either namespace).
- **`in_taken_branch(tree, node)`** is true unless an ancestor is a `mc:Choice` that is not taken or a `mc:Fallback` whose `mc:AlternateContent` has a branch that is. `ctx_at` gives a node in an untaken branch the context it would have if Word read it, because the context is a fact about structure, and its documentation says so. A caller that must not edit what Word does not read asks `in_taken_branch` first. D's compiler needs it only to be sure: the text model it edits through reads the taken branch alone.

## 8. Module layout

```text
check/       mod.rs          //! contract, the entry points                                             ~90
             keys.rs         Key, KeySet (added_since, union), schema_code, the renames                ~170
             found.rs        Found: a key, a code, a node and a sentence                                ~45
             units.rs        the walk, the shell test, the equality test, the unit list of a part      ~330
             examine.rs      the keys and the findings of one unit                                      ~95
             origins.rs      Declared, Origin, the origin keys of a unit, unit_keys, the base cache     ~300
             rules/mod.rs    scan_unit, scan_part: one walk, the hooks                                  ~150
             rules/text.rs   W1, W2                                                                      ~65
             rules/revs.rs   W5, W10                                                                     ~55
             rules/tables.rs W9                                                                         ~110
             census.rs       per part: W3, W7, one walk, sorted vectors, cached per generation         ~270
             fields.rs       W8 over a fresh scan of the staged story                                  ~135
             package.rs      P1 to P3, over the content types and the relationships                    ~110
             audit.rs        the adapter over threads::audit, cached per generation                    ~165
             edit.rs         edit: parts, units, census, the order of the rules                        ~275
             verdict.rs      Checked, Verified, the refusal and its message                            ~215
             document.rs     document: every part, the report                                          ~250
             report.rs       the sentences of the findings, the report builder                         ~210
             tests_*.rs      one file per family                                                       each under 500
```

Every file is under 600 lines, and `mod.rs` opens with its contract. `FindingCode`, `Finding`, `Class` and `CheckReport` are in `api/check.rs` (with the definitions here). `check` depends on `api`, `package` (its tests add a part with `Tx::add_part`, which takes a `package::NewPart`), `xml`, `schema`, `wordml`, `state` and `threads`.

## 9. Tests

The corpus is the testkit's (`STETDOC_CORPUS_DIR`, curated in `cargo xtask check`, the full set nightly). The tests that read it are added to `xtask/core-io-exceptions.txt`, as the schema simulation's are.

| Law | Statement |
|---|---|
| V1 Total | `document` never panics, never changes a byte (no part changes generation or becomes dirty), and gives the same report twice (for documents of up to 400 KB: a debug build is slow, and a report does not depend on size), on every document of the corpus and on every hostile one that opens |
| V2 Census | the findings of `document` on the corpus equal a ledger: the counts per tier and code of 3.2 for the full set, and per document for the curated set (`expected_findings.tsv`, read with `include_str!`). A document is clean, or its findings are in the ledger. A new code, a new document or a count that moved fails the test, and the ledger is changed by hand after the finding is read |
| V3 Agreement | for every corpus paragraph, `check`'s W1 and W2 findings include every `TextInDeletion`, `DelTextOutsideDeletion` and `EdgeSpaceWithoutPreserve` anomaly of `wordml`'s text map, and the census's unpaired bookmarks equal the unpaired ones of `Story::anchors`. Two readers written apart agree, and each difference is listed with a reason (`Story::anchors` does not enter text boxes) |
| V4 Accepted | edits that add nothing are accepted. For two paragraphs of each story part that have a finding and two spread through it, four edits are made through `Editor` on a `Tx`: the text of a run is replaced, a run is inserted where `insert_in_order` puts it, a paragraph is inserted next to it (before it in the comments part: a paragraph after a comment's last paragraph would make the one its extension rows name no longer the last, which the audit rightly refuses), and an attribute is set. On every curated document that opens, those with violations included. Zero refusals, and a count of the edits that met a problem already there (344 of 2,477 on the curated set) |
| V5 Injected, refused | for each code, a mutation that introduces exactly that finding into a clean unit is refused with that code, and the same mutation in a unit that already has it is accepted (a table of fixtures) |
| V6 Origins | split, clone and merge through `Editor` with declared origins, on documents that have violations: accepted. The same without the declaration: refused exactly when the origin had one, except a merge, whose survivor absorbs the removed second paragraph and carries its keys |
| V7 Declarations are not trusted | V4, V5 and V6 never call `Tx::touch`, so a change that is declared nowhere is still found |
| V8 The shortcuts are exact | with the shell test and the equality test turned off, `edit` gives the same verdict on every case of V4 to V6 |
| V9 Accumulation | after 20 committed batches, a batch costs what its own units cost and not what the 20 left dirty. The test counts instead of timing: the 20 marked paragraphs are met and dropped by the equality test, and one unit is validated (`Checked`: 21 dirty, 20 equal, 1 validated). A count is the cause of the time and does not depend on the machine |
| V10 Audit | C3 of `comments-and-notes.md` holds as verdicts of `check::edit` |
| V11 Through `Document` | no new violation on at least 50 random batches per curated document, zero refusals on valid inputs and on inputs with violations, and `Document::check` of every output has no finding that its input lacked |

Unit tests are tables: a name, a fragment, the expected codes and the expected subject for each rule, including the look-through of W9, the snapshot exemption of W10, the untaken branch, the Strict namespace, a text box whose paragraphs are units of their own, and a deleted-field fixture for W8. `bench/` gains `check_units`, `check_census` and `check_document`, with baseline rows, held to the budgets of section 6.

## 10. What check needs from other modules

Each item below is in the code.

**From `state`.**
- **No new part tracking.** A changed part and a part `add_part` made are found by comparing `PartsView::generation` in the transaction's view and the base's, and the parts by the main part's relationships.
- **No staged content types.** P1 reads `[Content_Types].xml` through `PartsView::tree_opt`, which parses it from the transaction's own package after `add_part`.

**From `revisions`.**
- **RV1.** An `Origin` for **every unit that receives content copied from another unit** (2.3), not only for a new row: each paragraph of a cloned row, the twin of an X paragraph, the original of a split, the target of a tracked move, a run copied into another paragraph, a block `compare` imports.
- **RV2.** `verify.rs` calls `check::scan_unit` on the created subtrees, expecting nothing in them (the strict form: instance by instance, which the key rule cannot give), so there is no second implementation of W1 to W10. W11 stays in `finish`.
- **RV3.** Section 7.3 of `revisions.md` matches 3.1 here: W4, W6 and the hyperlink half of W5 are the schema's, W9 looks through `w:sdt` and `w:customXml`, and W3 scopes bookmarks to a part and revisions to the document.
- **RV4.** `Outcome` exposes the `origins` of every call, in order, for `document` to convert into `Declared`.

**From `threads`.**
- **TH1.** `audit`'s `Finding` carries the part, the node when there is one, a `subject` string for the key, and the class, so `check` can locate and compare it. It is cheap with no comments part, and `check` calls it on two views.

**From `ops` and `document`.**
- **DOC1.** `document` calls `check::edit(&tx, &declared)` where the pipeline has `check`, with `Declared` built from the effects (by `document`, which may name both `revisions` and `check`), and maps the refusal to its operation with `at_op`. The scope guard's second pass reads `Verified::changed`, the paragraphs and tables the trees say changed, and the frame of a story part when it differs. `Document::check` is `check::document(&state.parts())`.
- **DOC2.** The receipt of section 5, and the exit status of `check`.
- **DOC3.** `api/check.rs` with the types of 7.2, the `finding_codes!` table, and `docs/reference/findings.md` from `cargo xtask docs`.

**From `compare`.** `compare` passes `Declared::foreign` for imported blocks, built with `check::unit_keys` on the original's state. Without it, a problem in the original's paragraph is new in the redline.

**From `xml`.** Nothing is needed. If a session ever makes the equality test costly, `SpanTree::unchanged_since(&base, id)` over chunk identity would answer it in constant time, and it is not worth the exposure of the arena now.

**From the testkit.** The bench rows, the ledger tooling for V2 (a flag that rewrites `expected_findings.tsv` for review, never in `cargo xtask check`), and the census figures of 3.2 as the nightly assertion.

## 11. Design decisions

Rows 1 to 8 settled the first version of this document. Rows 9 and 10 came later, from the review of `revisions` and the design of `document`. Each is applied where the right-hand column says.

| # | Decision | Applied in |
|---|---|---|
| 1 | `revisions` depends on `check`: the compiler's `finish` calls `check::scan_unit`, and W1 to W10 have one implementation. `check` depends on `threads`, `state`, `wordml` and `schema`, none of which depends on `revisions`, so the table stays acyclic | 7.1, RV2, 14 |
| 2 | Every rule, W10 included, is a no-new-violations rule | 3.5 |
| 3 | Origins travel in `Declared`, from `ops` | 7.1 |
| 4 | `stetdoc check` exits 0 when only pre-existing findings exist, and `--strict` fails on them | 5 |
| 5 | A field belongs to the unit that holds its `begin`, so one old split field cannot hide a new one | 3.1 |
| 6 | Every unit that receives content copied from another unit declares it as an origin | 2.3, RV1 |
| 7 | Each note and comment is a unit of its own, as a top-level element of a part that is not a story is | 2.1, 2.2 |
| 8 | The equality test requires an unchanged parent id, so a moved block is checked again | 2.2 |
| 9 | A pre-existing violation carried by an element that the compiler renames (`w:t` to `w:delText`, `w:instrText` to `w:delInstrText`) keeps its key, so it is not new. 1,274 of 41,688 lab documents carry python-docx's invalid `w:space` on `w:t`, and without this rule every tracked deletion in them would be refused. From the review of `revisions` | 2.3 |
| 10 | `document` builds `Declared` from the compiler's origins, `ops` does not. `Verified::changed` lists the paragraphs and tables the trees say changed, for the scope guard's second pass. From the design of `document` | 7.1, 10 |

## 12. Build order

The module was built in this order. Every step is in the code.

| Step | Content | Done when |
|---|---|---|
| 1 | `schema`: `walk_unit`, `is_unit_boundary`, `in_taken_branch`, the `ctx_at` documentation, the bench rows | the walk gives what the validator visits on the simulation and the corpus runs |
| 2 | `check` core: keys, `rules/`, `census.rs`, `report.rs`, `api/check.rs`, `document()` | V1, V2 and V3 on the curated set, then the full set; the figures of 3.2 reproduced |
| 3 | `units.rs`, `origins.rs`, `edit()` over a `Tx` made with `Editor` | V4 to V9 |
| 4 | `fields.rs`, `audit.rs` (needs TH1), `package.rs` | V5 for W8, V10, the package fixtures |
| 5 | contract, `ARCHITECTURE.md`, `GLOSSARY.md`, `CHANGELOG.md`, `finding_codes!` and the reference | `cargo xtask check` |
| 6 | with `revisions`, `ops` and `document`: `Declared` from the effects, `scan_unit` in `finish`, V11 | no new violation through `Document` |

The benches `check_edit`, `check_frame` and `check_document` are in `crates/stetdoc-core/src/bench/check.rs`. Not implemented: a `Measure` per attribute and `schema::measure_of`, so that findings can show human units (twips, half-points), which needs the generator to carry the simple type of each attribute (`ops/units.rs` keeps its own table until then); the census refreshed from the base's; and, if the speed budgets need it, the W8 scan of the staged part refreshed the same way.

## 13. Risks

1. **Word is the only judge.** The rules follow files, and whether Word complains about an `edge_space_without_preserve` or a cell with no paragraph is for the Word checklist (a hand check in Word for the web). I propose five documents for it, each with one violation and otherwise clean: an edge space with no `xml:space` in a `w:t`, a `w:t` inside a `w:del`, a cell that holds only `w:tcPr`, a table with no row and a bookmark with no end. A rule that Word ignores becomes a note.
2. **The shortcuts rest on ids.** The shell test and the equality test compare two trees by `NodeId`. A part that was not cloned from the base is validated whole, and V8 shows the shortcuts and the full check agree. If a future change to `state` stopped cloning, V8 would fail first.
3. **Keys can hide.** A new empty `w:rPr` in a paragraph that already had one is not new. The compiler's `finish` checks created content instance by instance (RV2), which covers what the key rule cannot.
4. **A missing origin refuses a valid edit** on a document that had a problem. It fails in the safe direction, and V6 finds it.
5. **A census on a hostile part.** The vectors are bounded by the node limit (20 MB for the ids), the sorts are `n log n`, and every walk is iterative, so a document nested past the limit cannot overflow the stack.
6. **The ledger rots.** V2 fails on any change of a rule, which is what it is for, and a rule change comes with a reviewed ledger change.

## 14. Changes to other documents

Applied with the code:

- `ARCHITECTURE.md`: the `check` row of the module table becomes `edit(&Tx, &Declared)`, `document(&PartsView)`, `scan_unit`; the allowed dependencies of `check` add `threads`, and those of `revisions` add `check` (decision 1, done with the compiler's `finish`, so that the contract of `revisions` and the table change together). The Errors table is unchanged.
- `GLOSSARY.md`: **Finding**, **Word rule**, **shell** (the elements of a table or frame that belong to it and not to a paragraph in it), and a sentence on **No new violations** for the equality test and the Word rules.
- `check/mod.rs`: the contract below.
- `docs/reference/findings.md`: generated.

```text
//! Validity under the no-new-violations rule, and check reports.
//!
//! **Owns.** The rules that decide whether a document is valid (the schema's keys, the Word
//! rules, the package rules), the finding vocabulary of `api::check`, and the reports built from
//! them.
//!
//! **Interface.** `edit(&Tx, &Declared) -> Result<Verified, StetError>` for the commit pipeline,
//! `document(&PartsView) -> CheckReport` for `Document::check`, `scan_unit` and `scan_part` for the
//! Word rules of one unit and of a whole part, which the compiler's post-conditions use as well,
//! and `unit_keys` for `compare`'s origins in another package.
//!
//! **Invariants.**
//! - No new violations: after an edit, the keys of every unit that differs from the base are a
//!   subset of the keys of its origin. Problems already present in the input are reported, not
//!   blocked.
//! - The units are found from the span tree's own dirty flags and an equality test against the
//!   base, never from the operations' declarations. Declarations only say where a new block came
//!   from, and a missing one makes `check` stricter.
//! - Only the parts a transaction changed are examined before commit, and a part that `add_part`
//!   made, which is not a clone of the base's, is examined whole and has no origin.
//! - A key names what is wrong and never where, and an element that the compiler renames
//!   (`w:t` to `w:delText`) keeps the key of the problems it had.
//! - A finding has a stable code, and the same bytes give the same findings in the same order.
//!
//! **Depends on.** `api`, `package`, `xml`, `schema`, `wordml`, `state`, `threads`.
//!
//! **Start reading.** `check/mod.rs`, then `check/units.rs` and `check/edit.rs`; the Word rules
//! are in `check/rules/mod.rs`, and `check/document.rs` is `Document::check`.
//!
//! **Design.** `docs/design/check.md`.
```
