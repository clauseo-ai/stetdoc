# Design: `api`, `document` and `ops`

Design notes for the public interface and the integrator. [`ARCHITECTURE.md`](../../ARCHITECTURE.md) is the
map. The contracts in `document/mod.rs` and `ops/mod.rs` are the interfaces, and `api/` is the public data. This
document says what a caller sees, which operations exist and what each compiles to, how one edit moves through
`state`, `ops`, `revisions`, `comments`, `check` and `projection`, what a receipt holds, how the working copy is
stored, what the budgets leave for this layer, and what the other modules provide. Terms follow
[`GLOSSARY.md`](../../GLOSSARY.md): operation, batch, edit, receipt, stamp, scope guard, ref, base token, working copy,
complement, readable file, check, no new violations. "Final" is the text of a block with every revision applied.

The designs this one builds on are cited by name and section: `revisions.md`, `projection-and-index.md`, `check.md`,
`wordml.md`, `refs-and-state.md`, `styles-and-numbering.md` and `comments-and-notes.md`, all in `docs/design/`. Where a
design left a shape open, the interfaces below were read from the code.

The design is built. Section 7.1 reports a throwaway spike, not kept, that measured what a receipt costs on top of the
compiler before the code existed, and section 11 lists where the code differs from the design.

## The decisions that matter most

1. **One handle, thirteen methods, three free functions** (1). `Document` is `open`, `resume`, `base`, `view`, `edit`,
   `save`, `inspect`, `revisions`, `lookup`, `check`, `check_pending`, `files` and `export`. The free functions are `compare`,
   `capabilities` and `json_schema`. `files` is new: the text editor backend needs the readable files without the
   complement, which `export` costs a ZIP write to build. `check_pending` is new: it is the whole of `stetdoc check`, the
   dry run of the pending files, the revisions already there by author and the exit status, in one call that the CLI, napi
   and a host application share. Every method that can fail returns `Result`, which `ARCHITECTURE.md` sketched as infallible for
   `revisions` and `export`.
2. **Operations are plain data with one spec table** (2). `Op` is a closed enum with serde and schemars derives, so
   JSON and the JSON Schema cost nothing. A table `OPS` in `api/op_spec.rs` lists each operation's name, fields,
   whether it is trackable and whether it is reversible. The block-text parser, `capabilities()`, the generated
   operations reference, the skill table and the conformance suite are all driven by that table, so an operation is
   one enum variant, one table entry and one file in `ops/`.
3. **A batch is written blind and always names its base token** (2.2, 3.5). The token is a required field, an empty
   batch is `invalid_payload`, in a working copy too, as the `refs-and-state` design requires. Every operation sees the effect of the earlier ones, and
   no operation can name a block that an earlier operation of the batch created.
4. **Text offsets are Unicode scalar values in the block's Final text by default, UTF-16 units on request, and `find` comes
   first** (2.2). The core owns the conversion: `Edit::offsets(OffsetUnit::Utf16)` makes it count as JavaScript does, so
   napi ships no helper. An agent never counts characters, because the view escapes invisible ones. `find` must be unique, or
   `occurrence` picks one. When it matches nothing, a second try treats curly and straight quotes as equal and says so.
5. **The pipeline is stage, apply, reconcile, scope guard, check, scope guard again on what `check` found changed, commit,
   receipt, install** (3). The first guard runs before `check`, because it is nearly free and `check` is not. The receipt is
   built from the committed state and **before** `Document` takes it, so a receipt that cannot be built leaves the document
   as it was. **A dry run is the same path** and drops the state it committed instead of installing it, so its receipt is
   the real one apart from `dry_run`, the base (the current one) and the refs of new blocks, which a dry run never issues.
6. **The scope guard compares what the batch touched, what each operation targeted and which parts it changed with the
   scope** (3.3). `accept` and `reject` by `all` or by author are narrowed to the revisions in the scope's blocks, an id outside
   it is `scope_violation`, and a changed part that no operation declared (`styles.xml`, `numbering.xml`, the body's last
   section) is refused. The comments story is ignored for touches, containment follows the block tree, a scope may name a
   range of refs, and the last section of the body, which belongs to no paragraph, cannot be edited under a scope.
7. **The receipt is data with a rendering** (4). It carries one result for each operation, the revisions the batch wrote
   by author, resolutions, comment effects, the refs retired, the post-edit text of every block touched or added
   (rendered from the committed state, or for a dry run from the staged one, so a new block has no ref), warnings, and what
   `check` verified. Its `text` is the agent-facing form, with the block bodies from `render_blocks`, and the structure is
   the `--json` form.
8. **The working copy is a plain value, and writing it survives a crash** (5). `WorkingCopy` is the readable files and the
   complement, and the complement is a few named byte entries under `.stet/`: a text manifest, `original.docx`, a head
   and a ref map named by generation. The manifest records the base token, the digests, the format, the SHA-256 of each
   readable file as written and its own digest. `pending_files()` hands those hashes back with the pending files, so the
   release-skew check needs no state in `Document`. The core returns a `WritePlan` whose one commit point is the atomic
   rename of the manifest, and `recover` finishes or rolls back an interrupted write without losing a hand edit.
9. **Typed text follows the quotes of the paragraph, and says so** (2.8). Straight quotes typed into a paragraph that uses
   curly ones become curly, curly ones typed into a straight paragraph become straight, and a quote after a digit stays
   straight (inches and feet) unless it closes a quotation that is open in the typed text (`"Clause 12"`). It is one pure function of the paragraph and its neighbours, and the receipt lists the
   change. `Edit::quotes(Quotes::AsTyped)` turns it off.
10. **Limits are in `Limits`** (3.6): 5,000 operations, 16 MiB of batch text, 1 MiB for one text field, and a scope of at
    most 10,000 refs. The numbers come from the document census and from the cost of an operation on the worst body.
11. **The budgets hold, with a thin margin on the worst stress body** (7). On a 1 MiB body a 50-operation
    tracked edit costs about 45 ms through the receipt. On a synthetic body of 28.6 MiB (1.6 times the biggest benchmark
    input) the compiler, the commit and the receipt take about 0.4 s, and `check` and the files path add up to 0.45 s more.
12. **The laws move to `stetdoc-testkit`** (9). A `laws` module there runs L1 to L6, the operation conformance suite,
    crash injection at every step of a write, and `projection`'s T6 and T8 through `Document`. The testkit already depends on the core, so
    no feature is needed (9.1).
13. **Three crates and three table changes** (8): `serde`, `serde_json` and `schemars` join the core, the
    `ops` row gains `schema`, `comments` and `threads`, and `fuzz` and `bench` gain `document`. Origins reach `check` through
    `document`, so `ops` does not depend on `check`.

## 1. The public interface

### 1.1 The Rust surface

`lib.rs` re-exports `Document`, `Edit`, the data types of `api/`, `compare`, `capabilities`, `json_schema` and the error
vocabulary. Everything else stays `pub(crate)`. A change to this list needs a decision record, and
[`docs/decisions/0004-public-interface.md`](../decisions/0004-public-interface.md) holds the list below.

```rust
impl Document {
    pub fn open(bytes: Vec<u8>, options: &OpenOptions) -> Result<Document, StetError>;
    pub fn resume(working_copy: &WorkingCopy, options: &OpenOptions) -> Result<Document, StetError>;
    pub fn base(&self) -> String;                                   // the base token, for building a batch with no view
    pub fn view(&self, spec: &ViewSpec) -> Result<View, StetError>;
    pub fn edit(&mut self, edit: Edit) -> Result<Receipt, StetError>;
    pub fn save(&self) -> Result<Saved, StetError>;
    pub fn inspect(&self, target: &str) -> Result<Inspection, StetError>;   // "p42", "t3", "r17", "styles", "numbering", "sections"
    pub fn revisions(&self, filter: &RevisionFilter) -> Result<Vec<RevisionInfo>, StetError>;
    pub fn lookup(&self, query: &str) -> Result<Lookup, StetError>;
    pub fn check(&self) -> CheckReport;
    pub fn check_pending(&self, pending: Option<ReadableFiles>, stamp: Option<Stamp>, strict: bool) -> Result<CheckResult, StetError>;
    pub fn files(&self) -> Result<ReadableFiles, StetError>;       // what `export` writes as files, with no ZIP write
    pub fn export(&self) -> Result<WorkingCopy, StetError>;
}
impl WorkingCopy {
    pub fn pending_files(&self) -> Result<Option<ReadableFiles>, StetError>;   // the files that differ from what was written
    pub fn ensure_clean(&self) -> Result<(), StetError>;                       // `pending_edits`, naming the files
    pub fn write_plan(&self, disk: Option<&WorkingCopy>, options: &WriteOptions) -> Result<WritePlan, StetError>;   // 5.4
    pub fn recover(disk: &WorkingCopy, marker: &[u8]) -> Result<Recovery, StetError>;                          // 5.4
}
impl Edit {
    pub fn ops(batch: Batch, stamp: Stamp) -> Edit;
    pub fn files(files: ReadableFiles, stamp: Stamp) -> Edit;
    pub fn untracked(self) -> Edit;
    pub fn scope(self, refs: Vec<String>) -> Edit;                 // refs and ranges such as "p40..p60"
    pub fn dry_run(self) -> Edit;
    pub fn quotes(self, quotes: Quotes) -> Edit;                   // default Quotes::Match
    pub fn offsets(self, unit: OffsetUnit) -> Edit;                // default Scalar. Applies to operations, not to files
}
impl Batch {
    pub fn new(base: impl Into<String>, ops: Vec<Op>) -> Batch;
    pub fn parse(text: &str, limits: &Limits) -> Result<Batch, StetError>;   // JSON if the first character is { or [, else block text
    pub fn to_json(&self) -> String;
    pub fn to_block(&self) -> Result<String, StetError>;                      // the block-text form, for the testkit and the skill's examples. A batch that block text cannot say is `invalid_payload`
}
impl Stamp { pub fn new(author: impl Into<String>, date: impl Into<String>) -> Stamp; }
pub fn compare(original: Vec<u8>, revised: Vec<u8>, options: &CompareOptions) -> Result<Compared, StetError>;
pub fn capabilities() -> Capabilities;
pub fn json_schema() -> String;                                    // one JSON Schema document for every public type
```

Five of these differ from the sketch in `ARCHITECTURE.md`, and decision 0004 records each:

- **Refs are `String`**, as in `projection`'s types, not a `Ref` newtype. They are parsed where they are used, with `invalid_payload`
  for a malformed one, so the napi and JSON forms need no wrapper. `inspect` takes the target as text for the same reason,
  because `styles`, `numbering` and `sections` sit beside refs in the CLI.
- **`revisions`, `export` and `files` return `Result`**, because reading a story and writing a ZIP can fail.
- **`files`, `base` and `check_pending`** are new. Each is small, and each is used by a surface that would otherwise rebuild it.
  `check_pending` takes `&self` because the dry run it makes never installs its state.
- **`json_schema`** is new. The generated TypeScript types and the CLI reference read the schema from the binary, because
  `xtask` is built without the core and cannot derive it.

`Document` is `Send` and not `Sync`, as `State` already is (it holds a `RefCell` for caches). A compile-time assertion in
the tests keeps that true. A `Document` holds the committed `State`, the `Limits`, the working-copy `FormatId` and the
bytes the package was opened from (a shared handle that `State` keeps, `State::opened_source` (10), so `open` does not
copy the input a second time). Nothing in it is mutated by a method that takes `&self` apart from the caches `State` keeps.

`OpenOptions { limits: Limits, format: FormatId }` has a `Default`. The format is the one `export` writes, that receipts
render blocks in, and that `lookup` writes its text in. A resumed document takes the format recorded in the complement, and
`OpenOptions.format` applies to `open` only. `CompareOptions { limits, stamp, format }` has a **required** `Stamp`.

### 1.2 The types, by file

All public types live in `api/`, hold strings, numbers, booleans, vectors and `api` enums, and derive `Serialize`,
`Deserialize` and `JsonSchema`. `WorkingCopy` is the one exception: its byte entries are marshalled by hand by the CLI and by
napi, because a JSON byte array would be wrong for both. `projection`'s and `check`'s files are used as they are, with the derives added.

| File | Holds | Owner of the shape |
|---|---|---|
| `error.rs`, `limits.rs` | `StetError`, `ErrorCode`, `ExitClass`, with a file and a line as structured fields (3.4); `Limits` with the new batch fields (3.6) | scaffold, `package`, `document` |
| `view.rs` | `Mode`, `FormatId`, `ViewSpec`, `Scope`, `View`, `Warning`, `WarningCode` and the rest | `projection`; `document` extends `WarningCode` and makes `Warning.story` optional |
| `files.rs` | `ReadableFiles`, `Place`, `NewParagraph`, `PlannedOp`, `Origin`, `Planned`, `BlockSpec`, `RenderedBlock`, `BlockRender` | `projection`; `document` adds `ReadableFiles.rendered` |
| `lookup.rs` | `Lookup` and its hits, with a new `text` field | `projection`, `document` |
| `check.rs` | `Class`, `FindingCode`, `Finding`, `CheckReport` | `check` |
| `stamp.rs`, `options.rs` | `Stamp { author, date }`, `OpenOptions`, `CompareOptions`, `Quotes`, `OffsetUnit` | `document` |
| `op.rs`, `op_spec.rs`, `batch.rs` | `Op` and its payloads; the `OPS` table, `OpSpec`, `FieldSpec`; `Batch { base, ops }` | `document` |
| `edit.rs` | `Edit` and its builder | `document` |
| `receipt.rs` | `Receipt`, `OpResult`, `AuthorRevisions`, `ChangeEntry`, `ChangeKind`, `Resolution`, `CommentEffect`, `Retired`, `MoveEntry`, `Validation`, `CheckResult` | `document`, from check 5, revisions 2.1, comments 7 |
| `revision.rs` | `RevisionInfo` | `document` |
| `inspect.rs` | `Inspection`, `InspectItem` | `document` |
| `working_copy.rs` | `WorkingCopy`, `Complement`, `WriteOptions`, `WritePlan`, `WriteStep`, `Recovery`, `Saved`, `SaveReceipt` | `document` |
| `compare.rs` | `Compared` | `document`, shaped by `compare` |
| `capabilities.rs` | `Capabilities`, `OpInfo` | `document` |

### 1.3 Wire conventions

One convention serves the CLI's `--json`, the napi marshalling and the schema, so nothing is translated:

- Field names are snake_case. The napi options object spells it `dryRun`, and napi maps it onto the `Edit`
  builder. The data types cross as they are.
- Operation names are lowercase, with a hyphen where there are two words (`delete-comment`, `edit-comment`,
  `delete-note`). Error codes, warning codes and finding codes are snake_case, as they are.
- A ref is `p42`, `t3` or `r17`. A comment is `c3`, its `w:id`. A note is `fn3` or `en2`, its ordinal, as the view shows it. A
  revision id is the number after `#` in the view. A base token is the opaque `namespace:version:lineage` string, copied and
  never built.
