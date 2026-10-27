# Glossary

These are the exact words stetdoc uses in code, docs, errors and agent instructions.
A type, module or message should use the term listed here, not a synonym. When a new
concept appears, add it here in the same change.

Architecture vocabulary (module, interface, seam, adapter, depth, leverage, locality)
follows common usage, and depth in the sense of John Ousterhout's *A Philosophy of Software
Design*. `ARCHITECTURE.md` defines the terms briefly.

## Package and XML

**Package.** The `.docx` file: a ZIP container of parts (an OPC package).
*Avoid:* archive, file, zip (except when talking about the ZIP format itself).

**Part.** One XML or binary file inside the package, such as `word/document.xml` or
`word/media/image1.png`. A part is named by its path in the package.
*Avoid:* file, entry, document (for a part).

**Entry.** The ZIP record that stores a part: local header, compressed data, central
directory record. Entries are the unit of byte preservation at package level.

**Span tree.** stetdoc's parsed form of an XML part. Each **node** records byte spans
in the part's original bytes. Clean subtrees serialize by copying those bytes.
*Avoid:* DOM, AST, model.

**Clean / dirty.** A node is **dirty** when an edit changed it or anything inside it, or
made it meet another text node, which the writer joins and can then write differently.
Clean nodes are copied verbatim on save. The one exception to "an edit changes only the
bytes of what it edits" is the part's root start tag, which may gain the namespace
declarations and `mc:Ignorable` tokens that new content needs.

**Part generation.** The pair (epoch, edits) that says whether a part's span tree has
changed: the epoch counts replacements of the tree, and the edits count changes made to
it. A derived cache records the generation of every part it was built from and is
rebuilt on the next read when one differs. The generation is a state concept and is
not stored in the package.

**Element, attribute, text.** XML terms, used in their XML sense only. A `w:p` is an
element, and a paragraph is the content it represents.

**Alternate content.** An `mc:AlternateContent` element, from Markup Compatibility (ECMA-376
Part 3): `mc:Choice` branches, each saying in `Requires` which namespaces it needs, and an
optional `mc:Fallback`. Word reads one branch, the **taken branch**: the first `mc:Choice` whose
`Requires` names only namespaces the schema tables know, otherwise the `mc:Fallback`, otherwise
none. One function in `schema` (`taken_branch`) decides, and everything that reads or checks
content uses it, so that reading and validating cannot disagree. An element in a namespace that
the document lists in `mc:Ignorable`, and that the tables do not understand, is skipped with its
content unless `mc:ProcessContent` lists it, which keeps its children in its place
(`disposition`). *Avoid:* alt content, and "the fallback" for the branch that was taken.

## Document content

**Document.** An opened package plus everything stetdoc derives from it (span trees,
refs, indexes). This is the central object of the core interface.

**Block.** A unit of content that the projection shows on its own line or lines, and
that edits can target:

- a paragraph (in the body, a table cell, a note, a header or footer, or a comment)
- a table
- a table row

**Run.** A `w:r`: a stretch of text with uniform formatting. Runs are internal.
Agents never address runs.

**Story.** A separate flow of content: the body, each header and footer, the
footnotes, the endnotes, the comments.
*Avoid:* stream, section (for this sense).

**Section.** A Word section: a range of the body with its own page setup and headers
and footers (`w:sectPr`). It is not a heading-delimited part of the text.

**Clause number.** The list number Word displays for a paragraph ("4.2(a)"), computed
from numbering definitions: the level's text with the counters filled in, without the
suffix. Its **full-context** form adds the labels of the levels above it, as Word's
cross-references do. It is read-only in projections.

**List item.** A paragraph that Word numbers or bullets, with its level and its list.

**Ghost number.** In a redline, the number a deleted paragraph shows: the value the counters
hold at its position. It advances nothing, so the next live paragraph repeats it.

**Toggle property.** One of twelve run properties (bold, italic, caps, small caps, strike,
emboss, imprint, outline, shadow, vanish, and the complex script bold and italic) whose
values from the table style, the paragraph style and the character style combine by
exclusive-or, and which direct formatting sets absolutely.

