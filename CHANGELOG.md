# Changelog

All notable changes to stetdoc are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[semantic versioning](https://semver.org/spec/v2.0.0.html) from 1.0.0. Nothing has been
released yet.

## Unreleased

### Added

- The CLI is published as the `stetdoc` crate, so `cargo install stetdoc` installs the `stetdoc` binary. The library is
  published as `stetdoc-core`.
- `stetdoc view FILE --revisions` lists the tracked changes as a ledger, one line each: the ids, the kind (`insert`,
  `delete`, `move` or `format`), the author, the day, the block and the text. `--author` and `--kind` filter it, and
  `--after change:N` continues a long list. It lists what `Document::revisions` returns. It was `unsupported` before.
- The agent skill says where the author of a revision comes from: `--author`, then `STETDOC_AUTHOR`, then the name saved
  by `stetdoc init`, so an agent no longer asks for a name the config already holds.

- Docs for users and builders: `docs/agents.md` (setup for each agent), `docs/self-hosting-mcp.md` (serving `stetdoc mcp`
  to cloud apps) and `docs/build-on-stetdoc.md` (the crate, the Node package, the CLI and MCP).
- Packaging for the releases: `packaging/install.sh` (checks the SHA-256 of the release archive), a Homebrew formula template,
  npm packages for the CLI (`packaging/npm/stetdoc`), plugin and listing manifests for Claude Code, Cursor, OpenAI and the MCP
  Registry, and a draft release that carries all of them. See `packaging/README.md`.

- White space at the ends of a `w:t` without `xml:space="preserve"` is text, as Word for the web shows it (hand check 37),
  where LibreOffice drops it, a documented deviation; `xml:space` is read as inherited from an ancestor, and what stetdoc
  writes or copies says it where needed; a `w16se:symEx` emoji shows its character (or its fallback's text); the text of a vertically merged
  continuation cell is flagged hidden; the `Final` view of the notes leaves out a note whose reference is a pending
  deletion, and the `Original` view one whose reference is a pending insertion, each giving a note the mark Word shows once
  its changes are resolved (`mark=`), while its id stays the same in every view; and a text edit never deletes a note's own
  number. Found by the LibreOffice oracle
  (`cargo xtask oracle libreoffice`), which reads a seeded sample of 500 outputs.

- `stetdoc pack` overwrites an existing output only when it is the working copy's original
  or a .docx it packed; anything else is the new error code `stale_target` (exit 4), unless
  `--force`. The default date of a revision honours `TZ`, with the time zone database built
  in as a fallback, and a zone that cannot be read gives UTC with a warning. An error from a
  readable file names its file and line once.
- The `stetdoc` command surface: all twelve commands with their flags, `--json` on every
  command, exit codes from the error table, and the stamp from `--author` or
  `STETDOC_AUTHOR` with the local clock as the default date. `view`, `inspect`, `lookup`,
  `check`, `edit -o`, `compare` and `capabilities` work on a `.docx`.
- The agent skill, embedded in the binary: `stetdoc skills get [core|operations|errors]`
  and `--full`. The operation and error tables are generated from `capabilities()` and the
  command list from the CLI's own definitions, so they cannot drift.
- `stetdoc init` picks agents: Claude Code, Codex, Cursor, OpenCode, OpenClaw, Gemini CLI,
  Copilot, Amp, Hermes, Grok Build, cmx2 and Factory. It installs into the home folder by
  default and into a project with `--project`, which also keeps a short block in `AGENTS.md`.
  It asks in a terminal, installs for the detected agents otherwise, and takes `--agent a,b`
  or `all`; a folder several agents share is written once. `--list` shows each agent's
  detection, folder and installed version, `--remove` deletes only folders that hold nothing
  but an unchanged stetdoc stub, `--mcp` prints how to register `stetdoc mcp` with each agent without
  editing its config, and `--dry-run` changes nothing. The stub records the version that
  wrote it. `stetdoc init --author NAME`, or the answer to its question in a terminal, saves
  the author to `stetdoc/config.toml` in the config folder, and `--list` shows it. `skills/stetdoc/SKILL.md` is the same stub, and a test fails when the two drift.
- The author of a revision comes from `--author`, then `STETDOC_AUTHOR`, then the name saved
  with `stetdoc init`; without any, the command says to run `stetdoc init`. It is never a
  name the user did not confirm.
- `stetdoc mcp`: a Model Context Protocol server on stdio, speaking revision 2026-07-28 and
  the earlier handshake revisions. Every command but `init` is a tool whose schema comes from
  the CLI's own definitions, a call returns the JSON the command prints with `--json`, and a
  failure is a tool error with its message and next step. `--root DIR` confines every path
  argument to `DIR`, refusing `..` and links that lead out of it. `--author NAME` stamps the
  calls that name no author.
- [`docs/reference/cli.md`](docs/reference/cli.md), generated from the CLI's own definitions
  by `cargo xtask docs`, with examples that run as tests.
- [`FIDELITY.md`](FIDELITY.md): per feature, what is kept exactly, what is normalized, what
  is read-only and what is left out, with the design behind each settled detail. The README
  shows the working CLI.
- Working copies on disk: `unpack`, `pack`, `edit`, `check`, `view`, `inspect` and `lookup`
  on a directory. Every write follows the core's write plan with atomic renames under an
  advisory lock on `.stet/lock`, and a command that finds an interrupted write recovers it
  first.
- The workspace: `stetdoc-core`, `stetdoc-cli`, `stetdoc-napi`, `stetdoc-editor`,
  `stetdoc-testkit` and `xtask`, on a pinned toolchain.
- The `stetdoc-core` module tree, with a contract in each module.
- `ErrorCode`, `ExitClass` and `StetError`: the error vocabulary, with a generated
  [error reference](docs/reference/errors.md).
- The `internal` error code and exit class 7: a bug in stetdoc, which callers report and never
  retry around.
- `cargo xtask check` and `cargo xtask check --quick`, and CI that runs the same check.
- The entry-point docs, the licence files (`MIT OR Apache-2.0`) and `NOTICE`.
- `cargo xtask corpus fetch`: downloads the corpus manifest's documents from their pinned
  commits, verifies every SHA-256 and caches them, offline once cached. `cargo xtask check`
  runs it and sets `STETDOC_CORPUS_DIR` for the tests.
- `stetdoc_testkit::fixtures`: 84 edge, 7 Word and 87 hostile synthetic documents built in
  memory, each with the outcome stetdoc must give and, for a hostile one, why. They include
  the package layer's own 117 cases, ported without duplicates: 53 are new fixtures and the
  rest are the fixtures the testkit already had. A snapshot pins the SHA-256 of every fixture,
  so a change to the builder or to the deflate library shows up as a reviewed diff.
- `stetdoc_testkit::expect`: `Expect` and `Stage`, shared by the corpus and the fixtures. A
  refusal is an error code at a stage (`open`, `read`, `xml` or `document`), and a layer sees
  only the refusals it reaches. A refusal at `read` or `xml` also names the parts that carry
  it (`Entry::parts`, `Fixture::parts`), and `Expect::seen_by_reading` gives `Ok` to a layer
  that never reads them. Every refusal in `corpus/manifest/` has a stage, a part where that
  applies and a reason from `corpus/reasons.tsv`, and `cargo xtask check` enforces it.
- Fuzz targets for the container and the span tree, run with `cargo xtask fuzz <target>`: a
  libFuzzer target that opens, reads, replaces a part of and writes a package, and one that
  parses, serializes clean and with every tag dirty, and edits a part, each aborting when a law
  of the module breaks. They are seeded from the fixtures and the curated corpus, built with a
  nightly toolchain pinned for fuzzing alone, and run for 60 seconds per target in CI. The core
  reaches them through the off-by-default `fuzzing` feature ([decision
  0002](docs/decisions/0002-fuzz-entry-points.md)), and `cargo xtask check` now runs clippy with
  the default features and with all of them, the tests and rustdoc with all features, and
  cargo-deny on the fuzz crate under its own policy (`fuzz/deny.toml`, the root policy and the
  NCSA licence of libFuzzer).
- `cargo xtask bench` and the benchmark harness, `stetdoc_testkit::bench`: the time and the peak
  memory of package open and save, span tree parse and serialize, schema validation, the state
  (open, a batch of fifty writes, save, export, resume), `wordml` (scan, field pass, text pass,
  refresh, fifty text edits, field edits and paragraph insertions) and the style cascade and clause
  numbers (the gate of 25 ms on a 20,000-paragraph body), on a 300-page contract and on four more
  inputs (the densest real document and three synthetic worst bodies), held to the budgets of `ARCHITECTURE.md` and to a baseline for each platform. The memory
  is counted by an allocator that the core's test binary installs, so it is the same on every run.
  The only exemption from a budget is a named pair of a benchmark and an input that is a known
  memory gap, which ends by itself when the pair fits (there is none today). A time regression against the baseline is a
  warning (`--strict-time` fails it), and the other failures are the budgets, a memory regression
  and a closed gap. `--ci` has the margins of a shared runner, `--out DIR` keeps the table and the
  rows, and a nightly workflow runs it with the budgets as written and uploads both. A benchmark is
  a small file in `stetdoc-core/src/bench` ([decision 0003](docs/decisions/0003-benchmarks.md)).
- `projection` and `index` in the core: what an agent reads and edits. `view` gives a page of a document
  within a token budget (the estimate is ceil(ascii / 4) plus the other characters): the whole body when it
  fits, otherwise its outline, or a range, a clause, the results of a find or one story, with a cursor that is
  the ref of the last block shown, margin notes (what a find matched, what a citation such as `clause 9.1` came
  to, what cites a clause), and the notes and comments a page cites. `render_files` writes the readable files
  (`document.md`, `notes.md`, `headers.md`, `comments.md`) in the `stet/1` format or as `json/1`, and
  `files::to_batch` turns an edit of them into operations by comparing two drafts, in no syntax, so the
  alignment is written once: a changed slot is one `replace`, a line removed is a `delete`, a line with no ref
  is an `insert` after the paragraph it follows, and a paragraph cut into lines or lines joined are recognized
  as exact splits and merges only when the text is conserved. Everything else is applied as written or refused
  with the file, the line and the way out (`readonly_region`, `unsupported`, `cross_block`, `stale_files`),
  and the files of an unedited document give no operation. `render_blocks` shows the blocks of a receipt as the
  files hold them, with warnings (`looks_hidden`, `duplicate_number` and the rest). `lookup` finds where a
  term is defined and used, which block is `clause 4.2(a)` (computed or typed, composed across lists), and what
  refers to a block, and returns every candidate when it is not unique. Laws M1 to M7, F1 to F8, T1 to T8, P1 to
  P8 and I1 to I6 run over the fetched corpus in `cargo xtask check`, and the benchmarks time the
  first view, a page with notes, the readable files, `to_batch` and the first lookup on a 300-page contract. See
  the [design notes](docs/design/projection-and-index.md).
- Every testkit fixture is opened with `Package::open` and held to the container laws and to its
  expectation by a test of the core, with the row to add for `State::open` when it merges. The
  laws now also check that the parts a refusal at `read` or `xml` does not name read and parse
  cleanly, which `Expect::seen_by_reading` relies on, and a part refused with another code than the
  index gives is named in the message, where it used to be reported as a clean one.
- The corpus cache is safe to share between checkouts and between concurrent runs: fetching
  holds a lock, work happens in a per-process directory, and a set is built once, under a name
  that carries the manifest hash, and never changed. A repair of a damaged blob builds a new
  generation of the set behind the stable link, so a reader of the old one sees no change.
  A repository that was deleted or made private fails its source alone, with a message that
  asks for a new pin.
- `cargo xtask codegen`, which generates Word's element order and value types from the Open
  XML SDK's schema data (pinned, MIT), and the `schema` module that uses them: where a new
  child element goes (`insert_in_order`, `reorder`) and the violation set behind the
  no-new-violations rule (`validate_part`). The schema code reads the span tree (`SpanTree`
  implements `SchemaTree`, and `Refs` lets code that holds `NodeRef`s call the rules), finds the
  context of an element (`ctx_at`), and is the one place that says which branch of an
  `mc:AlternateContent` Word reads and what `mc:ProcessContent` keeps (`taken_branch`,
  `disposition`).
- `Mode` (`Final`, `Original`, `Redline`) in `api::view`, re-exported from the crate root: how
  tracked changes appear when a paragraph's text is read. `Final` applies every revision,
  `Original` rejects every one, and `Redline` keeps both.
- `wordml` in the core: the stories of a document and the blocks in them, each paragraph's text in
  the three modes with a map from every character to the XML it came from, and the definitions of
  styles and numbering. The map is the base that edits, revisions and projections build on. See
  the [design notes](docs/design/wordml.md).
- `package` and `xml` in the core: the ZIP container and the XML span tree. A package opens
  lazily, checks a part's size and CRC-32 when it is read, and writes back the input bytes
  exactly when nothing changed. Replacing one part rewrites that entry and moves the offsets
  after it, and an edit through the span tree changes only the bytes of the nodes it touched
  (the root start tag may gain the namespace declarations new content needs). A package is read
  as Python's `zipfile`, `unzip` and `bsdtar` read the same file, or it is refused: the central
  directory is found from the end record, entries lie end to end, and a ZIP64 record has to agree
  with the end record. `Limits` bounds the archive, the entries, the compression ratio and the
  XML (depth, attributes, nodes, namespace declarations in scope). The laws L1 and a one-edit L2
  run over the fetched corpus in `cargo xtask check`, against a second ZIP reader and against
  the stage and part the index gives for each refusal. See the
  [design notes](docs/design/package-and-xml.md).
- The span tree stores its nodes and attributes in chunks that a clone shares until it changes
  one, so cloning a tree for copy-on-write staging copies a few pointers and an edit copies the
  chunks it changes, and a tree holds what it uses where a growing vector held up to twice that.
  The densest benchmark inputs went from 103% of the memory budget to 70% (writing a part while
  its tree is held), a clone of the densest input with 50 paragraphs changed from 207 MiB to
  97 MiB, and the four known memory gaps closed. `SpanTree` refuses `]]>` in text
  (`malformed_input`, reason `xml.cdata_end_in_text`; none of 51,831 documents has one), an element
  holds at most 4,096 attributes, and a text node that an edit makes meet another text node reports
  `is_dirty`, so a node that is not dirty is written exactly as it was read.
- `check` in the core: the findings and the report of a check (`FindingCode`, `Finding`,
  `CheckReport` in `api::check`, with stable codes and a sentence for each), `check::document`
  (every part: the schema, the Word rules for deletions, `xml:space`, nested revisions, unique ids,
  paired ranges, cells, tables, notes, empty revisions and properties, and fields cut by a
  deletion, and the comment audit) and `check::edit`, the check of a batch under the
  no-new-violations rule. `edit` finds the paragraphs, tables, notes and frames that a transaction
  changed from the span trees, with an equality test against the base so that a long session does
  not pay for its history, and compares each with its origin, so a problem the input had never
  blocks an edit, and an element the compiler renames keeps the key of its problems. `schema`
  gained `walk_unit`, `read_children`, `is_unit_boundary` and `in_taken_branch`, so the Word rules
  read a unit as the validator does. See the [check design](docs/design/check.md).
- Under a scope, `accept` and `reject` never take a change of the body's last section properties,
  which name no paragraph, and the scope guard's second pass refuses any change to what lies between
  blocks or after the last one unless the block after it is in scope. Typed quotes read the character
  before the text, so `'s` typed after a word becomes an apostrophe. The `document_*` benchmarks
  time an edit, its dry run, an edit of the files, the first view and `export` through `Document`.
- The comment operations through `Document::edit`: `comment`, `reply`, `resolve`, `edit-comment` and
  `delete-comment`, and `delete-note` (a note named as the view shows it, `fn3`). The receipt lists what each
  did to which comment, and `reconcile` runs after the operations. Under a scope a comment operation is checked
  by the paragraphs it is anchored to.
- The operations on properties through `Document::edit`: `set` (any property of a paragraph, run, mark,
  section, table, row or cell), and `format`, `paragraph`, `style` and `list` over it, with values in
  human units (`12pt`, `0.5in`, `1.5cm`). Each is a tracked property change. None of them writes or removes
  revision markup, and a section break through `set` is untracked only.
- The `Comments` scope of `view`: one entry per comment thread, with the root's anchor, every comment's
  author, date and text and whether the thread is resolved, filtered by author and by resolved, and paged by
  the root of the last thread shown (`c7`). The comments story says which comment a reply answers and that its
  thread is resolved, and the summary line counts resolved threads.
- A JSON view gives each paragraph its `final_text`, the text that the offsets of an operation count in.
- A relative cross-reference lists the candidates it was chosen over after its target, in a margin note
  (`relative to this block, or else p3 (...)`) and in a lookup.
- `compare` through `Document`: an original and a revised document into a redline whose transaction
  `check` verifies before it commits, with the original's content, style and numbering definitions and
  copied parts declared as foreign origins, so a problem they already had is carried and not refused. The
  receipt has the revisions by author, the moves as refs, and compare's warnings, with a warning
  naming each block it could not show as changes (`compare_not_redlined`) or that rejecting the
  changes does not give back (`compare_not_restored`), by a block of the redline or, when there is
  none, by the original's first words, and one naming each copy whose paragraphs and tables did not
  pair with the original's and so may keep any of their problems (`compare_keys_merged`).
  `stetdoc compare` now writes the redline.
- `laws::compare` in the testkit: N5 and N7 through `Document`. A redline must pass `check` when
  `compare` writes it, and once saved it has no violation that neither input has after its revisions
  are accepted. It runs over the contract, over the compare pairs of the corpus (`corpus/pairs.tsv`,
  the 241 curated ones in `cargo xtask check`), and over forty curated documents against an edited
  form of themselves, and it counts the `compare_keys_merged` warnings.
- The benchmarks `document_open`, `document_save` (after a fifty-operation edit), `document_resume`
  (the working copy of an edited document) and `document_files` (the readable files alone).
- The fuzz targets `batch` (a batch parsed as JSON or block text, written as JSON and parsed back, then
  applied to a small document tracked, untracked or as a dry run) and `files` (an edit of
  `document.md`), with their dictionaries and seeds. Both run in CI's fuzz job.
- The TypeScript library over Node-API (`crates/stetdoc-napi`, package `@stetdoc/node` in its `npm/` directory): `Document` with
  `open`, `resume`, `view`, `edit`, `save`, `inspect`, `revisions`, `lookup`, `check`, `checkPending`, `files` and
  `export`, and `compare`, `capabilities` and `jsonSchema`. Calls are synchronous, data crosses as the core's JSON, offsets
  count UTF-16 code units unless an edit asks for `"scalar"`, and every failure is a thrown `StetdocError` with its `code`,
  `next`, and the `opIndex`, `file` and `line` the core set. A smoke test runs the same file under Node and Bun.
- `stetdoc-editor`: the backend of Claude's text editor tool (`text_editor_20250728`) over the readable files. `view` shows a
  file with line numbers or lists the files, `str_replace` needs a unique match (otherwise `ambiguous_match`, listing each
  match with context, or `not_found`), `str_replace` and `insert` commit at once as one tracked edit and show the touched
  blocks where they now are, and `create` and `undo_edit` are `unsupported`. The library offers it as
  `Document.textEditor(input, { author, date, pathPrefix })`, which takes the input exactly as the model sends it and returns
  the `tool_result` text, its `isError` flag, whether it committed and the receipt.
- The library's data types in `npm/types.d.ts` are generated from the core's JSON Schema by `cargo xtask codegen`, and a test
  fails when they are stale. The library exports `version` and `formatVersion`, and loads in Node and Bun worker threads.
- Packaging and release: one npm package per target (`@stetdoc/node-<target>`, seven of them) as exact optional dependencies of
  `@stetdoc/node`, with no install script; `.github/workflows/release.yml`, which builds the CLI and the addon for the seven
  targets, smoke-tests them on Bun 1.3.13, Bun 1.3.14 and Node 24, and attaches the archives, their SHA-256 files and the
  npm tarballs to a draft GitHub release without publishing anything; a `napi` job in CI; and `cargo xtask release check`,
  which replaces the placeholder `cargo xtask release` and keeps the packages and the workflow in step with the workspace.
- The library's documentation, `crates/stetdoc-napi/npm/README.md`, shipped in the package, and its sections in `README.md`
  and `CONTRIBUTING.md`.

### Fixed

- `compare` places the deleted blocks at the end of a story after the last inserted table, before the paragraphs that end the
  story, so that rejecting everything restores the original's last paragraph. Placed before the table, the paragraph that took
  the inserted end mark could not join across the table that rejecting removes, and an empty paragraph was left (`DB010`).
- The random law L3 expects a whole-paragraph `replace` of a note's paragraph to keep the note's own number (U+0003), as
  every text edit does (design `revisions.md`, 3.6.4). It expected the bare text and failed on a note of `tdf129659.docx`.
- A paragraph inserted after the last paragraph of a cell, a note, a comment or the body is written in Word's Enter shape:
  the anchor's mark is inserted and the new paragraph keeps the container's own, so Word's Reject All gives the original
  back. With an inserted mark of its own, Word left the cell an empty paragraph (hand check C11), and stetdoc's own reject
  now does the same for such a paragraph, as Word and LibreOffice do. Where that shape cannot go (the last paragraph's mark
  is a pending deletion or move) the insertion is `readonly_region`. `compare` writes a run of new paragraphs at the end of a
  container the same way. A paragraph that holds only a field and joins the next one in a view keeps its place there, with
  the field.
- `compare` never leaves a deleted table directly against an inserted one, which Word showed with a gap that stayed after
  Accept All and after Reject All: a table replaced by another is one table of deleted rows and inserted rows, whatever
  their grids: the rows take one grid, with every column edge of both when the deleted rows would
  not fit the revised grid as they are.
- Deleting a whole note with `delete-note` deletes the note's own number with its text, as Word writes it (`RP050`): with the
  number left live, Word for the web kept an empty note after Accept All. A text edit of a note still keeps its number.
- A field that spans paragraphs, such as an index, collapses in the `Final` view when the only text beside it is a
  pending deletion, as it does once the deletion is accepted, so accepting no longer changes that view (law L5, release
  seed 303). `Redline` still shows the paragraphs, with the deletion.
- A text or property edit of a paragraph inside a body-level move away (`w:moveFrom`) or deletion (`w:del`) is
  `readonly_region`, as an edit of deleted text is, where it used to write text that the Final view does not show.
- A paragraph inserted next to a block inside a body-level move, insertion or deletion goes outside that change, before
  its first block or after its last, where it used to land inside it and read as empty; anywhere else inside it is
  `readonly_region`.
- An `insert` of many paragraphs costs time in proportion to them, where each new paragraph used to scan the story
  again (4,000 empty paragraphs took 2.2 s, now about 0.1 s). Each paragraph of an `insert` now counts as one operation
  toward `max_batch_ops`, as each new paragraph already did on the files path, and the paragraphs of one `insert` follow
  the quotes of their anchor as it was before the operation.
- A style that does not exist, in `style`, `format`'s `char_style` or an inserted paragraph, is `invalid_payload` with the
  nearest names of that kind, as the error table says, and no longer `not_found`.
- `set` refuses a property its container does not allow wherever the paragraph sits, including a paragraph inside a
  body-level move, where it used to be written.
- An operation that names a block of a header or a footer that an earlier operation of the batch took out of the document
  (an accepted deletion of the section break that named it) is refused as `stale_ref`, and an edit of that footer earlier
  in the batch is applied and its refs retired. Both used to fail as `internal`.
- A receipt's `retired` lists the ref of every block a merge or a join absorbed, as `merged into p4` (or the survivor's
  reason, when the same edit removed the survivor too), as the design says. It used to list only blocks that left the
  document.
- A `delete` from `ref` through `to` compares its two points in document order: `start` in the first paragraph may be
  greater than `end` in the last, and `to` naming the same paragraph deletes the range between the two offsets.
- `check` reads a paragraph, run, table, row or cell by its own content model wherever it stands (inside a body-level
  move, under an unknown element), and the survivor of a join carries the problems the joined paragraphs already had.
- Accepting a merge, or any move, in a document that binds one prefix to different namespaces in different places
  declares the prefix on what moves, where it used to be refused as `unsupported`.
- A block that a dry run creates shows its tags (`{¶+ ...}` and the others) in the receipt, so it reads as the real edit's
  line without the ref.

- A refusal of readable files (a ref on two lines, a syntax error, a read-only line) carries its file and
  line in `StetError::file` and `StetError::line`, not only in its message, as a scope violation does.
- A merge across an imported document part (`w:altChunk`) is refused, and a paragraph deleted right before one keeps
  its mark, so accepting all reads as `Final`.
- Text put into an empty paragraph whose mark is hidden is not hidden.
- A `w:t` in a `w:moveFrom` is no longer reported as text in a deletion.
- Paragraph ids in the `w15` namespace of Word 2013's preview are read and validated as `w15`.
- A row inserted by one author and deleted by another is gone in `Final` and in `Original`.
- `Section 9.2 of This Agreement` and `PARAGRAPH (a) OF THIS SECTION` cite this document, not another: the
  word after `of` is compared in any case with `this`, `these` and `hereof`.
- The readable files are never written or read in `json/1`: `render_files` and `to_batch` refuse it with
  `unsupported`.

- Rejecting a tracked format on a run whose properties hold a property twice (tdf104649) puts
  both back instead of failing with `invalid_payload`.
- A paragraph added after one with a hidden paragraph mark is not hidden text, a copied content
  control loses its `w:dataBinding`, and new paragraphs get `w14:paraId` and `w14:textId` only in
  the Word 2010 namespace.
- A tracked deletion of an equation that already holds a tracked deletion is refused with
  `readonly_region` instead of writing a deletion inside a deletion.
- The quote policy shapes only the quotes an edit types: a paragraph that mixes curly and straight
  quotes, rewritten with one word changed, keeps the quotes it carried over.
- Text put into the empty paragraph after an equation does not copy the mark's `w:oMath` into
  its run.
- Rejecting a table whose rows were all inserted keeps a bookmark that starts in it and ends
  after it.
- The old run properties in a `w:rPrChange` are checked for Word's order, so rejecting a format
  that removed a duplicate property (tdf168070) is not refused as adding it.
- Clearing the last property of a run removes its empty `w:rPr`.
- `check` carries a copy of a defect in a `w:rPr` or a `w:pPr` (a mark's properties copied onto a
  run, a paragraph's into a `w:pPrChange`) instead of refusing the edit for it.
- An untracked edit never rewrites or hides another author's pending change: text inside their insertion,
  properties under their formatting change, a run they inserted, or a paragraph or row that holds their
  changes is `readonly_region`, and the message says to accept or reject that change first. Untracked still
  means your own changes go untracked.