- Enums that may grow carry `#[non_exhaustive]`. `Op` does too, so adding an operation is a minor change, and the
  testkit's conformance suite finds a new one through `capabilities()` and not through a `match`.
- Lists have a deterministic order: operation order, then document order. No receipt or report iterates a hash map.

## 2. Operations

### 2.1 The set

Nineteen operations, among them `edit-comment` and `delete-note` from `comments-and-notes`. `Within` stands for a text range inside one paragraph, which is `find` (with an optional `occurrence`) or
`start` and `end`, or nothing for the whole paragraph. It is written out as the four fields on each payload struct, by a macro,
and not as a `#[serde(flatten)]` struct, because serde cannot refuse an unknown field through `flatten`. A field in bold is required.

| Operation | Fields | What it does | Tracked as | Reversible |
|---|---|---|---|---|
| `replace` | **ref**, **with**, Within | replaces the range or the whole paragraph with `with`. The compiler diffs by words, so a rewrite is a few insertions and deletions | `w:ins`, `w:del` | yes |
| `insert` | **ref** (a paragraph or a table), **place** (`before` or `after`), **paragraphs** (text, optional style) | new paragraphs, in order. JSON also accepts bare strings in the list | paragraph marks, `w:ins` | yes |
| `delete` | **ref**, `to`, Within | a whole paragraph, a table, a text range, or the blocks from `ref` through `to`, with `start` in the first and `end` in the last | `w:del`, mark `w:del` | yes |
| `split` | **ref**, one of `at`, `before`, `after` | cuts a paragraph in two: at an offset, or before or after some text | paragraph mark `w:ins` | yes |
| `merge` | **ref**, `second` | joins a paragraph and the next one (or `second`) | paragraph mark `w:del` | yes |
| `format` | **ref**, Within, `bold`, `italic`, `underline`, `strike`, `color`, `highlight`, `size`, `font`, `char_style`, `clear` | run formatting on a range | `w:rPrChange` | yes |
| `paragraph` | **ref**, `align`, `indent_left`, `indent_right`, `first_line`, `hanging`, `space_before`, `space_after`, `line_spacing`, `keep_next`, `keep_lines`, `page_break_before`, `clear` | paragraph formatting | `w:pPrChange` | yes |
| `style` | **ref**, **style** (a style name or id) | the paragraph style | `w:pPrChange` | yes |
| `list` | **ref**, one of `level`, `restart` (a number), `remove` | list level, restart at a number, or no list | `w:pPrChange` | yes |
| `set` | **props**, `clear`, `target`, `ref`, `section`, `cell`, Within | any schema property on a paragraph, run, mark, section, table, row or cell | the matching `*PrChange` | yes |
| `row` | **ref** (a row), **action** (`insert` or `delete`), `place`, `cells` | inserts a row with the given cell texts, or deletes the row | `w:trPr/w:ins`, `w:trPr/w:del` | yes |
| `comment` | **ref**, **text**, Within, `initials` | a new comment anchored to the range or the paragraph | comment parts, never tracked | no |
| `reply` | **comment**, **text** | a reply to a thread | never tracked | no |
| `resolve` | **comment**, `reopen` | resolves a thread, or reopens it | never tracked | no |
| `edit-comment` | **comment**, **text** | the text of one of the author's own comments | never tracked | no |
| `delete-comment` | **comment** | one of the author's own comments, with its replies | never tracked | no |
| `delete-note` | **note** | a footnote or endnote and its reference | `w:del` | yes |
| `accept` | one of `all`, `author`, `ids` | applies revisions | none | no |
| `reject` | one of `all`, `author`, `ids` | undoes revisions | none | no |

Notes on the table:

- **`set` is the generic setter.** `props` is a list of `{ name, val, attrs, children }`, a property element by its local name
  (`spacing`, `jc`, `ind`; a `w:` prefix is accepted). `target` defaults from the ref: `p` is a paragraph (a run when a
  range is given), `t` a table, `r` a row (a cell when `cell` is given, 1-based). `target: "section"` takes `section: 3`, the
  number `inspect sections` shows. The values take human units (2.7). **No property operation writes or removes revision markup**: a
  name in `props`, in their children or in `clear` that is `ins`, `del`, `moveFrom`, `moveTo`, a move range, a `*PrChange`, `tblGridChange`,
  `numberingChange`, `cellIns`, `cellDel` or `cellMerge` is `invalid_payload`, tracked or not, because it would forge or erase someone's change.
  The mark's `w:rPr` is the `mark` target's, never part of a paragraph's set. **In a tracked edit**, a property that the container's
  `*PrChange` snapshot cannot hold is `invalid_payload`: a section break (`w:sectPr` in a `w:pPr`) is added only by an untracked edit in 1.0,
  and a section's `headerReference` and `footerReference` change only untracked. The conformance suite holds a list of such forgeries.
- **`format`, `paragraph` and `style` are typed sugar over `set`.** They exist because agents ask for them by name and the
  skill teaches them, and each compiles to the same `set_props`. A table in `ops/props.rs` says which elements each typed field
  writes. It follows what Word's own interface writes and is checked against the 76 scenario documents of the `revisions` census.
- **`accept` and `reject` select by `ids`, by `author`, or `all`.** `ids` are the numbers the view shows after `#`. `author` takes a
  name as it is written in the document or the short code the view shows (`JD`). A full name wins when both match.
  `document` resolves a code to a name before `ops` runs (3.1), because `ops` may not read `projection`'s document facts. Under a scope,
  `all` and `author` mean the revisions in the scope's blocks, and an id outside the scope is `scope_violation` (3.3).
- **`reply` names the thread through any of its comments.** `resolve` with `reopen: true` reopens. `edit-comment` and
  `delete-comment` are the author's own comments only, which is `readonly_region` otherwise (`comments-and-notes` 4.6).
- **Tracked and reversible are the two flags of the spec table.** `trackable` is true for every operation that writes a
  revision. `reversible` is true where a reject of everything the batch wrote restores the original content and structure after
  normalization (the `revisions` law R2 and law L5). A `list` restart leaves an unused `w:num` in `numbering.xml`. The laws compare the stories, so that leftover is not a difference.

### 2.2 Targets: refs, `find`, offsets

- **A ref** is parsed with `BlockRef::parse` and resolved with `Tx::resolve(Claim::Base(token), text)`, the edit path of
  `refs-and-state` 5. Every failure there already carries a sentence and a hint.
- **`find`** is matched in the block's Final text with `TextMap::find_all`. Exactly one match is the range. No match is
  `not_found`, and when the text exists only in a deletion the message says whose deletion it is and to reject that change or
  edit another paragraph. Several matches are `ambiguous_match` and the message lists the first five with 20 characters of
  context and their offsets, unless `occurrence` (1-based) picks one. An empty `find` is `invalid_payload`.
- **A quote-folded second try.** When an exact `find` matches nothing, both sides are compared with curly quotes folded to
  straight ones. A unique match is used, and a `find_matched_loosely` warning says so. This is the one place where `find` is
  not exact, and it exists because agents type straight quotes into documents that use curly ones.
- **Offsets** (`start`, `end`, `at`) count Unicode scalar values in the block's Final text by default, which is what `TextMap::locate`
  takes and what `projection`'s `PlannedOp` carries. **The core owns the other unit too.** `Edit::offsets(OffsetUnit::Utf16)` makes every
  offset of the batch count UTF-16 code units, as a JavaScript string does, and `ops/target.rs` converts each one to scalar values
  from the Final text of its block as that text is when the operation runs. An atom is one character of that text and one UTF-16
  unit. `View` in the `json` format carries the same Final text, so a program that computes an offset from it and a core that
  reads it cannot disagree. An offset inside a surrogate pair or an atom is `invalid_payload`, from `revisions`. The unit applies to `Edit::ops`.
  The operations that `to_batch` makes carry scalar offsets and are not converted. Testkit cases cover a surrogate pair (an
  emoji) and an atom (a tab, a drawing), in both units.
- **The text an operation sees is the staged text.** An offset or a `find` in operation 2 is read from the document as operation 1
  left it. `Edit::files` orders its operations so that offsets stay valid (`Planned.ops` is in the order they must be applied).