**Looks hidden.** Text that Word shows and a reader cannot see: white text (or text so
near white that every colour channel is at or above `F8`) with nothing coloured behind it,
or text of 2 pt or less. Different from hidden text, which Word does not show.

**Field.** A Word field (`REF`, `TOC`, `PAGE` and so on). Projections show its
**cached result**, and its **field code** is read-only.

**Content control.** A `w:sdt`.
*Avoid:* SDT in user-facing text.

**Item.** One piece of a paragraph's text model, in XML order: a stretch of text from one XML
text node, one character that stands for something that is not text (a tab, a break, a drawing,
a note reference, a field's displayed value), or a mark. Each item knows the XML node it came from
and the offset in the paragraph's text where it sits. A run is XML. What the model makes of a run
is items.
*Avoid:* token, segment, piece (for this sense).

**Mark.** An item with no width: a bookmark or comment range boundary, the begin, separate or end of
a field, the instruction text of a field, a paragraph's comment reference. It has a position in
the text and no character, so it never counts in an offset.
*Avoid:* marker, anchor (an **anchor** pairs a start mark with its end mark across paragraphs).

**Anomaly.** Something the reading skipped or repaired in input that parsed: an element it does not
know in a paragraph, a field character with no field, a second `w:body`. An anomaly is counted and
never fails the read, and it exists so that an agent is told about content it cannot see.
*Avoid:* warning (for this sense), error (an error refuses the input).

**Unit.** One note or one comment of a notes or comments story. The scan reads each unit as a
story of its own blocks, so a field that is open at the end of one does not carry into the next.
*Avoid:* entry (reserved for the ZIP record).

## The stetdoc format (agent-facing)

**Projection.** The agent-facing text rendering of a document. It is deterministic
for a given document and format version.

**View.** A projection of a chosen scope (an outline, a range of blocks, find results,
the revisions, the comments) in a chosen mode.

**Mode.** How tracked changes appear in a view:

- **final**: all revisions applied
- **original**: all revisions rejected
- **redline**: both, marked

**Ref.** stetdoc's short, stable reference to a block, such as `p42`, `t3` (a table) or
`r17` (a table row). Refs are assigned in document order when a document is opened, and
belong to that document's ref namespace. A ref follows its block's element, and within the
namespace it never changes: deleting a block never renumbers others, and a block that an
edit adds gets a new ref when the edit commits. A ref is retired, as a **tombstone**, when
its block leaves the document. Refs are stored in the complement and never written into the
package.
*Avoid:* id (reserved for OOXML ids such as `w:id`), anchor, locator, handle.

**Ref namespace.** The scope in which refs are valid. Every open starts one, with an id
derived from the whole opened package, so the same bytes give the same namespace. A
namespace survives edits and `export`/`resume`. Reopening saved bytes starts a new one,
and refs from another namespace are rejected as `stale_ref`.

**State version.** A counter of committed edits within a namespace.

**Lineage.** A hash chain over the committed states of one namespace: the lineage of
version 0 is the namespace id, and each commit extends it with a hash of what the commit
changed. Two sessions opened from the same bytes share a namespace, and their lineages
differ as soon as their edits do. Identical histories give identical lineages.

**Base token.** The namespace, state version and lineage of one state of a document,
written as one opaque string. Views, receipts and readable files carry it, and an
operation batch names the one it was written against. A batch from another namespace,
from a history this document does not have, or naming a block that changed after the
batch's version, is rejected as `stale_ref`. Readable files from any other base are
rejected as `stale_files`.

**Tombstone.** The record that a ref's block left the document (deleted, rejected or merged
into another block). The ref resolves to `stale_ref`, with the reason, and its number is
never reused.

**Text range.** A character range inside a block's text, given by `find` text or
offsets.
*Avoid:* span (reserved for byte spans in the span tree).