- **A block that is not a paragraph** given to a text operation is `cross_block` ("t3 is a table. Name a paragraph in one of its
  cells"), as `revisions` specifies. A ref of the wrong kind for `row` is `invalid_payload`.

### 2.3 What each operation compiles to

`ops` resolves refs and ranges, applies the quote policy, and makes one or more calls on `revisions::Compiler`. It adds nothing
of its own to the document. The cross-paragraph `delete` is table 2.3 of `revisions.md` as written.

| Operation | Calls |
|---|---|
| `replace` | `replace_text(block, range, with)`. A whole paragraph is `0..len` |
| `insert` | `insert_block(anchor, place, NewParagraph { text, style })` for the first paragraph. Each next one takes `Effect::new_blocks` of the one before as its anchor with `Place::After`. The anchor may be a paragraph or a table |
| `delete` of a paragraph or table | `delete_block(block)`. A table is one call that walks its rows |
| `delete` of a range | in one paragraph `replace_text(.., "")`. Across paragraphs: when `start` is 0, `delete_block` of the first, otherwise `replace_text` for its tail, then `delete_block` for each paragraph in the middle, `replace_text` or `delete_block` for the last, and at most one `merge_blocks` at the end. Only paragraphs may lie between `ref` and `to`, else `cross_block` |
| `split`, `merge` | `split_block(block, at)`. `merge_blocks(first, second)`, where `second` defaults to the next paragraph of the same container that is not a deleted block, and a merge with none is `cross_block` |
| `format`, `paragraph`, `style`, `list`, `set` | `set_props(target, PropEdit)`. `style` checks that the style exists, by id first and then by name ignoring case. `list level` sets `numPr`, `list restart` is `new_list_instance(num_id, ilvl, at)` and then `set_props` with the new `numId`, and `list remove` sets `numId` 0 |
| `row` | `insert_row(anchor, place, cells)`, or `delete_block(row)` |
| `accept`, `reject` | one `resolve(tx, index, steps)` for each run of **consecutive** accept and reject operations, never moving an operation across an edit (`revisions` H4) |
| `comment`, `reply`, `resolve`, `edit-comment`, `delete-comment` | `comments::add`, `reply`, `set_resolved`, `edit`, `delete` with the `Compiler` and the stamp (`comments-and-notes` 4) |
| `delete-note` | `comments::delete_note` (`comments-and-notes` 6) |
| any, untracked | the same calls on a `Compiler` made with `Track::UntrackedBy(author)`: the stamp's author's own pending changes are edited in place, and **an edit that would rewrite or hide another author's pending change is `readonly_region`**, tracked or not (`revisions.md` decision 4): text inside their insertion, deletion or move, a run they inserted, properties under their `*PrChange`, a paragraph whose mark they inserted or deleted, and a paragraph, row or merge that would take their changes with it. Accept or reject that change first |

### 2.4 `projection`'s `PlannedOp`

`Edit::files` turns `Planned.ops` into `Batch` operations one to one, in order, and `Planned.origins[i]` stays attached to
operation `i`. The mapping is mechanical, and `ops/planned.rs` holds all of it:

| `PlannedOp` | `Op` |
|---|---|
| `Replace { block, start, end, with }` | `replace { ref: block, start, end, with }` |
| `Insert { anchor, place, paragraphs }` | `insert { ref: anchor, place, paragraphs }`, where each `style` is a `w:styleId`, which the style lookup takes first |
| `Delete { block }` | `delete { ref: block }` |
| `RowDelete { row }` | `row { ref: row, action: delete }` |
| `Split { block, at }` | `split { ref: block, at }` |
| `Merge { first, second }` | `merge { ref: first, second }` |

The batch carries the base token of the files, which `to_batch` has just checked against the state, so the per-block version
check cannot refuse it (`projection-and-index` 6.1). The quote policy applies to the texts of `replace` and `insert` here as well (2.8).

### 2.5 Validation, and which code each failure has

`ops::validate` runs first and touches nothing. It checks the whole batch before the first `Tx` is begun, so a malformed
batch costs no staging. Everything it finds is `invalid_payload`, except the limits, which are `limit_exceeded`. The checks that
need the document run as each operation compiles, and the first failure ends the batch. Every error raised in `ops` carries
`at_op(i)`, the zero-based index of the operation in the batch, and for `Edit::files` `document` maps that index to a file and
line (3.4).

| Code | When |
|---|---|
| `invalid_payload` | an unknown operation or field, a missing required field, two of a one-of group (`find` with `start`, `at` with `before`), a field of the wrong type, a malformed ref, comment id, note id, revision id or base token, an empty batch, an empty `find`, `text` or `with` where text is required, a character XML cannot carry or that stands for an atom (`revisions`), a style that does not exist (the message lists the nearest names), a property name the container does not allow (the message lists what it allows), a value that is not a number with a known unit, `list level` on a paragraph that is not a list item |
| `limit_exceeded` | more than `max_batch_ops` operations, each paragraph of an `insert` counting as one (for `Edit::files`, more changed blocks, and the message says how many and to apply the edit in parts), a batch over `max_batch_bytes`, a text over `max_op_text_bytes`, a scope over `max_scope_refs` |
| `not_found` | a ref, comment, note or revision id that does not exist, a `find` with no match, a section number out of range, the document has no such note |
| `ambiguous_match` | a `find` with several matches and no `occurrence`, a style name that two styles share, a comment id that two comments share (`comments-and-notes`) |
| `stale_ref` | refs: another document, another history, a block changed after the batch's version, a block removed earlier in the batch |
| `readonly_region` | the `revisions` list (a deleted block, a field, a locked control, another author's pending change), another author's comment for `edit-comment` or `delete-comment` |
| `cross_block` | a text operation on a table or row, a `delete` through blocks that are not all paragraphs of one container, a `merge` with no neighbour |
| `unsupported` | a comment on a block of a header, footer, note or text box, a reply to an unanchored thread, a target inside a text box (`comments-and-notes`, `revisions`), a readable file that was never written (5.2) |
| `scope_violation` | from the scope guard (3.3): a block or a part outside the scope, and an `accept` or `reject` by id of a revision outside it. It runs twice, before `check` on what the operations declared and after it on what `check` found changed |
| `validation_failed` | after the scope guard, from `check::edit` (3.1) |
| `stale_files`, `invalid_payload`, `readonly_region`, `cross_block`, `unsupported` | from `to_batch`, as `projection-and-index.md` lists them |
| `internal` | a post-condition of the compiler or the commit audit failed. It is a bug, and the batch is discarded |

### 2.6 Parsing a batch

`Batch::parse` is the one entry point for text. The first character `{` or `[` selects JSON, and anything else is block text.
A JSON batch is `{ "base": "...", "ops": [...] }`, and a bare array is refused because it has no base. Parsing checks the byte
limits before it allocates.

- **JSON** is serde over `Op`: `{ "op": "replace", "ref": "p42", "find": "shall", "with": "must" }`. Every payload struct has
  `#[serde(deny_unknown_fields)]`, so an unknown field is `invalid_payload` and the message lists the fields of the operation. A typo
  is never dropped.
- **Block text** is read from the table, not from per-operation code. Each `OpSpec` lists its fields with a kind (`Ref`,
  `Text`, `Number`, `Bool`, `Choice`, `List`), and the block parser builds a `serde_json::Value` for each stanza from those
  kinds and hands it to the same serde path. So a new operation needs no parser, and the two syntaxes cannot disagree about
  what a field is.
- **The grammar is provisional**, because agent evals may still change it, and it is confined to `ops/parse/block.rs` and
  the `FieldKind` list so that a spelling can change without touching the model:

  ```
  base: 3f9a0c11d2b4e577:7:51ac09f2e8b34d11
  ---
  @replace
  ref: p42
  find: the Supplier shall
  with: the Supplier must
  ---
  @insert
  ref: p42
  place: after
  paragraphs: <<END
  A first new paragraph.
  A second one.
  END
  ```

  A stanza starts with `@name`. A key that is not a field of that operation in `OPS` is `invalid_payload`, listing the fields. A field
  is `key: value` on one line, or `key: <<TAG` and raw lines up to a line that is exactly `TAG`, which is how text with quotes, braces or
  backslashes goes through with no escaping. What a newline means depends on the kind of the field, and is the same in the parser and in
  `to_block`:

  | `FieldKind` | One line | Heredoc | A newline inside the value |
  |---|---|---|---|
  | `Text` | the value is everything after the first `: `, trailing spaces kept | the lines, joined by `\n`, with no newline after the last | allowed only in a heredoc |
  | `List` | one entry | one entry for each line, an empty line is an empty entry (refused where an entry may not be empty) | never inside an entry |
  | `Ref`, `Number`, `Bool`, `Choice` | the value, trimmed | not allowed | never |

  A list field may also repeat its key. In block text each line of `paragraphs` is one paragraph and a style applies to all of them. A
  per-paragraph style is JSON only. `to_block` uses a heredoc for any text that has a newline or a leading or trailing space, and picks a
  tag (`END`, `END1`, ...) that no line of the text equals. A carriage return or a control character other than tab is `invalid_payload`
  in either syntax.
- **`Batch::to_json` and `to_block`** exist for the testkit, the editor and the skill examples, which print a batch the same way they
  read it. Block text cannot say everything JSON can (a per-paragraph style, a text that holds a carriage return), so the law that the
  two agree (H2) runs on the batches that block text can express, and the generator stays inside them.
- **A newline has a meaning, and each text `FieldSpec` says which in its documentation**, so that the generated reference shows it: in `replace.with` and in each
  `NewParagraph` text it is a line break inside the paragraph (and `\t` is a tab), in `comment.text`, `reply.text` and `edit-comment.text` it is a paragraph break
  (there is no line break inside a comment paragraph), and in `find` and `before` and `after` it matches a line break of the paragraph.
- **The operations reference** (`docs/reference/operations.md`) and the skill's table are generated from `OPS`. `cargo xtask docs`
  reads the table with `syn`, as it reads `error_codes!`, so `OPS` is written as one macro invocation of literals.

### 2.7 Human units

`set` and the typed fields take `12pt`, `0.5in`, `1.5cm`, `10mm`, `20twips` and, where the schema allows it, a percentage. `ops/units.rs`
converts to the attribute's own unit: twips for indents and spacing, half-points for font sizes, eighths of a point for borders.
Which unit an attribute has comes from `schema`, which the SDK input already names (`w:ST_TwipsMeasure_O12` and the others), so the
schema generator could emit a `Measure` for each attribute that has one. It does not yet, because `schema::measure_of` is not implemented. A value with no known unit for its attribute is passed
through as written, so `w:val` and the enumerations still work, and `check` validates the result under the no-new-violations rule.
An unparsable number is `invalid_payload`. Colours are `RRGGBB`, with or without `#`, or `auto`. **As built**, the table in
`ops/units.rs` holds only attributes whose unit is fixed: not `w:spacing/@w:line` (240ths of a line under `lineRule="auto"`), not a width whose
`w:type` decides its unit, not the `*Chars` indents, and no percentage. `paragraph`'s `line_spacing` handles the line itself.

### 2.8 Quotes

Evidence: the `revisions` design's open item 2 and the ambiguity table of `projection-and-index.md` both say that agents type `"` and `'` into documents that use curly quotes,
that the compiler writes the text it is given, and that the normalization is `ops`'s. Word's own typing converts them, so a
paragraph that mixes the two shows up in a lawyer's review. The policy is one pure function in `ops/quotes.rs`:

1. It applies to the `with` of `replace`, the text of `insert` (and the cells of `row`), including the files path, and not to
   comments, which are not document text. It shapes only what the text types: the stretches of a `with` that differ from
   the text it replaces, found by the minimal difference of the two. Text the agent carries over unchanged keeps its
   characters, so a paragraph that mixes curly and straight quotes, rewritten whole with one word changed, changes in that
   word only.
2. **Context.** The block being edited, and for an inserted paragraph its anchor. If that text holds any of the four curly quotes,
   the context is curly. If it holds a straight quote and no curly one, it is straight. If it holds neither, the nearest five
   paragraphs on each side of the block in the same container decide, in order of distance. If still neither, nothing changes.
   The paragraphs of one `insert` all follow their anchor and its neighbours as they were before the first of them went in, so they
   do not decide each other's quotes, and the context is read once for the operation.
3. **Conversion goes both ways.** In a curly context a straight `"` is opening after the start, a space or an opening bracket or
   dash, and closing otherwise. A straight `'` is an apostrophe or a closing quote after a letter, an opening quote after a space,
   bracket or dash, and an apostrophe before digits (`'90s`). In a straight context a curly quote typed into the text becomes the
   straight one, so the document stays as straight as it was. **A straight `"` or `'` right after a digit stays straight**, because `6'` and
   `12"` are feet and inches and a curly quote there would be wrong, **unless a quotation of that kind is open in the typed text, which it closes**: `"Clause 12"` becomes
   `“Clause 12”` and `'Clause 12'` becomes `‘Clause 12’`, while `a 12" pipe` and `5'11"` keep their marks, as does `"a 6' pole"` inside its quotation. Whether one is
   open is read off the characters written so far (a curly opening quote typed by the agent counts), so following a text a second time gives the same text.
4. **It runs before the diff.** `replace_text` diffs the old text against the new, so a straight quote the agent typed over a
   curly one becomes the curly character that is already there, and the diff does not record a change.
5. **It is reported.** Each operation that changed quotes carries a `quotes_curled` or `quotes_straightened` warning that says how
   many. The receipt's block text shows the result.
6. **`Quotes::AsTyped`** (`--quotes as-typed`) keeps the characters as typed.

The context depends on the paragraph and its neighbours only, not on a document-wide count that would have to be cached and
recomputed after an `export` and `resume`, so the same operation gives the same bytes either way (the comments law C10).

## 3. The apply pipeline

### 3.1 The steps

`Document::edit` is in `document/edit.rs`. Nothing in `self` changes until the last step, so every failure before it leaves the
document as it was, and **a dry run is the same path** up to the last step, which drops the state instead of installing it.

| # | Step | Fails with |
|---|---|---|
| 1 | **The envelope.** `RevStamp::new(author, date)` checks the stamp. The scope strings are parsed. For `Edit::ops`, `ops::validate` checks the batch (2.5) | `invalid_payload`, `limit_exceeded` |
| 2 | **Files only.** `projection::to_batch(&state, &files, &self.limits)`, whose first act is `check_files_base`. Then the baseline hashes are compared with `files.rendered` (3.5). Then **`max_batch_ops` is applied to the blocks the plan changes** (`to_batch` does not count them, so `document` counts the distinct blocks of `Planned.origins`, and an operation that names none counts as one), and `ops::planned_batch` maps `Planned.ops` to a `Batch` that names `files.base`, which `ops::validate_batch` checks with the operation limit lifted. A plan of no operations is an edit that changes nothing: a receipt with no operations, and the version does not move. `document/edit_files.rs` holds all of this, and the unit of offsets is scalar for files whatever `Edit::unit` says (H11) | `stale_files`, `limit_exceeded`, and G's list |
| 3 | **Authors.** When the batch has an `accept` or `reject` by author, `document` asks `projection::authors` for the code table, and gives `ops` `(code, name)` pairs | none |
| 4 | **Stage.** `let mut tx = state.begin()`. The scope is resolved against the batch's base token with `Tx::resolve`, so a scope naming a block that changed after the batch's version is `stale_ref` | `stale_ref`, `not_found` |
| 5 | **Apply.** `ops::apply(&batch, &mut tx, &cx)` (3.1.1). At its end, when the document has a comments part, `comments::reconcile(&mut tx)` | the codes of 2.5, each with `at_op(i)` |
| 6 | **Scope guard, first pass**, when the edit has a scope: the declared touches and targets, and the parts that changed (3.3) | `scope_violation` |
| 7 | **Check.** `check::edit_located(&tx, &declared)` (`check::edit`, and on a refusal the block that holds the first new violation, which the message cannot give), where `declared` is `Applied::origins` converted from `revisions::Origin` to `check::Origin` in `document/declared.rs`. `document` converts, not `ops`, so `ops` does not depend on `check`, and it **drops what `check` would call a bug of the converter**: an origin that names a block of a part with no schema (`check::schema_parts` says which parts have one), and a source that is not in the base and was made from nothing by an earlier call of the same edit. Then, when the edit has a scope, the **second guard pass** over `Verified::changed`, which no declaration can hide (3.3) | `validation_failed`, with `at_op` found by asking which operation touched or created that block (`Applied::op_touching`), and none when the violation is in no block. `scope_violation` |
| 8 | **Dry run only: the receipt**, built from `tx.parts()` before the commit consumes the `Tx` (section 4). A block that the batch created has no ref in the staged state, so none is shown | `internal` |
| 9 | **Commit.** `tx.commit()` returns the next `State` and a `Committed` (the refs minted, retired, rebound and merged). A batch that changed nothing returns the same state and the version does not move. A dry run commits as well, so every failure that the real edit would meet here is met here | `limit_exceeded`, `internal` |
| 10 | **A real edit: the receipt**, built from the next state, where a new block has its ref (section 4). A dry run only adds what the commit knows and the staged state does not: the refs retired and the number of blocks added | `internal` |
| 11 | **Install, or drop.** A real edit sets `self.state = next`. A dry run drops `next` and the `Committed`, and no ref it minted has been issued to anyone | none |

**What a dry run shows, exactly.** Its receipt equals the one the real edit would give, with three differences and no others: `dry_run` is true, `base` is the document's current
token (the one the next batch must name, not the one the edit would make), and a block that the batch created carries **no ref**: it is shown as new, with the ref of the block it
follows as its anchor, `added` lists no refs, and `added_count` says how many blocks there are. This keeps `ARCHITECTURE.md`'s invariant that a dry run never issues a ref, and the
reason is a hazard and not a taste. Refs are minted from counters, and counters are deterministic, so a dry run that showed `p301` and a real edit of a different batch afterwards would
give `p301` to another block, and an agent that trusted the receipt would edit the wrong paragraph. A ref that no one has seen cannot be trusted by mistake.

Four reasons for the order:

- **The scope guard runs before `check`, and once more after it.** `ARCHITECTURE.md` lists check, then the guard. The first pass reads only
  `Tx::touched()`, the targets that `ops` recorded and the generations of the parts, so it costs microseconds, and `check` costs up to
  200 ms on a structural batch (`check` 6). A refused batch should not pay for validation. The second pass needs what `check` already computed,
  the list of units it found changed from the dirty flags and an equality test, independent of any declaration (`check` 2.2), and it is the net
  under a compiler that forgot to touch a block. Both refusals are the agent's to fix, so either may come first.
- **A real edit's receipt is built from the committed state, and a dry run commits too.** `render_blocks` shows a block as the readable file will
  hold it, with its ref on the line, and a block that the batch created has its ref only after the commit. Rendering from the staged
  state and patching the refs in would make a real receipt differ from the file, so a real edit renders from the next state. A dry run
  needs the opposite, a receipt with no refs for new blocks, and `render_blocks` and `changes` read a `PartsView` of the staged state, which
  has none (`refs-and-state` S2). So one builder runs over a `PartsView`, over `next.parts()` for a real edit and over `tx.parts()` for a dry
  run, and the two receipts differ in the refs of new blocks and nowhere else. The dry run then commits into a `State` that it drops, for
  one reason: a commit has its own failures (a part over its size limit, the ref table over `max_refs`, a failed audit), and a dry run that
  skipped it would say yes to an edit that then fails. The cost is a commit, 5 to 103 ms in the spike (7.1), inside "a dry run costs the
  same". The caches that a real edit's receipt builds (document facts, numbers) are on the next state and serve the next view.
- **The receipt is built before the install.** The receipt reads the next state and the `Committed` report, and nothing that
  fails after the install can leave a half-edited `Document`. This is what makes "on an error, the `Document` is unchanged" true for
  the receipt too, and it costs one `State` kept alive for a few milliseconds. A dry run needs no other rule, because it never installs.

#### 3.1.1 `ops::apply`

```rust
pub(crate) fn apply(batch: &Batch, tx: &mut Tx<'_>, cx: &Context<'_>) -> Result<Applied, StetError>;
pub(crate) struct Context<'a> { pub stamp: &'a RevStamp, pub track: Track, pub quotes: Quotes, pub unit: OffsetUnit,
                                pub authors: &'a [(String, String)], pub scope: Option<&'a ScopeSet>, pub limits: &'a Limits }
pub(crate) struct Applied {
    pub ops: Vec<OpApplied>,                         // one per operation: name, effects, targets, owned blocks, changed
    pub side_effects: Vec<SideEffect>,               // the parts an operation declared it changes outside any block (3.3)
    pub outcomes: Vec<Outcome>,                      // one per compiler; the batch's receipt takes the union of their `created`
    pub resolutions: Vec<(usize, Resolution)>,       // by the index of the first operation of the run
    pub comments: Vec<(Option<usize>, CommentEffect)>,   // None for what `reconcile` removed
    pub origins: Vec<revisions::Origin>,             // Outcome::origins and Resolution::origins, in order
    pub touched_by_ops: usize,                       // how many of Tx::touched() came before `reconcile`
    pub warnings: Vec<(Option<usize>, Warning)>,
}
```

The loop makes one `Compiler` over the `Tx` and calls the primitive that each operation maps to. A run of consecutive `accept` and
`reject` operations ends the compiler (`finish`, which runs the compiler's post-conditions), calls `resolve` with the whole run, and starts a
new compiler, because both need `&mut Tx`. `ChangeIndex::note` is called after each compiler call only while a later operation of the
batch is an `accept` or `reject`, since the index is built at the first `resolve` from the staged trees and not before. Each call's `Effect` is kept with the
operation that made it, which is how a receipt, the scope guard and `validation_failed` find the operation of a block. A `SideEffect` is a part name and the
reason (comment parts, the comment styles, a list instance), which `ops` fills from `comments`'s `CommentEffect::parts` and from its own `list` restarts. `ops` never
refreshes a story. The `revisions` primitives and `state` do that.

### 3.2 Stamps, dates, authors and the clock

- **`Stamp { author, date }`** has public fields and no default. `Edit::ops` and `Edit::files` take one, so it cannot be forgotten, and
  `Edit::untracked()` still needs one because comments are written with an author and a date. `RevStamp::new` validates it: the author is
  non-empty, at most 255 characters and free of control characters, and the date is a real calendar date and time.
- **The date is the wall-clock time of the date the caller gives**, `YYYY-MM-DDTHH:MM:SS` with an optional fraction and an optional `Z`
  or offset, and what is written is its first 19 characters and `Z`. This is decision 5 of `revisions.md`: Word writes the author's local time with a
  `Z`, a UTC caller gets UTC, and `w16du:dateUtc` is not written. The receipt carries the date as written.
- **The core reads no clock.** The CLI passes its local time (`stetdoc-cli/src/clock.rs` reads it without `unsafe`),
  and `--date` overrides it. An agent harness should pass the user's local time, because a sandbox clock is often UTC and a redline stamped in
  UTC would show its reviewers a time that is hours off. The napi layer requires `author` and `date` and has no default for either.
- **The author is never defaulted anywhere in the core.** `STETDOC_AUTHOR` is the CLI's, and a missing author is the CLI's usage error.
- **Comments** take the stamp's author and date, and `initials` default to the first letters of the author's words, at most four
  (`comments-and-notes` 4).
- **Determinism.** The same bytes, batch, stamp and options give the same `save`, `export` and receipt, ids included.

### 3.3 The scope guard

`Edit::scope(refs)` takes refs and ranges. A range is `p40..p60`: both ends are refs of one story and in document order, and a reversed
range, or one with ends in two stories, is `invalid_payload`. It covers every block entry between its ends in the story's pre-order, so a
table that lies wholly between the ends is covered with its rows and cells, and a table that holds an end is covered from that end on.
`ops::scope` holds the resolution and the guard. The guard has three parts and runs in two passes, because a declaration can be wrong and
the scope is a promise to the person who gave it.

**Pass 1, before `check`, on what the operations declared.**

- **Blocks.** The set of blocks the batch changed is `Tx::touched()` up to `Applied::touched_by_ops`, plus the **targets** that each
  operation recorded in `OpApplied::targets`: the block of a `replace`, the anchor of an `insert` (a paragraph inserted before another
  touches nothing in the anchor, shape X, so the touch list alone would miss it), both blocks of a merge, the blocks of a range `delete`, the
  anchor blocks of a comment, the paragraph that holds the reference of a `delete-note`, and the block of every `set`. A new block has no
  ref and cannot be named in a scope, so it is covered by the anchor it was created next to. **Targets are never filtered by story.** The
  comments story is ignored for touches, and by the second pass, because `comments` creates and rewrites its own paragraphs and no scope can name them (`comments-and-notes` 8).
- **Containment follows the block tree.** A block is in scope when it or an enclosing block is named. An enclosing block is read from
  `BlockEntry::parent`, so a scope that names `t3` covers its rows and the paragraphs in their cells, and one that names `r17` covers the
  paragraphs of that row.
- **Owned blocks.** An operation may own blocks that it changes because of its target and that no scope can name: the paragraphs of a note that
  `delete-note` deletes, covered by the paragraph that holds its reference. They are `OpApplied::owned`, and they are not checked.
- **Parts.** The parts that the `Tx` changed or added are found by comparing generations with the base, as `check` does. A story part is covered by
  its blocks. Any other changed part is refused unless an operation declared it as a side effect, and what is declared is exact:

  | Part | Allowed only as | Declared by |
  |---|---|---|
  | `comments.xml`, `commentsExtended.xml`, `commentsIds.xml`, `commentsExtensible.xml`, `people.xml`, `[Content_Types].xml` and the main part's relationships | what a comment operation wrote, or the content type and relationship of a part it added | `comments`, in `CommentEffect::parts` |
  | `styles.xml` | the two comment styles that `add` and `reply` append | `comments`, the same list. As built, `ops` records the ids of the styles each comment operation appended, and the guard checks that the part differs from the base only by those `w:style` elements appended to its root |
  | `numbering.xml` | the `w:num` and `w:abstractNum` that a `list` restart of a block in scope adds | `ops`, when it calls `new_list_instance` |
  | the body's `w:sectPr` | nothing under a scope (below) | nobody |
  | every other part, `settings.xml` and `theme` included | nothing | nobody |

- **`resolve` and `edit-comment` change no document block**, so `ops` checks them by their anchor: in scope when every block of
  `Comment::anchor.blocks` is in scope, and always when the comment is unanchored. `delete-comment` is checked the same way, with its markers.
- **`accept` and `reject` are narrowed.** Resolving can change `styles.xml`, `numbering.xml` and the body's section properties, none of
  which a scope can name, so `Selector::All` and `Selector::Author` are not passed to `resolve` under a scope. `ops` calls `changes()` over the
  scope's blocks (one call for each cluster), keeps the changes of the author when one was named, and passes their ids as `Selector::Ids`.
  **A change with an element in the body's own `w:sectPr` is left out**, though `revisions` gives it to the last block of the body, and an id that names it is
  `scope_violation` with the message that the last section's properties name no paragraph. An id that is not among them is `scope_violation` ("revision #9 is not in the scope (p40..p60), or does not exist"). A change that
  reaches out of the scope (a move whose other end is outside, a deletion that spans the edge) is taken whole by the resolver, touches a block
  outside, and is refused by the block check, with the block named.
- **`reconcile` is exempt, and listed.** The markers it removes were taken away by a text edit that was itself in scope. They are the touches
  after `touched_by_ops`. The receipt lists them with the comment they belonged to, and the `comment_removed` warning names the blocks.
  This is the one exception to the rule that a change outside the blocks declared with `--scope` rejects the whole batch.
- **The body's last section.** `set` with `target: "section"` on the last section of the body edits the body's own `w:sectPr`, which belongs to no
  paragraph. Under a scope that edit is `scope_violation`, with the message that the last section's properties name no paragraph
  and that the edit must run without a scope. Every other section is the paragraph that ends it and is checked like a block. Without a scope
  nothing is refused.

**Pass 2, after `check`, on what changed.** `check::edit` finds the changed units from the dirty flags and an equality test with the base, and
it does not read a declaration (`check` 2.2). It hands the list on in `Verified::changed`, **as built a list of the paragraphs and tables that
changed, and the frame of a story part** (keyed by the part's root element), with no account of what differs in a table or a frame. Every block in the list
must be in scope, owned, or new, and **a block that is neither in the base nor among `Applied::new_blocks` is `internal`**: no operation reported making
it. **The frame is compared with the base's by `ops/scope_frame.rs`**, because a paragraph inserted into the body changes the frame's list of children
and that is no difference of the frame: blocks added or removed are skipped, and what else differs (a marker between blocks, a block-level content
control, the root's or the body's own attributes, the body's `w:sectPr`) belongs to the first block at or after it, as `revisions`'s `owner_entry` gives a revision
there. That block must be in scope, owned or new. The body's `w:sectPr`, the root's and the body's own attributes, and anything after the last block
belong to no block and are refused under a scope. **A table whose own attributes, properties and grid are the base's, so that only its rows differ, is covered by the rows**: they are
blocks that the operations named and the first pass checked, and the properties are compared by id and by the dirty flag against the base's
(`ops/scope.rs` `rows_only`), because a table in the list says only that something in its shell changed. Any other table in the list that no
scope holds is refused. A compiler that forgot to touch a block, or a resolver that changed one it did not report, is caught here, and the
failure names the block. **A row inserted or removed touches its table**, so a scope that names the row and not the table is refused by the
first pass, and the message names the table: scope the table to change its rows.

- **The error** names the first block or part outside the scope in operation order, its ref, the operation (and for `Edit::files` its line) and
  the scope as the agent gave it: "p57 is outside the scope (p40..p60). Operation 3 (replace p57) would change it."
- **Stamped blocks are not seen.** Commit stamps blocks that changed without an edit of their own (the paragraph after a changed mark, for
  one), after the guard has run. That follows `refs-and-state`: the guard reads what the batch touched, and those blocks changed because a
  neighbour did.
- **Tests.** Each row of the side-effect table has a test that passes and one that is refused (a comment `add` under a scope that holds its
  paragraph, then `accept` of everything with a `styles.xml` revision under a scope), and so does each narrowing and each exemption (H4).

### 3.4 Errors, and the operation that failed

A `StetError` from `ops` carries `at_op(i)`, set by the loop that calls each operation, unless a lower layer set it already. `check`
and the guard return errors without an index, and `document` finds it: `Applied::op_touching(block)` is the first operation whose
`Effect::touched`, `new_blocks` or targets hold the block.

For `Edit::files` the index of an operation is the index in `Planned.ops`, and the agent wrote files, not operations. `document` maps the
index through `Planned.origins[i]`, calls `StetError::at_line(file, line)`, begins the message with the ref (`p42: ...`), and clears the
operation index, so that `StetError`'s display reads "... (document.md line 57)" and never "operation 12". A deletion's origin is the line
next to it (`projection-and-index` 2). `StetError` gains two structured fields beside the operation index, `file` and `line`, with getters, so napi's
`StetdocError` and `--json` carry them and no surface parses a message. Adding fields to `StetError` changes no code and no exit class.

### 3.5 What is stale, and who says so

| Case | Checked by | Result |
|---|---|---|
| A batch from another document, another history or another version | `Tx::resolve` in `ops` | `stale_ref`, with the sentence of `refs-and-state` 5 |
| Readable files from another base | `to_batch`, first | `stale_files` |
| Readable files that another release wrote | `document`, after `to_batch`: each `(name, sha256)` of `Planned.baseline` against `files.rendered`, the hashes recorded at export. If `to_batch` fails with `readonly_region`, `invalid_payload`, `unsupported` or `unsupported_version` and `rendered` is not empty, `document` renders the files once more and compares, because a release that renders another tag would make every difference look like an edit | `stale_files`: "these files were written by another release of stetdoc. Export again and redo the edit" |
| A working copy with hand edits and an operation batch | `WorkingCopy::ensure_clean`, which the CLI calls before `edit <dir>` | `pending_edits` |
| A complement that was edited | `resume`: the digests, then `State::resume` | `corrupt_complement` |
| A complement of a newer major or minor version, or an unknown format | `resume` | `unsupported_version` |
| An interrupted write of the working copy | `WorkingCopy::recover`, called by any command that finds the marker (5.4) | finished or rolled back, never `corrupt_complement` |

An in-process `Document` whose files came from `files()`, as the editor's do, has an empty `rendered`, because `files()` sets none, and skips the
release-skew check. The release cannot have changed inside the process.

### 3.6 Limits

`Limits` gains four fields. Defaults are for production and every number says where it comes from.

| Field | Default | Bounds | Where the number comes from |
|---|---:|---|---|
| `max_batch_ops` | 5,000 | operations in a batch, each paragraph of an `insert` counting as one, since each costs what an operation does. On the files path, the **changed blocks** that `to_batch` counts. The refusal says how many blocks the edit changes and to apply it in parts (edit fewer paragraphs, pack, continue) | The budget is 50 operations in 1 s. The largest corpus document has 15,878 blocks. On the dense spike body an operation costs about 4 ms with its rescan, so a full batch takes about 20 s, which is bounded and which callers avoid by splitting |
| `max_batch_bytes` | 16 MiB | the bytes of a JSON or block-text batch | A rewrite of every paragraph of the largest document, at 1 KB each |
| `max_op_text_bytes` | 1 MiB | one text field | 300 pages at 3,000 characters is 0.9 MB |
| `max_scope_refs` | 10,000 | entries of a scope. A range is one entry, however many blocks it covers | A scope is a few refs and ranges that an agent was given, so 10,000 is far above any real one. It bounds the resolved list at about 1 MB, which is what a hostile scope can make the guard allocate |

The limits of `open`, `resume` and `compare` are `Limits`, as they are. `projection` bounds a readable file by `max_entry_bytes`, its lines by
`max_xml_nodes` and an escape payload at 64 KiB, and takes `Limits` as an argument of `to_batch`, which `document` gives it. `resume` bounds
each complement entry before it reads it: a package by `max_archive_bytes`, the manifest by 64 KiB and the ref map by `max_refs` times 128 bytes.

### 3.7 Atomicity, determinism, and what a dry run is

- **Atomicity.** The `Tx` never writes to the committed state, the receipt is built before the install, and a failure anywhere returns
  with `self.state` unchanged. A dry run runs steps 1 to 10 and drops the state, so it costs what an edit costs, fails exactly when the real edit fails, and shows the same
  warnings and the same `validation_failed`. The conformance suite checks that `export()` before and after a failed or dry batch is the same
  bytes, and law H14 checks the rest.
- **Caches** that a failed batch filled on the base are allowed by `state`'s contract: they hold only what a first read would build.
- **Determinism.** Ids come from `Tx`, dates from the stamp, quote conversion from the paragraph, and every list from document order, so
  two runs give byte-identical output (the `revisions` law R9, the comments law C9).

## 4. Receipts

### 4.1 The shape

`Receipt` is the form that `check.md` section 5 proposed, with `revisions.md` H2, `comments-and-notes.md` section 7 and `projection-and-index.md` section 8 folded in.

```rust
pub struct Receipt {
    pub base: String,                       // the new base token. For a dry run, and for a batch that changed nothing, the current one
    pub dry_run: bool,
    pub tracked: bool,
    pub stamp: Stamp,                       // the author, and the date as written
    pub operations: Vec<OpResult>,          // one per operation, in order
    pub revisions: Vec<AuthorRevisions>,    // the revisions the batch wrote, by author
    pub comments: Vec<CommentEffect>,       // what the comment operations did, and what reconcile removed
    pub retired: Vec<Retired>,              // the refs that are stale now, with the reason
    pub added: Vec<String>,                 // the refs of the blocks the batch added. Empty for a dry run, which issues no ref
    pub added_count: u32,                   // how many blocks the batch added: the same in a dry run and in the real edit
    pub moves: Vec<MoveEntry>,              // reserved for compare, which lists moved paragraphs here. Empty for an edit
    pub blocks: Vec<RenderedBlock>,         // the post-edit text of every block touched or added, at most 200
    pub blocks_omitted: u32,
    pub warnings: Vec<Warning>,             // every warning, deduplicated by code, block and message
    pub validation: Validation,             // what `check` verified
    pub text: String,                       // what the CLI and the editor show an agent
}
pub struct OpResult { pub index: usize, pub op: String, pub changed: bool, pub origin: Option<Origin>,
                      pub blocks: Vec<String>, pub added: Vec<String>, pub added_count: u32, pub revisions: Vec<ChangeEntry>,
                      pub removed_own: Vec<RemovedOwnEntry>,   // the author's own pending text, marks or rows that the operation removed outright
                      pub folded: Vec<FoldedEntry>,            // the author's own pending property changes that the operation edited in place
                      pub swallowed: Vec<i64>,                 // ids of other revisions that went with removed text
                      pub resolution: Option<Resolution>, pub comment: Option<String>, pub warnings: Vec<Warning> }
pub struct RemovedOwnEntry { pub kind: String, pub id: Option<i64> }          // text, mark, paragraph or row, and the id of the revision it was
pub struct FoldedEntry { pub id: i64, pub cancelled: bool }                  // revisions' Folded: the id of the change, and whether the edit undid it
pub struct MoveEntry { pub from: String, pub to: String, pub text: String }   // compare only
pub struct AuthorRevisions { pub author: String, pub counts: Vec<(ChangeKind, u32)>, pub changes: Vec<ChangeEntry> }
pub struct ChangeEntry { pub kind: ChangeKind, pub ids: Vec<i64>, pub date: String, pub block: Option<String> }
pub struct Resolution { pub action: String, pub resolved: Vec<ResolvedCount>, pub swallowed: Vec<i64>, pub removed: Vec<Retired>,
                        pub joined: Vec<(String, String)>, pub notes_removed: Vec<String>, pub warnings: Vec<Warning> }
pub struct CommentEffect { pub action: String, pub id: String, pub thread: String, pub author: String,
                           pub blocks: Vec<String>, pub warnings: Vec<Warning> }
pub struct Validation { pub parts: u32, pub units: u32, pub carried: Vec<Finding>, pub carried_total: u32, pub notes: Vec<Finding> }
```

`ChangeKind` is a public mirror of `revisions`'s, with the same twenty names, converted in `document/changes.rs`. `RemovedOwnEntry`, `FoldedEntry` and
`swallowed` are `revisions`'s `Effect::removed_own`, `folded` and `swallowed` as structured ids, so a program can see which of its own earlier
revisions an operation took away, and the warnings of 4.2 say it in words. A `ResolvedCount` is `{ author, kind
(insertion, deletion, move or format), count }`, which is a coarser view than `revisions`'s `RevKind` and what an agent needs. `Retired.reason` is
the text of `refs`' reason: `deleted`, `rejected` or `merged into p5`. `CommentEffect.action` is `added`, `replied`, `resolved`,
`reopened`, `edited`, `deleted` or `removed` (by `reconcile`), and its `blocks` hold the paragraphs of its markers, so the markers
that `reconcile` removed are listed with the comment they belonged to.

### 4.2 How each part is built

| Part | From | Notes |
|---|---|---|
| `operations[i]` | `Applied::ops[i]` and its effects | `changed` is false when the effects are empty: `replace` with the same text, `resolve` of a resolved thread. A no-op has a `no_change` warning |
| `revisions`, `operations[i].revisions` | `revisions::changes(story, tree, Some(range))` over the touched and new blocks of the next state, one call for each cluster of neighbouring entries, kept to changes with an id that `Outcome::created` lists | `revisions` 6.1: a window is extended to whole changes. An id is attributed to the first operation whose `Effect::minted` holds it. An insertion that coalesced into the revision of an earlier operation is listed there, and its own operation lists none |
| `Receipt.revisions` | the same, grouped by author | by construction one author, the stamp's. `AuthorRevisions::group` also groups the list of `Document::revisions`, for `check` of a file with nothing pending |
| removals the agent must know | `Effect::removed_own`, `folded`, `swallowed` | the structured `OpResult.removed_own`, `folded` and `swallowed`, and the warnings `own_text_removed`, `property_change_folded`, `revision_removed_with_text`, so an operation that took an earlier change of the agent's away says so (`revisions` H2) |
| `operations[i].resolution` | `Resolution` of the run, shown on its first operation | counts by author and kind, the refs retired (`Removed`, with the reason), the joins, the notes removed, the `revisions` warnings |
| `comments` | `comments`'s `CommentEffect`, with the ids as `c3`, and `reconcile`'s removals | an added comment is listed with its id so the next operation can name it, which a ref cannot do within a batch (`comments-and-notes` 7). A removal by `reconcile` carries the `comment_removed` warning, which names the blocks whose markers went |
| `retired`, `added`, `added_count` | `Committed::retired` and `minted` | `retired` holds refs of blocks that existed, so a dry run has the same list. `added` holds the refs minted and is empty for a dry run, which has only `added_count` |
| `blocks`, block `warnings` | `projection::render_blocks(next.parts(), base, keys, BlockSpec { format, context: 0 })` | the keys are the touched and new blocks and the survivors of merges. A row or table key is expanded to the paragraphs of a **new** row. At most 200 blocks, in document order, with the rest counted in `blocks_omitted`. A dry run renders `tx.parts()`, so a new block has `ref: None` and an `anchor`, and the line in its text is the one the agent would write |
| `warnings` | the warnings of the blocks, `Planned.notes` (`looks_like_rewrite`, `split_at_wrap`), `Outcome::warnings`, the comment warnings, the quote and `find` notes, and the notes of `check` | one list, deduplicated, in the order above |
| `validation` | `Verified` from `check::edit` | `carried` lists up to ten problems that were already in the blocks the edit touched, so an agent knows a later `check` will still show them |
| `numbering`'s tier-3 number format `Note` | `render_blocks` | a block whose clause number may not match Word carries the `numbering_approximate` warning of `projection-and-index` 3.6, and so does the head of every view. `check` does not read numbering (`styles-and-numbering` 2.4), so this is where the note reaches an agent, in the receipt of a block it edited and in the view |

### 4.3 The text

`Receipt.text` is the agent-facing form: a head line, one line for each operation, the blocks as the readable file will hold them, then the
warnings and what `check` carried. It is deterministic and token-lean, and it lives in one file, `document/receipt_text.rs`, so that the format experiments
can change its wording without touching the data. The block lines are `projection`'s, in the working-copy format. An illustration of the layout:

```
applied 2 operations, tracked as Jane Doe 2026-10-06T10:00:00Z. base 3f9a0c11d2b4e577:8:77c1d2e3f4a5b6c7
0 replace p42: deletion #7, insertion #8
1 comment p57: c12 added
p42 The Supplier {--shall--}{++must++} indemnify the Customer. (the line as the file holds it)
p57 ...
warning quotes_curled p42: 2 straight quotes were curled to match the paragraph.
checked 3 units. 0 problems were already there.
```

A dry run says `dry run: nothing was applied. A new block has no ref until the edit is applied`, shows each new block as new with its anchor, and shows the rest as the real edit
would. Its base is the current one, so the next batch can use it. An untracked edit says so and lists no revisions.

## 5. The working copy

### 5.1 Layout

```rust
pub struct WorkingCopy { pub files: Vec<(String, String)>, pub complement: Complement }   // readable files by name, and .stet/
pub struct Complement { pub entries: Vec<(String, Vec<u8>)> }                              // names relative to .stet/
```

The CLI writes `files` into the directory and `entries` into `.stet/`, and reads them back the same way. napi marshals the two
lists as objects of strings and buffers. The editor keeps them in memory. `WorkingCopy.files` holds the top-level files of the directory
whose extension the format uses (`.md` for `stet/1`, which `Capabilities` lists), and nothing else.

The complement is the entries below. Everything that a later export may replace is **named by generation**, the state version it was
written at and by a short digest of their bytes, so that no entry of the manifest that is on disk is ever overwritten, even by an export at the same generation (a refresh
after a release upgrade, say) whose bytes differ (5.4):

| Entry | Holds |
|---|---|
| `manifest` | UTF-8 text of at most 64 KiB in one canonical form, one line each, in this order: `stetdoc-complement 1.0`, `generation <n>`, `format stet/1`, `base <token>`, `original original.docx <sha256>`, `head head.<n>.<sha8>.docx <sha256>` or `head original`, `refs refs.<n>.<sha8>.map <sha256>`, `file <name> <sha256>` for each readable file as written, by name, and last `manifest-sha256 <hex>`, the SHA-256 of every byte above it |
| `original.docx` | the bytes the namespace was derived from: what `open` was given. Written once and never replaced |
| `head.<n>.<sha8>.docx` | the package as of the export at generation `n`, with the first eight hex digits of its SHA-256 in the name. **Absent** when it equals the original (the manifest says `head original`), so an unedited working copy costs one package and not two |
| `refs.<n>.<sha8>.map` | the ref map of `State::export` (`refs-and-state` 8), at generation `n`, named the same way |

Because the digest is in the name, one name means one content. Two exports at one generation with different bytes get different names, and an export never has to
write over an entry that a committed manifest still names. (The eight digits are for the name only. The manifest holds the full SHA-256, which `resume` checks.)

`.stet/` holds no span index, id map or source maps. The ref map replaces them.

### 5.2 `export`, `files` and `pending_files`

- **`files()`** is `projection::render_files(&state, &format)`, and the files carry the base token, the format and an empty `rendered`.
  The editor calls it after each commit. It costs 16 to 360 ms on the spike bodies (7).
- **`export()`** calls `files()`, then `State::export()` (the head bytes, the ref map, the token, the digests), hashes each file, and
  hashes the original. The generation is the state version. It names the head and the ref map by it, writes no head when the head equals the
  original, and ends the manifest with its own digest. Entries and manifest lines are in a fixed order, so the same state gives the same bytes.
- **`WorkingCopy::pending_files()`** parses the manifest, compares the SHA-256 of each `file` entry with the text in `files`, and returns the
  files that differ, as `ReadableFiles { base: the manifest's, format: the manifest's, files: the pending ones, rendered: the manifest's
  hashes of those names }`, or `None` when none differ. A file that was written and is now missing is `invalid_payload` ("document.md
  was written and is now missing. Restore it, or export again."). **A file that was never written is `unsupported`, never ignored**: a
  `notes.md`, `headers.md` or `comments.md` for a document without one is the agent trying to add a footnote, a header or a comment
  from text, and the message names the operation to use (`projection-and-index` 6.1), and any other name with the format's extension is refused the same way,
  because dropping text an agent wrote is the one thing a working copy must not do. This is `projection-and-index` 6.1, kept as a pure function of the
  working copy.
- **`ensure_clean()`** returns `pending_edits` naming the pending files when there are any. The CLI calls it before `edit <dir>`.

### 5.3 `resume`

1. A working copy that still has the interruption marker (an entry named `writing`: the CLI reads every file of `.stet/`, the marker included) is refused with
   `corrupt_complement` and the message to recover it first, as are `pending_files`, `ensure_clean` and `write_plan`. Only `recover` reads a directory in the middle of a
   write, because the readable files of one can be the old render under the new manifest, which would read as hand edits. (To re-render during a recovery, the caller
   resumes the directory without its `writing` entry.) Then entries are found by the names the manifest gives, and each is bounded by `Limits` before it is read (3.6).
   Entries with other names are ignored. The manifest is read first.
2. The manifest is parsed strictly, and **re-serialized**: its bytes must equal the canonical form, and its last line must be the SHA-256 of
   the rest. So any single-byte change to it is `corrupt_complement` (law H7), including a change to a file hash that would otherwise make an
   untouched file look edited. A `stetdoc-complement` of a newer major **or minor** version, or a `format` that no registered adapter reads, is
   `unsupported_version`: a release reads the versions up to its own, upgrading older minors on read, as `ARCHITECTURE.md` says.
3. The SHA-256 of `original.docx` is checked against the manifest. Then `State::resume(head, refs, &Recorded { base, head_sha256, refs_sha256 },
   Some(original), limits)` checks the other two digests, opens the head (the original when the manifest says `head original`), rescans every
   story and restores the ref map against the scan, which refuses anything that is not exactly what `persist` wrote.
4. The `files` of the working copy are not read. The agent can write to the directory, so `resume` trusts nothing in it, and the readable
   files matter only to `pending_files`, which compares them with the manifest and never trusts them as input to the ref table.

After `resume`, counters, stamps and lineage continue where they were, so the next new block gets the number it would have had in an
unbroken session, and a batch written before the export is still valid after it, subject to the same checks (`refs-and-state` 8.5). A view
of a working copy shows the state at its last export and ignores hand edits that are pending, and the skill says so. `check` shows what
they would do.

### 5.4 Writing it to disk, and surviving a crash

The core does no I/O, and the order of the writes is still the core's, because the order is what makes a crash harmless. `WorkingCopy::write_plan`
returns the steps and the CLI executes them one at a time. Every write is a temporary file, an fsync and an atomic rename, so a file is whole or
absent. **The one commit point is the rename of `manifest`.** Until then the old manifest names the old entries, which no step has touched.

```rust
pub struct WriteOptions { pub consumed: Vec<String>, pub refresh: bool }   // readable files whose hand edits this export took in; refresh every file
pub struct WritePlan { pub steps: Vec<WriteStep> }
pub enum WriteStep {
    Marker { path: String, bytes: Vec<u8> },   // .stet/writing, first
    Entry  { path: String, bytes: Vec<u8> },   // a new complement entry
    Commit { path: String, bytes: Vec<u8> },   // the manifest, the commit point
    File   { path: String, text: String },     // a readable file, after the commit
    Remove { path: String },                   // a stale entry, then the marker last. A missing file is not an error
}
pub struct Recovery { pub steps: Vec<WriteStep>, pub finish: WriteStep, pub rerender: Vec<String>, pub kept: Vec<String>, pub warnings: Vec<String> }   // finish: the marker's removal, run last
```

`write_plan(disk, options)` takes the working copy as it is on disk now (`None` for the first unpack) and gives, in this order:

1. **The marker**, `.stet/writing`: `stetdoc-writing 1`, `from <generation or none>`, `to <generation>`, `old-manifest <sha256 of the manifest on disk, or none>`,
   `new-manifest <sha256 of the manifest being committed>`, `drop <name>` for each readable file that the plan removes, and `file <name> <sha256>` for the text of each readable
   file on disk now. The two manifest digests, and not the
   generations, tell recovery which side of the commit it is on, because a refresh export at the same generation has two manifests of one generation.
2. **The new entries**: those whose name is not in the old manifest (the head and the ref map of the new generation, and `original.docx` for the first unpack). A
   name that the old manifest has means the same bytes, so it is not written again, and a name that is new never overwrites an entry that the old manifest names,
   so the old manifest is still valid, even for a refresh at the same generation whose ref map differs. The plan does not trust the name alone, which holds eight
   hex digits of the digest: it compares the full digest that the old manifest records for that name with the new one's, and a mismatch (a collision) is refused with `internal`.
3. **The commit**: the new manifest.
4. **The readable files**, each only if its text differs from the new render, and each only if it is **unmodified** (its hash is the old manifest's),
   missing, in `consumed`, or `refresh` is set. Any other file holds a hand edit that this export did not take in, and the plan refuses with
   `pending_edits` before step 1 rather than overwrite it.
5. **The removals**: the readable files that the new render no longer has (under the rule of step 4: an unmodified file goes, a hand-edited one is refused unless
   consumed or refreshed), then the entries of the old manifest that the new one does not name (including a `head.<n>.<sha8>.docx` when the head equals the original
   again), then the marker.

`recover(disk, marker)` is called by any command that finds `.stet/writing`, before it does anything else, and returns the steps that finish or undo
the write. It compares the SHA-256 of the manifest on disk with the marker's two digests:

- **It is the `old-manifest`: the commit was not reached.** Roll back: remove every head and ref-map entry that the manifest does not name (only names of that form, so a lock
  file of the CLI's is never touched). The readable files were not touched, because they are written after the commit, so any hand edits are intact and still pending.
- **It is the `new-manifest`: the commit happened.** Finish: remove every such entry the manifest does not name. Each readable file is checked against the manifest
  and the marker. If its text is the new render, it is done. If it is what the marker recorded (the old render, or the hand edits that this write
  consumed, which the new head holds as tracked changes) or the file is missing, it is **re-rendered** from the committed head with
  `Document::resume` and `files()`.

  **Any other text was written after the crash**, and what happens to it depends on what it was written against. If the marker recorded the same text that the new manifest
  records (the file did not change with this write), the edit is against the new render, and it stays pending (`kept`): `pending_files` reports exactly it. Otherwise
  the edit was made on a render that the commit replaced, and a pending edit would be read against the new render, so the committed change would look like the agent's
  own and be undone. That text is **saved aside** instead, as `<name>.<8 hex digits of its digest>.stale`: a name with no readable extension, so that it is never a
  readable file, and one that a text always has the same of and two texts never share, so saving it again is harmless and an earlier saved edit is never overwritten.
  The file is re-rendered, and `Recovery.warnings` names the saved file and tells the agent to redo the edit on the fresh one. The caller prints the warnings before it
  runs the steps. A file that the plan removes (the marker lists it in a `drop` line) is removed if it is unmodified, and saved aside first if it was edited after the crash.
  Recovery removes nothing else: the `file` lines record every readable file on disk, a stray `.md` file in neither manifest included, and a write leaves that alone, and
  so does recovery. A refinement would keep an edit that was made on a file the plan had already rewritten, but that cannot be told from one made
  on the old render, and losing no edit is the rule.
- **Anything else** is `corrupt_complement`. A marker with no manifest is an interrupted first unpack, with the message to delete the directory and
  unpack again.

`Recovery.steps` end before the marker's removal, which is `Recovery.finish`. The caller writes the files of `rerender` between the two, so that a crash after the
steps and before the re-rendering is recovered again from the same marker, and not left with a file that looks hand-edited.

Every step of `recover` is idempotent, so a crash during recovery is recovered by running it again. `export` on request is `write_plan` with
`refresh: true`, which the CLI offers as a flag and uses after a recovery that kept nothing, and which overwrites hand edits only when the agent
asked. The CLI also takes an advisory lock on `.stet/` for the whole of a write, which keeps two processes from interleaving steps and is the CLI's.

## 6. The smaller methods

- **`view`** is `projection::view(&state, spec)`. `Scope::Revisions` and `Scope::Comments` need nothing from `document`: they are built in
  `projection` over `revisions::changes` and `threads`, which are rows of its dependency list (the tables of `projection-and-index.md` and `comments-and-notes.md`). A view of a working copy
  shows the state at its last export and ignores pending hand edits (5.3). The Final text in the `json` format is the text that offsets count
  (2.2).
- **`lookup`** is `projection::lookup`, and the text of the answer is `projection::render_lookup(&state, &lookup, &self.format)`, put in a new
  `Lookup.text` so a surface prints one field, as it does for `View.text`.
- **`check`** is `check::document(&state.parts())`: every part, every unit, every finding, and it never blocks. It returns `check`'s `CheckReport` as
  it is.
  (`document/checking.rs` holds `check` and `check_pending`, and `document/declared.rs` the `Declared` of an edit.)
- **`check_pending`** is the whole of `stetdoc check`, so that the CLI, napi and a host application call one function and cannot disagree about it:

  ```rust
  pub struct CheckResult {
      pub receipt: Option<Receipt>,            // the dry run of the pending files. None when none are pending
      pub existing: Vec<AuthorRevisions>,      // the revisions already in the document, by author
      pub report: CheckReport,                 // Document::check
      pub strict: bool,
      pub exit_status: u8,                     // 0, or 5 when strict and the report has a violation
  }
  ```

  With pending files it runs `edit(Edit::files(pending, stamp).dry_run())` **through `Document::run_edit(&self, &Edit)`, which `edit` calls too and which
  returns the receipt and the next state without installing it**, so `check_pending` takes `&self` and cannot change the document, and returns its
  receipt, which is the dry-run receipt of 3.1 and so shows new blocks with no ref and the current base. Pending files with no stamp are `invalid_payload`, because the receipt shows the revisions by author and date. The exit status is the one `check.md` proposed: 0 when the pending edit would add nothing, whatever the document already holds; 5 when it would, which is
  `validation_failed` and comes back as the error, whose class is 5; 6 when the input cannot be read, which is an error of class 6; and with
  `strict` any violation in the report, new or old, makes `exit_status` 5. Every other failure is an error and its own class. The surfaces
  print the result and exit with `exit_status`, or with the class of the error.
- **`revisions`** is `revisions::changes(story, tree, None)` for every story part, filtered by `author` (a name or a code) and `kind`, and mapped to
  `RevisionInfo { ids, kind, author, date, block, story, inserted, deleted }`, where `block` is the ref of the first block of the change and
  `inserted` and `deleted` are the text the change adds and removes, each cut at 200 scalar values (a `text_truncated` flag says so). It is the
  list that an agent harness reads to tell which of its own claims a change touches, which is why it carries the text. It is one pass over
  each story, 11 ms at 13 MB. The text comes from the `inserted` and `deleted` fields of `revisions::Change`.
- **`save`** is `state.to_package()?.write()`. It validates nothing, because every edit was validated before it committed (`ARCHITECTURE.md`),
  and an unedited document returns the input bytes exactly (law L1). `Saved { bytes, receipt }` carries
  `SaveReceipt { base, edited, sha256, revisions }`, with every revision of the saved document by author.
- **`inspect`** takes `"p42"`, `"t3"`, `"r17"`, `"styles"`, `"numbering"` or `"sections"` (`document/inspect.rs`, `inspect_marks.rs` and `inspect_lists.rs`). Every block then lists its tracked changes (ids, kind, author, day, text) and the
  comments anchored in it, or says it has none, so text that only looks like a change can be told from one. A paragraph gives its kind and
  story, its style (name and id), its list and level with the number it shows, its clause (the full form, and whether it was composed across lists), then every
  effective paragraph property with the level of the hierarchy that gave it (default, table style, numbering, paragraph style, character style, direct, or the levels of a
  toggle), from `styles`'s `Cascade::effective` on the paragraph's `Site`, and the distinct run formattings with the characters of the **Final** text that each covers. `pStyle`,
  `numPr`, `rPr`, `sectPr` and `pPrChange` are not listed as properties, because the style and the list have lines of their own. The run formatting is the twelve
  character properties that a reader asks about (font, size, bold, italic, underline, strike, caps, small caps, colour, highlight, vertical alignment, hidden), not the whole
  set. **A table or a row gives its kind, story, style and what it holds, and not the effective properties through the table site**, because that would need a cell site, which nothing else uses. `styles` lists each style (id, name, kind, based on, next, how many paragraphs use it), `numbering` each list
  (`numId`, its abstract list, each level's format, text and start) and `sections` each section (the paragraph that ends it, its break type, page size and orientation, margins, columns,
  and the headers and footers it names). `Inspection { base, target, text, items }` has the lines the CLI prints in `text` and the same as `InspectItem { label, value, source }`.
  It reads `wordml`'s definitions and the `styles` cascade, and no new module.
- **`capabilities`** is built from `OPS` and the tables of codes: the release, the formats with the file extension each uses, the complement version, each operation with its
  fields, `trackable` and `reversible`, the error codes with their exit classes, the warning and finding codes, and the default `Limits`.
  The CLI reference, `stetdoc capabilities --json` and the skill's tables read it.
- **`compare`** opens both packages under `CompareOptions.limits`, calls `compare::compare(&original, &revised, &stamp)`, and wraps the
  `State` it returns in a `Document` with the bytes of the revised package as its original. `Compared { document, receipt }` reuses `Receipt`
  with no operations, the revisions by author, and `compare`'s warnings. The moves go in `Receipt.moves`, which only `compare` fills. `document` owns the
  hook that makes a `Document` from a state and the bytes it stands for, `Document::from_state`, and `document/compare.rs` is its one caller besides `open`.

## 7. Performance and memory

The budgets are `open` 300 ms, `view` 200 ms, a 50-operation `edit` with validation of the dirty units 1 s (a dry run costs the same),
`save` and `export` 1 s each, and the memory formula of `ARCHITECTURE.md`. `open`, `view`, `save` and `export` are other modules' code
called through one line each, so what `document` adds is in `edit`: the stage, the compile, the two passes of the scope guard, `check`, the commit and the
receipt. A dry run adds nothing to that, because it commits too (3.1).

### 7.1 A spike of the receipt

A throwaway test (not kept), run before `document` existed, made 50 tracked `replace_text` calls through `revisions::Compiler`
on synthetic bodies, then built the receipt's parts over the committed state with `projection`'s `render_blocks` and `revisions`'s `changes`. It is a release
build on the development Mac with other agents running, one run, so a figure moves by up to a factor of two. The first body has 5,000
paragraphs and a 1.0 MiB main part, the size of a 300-page contract. The second has 18,000 paragraphs of eleven runs and a 28.6 MiB main part,
which is 1.6 times the largest benchmark input (`node-dense-18mb`), so it is a stress case and not a benchmark input.

| Step, for 50 tracked operations spread through the body | 1.0 MiB | 28.6 MiB |
|---|---:|---:|
| `open` | 5 ms | 91 ms |
| compile: 50 `replace_text` calls on a `Tx` | 31 ms | 208 ms |
| commit | 5 ms | 103 ms |
| receipt: `render_blocks` of the 50 touched blocks, first call | 6 ms | 71 ms |
| receipt: the same, second call | 0.5 ms | 1.3 ms |
| receipt: `changes` for 50 windows of one block | 0.4 ms | 1.1 ms |
| `changes` over the whole story (`Document::revisions`, `save`) | 1.7 ms | 42 ms |
| first `view` after the edit | 15 ms | 80 ms |
| `render_files` (`files`, and the first half of `export`) | 16 ms | 357 ms |
| `State::export` (the other half of `export`) | 5 ms | 100 ms |

What it shows. The receipt costs 7 ms on the contract and 72 to 113 ms on the stress body, nearly all of it the first `render_blocks`, which
builds the document facts, the cascade and the numbers of the new state and leaves them cached for the next view. The windowed `changes` is
about a millisecond, because `revisions` makes it cost what its blocks cost. The 300-page case is not a risk. What it did not measure: `ops`'s own
work (a `Tx::resolve` and one text map per operation), `check`, `to_batch`, and memory.

### 7.2 The budget, accounted

| Component | 300-page contract | 28.6 MiB stress body | Source |
|---|---:|---:|---|
| stage, compile and commit | 36 ms | 311 ms | spike |
| receipt | 7 ms | 72 to 113 ms | spike |
| `ops` overhead: refs, ranges, quote context, scope | under 5 ms | under 20 ms | estimate, one `Tx::resolve` and one `find` per operation. The rows of 7.4 include it |
| `check::edit` | 3 to 13 ms, plus 5 to 60 ms for the census of a changed part | up to 200 ms | `check` 6, measured on the benchmark inputs |
| **the operations path** | **about 0.1 s** | **about 0.65 s** | sum |
| `to_batch`, the files path only | 49 ms | 235 to 254 ms | `projection-and-index` 6.9, on the benchmark inputs |
| **the files path** | **about 0.15 s** | **about 0.9 s** | sum |

The `revisions_edit_50` benchmark (compile and commit together) measures 61 to 229 ms on the five benchmark inputs, where the spike's
body gives 311 ms. With it, the operations path on the benchmark inputs is about 0.1 to 0.5 s and the files path 0.15 to 0.75 s. The margin on the
stress body is thin, and the benchmark harness holds every input to the budget. **`fields-13mb` was the one gap**: before a fix in `state`'s
story refresh, its compile alone was 445 to 543 ms when an edited paragraph held a field. With the fix it fits (7.4). If the benchmark goes over on an input, the remedies are behind interfaces and none is a smaller batch: the
census of `check` that a text-only batch skips, a receipt that renders at most the first 50 blocks in full and the others as refs, and
`to_batch`'s baseline cache, which `projection` designs. `view` is `projection::view` with nothing added, so its budget is `projection`'s (32 to 126 ms
cold on the five inputs).

### 7.3 Memory

- A `Document` holds the `State`, the `Limits`, the format and a shared handle to the opened bytes, so `open` costs no copy beyond the one
  `Package` already holds. `resume` holds the original once more, because the complement is the caller's and a document outlives the call.
- An edit adds the `Tx` overlay, which shares the chunks of the node arenas and costs what its edits write (`refs-and-state` 10.1), `Applied`,
  which is a few hundred bytes for each operation and each touched block, and the receipt, which is capped at 200 blocks of text.
- **The files path does not add its peak to the compile's.** `to_batch` peaks at 116 MiB on the 18 MB body (budget 142), builds a `Planned` of
  operations and a baseline of hashes, and returns. `document` consumes the `ReadableFiles` by value, so its text is gone before `state.begin()`.
  The peak of the edit is the larger of the two and not their sum.
- `export` holds the files' text (15.7 MiB on the stress body), the head and the ref map together, the way `State::export` and `render_files` each
  already do. The memory rows of the new benchmarks (7.4) use the counting allocator and are held to the formula.

### 7.4 Benchmarks

The rows are in `bench/document.rs`, which depends on `document` and nothing else (the row of the `bench` module gains `document`): `document_edit_ops`
and `document_edit_files` (50 operations on the five inputs, 1 s), `document_dry_run` (1 s), `document_open` (300 ms), `document_save`,
`document_export`, `document_resume` (1 s each) and `document_files`. They run through the public path only, so they measure what a caller pays.

Six of the rows: `document_edit_ops` (on a document just opened, so it pays for keeping the original
and for the caches a first edit builds), `document_edit_ops_again` (the same on a document edited once), `document_dry_run`,
`document_edit_files` (fifty changed lines of `document.md`), `document_view_cold` and `document_export`. Since the refresh fix in `state` the
operations edit paragraphs with fields too, and since `State::opened_source` the first edit keeps the original as a shared handle,
which removed a write of the whole package (it had cost 160 to 450 ms on a loaded machine). One run on the development Mac, release
build, with other agents' test runs going (load average 8 to 22), so a figure moves by up to a factor of two:

| Row | contract-300p | dense-contract-300p | worst-blocks-12mb | node-dense-18mb | fields-13mb | Budget |
|---|---:|---:|---:|---:|---:|---:|
| `document_edit_ops` | 106 ms | 257 ms | 340 ms | 487 ms | 421 ms | 1 s |
| `document_edit_ops_again` | 96 ms | 637 ms | 246 ms | 388 ms | 429 ms | 1 s |
| `document_dry_run` | 116 ms | 277 ms | 263 ms | 470 ms | 437 ms | 1 s |
| `document_edit_files` | 234 ms | 795 ms | 478 ms | 959 ms | 683 ms | 1 s |
| `document_view_cold` | 31 ms | 67 ms | 48 ms | 125 ms | 77 ms | 200 ms |
| `document_export` | 61 ms | 151 ms | 204 ms | 284 ms | 277 ms | 1 s |

Two runs of the edit rows alone on the two largest inputs, at a lower load, gave `document_edit_ops` 407 and 380 ms,
`document_edit_ops_again` 353 and 350 ms and `document_edit_files` 628 and 535 ms (node-dense-18mb, fields-13mb). The files path on
`fields-13mb`, 914 ms before that fix with field paragraphs skipped, is 535 to 683 ms with them. Every row is within its time and memory
budget in the run above, and the files path on `node-dense-18mb` has the thinnest margin.

The other four rows: `document_open` (300 ms), `document_save` of a document that the
fifty-operation edit changed (1 s), `document_resume` of the working copy such a document exported (held to the 1 s of the export it
reverses) and `document_files`, the readable files alone (1 s, the export's). One run, release build, at a load average of 27 to 80:

| Row | contract-300p | dense-contract-300p | worst-blocks-12mb | node-dense-18mb | fields-13mb | Budget |
|---|---:|---:|---:|---:|---:|---:|
| `document_open` | 16 ms | 89 ms | 44 ms | 123 ms | 54 ms | 300 ms |
| `document_save` | 70 ms | 134 ms | 49 ms | 103 ms | 216 ms | 1 s |
| `document_resume` | 149 ms | 216 ms | 204 ms | 387 ms | 190 ms | 1 s |
| `document_files` | 172 ms | 349 ms | 394 ms | 542 ms | 457 ms | 1 s |

Every row is within its time and memory budget.

## 8. Module layout and dependencies

Every file stays under 600 lines, and each `mod.rs` opens with its contract. Counts are estimates.

```text
api/         mod.rs, error.rs, limits.rs (+4 fields), view.rs (+ warning codes), files.rs, lookup.rs, check.rs   from projection and check
             stamp.rs, options.rs        Stamp, OpenOptions, CompareOptions, Quotes, OffsetUnit             ~130
             op.rs                       Op and its payload structs, Within, PropSet, NewParagraph          ~480
             op_spec.rs                  the OPS table (one macro invocation), OpSpec, FieldSpec, FieldKind ~450
             batch.rs, edit.rs           Batch, Edit and its builder                                        ~250
             receipt.rs                  Receipt, OpResult, AuthorRevisions, ChangeEntry, ChangeKind, ...   ~380
             revision.rs, inspect.rs     RevisionInfo, Inspection                                           ~120
             working_copy.rs, compare.rs, capabilities.rs   WorkingCopy, Complement, WritePlan, Recovery, Saved, Compared, Capabilities  ~320
ops/         mod.rs                      contract, apply, the loop over compilers and resolve runs          ~320
             context.rs                  Context, Applied, OpApplied                                        ~160
             validate.rs                 the checks that need no document (2.5)                             ~260
             target.rs                   refs, find with the folded retry, offsets, ranges                  ~380
             quotes.rs                   the quote policy                                                   ~200
             units.rs                    human units to attribute values                                    ~260
             text.rs                     replace, delete (and across paragraphs), split, merge              ~320
             insert.rs                   insert, row                                                        ~260
             props.rs, props_typed.rs    set, and the typed fields of format, paragraph, style, list        ~560
             review.rs                   accept and reject: selectors, author codes, runs                   ~200
             comments.rs, notes.rs       the comment operations and delete-note over comments' functions      ~300
             planned.rs                  projection's PlannedOp to Op                                              ~120
             scope.rs, scope_parts.rs    the scope: resolution, containment, both passes, the side effects  ~480
             parse/mod.rs, json.rs, block.rs, fields.rs   Batch::parse and the two syntaxes               ~620 in all
             tests_*.rs                  tables and laws, one file for each family
document/    mod.rs                      contract, Document, open, view, lookup, check, save, base, files   ~320
             edit.rs                     the pipeline of 3.1                                                ~450
             edit_files.rs               the files path: to_batch, skew, the mapping of errors to lines     ~230
             changes.rs                  revisions::changes to ChangeEntry and RevisionInfo, attribution    ~330
             receipt.rs, receipt_text.rs the receipt and its text                                           ~480
             blocks.rs                   the keys of the receipt's blocks, expansion of rows, the cap       ~160
             complement.rs               Complement, the manifest and its canonical form, pending_files     ~450
             write_plan.rs               WriteOptions, WritePlan, the marker, recover                       ~350
             export.rs                   export and resume                                                  ~260
             check_pending.rs            check_pending and CheckResult                                      ~130
             inspect.rs, inspect_doc.rs  inspect of a block, and of styles, numbering and sections          ~560
             capabilities.rs, compare.rs capabilities, json_schema, the public compare                      ~260
             tests_*.rs
```

### 8.1 Dependencies

The new rows of the Allowed dependencies table, each with the reason, applied with the code and not before, because the check fails on a row
with no module directory:

| Module | Change | Why |
|---|---|---|
| `ops` | adds `schema`, `comments`, `threads` | `schema` for `Ctx::allows_child` (a property name the container allows) and for the measure of an attribute (2.7); `comments` for the comment operations; `threads` for the anchor blocks that the scope guard reads |
| `document` | unchanged: any module | it converts `revisions::Origin` to `check::Origin`, so `ops` needs no `check` |
| `fuzz` | adds `document` | the targets that go through the public path (`ops_json`, `ops_block`, `resume`, `edit_files`) |
| `bench` | adds `document` | the rows of 7.4 |

The other designs add the edges `revisions` to `check` (`check.md` decision 1) and `check` and `projection` to `threads`, and the `comments` row. The graph stays acyclic, because `document` is above everything, `ops` reaches `comments`, `revisions` and `check` only through them, and nothing below
`ops` names it.

**New crates in the core:** `serde` with `derive`, `serde_json` and `schemars`. The architecture requires the derives on every public type, so one source gives
the napi types, `--json` and the schema. `serde_json` is in the workspace already and the core needs it for `Batch::parse` and the block parser's
intermediate value. All three are MIT or Apache-2.0 and pass `cargo deny`, which `cargo xtask check` runs. Output is deterministic whatever the crates do: struct fields
serialize in declaration order, the only `serde_json::Value` objects (inside the block parser) are sorted because `preserve_order` stays off, and no step
iterates a hash map.

## 9. Tests

All laws and the conformance suite sit at the interface, as `ARCHITECTURE.md` rule 5 says, and the core's direct unit tests are for the parts of this layer
that have invariants of their own.

### 9.1 The laws move to the testkit

The laws live in `stetdoc-testkit` and run through `Document`:

- **A `laws` module** in `stetdoc-testkit` holds each law as a function over `stetdoc_core`'s public API, and `tests/laws_*.rs` run them over the corpus
  (`STETDOC_CORPUS_DIR`) and over the fixtures. The testkit's own tests may read files, so no `core-io-exceptions` entry is needed.
- **No cargo feature is needed.** The testkit already depends on the core, and the core dev-depends on the testkit for the corpus and the fixtures, which is why those
  three APIs stay plain data (a test checks it). `laws` is a further module of that crate that names core types, which only the tests in `crates/stetdoc-testkit/tests/`
  use, so the core's own tests never see them. (An earlier version of this section proposed an optional `core` feature. It would have saved nothing: the core is
  compiled twice in the core's test build either way.)
- **The two corpus harnesses of `package` and `xml` stay** in `package/corpus_laws.rs` and `xml/corpus_laws.rs` as the unit-level checks of those two modules, and law L1 on every corpus is checked in the `Document` form.

| Law | Through `Document` |
|---|---|
| L1 Identity | `open(bytes).save().bytes == bytes`, and the same through `open`, `export`, `resume`, `save`, and through `resume`, `Edit::files` with nothing pending, `save`. A refusal in the manifest is the exemption: `open` must fail with its code, and a refusal at a stage that `open` does not read must not break L1 |
| L2 Locality | random batches built from `view` output (`laws::generate`, run by `laws::random`). Entries that the batch did not touch are identical, and inside touched parts every element outside the blocks of `Receipt.blocks` is identical, except the root start tag. The testkit checks it by package entry (no entry is lost, and every entry outside the story parts the view names and the declared side-effect parts keeps its raw bytes) and by block in the body (`laws::blocks`: a top-level block the batch does not name, and that no merge, split, insert, `accept` or `reject` touches, keeps its XML, except that the block before or after a touched one may gain the range markers and comment reference runs a removal hoists into it, each with the id of one that a touched block had or of a comment the batch made) |
| L3 Edits stick | after an edit, `files()` for the touched blocks equals the receipt's block text and the string model of the batch |
| L4 Stability | `export`, `resume`, `export` gives the same bytes, and a second unpack and pack gives the first's |
| L5 Reversibility | `reject` by the author's name restores the original after normalization for every `reversible` operation, and `accept` gives the intended text |
| L6 Validity | `Document::check` of the output has no finding that the input lacked, and the schema, LibreOffice and Word oracles where they exist. `check.md`'s V11 is this law |

### 9.2 The operation conformance suite

Every operation in `capabilities().ops` runs it. A generator for each name builds instances from `view` output only. A generator table keyed by name is in the
testkit, and the suite **fails with the operation's name** when a name has none, so adding an operation without a generator cannot pass. The suite checks:

- **atomic failure:** the batch `[op, a failing op]` leaves `export()` byte-identical
- **dry-run purity:** a dry run leaves `export()` byte-identical, and law H14 holds: it fails exactly when the real edit fails, and its receipt equals the real one
  apart from `dry_run`, the base, and the refs of new blocks
- **the scope guard:** a scope that names the target passes, and one that names another block is `scope_violation` with the operation's index
- **reject-all reversibility** for the operations whose spec says `reversible` (L5)
- for the others their own laws: the comments laws C3 to C13 for the comment operations through `Document`, the `revisions` law R8 for `accept` and `reject`

### 9.3 Laws of this layer

| Law | Statement |
|---|---|
| H1 Spec and enum agree | for every `OpSpec`, its example deserializes to the `Op` with that name, every field in the example is in the spec, every field of the spec is accepted by the block parser, and the fields in the JSON Schema of the payload are the spec's, so no field is accepted that the table does not list. A JSON field or a block key that is not in the table is `invalid_payload`. `Op::spec()` is an exhaustive match, so a variant with no entry does not compile |
| H2 The two syntaxes agree | for every generated batch that block text can express (texts with no carriage return and no control character but tab, list entries with no newline, no per-paragraph style), `parse(to_json(b)) == b` and `parse(to_block(b)) == b`. The generator stays inside that set and the law says so |
| H3 Receipt honesty | every revision element that the batch created is in `Receipt.revisions`, every block whose bytes changed is in `Receipt.blocks` or counted in `blocks_omitted`, and every ref that went stale is in `retired` |
| H4 The scope guard is complete | the touched blocks and the targets of a batch that passes a scope are all in it, any block whose bytes changed (the `revisions` law R6) is in `touched` or new, **and any part whose bytes changed is a story part covered by those blocks or a declared side effect**. Each declared side effect (the comment parts, their content types and relationships, the comment styles, the `w:num` and `w:abstractNum` of a restart in scope, `reconcile`'s markers, a deleted note's paragraphs) has a batch that passes, and each undeclared change has one that is refused: `accept` or `reject` of everything with a revision in `styles.xml`, `numbering.xml` or the body's section properties, an id outside the scope, a restart of a block out of scope, a comment on a block out of scope. A change the guard cannot see fails this law |
| H5 Blind batches | no operation resolves a ref that an earlier operation of the batch created, and a batch with an operation that does is refused with `stale_ref` or `not_found` |
| H6 Quotes | `Quotes::AsTyped` changes no character of any text. With `Match`, a text with no quote is unchanged, a quote after a digit is unchanged unless it closes an open quotation of its kind, following a text twice is following it once, curly quotes become straight in a straight context and straight ones curly in a curly one, and the result depends on the paragraph and its neighbours only |
| H7 Complement | `export`, `resume`, `export` is stable. Every single-byte mutation of the manifest (its digest line and its canonical form make this complete), the ref map or either package is refused with `corrupt_complement`, `unsupported_version` or `limit_exceeded`, and never panics. A manifest of a newer minor version is `unsupported_version` |
| H8 Skew | files rendered by a build whose rendering differs (a mutated baseline) are refused with `stale_files`, whether `to_batch` succeeds or fails |
| H9 Limits | a batch, a text, a scope, a complement entry and a readable file each one over its limit is `limit_exceeded` before it is parsed or allocated |
| H10 Send | `Document: Send`, checked at compile time |
| H11 Offsets | an offset in either unit lands on the same character, for text with a surrogate pair and with an atom. A UTF-16 offset inside a pair is `invalid_payload`. `Edit::files` is not converted |
| H12 Interrupted writes | the testkit runs `write_plan` against an in-memory directory and stops it after each step (the marker, each entry, the commit, each file, each removal), with and without hand edits, **including a refresh export at the same generation whose ref map has different bytes**, and also stops `recover` after each of its steps. After recovery `resume` succeeds at exactly the old or exactly the new version and never reports `corrupt_complement`, no stale entry or marker is left, no hand edit is lost (it is still pending, or the head holds it), an edit made after the crash is never lost (it stays pending when it was made on the render the new manifest records, and is saved aside as a `.stale` file with a warning otherwise, so the committed change is never undone by it), and a plan that would overwrite an unconsumed hand edit is refused before its first step. A directory with the marker is refused by every method but `recover` |
| H13 `check_pending` | with nothing pending the receipt is `None` and the exit status 0. With pending files that add nothing it is 0 whatever the document holds. With pending files that add a violation it is `validation_failed`. With `strict` any violation makes it 5. Pending files with no stamp are `invalid_payload`. Its receipt is the dry-run receipt of H14. It never changes `export()` |
| H14 A dry run is the real edit without its effect | for every generated batch and every generated edit of files, a dry run fails exactly when the real edit fails, with the same code, message and operation index. When both succeed the two receipts are equal field by field after three things are set aside: `dry_run`, `base` (the dry run's is the document's current token), and the refs of new blocks (a new block in the dry run has `ref: None` and an anchor, `added` is empty and `added_count` equals the real edit's `added.len()`, `ChangeEntry.block` is `None` for a revision in a new block, and a field whose target is in a new block has no arrow to it: `{field REF "4.2" ->p57}` in the real receipt is `{field REF "4.2"}` in the dry run's), **and the `block` of a warning about a new block, after which warnings that are the same collapse to one** (the real receipt lists the warning once for each new block, and a dry run cannot tell them apart). The dry run issues no ref: no ref of the real edit's `added` appears anywhere in the dry run's receipt or text |

**L6 and H13 through `Document`** (`check.md` V11): `laws::edits` and `laws::files` run batches and edits of files on every document that opens, and fail on any `validation_failed` (an edit is never refused for a violation that the compiler writes, whatever the input had) and on an output with a finding code that the input lacked; `laws::surface` checks `check_pending` with nothing pending. **A refusal that is known and has an owner is a `laws::sweep::Gap`**. The sweep takes it out of the failures, and fails when it stops occurring, so the entry goes when the fix does. The first run on the curated corpus found two, both the compiler's (`laws::edits::KNOWN_GAPS`). The `paragraph` batch found two more on the corpus and one on the fixtures (`KNOWN_FIXTURE_GAPS`), all the compiler's: a property placed into a `w:pPr` whose children are already out of order moves the violation to another element.

`projection`'s T6 (edited files become the expected text through `Edit::files`) and T8 (new paragraphs take the right style, and a reject-all restores the original) run in the
testkit through `Document`.

### 9.4 Direct tests in the core, and fuzzing

- **Tables** (a name, an input, an expected result, in `ops/tests_*.rs`): `ops::parse` (the grammar, every malformed shape, hostile sizes), `quotes`, `units`, `target`
  (unique, ambiguous, folded, in a deletion), scope containment and ranges (reversed, across stories, with a table between the ends), the side-effect table, the write plan and `recover` (every step), and `document::complement` (every malformed manifest line). These are modules with invariants of their
  own, so they have direct tests, though `ARCHITECTURE.md` rule 5 does not yet name `ops` among such modules.
- **The two nets under the compiler** (`check::edit`, and the second pass of the guard) refuse only what a bug of stetdoc writes, and an agent
  cannot provoke a bug, so `document/tests_tamper.rs` is a test-only hook: a test arms a change that `edit` applies to its staged transaction right
  after the operations (an element the schema does not allow in a paragraph, a text changed in a block no operation named, a property added to a
  table), and the test shows the refusal, the operation or line it names, and that the document is as it was. The hook is `cfg(test)` and costs a
  release build nothing.
- **Fuzz targets** through `document`, designed as: `ops_json` and `ops_block` (parse a batch and apply it to a fixture document), `resume` (a
  mutated complement) and `edit_files` (mutated readable files, `projection`'s `files` target through the whole path). A panic, a hang, or a failed batch that changes `export()`
  is a failure, and a refusal is not.

  **As built.** There are two targets. `batch` reads JSON or block text, as `Batch::parse` does, so it is `ops_json` and `ops_block` in
  one. It writes the parsed batch as JSON and parses it back, then applies it to a small document that the harness builds (paragraphs with
  quotes, a tab and an emoji, and a table), with the document's base token in place of `BASE_TOKEN`, tracked, untracked, with UTF-16
  offsets or as a dry run. `files` is `edit_files`: the input goes into that document's `document.md`, whole, spliced in or appended. A
  failed edit or a dry run that changes the saved bytes or the base token, an `internal` error, or a result that does not save and open
  again, is a failure. The seeds are the conformance suite's batches in both syntaxes and the contract's `document.md`. `resume` over a
  mutated complement is not built.

## 10. What this layer takes from other modules

**From `projection`.**
- `projection::authors(parts: PartsView) -> Vec<(String, String)>`: code and name, as the view's author line shows them. `document` hands it to `ops` for
  `accept` and `reject` by author, because the document facts are `projection`'s and `ops` may not read them.
- `Warning.story` is an `Option<String>`, and `WarningCode` holds the codes of 4.2: `no_change`, `own_text_removed`,
  `property_change_folded`, `revision_removed_with_text`, `quotes_curled`, `quotes_straightened`, `find_matched_loosely`, `already_resolved`, `reply_to_resolved`, `anchor_emptied` and
  `comment_removed`. A warning about an operation and not a block has no story.
- `ReadableFiles` has `rendered: Vec<(String, String)>` (file name and hex SHA-256), empty from `render_files` and filled by `pending_files`. `Lookup` has
  `text: String`.
- On the files path `max_batch_ops` counts the **changed blocks**, and the refusal says how many there are and that the edit should be applied in parts.
  `document/edit_files.rs` counts the distinct blocks of `Planned.origins`, because `to_batch` uses its `Limits` for the size of a file only.
- `to_batch` takes its `Limits` from `document`, `Planned.baseline` is compared with the recorded hashes (3.5), `render_blocks` takes the token (4.2),
  and `Scope::Revisions` and `Comments` need no wiring here (6).

**From `revisions`.** `Change` carries the text it inserts and the text it deletes, each cut at 200 scalar values with a flag, for `RevisionInfo` (6).
`insert_block` takes a paragraph or a table as the anchor, `Before` or `After`. `Effect::new_blocks` of `insert_row` holds the paragraphs of the new row's
cells as well as the row, because the receipt shows them. `Outcome::created` of every compiler of a transaction is complete for the blocks that compiler saw,
and `ops` takes the union. `ops` calls `RevStamp::new`.

**From `check` and `schema`.**
- `Verified::changed`: the units that `check::edit` found changed, each a part and a block or the shell of a table or frame with what differs in it (own
  properties, or only the list of children). The second pass of the scope guard (3.3) reads it. `check` finds them from the dirty flags and the equality test,
  and does not read a declaration.
- `check::edit` takes its `Declared` from `document`, which converts the compiler's origins, so `ops` does not depend on `check`.
- `cargo xtask docs` generates `docs/reference/findings.md` from `api/check.rs`.
- The exit status of `stetdoc check` is the one `check.md` proposed, and is `CheckResult::exit_status` (6).
- A `Measure` for each attribute whose simple type is a measure (`ST_TwipsMeasure`, `ST_HpsMeasure`, `ST_EighthPointMeasure`, `ST_PointMeasure`
  and the signed and percent forms), read through `schema::measure_of(container, element, attribute) -> Option<Measure>`, would replace the table in
  `ops/units.rs` (2.7). It is not implemented.

**From `comments`.** The functions take `&mut Compiler` and `&RevStamp`, and `reconcile` takes the `Tx`, as `comments-and-notes` section 4 says. `CommentEffect.blocks` is a
list, because the markers of a comment can be in several paragraphs, and the warning codes keep the wire names of 4.2 so that they are `WarningCode`s. `Comment::anchor.blocks`
is read by `ops` for the scope guard. `CommentEffect` also lists the parts it wrote or added (`parts`: the comment parts, `[Content_Types].xml`, the main part's
relationships, and `word/styles.xml` when `ensure_styles` appended the comment styles), which are the side effects the guard accepts (3.3). `delete_note` takes a kind and an
ordinal, which `ops` parses from `fn3` and `en2`, and the `comment_removed` warning of `reconcile` names the blocks whose markers it removed.

**From `state` and `wordml`.** `State::opened_source() -> Bytes`, the shared handle to the bytes the state's package was opened from, so `Document` keeps the
original for `export` with no copy. A story refresh rebuilds only the blocks touched since the previous story was built, so that a write to a paragraph with a
field does not make every later refresh of the transaction rebuild the field index (445 to 543 ms against 144 ms for 50 edits on `fields-13mb` before that change).

**From the test and benchmark harness.** The `laws` module of 9.1, the generator table of 9.2, the in-memory directory and the crash injection of H12, the rows
of 7.4 and the fuzz targets of 9.4.

**What the surfaces do**, and no more:
- **The CLI** parses arguments into `Stamp`, `Batch::parse`, `Edit` and `ViewSpec`, reads `WorkingCopy` from the directory and executes the steps of `write_plan` and of `recover` one at a
  time with an atomic rename for each write and an advisory lock around them (5.4), calls `recover` first when it finds `.stet/writing`, calls `ensure_clean` before `edit <dir>`,
  calls `check_pending` for `check` and exits with `exit_status`, prints `View.text`, `Receipt.text`, `Lookup.text` and `Inspection.text` (or the structure with `--json`), maps
  `ErrorCode::exit_class()` to the exit status, reads the local clock, collects the files that `Capabilities` says the format uses, and generates the CLI reference and the skill
  tables from `capabilities()`. It renders and validates nothing itself.
- **napi** marshals `Document`, `Edit`, `WorkingCopy` (the two lists as an object of strings and an object of buffers) and the data types as JSON, throws
  `StetdocError { code, message, next, opIndex, file, line }`, builds its TypeScript from `json_schema()`, passes `OffsetUnit::Utf16` so that offsets count as a JavaScript string
  does and ships no conversion helper, and notes that a call takes about a second on the worst document, so a host calls it from a worker.
- **A host application or agent harness** passes the user's local time as the date, keeps the `Document` or the `WorkingCopy` between turns, and reads revisions by author.

## 11. Where the code differs from the design

- **Comments.** One file, `ops/comments.rs`, holds the five comment operations and `delete-note`. `document` reads the note ordinals with `projection::note_ids`,
  since they are `projection`'s. An operation's anchor is its targets. The parts a comment adds bring `[Content_Types].xml` and the main part's relationships as side
  effects, the comments story is ignored by both passes of the guard, and what `reconcile` touches is exempt in both. The comment functions keep the effects of
  their compiler calls, so `ops` records one effect for each operation: the paragraphs whose markers changed, and for `delete-note` the revision ids it took.
- **Properties.** `schema::measure_of` is not implemented, so `ops/units.rs` holds its own table of the attributes Word writes with a unit, and any other value is
  passed through as written (2.7). `paragraph` merges `w:ind` and `w:spacing` with the attributes the paragraph has, since `set_props` replaces an element whole.
  `set` on the last section is `scope_violation` under a scope and edits the body's `w:sectPr` without one. The scenario documents of the `revisions` census are
  not run against the property operations.
- **Compare.** `compare` calls `compare::prepare` and `compare::redline` (which fails closed), builds `Declared` from the calls' origins (`declared::build`), from
  `Effect::imported` (each block of the redline that received the original's content, keyed by `check::unit_keys` of its block in the original) and from
  `Declared::definitions` (an imported style or numbering definition keyed by its element, a copied part by `check::part_keys`), runs `check::edit_located`,
  commits, and writes the receipt: revisions by author, `moves` as refs, compare's warnings with their counts (thirteen `compare_*` warning codes), a
  `compare_not_redlined` or `compare_not_restored` warning naming each excluded block by its kind (a block of the redline, or the original's first words, never a
  key of the original), a `compare_keys_merged` warning for each copy whose units did not pair with the original's, and `validation`. The redline's original is
  the revised input as given. `compare`'s laws N5 and N7 run through `Document` in `laws::compare`, over the contract, the compare pairs of `corpus/pairs.tsv`
  (241 curated in `cargo xtask check`: all compared and held, `compare_keys_merged` on none, exclusions on 17) and forty curated documents against an edited
  form of themselves. Accept-all of the redline gives the revised document and reject-all gives the original.
- **Fuzzing.** The `resume` target over a mutated complement is not implemented (9.4).

## 12. Risks

| Risk | What contains it |
|---|---|
| The receipt and the guard take the budget on the worst stress body | 7.2 says where the time goes and which remedies exist. The `document_*` benchmarks of 7.4 decide, and every remedy sits behind an interface |
| 50 edits of paragraphs that hold fields cost 445 to 543 ms for the compile alone on `fields-13mb` when `Story::refresh` rebuilt the field index after the first such write | `state` and `wordml` now refresh only the blocks touched since the previous story (10), and the `fields-13mb` rows of 7.4 hold the budget |
| Agent evals favour another block-text grammar | it is one file and a list of field kinds. The model, the JSON and the laws do not change |
| Curling quotes surprises a lawyer or an agent | it is reported in the receipt, shown in the block text, off with one option, and a pure function of the paragraph |
| The scope guard misses a change | H4 compares it with the `revisions` locality law on generated batches, and the target list is what makes shape X (an insertion before a block) visible |
| `ops` gains `schema` and the layering erodes | the row says why, and only `allows_child` is used (and `measure_of`, if it is added). A reviewer can read the call sites |
| The spec table and the enum drift | H1, and `Op::spec()` is an exhaustive match |
| The receipt's JSON becomes frozen API at 1.0 | fields are added with `#[serde(default)]` and never removed. The text is not frozen, and the skill says to read the structure |
| A hostile complement | every entry is bounded before it is read, the manifest has a canonical form and a digest, `State::resume` re-derives and verifies everything, and H7 mutates every byte of the small entries |
| A crash while a working copy is written | one commit point (the rename of the manifest), generation-numbered entries so the old manifest stays valid, a marker, and `recover` that never overwrites text written after the crash. H12 stops the write after every step, and stops `recover` after every step |
| A scope that something outside it can change (a resolve that edits `styles.xml`) | accept and reject are narrowed under a scope, changed parts must be declared side effects, and a second pass reads what `check` found changed. H4 tests each exception and each refusal |
| `serde` and `schemars` slow the build or grow the binary | `cargo xtask check` (the build, `cargo deny`, the layering check) passes with them, and the binary's size is measured with the CLI |
| An agent writes a batch against the wrong version | the base token is required, the message says to view again, and an empty batch is refused so a lost ops field is not a silent success |

## 13. Decisions

The decisions this design takes. Each is applied where the right-hand column says.

| # | Decision | Applied in |
|---|---|---|
| 1 | The public offset unit is Unicode scalar values by default, `find` is the primary way to name a range, and the core converts UTF-16 on request (`Edit::offsets`), so napi ships no helper (`refs-and-state` 17 left the unit to this layer) | 2.2 |
| 2 | An operation batch always names its base token, in working copies too, and an empty batch is `invalid_payload` | 2.6, 3.5 |
| 3 | The scope guard runs before `check` on declarations and after it on what `check` found changed, and compares touched blocks, operation targets and changed parts with the scope | 3.1, 3.3 |
| 4 | `save` does not validate, as `ARCHITECTURE.md` says | 6 |
| 5 | `Receipt.text` is composed in `document`, in one file, and not as a method of the format seam | 4.3 |
| 6 | Origins reach `check` through `document`, so `ops` does not depend on `check` | 3.1, 8.1 |
| 7 | The complement is a manifest with its own digest and entries named by generation, with the hash of each readable file in the manifest, and `pending_files` returns those hashes. The commit point of a write is the rename of the manifest | 5 |
| 8 | The default limits of 3.6 | 3.6 |
| 9 | The block-text grammar is provisional and the wire convention is snake_case fields and hyphenated operation names | 1.3, 2.6 |
| 10 | A dry run commits into a state that it drops, so it fails exactly when the edit fails, and its receipt equals the real one apart from `dry_run`, the base and the refs of new blocks. It never issues a ref | 3.1, 4.2, 9.3 |
| 11 | `check_pending` is the one function behind `check` | 6 |
| 12 | A file that was never written is `unsupported`, never ignored | 5.2 |

Three more:

1. **Quotes follow the convention of the paragraph and its neighbours, by default**, so straight stays straight where the document is straight. The receipt warns, and `--quotes as-typed`
   turns it off (2.8).
2. **The block-text grammar is provisional and lives in one file** (2.6).
3. **`Receipt.text` is composed in `document`, with the block bodies from `render_blocks`** (4.3).