**Outline.** The compact view of a document's structure: headings, clause numbers,
refs and sizes. The headings come from one source for the whole document: the styles'
outline levels, numbered paragraphs, short bold lines that look like headings, or, when
there are none, runs of blocks.

**Cursor.** What a page of a view gives back to ask for the next one: the ref of the last
block shown, so it stays valid when the document is edited. A table that a page cut between
rows has the ref of its last row.

**Hit line.** One line of a compact find: the ref of a paragraph that matched, the clause
it is or whose own text it is, and its text around the match, exact so that a piece of it can be
an edit's `find`. A hit in a table also shows the other cells of its row.

**Margin note.** A line of a page that is about the block before it and is not text: how
many times a find matched in it, what a citation in it came to, what cites a clause. It
starts with `{~`, is never in a readable file, and is refused if an edit holds one.

**Appendix.** The notes and comments that a page of a view cites, written at the end of the
page so that a reference and what it points to are on one screen.

**Draft.** What an edit of readable files says, in no format's syntax: for each paragraph its
tags, its editable text slots and the read-only seams between them. A format's `parse` makes
one from the agent's files and `Projection::draft` predicts the one that unedited files must
parse to, so that `files::to_batch` is the difference between two drafts and every format
shares it.

**Slot, seam.** The parts of a paragraph in a draft. A **slot** is text the agent may change,
and a **seam** is something between two slots that is read-only (a tracked change, a field, a
comment mark, a link), with the width it takes in the `Final` text so that an offset in a
slot is an offset in the paragraph.

**Working copy.** The stetdoc format as files: a directory of readable files plus the
complement, created by `unpack` and turned back into a package by `pack`.

**Readable file.** A working-copy file that agents read and edit, such as
`document.md` or `notes.md`.

**Complement.** The hidden part of the working copy (`.stet/`). It holds everything
needed to rebuild the exact package:

- the complement and format versions
- the base token, and the SHA-256 of the head package bytes and of the ref map
- the original package bytes, and the head package bytes once edited
- the ref map (the namespace and how it was derived, the block-scan and fingerprint
  versions, the lineage chain, and for each block its ref, story, ordinal in the story's
  block scan, text fingerprint and the version it last changed in, plus the tombstones)
- the hashes of the readable files as written

It is versioned. Because agents can write to it, `resume` verifies it rather than
trusting it.
*Avoid:* sidecar, metadata, cache.

**Format version.** The version of the projection and working-copy format. It is
independent of the software version.

## Editing

**Edit.** Any change an agent asks for, either as operations or by editing readable
files. An edit is intent. A revision is what lands in the package.

**Operation (op).** One typed, validated edit instruction: `replace`, `insert`,
`delete`, `split`, `merge`, `format`, `paragraph`, `style`, `list`, `set`, `row`,
`comment`, `reply`, `resolve`, `delete-comment`, `accept` or `reject`.

**Batch.** A list of operations applied atomically: all of them land, or none do.

**Occurrence.** One place a text is in a paragraph's Final text, matched as an operation's
`find` matches it: its ref, its clause, its character offsets and the text around it.
`Document::occurrences` lists them, and `stetdoc replace` and `stetdoc comment` turn them into
operations against the same state.

**Scope guard.** An optional list of refs a batch may touch. Any change outside it
rejects the batch.

**Compile.** To turn operations into changes to span trees, recorded as revisions
(tracked editing) or applied directly (untracked editing).

**Tracked / untracked editing.** Whether edits are recorded as revisions (the default)
or applied directly.
*Avoid:* tracked mode (mode means view mode).

**Manual markup.** An untracked edit that shows its text changes as direct formatting: the old
text stays, struck through, and the new text is underlined, with no revision. What a reader
means by a redline "without Track Changes".

**Receipt.** The result of applying or checking a batch: what changed, where, as which
revisions, the new state version, and warnings.

**Check.** Validation of a document or working copy, together with a receipt of the
revisions that `pack` would write.

**No new violations.** The rule that decides whether an edit may be committed. A **violation**
is a departure from the schema (an element out of order, a bad attribute value, an unknown
element) or from a **Word rule** that the check reports as a **key**: for the schema its kind, the
parent's declaration, the offending element or attribute, and for a bad value the value; for a
Word rule its code and the element, id or name it is about. The rule is applied per **unit**: in
a story part, the innermost paragraph an edit touches, the table's own shell (its properties,
grid, rows and cells, the insides of its paragraphs being units of their own) and the frame (what
is left of the part: the root, `w:body`, block-level content controls); in the notes and comments
parts, each note and comment, and the paragraphs and tables below it; in any other part, each
top-level element. A unit is found from the span tree itself: the dirty flags lead to it, and an
**equality test** against the base (same names, attributes, text, children and parent) drops what
only an earlier commit marked. The keys after the edit must be a subset of the keys of the unit's
**origin**: the same element in the base, plus the blocks the edit declared it was copied or
built from (a merge declares both, and the origin is the union of their keys); a new block with no
declaration has no origin, so every violation in it is new. An element that the compiler renames
(`w:t` to `w:delText`) keeps the key of its problems. The comparison is of keys, not counts and not
positions, and never of a whole part or a whole table, so a problem that was already in the input
never blocks an edit, copying an element that has a problem repeats a key, and one invalid
paragraph does not hide new problems in another. The limit: a new instance of a pattern already
present in the unit is not new. *Avoid:* error (for a violation), diff of counts.

**Foreign origin.** The origin of something that came from another package: the keys its block,
element or whole part had there. `compare` declares one for each block of the redline that holds
the original's content, each style or numbering definition it imports, and each part it copies
whole, so a problem the original already had is carried into the redline and not refused.

**Word rule.** A rule about what Word reads that the schema does not say, with a stable code: text
in a deletion is `w:delText`, text with white space at its edge needs `xml:space`, a revision does
not nest in one of its kind, ids are unique and ranges are paired, a cell ends with a paragraph, a
revision or a properties element holds something, and a field is not cut by a deletion. The
corpus shows that about one document in a hundred breaks one, so each is a no-new-violations rule
and none is absolute. See [the check design](docs/design/check.md).

**Finding.** One thing a check reports: a stable code, whether it is a **violation** or a **note**
(legal, and worth saying), the part, the block that holds it and a sentence. A report counts every
finding and lists the first 200.

**Shell.** The elements of a table or of the frame of a part that belong to it and not to a
paragraph in it: a table's `w:tblPr`, `w:tblGrid`, rows and cells, or the `w:body` and its
block-level content controls. The check validates a shell only when one of its elements changed
itself, so editing the text of a cell does not cost a check of the table.

**Property container.** An element whose children have a fixed order that the schema layer can
rewrite when an edit touches it. The generator marks a declaration this way when its content
model has no wildcard, at least two symbols, at most three symbols that share a rank, and symbols
that never occur together (the alternatives of a choice) sharing a rank. That is 98 declarations:
`w:pPr`, `w:rPr`, `w:tblPr`, `w:tcPr`, `w:sectPr`, `w:lvl`, `w:style` and the like, and also
`w:settings`, `w:styles`, `w:numbering` and `w:document`. Content (`w:p`, `w:r`, `w:tbl`,
`w:body`) is never rewritten.

**Generic setter.** The `set` operation. It writes any schema-defined property on
paragraph, run, section, table, row or cell properties, validated against the schema.

## Revisions and review

**Revision.** A tracked change stored in the package, with a kind, author, date and
OOXML id. The kinds:

- insertion and deletion
- move (from and to)
- run, paragraph, section, table, row or cell formatting change
- paragraph-mark insertion and deletion
- table row insertion and deletion
- table cell insertion, deletion and merge
- table grid change and numbering change

In user-facing text, "tracked change" means revision.
*Avoid:* change (for one revision: a **change**, below, is a group of them), diff, redline (for a single revision).

**Accept / reject.** To resolve revisions, applying or undoing them.

**Change.** The unit an agent sees and resolves: the adjacent revision elements of one kind, author and
date, a deleted or inserted paragraph with its mark, both halves of a move, a field with all its pieces.
Accepting or rejecting by id acts on the change that holds the id. A change can be several `w:id`s.
*Avoid:* hunk, revision (for a group of elements).

**Pending revision.** A revision that has been neither accepted nor rejected. Every revision in a package
is pending. *Avoid:* open change, live change.

**Snapshot.** The old properties that a property-change revision (`rPrChange`, `pPrChange` and the like)
stores inside itself, so that rejecting it restores them. *Avoid:* backup, previous state.

**Redline.** A document that contains revisions, and the mode that shows them.

**Compare.** To produce a redline showing the differences between an **original** and
a **revised** document. The redline is built on the revised package and checked like an edit,
with **foreign origins** for what came from the original.

**Move.** A paragraph that `compare` found in another place: it is written as a deletion where it
was and an insertion where it went, and the receipt lists the two refs. *Avoid:* moved revision
(Word's `w:moveFrom` and `w:moveTo`, which `compare` does not write).

**Comment, reply, thread.** A Word comment, a reply to it, and the comment with its
replies. A thread is **resolved** or open. Its **root** is the comment that has no parent, and a thread is
resolved when its root is. A comment is addressed by its `w:id`, written `c3`. Its **reference run** is the
run with the `w:commentReference` that follows the end of its range, and a comment that has none is
**unanchored**. Its **durable id** is the `w16cid:durableId` that Word keeps for the life of the comment,
a different number from the `w:id`.

## Formatting

**Direct formatting.** Properties set on a run or paragraph itself, as opposed to
inherited ones.

**Style cascade.** The order in which formatting is inherited: document defaults,
table style, numbering level, paragraph style, character style, direct formatting.

**Effective formatting.** The formatting that applies after the cascade, with the
**source** of each value.

## Index

**Defined term.** A term a document defines ("Losses" means …), with its definition
and every use.

**Clause.** A labelled paragraph, whose label Word numbers or the author typed, with the label
a lawyer would cite (`4.2(a)`), composed across lists when the sub-items are a list of their
own. A clause is flagged as computed or typed, and as composed when the full label was built
by the index and not taken from Word.

**Cross-reference.** A pointer from one block to another: a `REF` field, or a textual
reference such as "clause 4.2" resolved through clause numbers. A textual one, a **mention**,
comes out **resolved**, **ambiguous**, **relative** (read from the clauses around the citing
paragraph), **outside** (a citation of a law) or **unresolved**, and the first answer is never
given for the second or the third.

**Reference graph.** All cross-references and defined-term uses, queryable in both
directions.

**Lookup.** A query against the reference graph: where a term is defined and used, or
what refers to a block (`Document::lookup`, `stetdoc lookup`).
*Avoid:* refs (refs are block references).

## Quality

**Fidelity.** stetdoc's guarantees about preserving documents, stated as **laws**
L1 to L6: identity, locality, edits stick, stability, reversibility and validity. The `laws`
module of `stetdoc-testkit` checks them, and `ARCHITECTURE.md` (Cross-cutting rules) says
where they run.

**Oracle.** An independent source of truth used in tests: Word, the schema validator,
LibreOffice, the accept/reject reversibility check.

**Corpus.** A tiered set of test documents.

**Fixture.** A synthetic test document built in memory by `stetdoc_testkit::fixtures`. An
*edge* fixture is valid and unusual, a *Word* fixture carries one Word feature, and a
*hostile* one is built to be refused or survived.

**Refusal.** An input that stetdoc rejects with an error code instead of opening. The corpus
and the fixtures record, for each one, the code, the *stage* where it happens (`open`, `read`,
`xml` or `document`) and the reason. A refusal at `read` or `xml` also names the *part* that
carries it, and a layer that never reads that part does not meet it. The refusals are the
exemption list of law L1.

**Limits.** Input safety bounds such as entry count, sizes, compression ratio and
nesting depth.

**Error code.** A stable, documented identifier for a failure, such as `stale_ref`.
The same code appears in CLI output, JSON, exit-code mapping and the library.