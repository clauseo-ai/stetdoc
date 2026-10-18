# Design: `refs` and `state`

Design notes for the ref table and the copy-on-write state. `ARCHITECTURE.md` is the
map, and the contracts in `refs/mod.rs` and `state/mod.rs` are the interface. This document explains
why the two modules are shaped as they are, records the measurements behind the budgets, and lists
what it asks of other modules. `wordml`, which shares this design's seam, is designed in
`docs/design/wordml.md`, and section 2 here is the agreed seam between the two. Terms follow
`GLOSSARY.md`: ref, ref namespace, state version, base token, lineage, tombstone, complement, working
copy, block, story, part generation.

The two modules carry two public promises:

- **Refs are stable, and can never silently point at the wrong paragraph.**
- **An edit is atomic.** On an error or a dry run, the document is unchanged.

Every operation and every view goes through `State` and `Tx`, so this is where both promises are
kept or lost. Three rules do most of the work, and the rest of the document follows from them:

1. **A ref names an element, never a position.** The table holds (part, node) pairs, so no edit can
   shift a ref onto a different block. A block's ref is retired when its element leaves the tree.
2. **Committed data is never mutated.** A `Tx` is a private overlay on a shared `State`. Commit
   builds the next `State`, and discard drops the overlay. Atomicity is a property of the data
   structure, not of an undo log that has to be right.
3. **Everything that can fail fails before the new `State` exists.** Scanning, limits and
   consistency checks run on the way to a commit. The step that installs the result cannot fail.

The decisions a reviewer should look at first:

- **Refs follow elements, and are minted at commit** (sections 6 and 7). Nothing is minted inside a
  `Tx`, so a dry run can never hand out a ref that a later edit reuses. An audit at commit keeps
  "live refs are exactly the blocks in the trees" true whatever the operations did.
- **A batch names a base token, not just a namespace** (section 3.3). The token is the namespace, the
  state version and a lineage: a hash chain over every committed state. So a batch from another
  document, from a forked session of the same bytes, or written before a block changed, is refused
  with `stale_ref`.
- **A merge is one declaration, `merge`** (section 7). The audit checks the composed end state of all
  the declarations of a batch, so a merge can keep either paragraph's element and still keep the first
  ref, and a merge of three blocks is two declarations.
- **The ref map is a canonical text blob** (section 8). `restore` accepts only the exact canonical
  form, an exact alignment with a fresh scan of the head package, and the scan and fingerprint
  versions it was written with.
- **`State` is immutable and `Tx` is an overlay** (section 10). Copy-on-write is per part, commit
  builds a new `State`, and caches are validated on read against per-part generations, with no
  invalidation pass (section 12).
- **New identifiers are a SHA-256 of namespace, version and position**, and hashing has one home in
  `refs/digest.rs` (sections 3.4 and 13).
- **Measured on real documents, `open` takes 15 to 66 ms against 300**, and copy-on-write, which shares
  the chunks of the node arenas, holds every operation to at most 80% of the memory budget on the five
  benchmark inputs (sections 10.1 and 15).

This document covers:

1. the module layout and who calls whom
2. the seam with `wordml`
3. the ref, the namespace, the base token and the hash
4. the ref table
5. resolution rules and every error code
6. what happens to refs through each kind of edit, and through `export` and `resume`
7. minting, declarations and the commit-time audit
8. persistence: deterministic, versioned and safe on hostile input
9. `State`: parts, lazy parsing, relationships, stories, `save` and `export`
10. `Tx`: copy-on-write, declarations, commit and discard
11. how readable files carry their base
12. derived caches
13. identifier generation and determinism
14. concurrency
15. performance, with measurements on real documents
16. how it is tested
17. what other modules rely on, decisions, and what is open
18. dependencies

## 1. Layout and who calls whom

```
wordml    BlockKey, BlockKind, Story (the block scan), fingerprints, stories(), Rels
refs      BlockRef, NamespaceId, BaseToken, RefTable, Layout, persistence, digest   (pure data, no trees)
state     State, Tx, PartsView, commit audit, derived caches, ids                   (the glue)
ops, revisions, check, projection, index, document, compare                         (callers)
```

`refs` never touches a span tree and never parses XML. It is a deep, closed data structure: a dense
table of slots with strict invariants, a hash chain, a canonical text format, and the rules for
resolving a token. `state` owns everything that has to look at trees: it scans parts with `wordml`,
decides which refs are gone and which blocks are new, and hands `refs` plain data. The other modules
see `refs` only through `State::resolve`, `Tx::resolve` and `ref_of`. They never call `allocate`,
`tombstone`, `rebind` or `merge` on the table, which is how rule 1 stays true.

Files:

```
refs/mod.rs        the contract
refs/token.rs      BlockRef, RefKind: the grammar
refs/base.rs       NamespaceId, NamespaceOrigin, Lineage, BaseToken, Claim
refs/digest.rs     SHA-256 and hex (the one place hashing lives)
refs/errors.rs     the error constructors of the module
refs/table.rs      RefTable: slots, resolve, allocate, tombstone, rebind, merge, advance
refs/layout.rs     Layout, StoryBlocks, BlockSpec: the plain data that open, persist and restore take
refs/persist.rs    the canonical text format: persist
refs/restore.rs    restore, and every check on hostile input
state/mod.rs       State, the contract
state/open.rs      open and open_in: the namespace, the package, the stories, the refs
state/parts.rs     the part index and the lazily parsed slot of each part
state/stories.rs   relationships as plain data, the story list, one Story per story part, the layout
state/view.rs      PartsView: the one way to read parts and derived values
state/derived.rs   the derived-cache store and the generations
state/tx.rs        Tx: part_mut, add_part, add_relationship, discard
state/intents.rs   the declarations (retire, rebind, merge, touch) and Tx::resolve
state/audit.rs     the commit-time audit
state/commit.rs    commit: the size check, the hashes, the lineage and the build of the next State
state/ids.rs       the sets of ids a document uses, and durable identifier generation
state/export.rs    to_package, export and resume
state/errors.rs    the error constructors of the module
state/failpoint.rs fault injection for the atomicity tests
```

## 2. The seam with `wordml`

`refs` may depend only on `api` and `wordml`, and `NodeId` is an `xml` type that `refs` may not name.
So `wordml` defines the key and re-exports `NodeId` as an opaque handle, and `refs` and `state` use
them as values. This is agreed with the `wordml` author and recorded in `docs/design/wordml.md`, and
the wording below is the same in both documents:

```rust
// wordml
pub use crate::xml::NodeId;                                 // an opaque Copy + Eq + Hash handle
pub struct BlockKey { .. }                                  // new(part, node), part(), node(). Clone, Eq, Hash. No Ord.
pub enum BlockKind { Paragraph, Table, Row }                // a cell is not a block
pub enum StoryKind { Body, Header, Footer, Footnotes, Endnotes, Comments }
pub struct StorySpec { pub part: Arc<str>, pub kind: StoryKind }
pub struct Story { .. }                                     // Clone + Send + Sync, owned, no borrow of the tree
pub const SCAN_VERSION: u32;
pub const FINGERPRINT_VERSION: u32;

impl Story {
    pub fn scan(part: &Arc<str>, kind: StoryKind, tree: &SpanTree) -> Result<Story, StetError>;
    pub fn blocks(&self) -> &[BlockEntry];                  // document order: a table, its rows, then each row's blocks
    pub fn key(&self, e: &BlockEntry) -> BlockKey;
    pub fn index_of(&self, node: NodeId) -> Option<usize>;
    pub fn fingerprints(&self, tree: &SpanTree) -> Vec<u64>;
    pub fn opaque(&self) -> &[Opaque];
    pub fn refresh(prev: &Story, tree: &SpanTree, touched: &[NodeId]) -> Result<Refreshed, StetError>;
    pub fn own_parts(&self, tree: &SpanTree, i: usize) -> Option<OwnParts>;   // what a table or a row owns, apart from its nested blocks
}
pub struct Refreshed { pub story: Story, pub added: Vec<NodeId>, pub removed: Vec<NodeId>, pub moved: Vec<NodeId>, pub context_changed: Vec<NodeId>, pub field_starts_changed: Vec<NodeId>, pub fields_carried: bool }
pub struct OwnParts { pub nodes: Vec<NodeId>, pub list: Vec<NodeId> }
pub fn stories(main_part: &str, main: &SpanTree, rels: &Rels) -> Vec<StorySpec>;
pub enum IdKind { ParaId, TextId, CommentDurableId, AnnotationId }
pub fn id_parts(kind: IdKind, stories: &[StorySpec], main_rels: &Rels) -> Vec<Arc<str>>;
pub struct Rels(..); pub struct Rel { id, rel_type, target: RelTarget }
pub enum RelTarget { Part(String), External(String), Invalid }
```

What this seam rests on, and who is responsible:

- **The key.** A `BlockKey` is the part's canonical name and a `NodeId`. The name is the entry name as
  `Package::part_names` spells it. `state` canonicalizes names, because OPC compares them ignoring
  ASCII case, and `wordml` never normalizes.
- **`NodeId` is stable.** An `xml` node id is an index into an append-only arena. `SpanTree::clone`
  copies the arena, so every id means the same node in the clone. `detach` keeps the node in the
  arena, so a detached block keeps its id, and the editor only appends. This is true by
  construction, and the `xml` contract states it, with a test. Section 16 has
  `state`'s own test of it.
- **Ids do not survive a re-parse.** A re-parsed part numbers its nodes afresh. So the persisted form
  is not the node id but the position in a scan: (part name, ordinal in `blocks()` order, fingerprint).
  The `wordml` design proposed a structural path for this. `state` does not need one, because `resume`
  requires an exact alignment with a fresh scan, and the ordinal is the flattened form of the same
  position, shorter and checked the same way.
- **The fingerprint** is defined by `wordml` (its section 3.4), which owns the exact words. They are
  quoted here without change:
  > The hash is FNV-1a 64. A **paragraph** hashes one kind byte `1`, then the UTF-8 of every text and
  > CDATA node below the `w:p`, in document order, decoded exactly as `NodeRef::text` decodes them
  > (references decoded, line endings normalized). That includes `w:delText`, `w:instrText`,
  > whitespace-only nodes and the contents of text boxes inside the paragraph. It does not allocate a
  > `String` per paragraph, and it does not use the text model. A **table** hashes kind byte `2`, then
  > the 8 little-endian bytes of the fingerprint of every scan entry whose parent is this table, in scan
  > order. A **row** hashes kind byte `3`, then the 8 little-endian bytes of the fingerprint of every scan
  > entry whose parent is this row, in scan order.
- **Versions.** "`SCAN_VERSION` and `FINGERPRINT_VERSION` change only in a major release. The
  complement records both. A release that reads a working copy recording other values answers
  `unsupported_version`, and the namespace id is recomputed with the recorded `SCAN_VERSION`."
- **Stories.** `wordml::stories` decides which parts are stories and in which order: the body, then
  headers and footers in the order the section properties reference them, then footnotes, endnotes
  and comments. Each part appears once, and a relationship whose type does not match the kind of
  its reference gives no story. `state` only converts `package::Relationships` into `Rels`.
- **Rels.** "`Rels` targets are part names spelled as the package entry spells them. An external
  target is `External`, and a target that names no part of the package is `Invalid`. `state` builds
  `Rels`, and a dangling relationship never fails an open."
- **Scan failure.** "Only the body story can fail `Story::scan`, and only when its root is not the
  document element or `w:body` is missing. A document that fails there is refused at open, and an
  edit cannot cause it. Any other story whose root is not the expected element scans as an empty
  story with an anomaly, which `check` reports, so a re-scan after an edit never fails."
- **Opaque.** "`Story::opaque` lists `w:altChunk` and every unknown block-level element. They are not
  blocks, so they have no ref." Text-box paragraphs get no entry either, so they have no ref in 1.0.
- **Story cache.** "`state` caches one `Story` per story part. The cache key is the part, and an entry
  is rebuilt on the next read when the generation of its part has changed. Nothing is keyed by a
  story's position in the story list."
- **Invalidation.** "Derived caches are not invalidated when a part is dirtied. An entry records the
  generation of every part it was built from, and the next read rebuilds it if any generation has
  changed. A generation is a pair (epoch, edits): the epoch counts replacements of a part's tree, and
  `edits` is `SpanTree::edit_count`."
- **Refresh.** "A cache builder receives the stale previous value, if there is one, so that a refresh
  can reuse it. A refresh must give exactly the value a scan from scratch gives, and the coherence
  test compares them after every edit." For a `Story` the refresh is `Story::refresh(prev, tree,
  touched)`, which returns a `Refreshed`. The block scan is run again in full. **`refresh` does not
  trust `touched` for the shape of the story.** It compares `prev`'s block list with the new scan by
  `NodeId` and treats every new, removed and moved block as touched, whatever `touched` says. So
  `touched` only has to cover edits **inside** existing blocks. Those are the edits that a comparison of
  a block's bytes before and after can see. A new block (no before), a removed block (no after) and a
  moved block (the same bytes) would defeat that comparison, which is why `refresh` finds them from the
  block lists. It carries the field index of `prev` over only when every field's begin, separate and
  end is still attached and no new, removed, moved or touched paragraph held or holds field
  characters (`wordml.md` section 4.4 has the whole rule). Without a previous value `state` calls
  `scan`.
  - **Which previous value, and which `touched`.** `PartsView::story` passes `refresh` the previous
    value that the cache holds. That value comes only from this `Tx`'s own store or from the base as it
    was when the `Tx` began, never from an older state or another `Tx`. It also passes the nodes of
    this part that were touched since the story was last built in this `Tx`. The `Tx` keeps a log of
    every call of `touch` (repeats included, unlike `touched`) and, for each story part, how long the
    log was when the story was built. A story that has none was not built in this `Tx`, so its previous
    value is the base's or the audit's seed, and it gets every touched block. Passing all the blocks
    touched so far made every refresh after one write to a paragraph with a field rebuild the field
    index (8.5 ms a refresh on `fields-13mb`, 2 to 3 ms otherwise). A block touched again after the
    build is in the log again, so the dedup of `touched` cannot hide a second write. `add_part` and
    `add_relationship` clear the cursors, so the next refresh gets every touched block. The bench row
    `state_field_writes` holds it.
- **What a refresh reports.** `Refreshed { story, added, removed, moved, context_changed,
  field_starts_changed, fields_carried }`. The first three list the blocks, by `NodeId`, that the
  comparison of the block lists found. A block is moved when it left the longest run of blocks that
  kept their order, or its parent, or its note or comment, changed. Two more lists name blocks whose
  text changes while their bytes and their place do not, which no comparison of bytes can see, so
  `wordml` finds them and `state` stamps them as changed:
  - `context_changed` lists the blocks that are in both stories and whose wrappers changed. The block
    was wrapped into or out of a block-level content control, `w:ins`, `w:del`, `w:moveFrom` or
    `w:moveTo`, or a branch of an `mc:AlternateContent`, or its row or cell gained, lost or changed a
    `trPr` or `tcPr` revision (`ins`, `del`, `cellIns`, `cellDel`). What it reads in `Final` or
    `Original` changed. The scan keeps one hashed key per block for this, so the comparison is exact
    and costs nothing. A block can be in both `moved` and `context_changed`. The row's own `trPr`
    revision is in the row's bytes, so it is a touch of the row, and the paragraphs in the row are the
    blocks listed here.
  - `field_starts_changed` lists the paragraphs that start in a different field state than they did in
    `prev`, a new paragraph counting as having started in none. It is empty when `prev` had no field
    index, which is why the audit builds the base's first (section 7, step 1).
  `fields_carried` says whether the field index was shared with the new story, and the audit does not
  use it. A block whose mark revision changes changes how the paragraph before it joins the next
  (`wordml.md` section 3.6), so `state` stamps the paragraph after it too (section 7, step 6).
- **A table and a row own less than their bytes.** A table's bytes contain its rows and a row's its
  cells, so comparing them before and after an edit would call every cell edit an edit of the table
  and the row. `Story::own_parts` says what a block owns itself. For a table: every child of `w:tbl`
  that is not a row (`w:tblPr`, `w:tblGrid` and the others) and the list of its rows. For a row: its
  `w:trPr`, `w:tblPrEx` and the `w:tcPr` of each cell, and the list of its cells. A paragraph owns all
  of its bytes, so `own_parts` is `None`. `touch` and the touch-coverage oracle (section 16) use this
  definition, so an edit inside a cell touches the paragraphs in it and not the table or the row, and
  the scope guard does not ask for their refs.
- **A story and its tree.** A `Story` records `SpanTree::edit_count` of the tree it was scanned from,
  and its field pass and `paragraph` refuse a tree whose count differs, so an index can never be built
  from another tree. That needs the counter, `SpanTree::edit_count`, which `xml` provides. `state`
  always reads a story through the same view as its tree, and the generation in the cache key makes the
  pair match.
- **Durable ids.** "`existing_ids` reports `w14:paraId` and `w14:textId` from every story part, and
  `paraId` from `commentsExtended.xml` and `commentsIds.xml` (in the `w14`, `w15` and `w16cid`
  namespaces). It reports `durableId` from `commentsIds.xml` and `commentsExtensible.xml`. It reads
  both branches of an `mc:AlternateContent`. `state` mints, and never mints a value that either branch
  uses." `id_parts` names the parts to read for a kind, and `IdKind::AnnotationId` is the highest
  existing `w:id`, which `revisions` reads.
- **Test support.** `wordml::testing` (`cfg(test)`) builds a `Story` and its keys from a small XML
  string, so `refs` and `state` tests need nothing from `xml`.

## 3. The ref, the namespace, the base token and the hash

### 3.1 The token

A ref is a kind letter and a decimal number: `p42` for a paragraph, `t3` for a table, `r17` for a
table row. Cells are not blocks and have no ref.

- **Per-kind counters.** Paragraphs count `p1, p2, ..`, tables `t1, t2, ..`, rows `r1, r2, ..`. So `p42`
  is the 42nd paragraph minted, and `t3` is the third table. A single counter would put gaps in every
  paragraph sequence at each table and row. In the Harvey LAB set, rows are 12.4% of all blocks and
  tables 1.4%.
- **Grammar.** `[ptr][1-9][0-9]*`, with the number at most 4,294,967,295. It is case sensitive and has
  no sign, no leading zero and no whitespace. `BlockRef::parse` is the only parser and `Display` the
  only writer, so the token can change in one place. The projection formats may wrap a ref in their
  own syntax (the format experiments decide that), but must call `BlockRef::parse` on what they
  extract.
- **A malformed token is `invalid_payload`.** `p0`, `p007`, `P42`, `p-1`, `banana` and a number above
  the range are all one rule, with a message that shows what a ref looks like. A well-formed token
  that was never minted is a different thing, `not_found`, in section 5.

### 3.2 The namespace id

```rust
pub struct NamespaceId([u8; 8]);     // Display: 16 lowercase hex digits, such as 3f9a0c11d2b4e577
pub enum NamespaceOrigin { Opened, Derived { label: String, parents: Vec<NamespaceId>, extra: Vec<u8> } }
NamespaceId::of_package(opened: &[u8], scan_version: u32) -> NamespaceId
NamespaceId::derived(label: &str, parents: &[NamespaceId], extra: &[u8]) -> NamespaceId
```

`of_package` is the first 8 bytes of SHA-256 over a domain tag, `REF_SCHEME`, the scan version and the
bytes passed to `open`, each length-prefixed. `open` passes `wordml::SCAN_VERSION`. So:

- The same bytes give the same namespace and the same refs, on every run and every platform. Opening a
  file twice, or opening the bytes of an unedited `save`, gives the same namespace. That is the
  determinism the architecture asks for, and it is what makes `stetdoc view a.docx` followed by
  `stetdoc edit a.docx -o b.docx` work with a batch that names the base token.
- Bytes that differ, which any edit that saved a changed part produces, give a new namespace, so every
  ref from before is `stale_ref`.
- A release that numbers blocks differently (a new `REF_SCHEME` or scan version) gives a different
  namespace for the same bytes, so it can never share refs with a release that numbered them
  otherwise.
- The id is the identity of the opened bytes, not a random token and not a clock reading.

Sixty-four bits is an accident guard and a second-preimage bound. The namespace is not a secret and
is not meant to stop an attacker who controls both documents.

`derived` is for a `State` that is not the result of opening bytes. `compare` builds its redline on
the revised package, with refs that differ from a fresh open of those bytes, so it uses
`derived("compare", [original, revised], stamp bytes)`, and a redline never shares a namespace with
either input. The origin is recorded in the ref map (section 8), so that `resume` can check a derived
namespace by recomputing it from its recorded inputs. An opened namespace is checked against the
original package bytes in the complement. The rule that compare uses belongs to `compare`, and is
recorded here because the function is here.

### 3.3 The base token, the lineage and the claim

Two sessions opened from the same bytes share a namespace and the same refs at open. They then
diverge. In one, an inserted paragraph becomes `p301`, "Governing law". In the other, a different
inserted paragraph becomes `p301`, "Notices". A batch from the first, naming `p301`, would silently edit
the second's "Notices". A namespace alone cannot tell the two histories apart, so a batch names more.

```rust
pub struct Lineage([u8; 8]);
pub struct BaseToken { namespace: NamespaceId, version: u32, lineage: Lineage }
    // Display: "3f9a0c11d2b4e577:7:51ac09f2e8b34d11". Opaque to callers: copy it, never build one.
pub enum Claim<'a> { Base(&'a BaseToken), Implied }
```

**The lineage is a hash chain over the committed states of one namespace.** `L0` is the namespace id.
For each commit that makes version `v`, `Lv` is the first 8 bytes of SHA-256 over a domain tag, `L(v-1)`,
`v`, and a digest of the commit's changes. The digest (section 10.4) covers the SHA-256 of the new
serialized bytes of every part the commit changed, the bytes of every part it added, and the bytes of
every part that `add_part` rewrote (`[Content_Types].xml` and the relationships part of the source,
which section 10.2 calls the raw set), and the canonical text of the commit's ref events (refs minted,
retired, rebound and merged, with their story and block ordinals, never node ids). The table keeps the whole chain, `L0` to the current `Lv`.

- **Two histories get two lineages.** The two sessions above differ in the bytes of the body part, so
  `L1` differs. Two sessions that made byte-identical edits from byte-identical documents have the same
  lineage and the same refs, and for them a batch is correct in either. So determinism holds: the same
  inputs give the same lineages.
- **Views, receipts and readable files carry the document's base token.** Every one of them shows the
  token of the state it describes. Version 0 has the lineage equal to the namespace.
- **A batch names the token it was written against, always.** In a working copy as well, and for a
  `.docx` edited directly. The claim is checked against the table's history (section 5). `Implied`, a
  claim that names nothing, is accepted only on the read path, for `inspect` and for the operations
  that `Edit::files` derives, which already compare the files' token with the document's
  (section 11). `Tx::resolve`, which every operation goes through, refuses `Implied` with
  `invalid_payload`.
- **The state keeps, for each live block, the version it last changed in** (section 4). A batch written
  at version 3 may edit a block untouched since, and is refused for a block changed in version 5, with
  `stale_ref` and a message that says to view the document again. This is what protects a text offset
  or a `find` from landing on text that is no longer there. The same rule refuses a ref minted after
  the claim's version, because a block's changed version is at least its minted version.
- **Cost.** The chain is 8 bytes per version in memory and 17 in the ref map. A commit hashes the
  serialized bytes of the parts it changed, which `commit` serializes for the size check anyway:
  36 ms for the 12.6 MB body of `bug65649`, 14.7 ms for the 5.3 MB handbook, and about 4 to 8 ms for
  a 300-page contract. Section 15 has the numbers. The chain is capped at `Limits::max_refs`
  versions, with `limit_exceeded`.

### 3.4 Where the hashing lives

There are three hashes, and each has one home:

| Hash | Used for | Home | Algorithm |
|---|---|---|---|
| Namespace id, lineage, durable ids, the complement's digests | identity and identifiers that must be stable across releases and platforms | `refs/digest.rs` | SHA-256, truncated where a short id is wanted |
| Block fingerprint | a consistency check between the ref map and a re-parsed package | `wordml` (`Story::fingerprints`) | FNV-1a 64, frozen |
| In-memory maps | lookup only | `std` hash maps | never used for anything persisted, minted or ordered |

`refs/digest.rs` is the home of SHA-256 because the namespace id is its first user and `state`,
`projection` and `document` all depend on `refs`. It exposes `Hasher`, which length-prefixes each field so that concatenations cannot collide, `sha256`
for the plain digests that the complement records, and `hex` and `unhex`. The durable-id
rule (section 13) lives in `state/ids.rs`, because it needs the namespace and the state version,
which only `state` knows. `compare`'s alignment hashes are not stable-identity hashes, so `compare`
uses its own in-process hash and never this one.

**SHA-256 comes from the `sha2` crate**, which is already a workspace dependency (corpus fetch uses
it), is widely used and fuzzed, and passes `cargo deny`. Rule 6 of `ARCHITECTURE.md` allows a
dependency that uses `unsafe` internally on those terms. A hand-written SHA-256 would be safe code,
and it would run at about the same speed (section 15 measures 330 MB/s for the crate on the
development Mac, where it did not use hardware instructions). The crate is the better choice for
linux-x64, where it detects SHA extensions at run time and runs several times faster.

**Every ordered output is ordered by document or story position, or sorted. None comes from iterating
a `HashMap`.** Rust's default hasher is seeded per map, so iteration order differs between runs. This
is a rule for both modules and is tested (section 16).

## 4. The ref table

```rust
pub struct RefTable {
    namespace: NamespaceId,
    origin: NamespaceOrigin,
    history: Vec<Lineage>,                   // the lineage of versions 1 to the current one; version 0 is the namespace
    parts: Vec<Arc<str>>,                    // interned part names, in first-use order
    slots: [Vec<Slot>; 3],                   // by kind; index is the ref number minus one
    by_key: HashMap<(u16, wordml::NodeId), BlockRef>, // reverse map, lookup only
}
enum Slot { Live { part: u16, node: wordml::NodeId, changed: u32 }, Gone { reason: Reason, at: u32 } }
pub enum Reason { Deleted, MergedInto(BlockRef), Rejected }
```

The current version is `history.len()`.

- **Numbers are dense and never reused.** Minting a ref pushes a slot, so the number is the slot's
  index plus one. A ref goes from `Live` to `Gone` and never back. So every number up to the counter
  is exactly one of live or gone, and `resolve` is an array index, not a hash lookup. A ref that is
  above the counter never existed.
- **The stamps.** A live slot records `changed`, the version the block last changed in, which is its
  minted version until something touches it. A gone slot records `at`, the version that retired it.
  Both are used for messages and for the claim checks of section 5, and both are persisted.
- **The map is one to one.** A key has at most one live ref, and a live ref has one key. `rebind` and
  `merge` are the only operations that move a ref, and they refuse a key that already has one, except
  as `merge` allows (section 7).
- **Size.** A slot is 16 bytes and a map entry 16, so about 40 bytes per block with the map's spare
  room. The largest document measured (18,260 blocks counting tables and rows) is 0.8 MB of table.
  Cloning and dropping that table takes 0.02 ms warm and 0.13 ms the first time, so commit may clone
  it instead of keeping an undo log. A resolve that parses the token, checks the range, reads the slot
  and builds the key takes 10 ns.
- **A cap.** The table refuses to grow past `Limits::max_refs` (live and gone together), and so does
  the history, with `limit_exceeded`. The default is 1,048,576, which is 57 times the largest document
  measured. A session cannot reach it without about a million deletions or commits, and `restore` uses
  the same cap against hostile input (section 8).
- **Interface to `state`:** `RefTable::open(ns, origin, &Layout, limits)` assigns refs in document order
  (this is `allocate` over the whole layout), `allocate(&BlockKey, BlockKind, at)` mints one ref,
  `tombstone(BlockRef, Reason, at, &Merged)` retires a ref (and gives the same reason to the refs that
  merges of this commit retired into it), `rebind(BlockRef, &BlockKey)` and `merge(survivor, absorbed,
  &BlockKey, at, &mut Merged)` move refs (a merge retargets what earlier merges of this commit retired
  into the absorbed ref), `touch(&BlockKey, at)` stamps a change, `advance(&digest)` extends the lineage
  and the version, `ref_of`, `key_of` and `live_in(part)` read it, and `persist` and `restore`
  serialize it. For everyone else it is `resolve(claim, token)` and `ref_of(&BlockKey)`.
- **`Merged` and who may change the table.** `Merged` is the list of the refs that merges of one commit
  retired. The audit owns it for the commit it builds, so it is not part of the table, which does not
  keep, copy or save it. The methods that change the table are `pub(crate)`, because Rust cannot name a
  sibling module in a visibility: `state` is a sibling of `refs`, and `pub(super)` would mean the whole
  crate anyway. Only `state` calls them. `ops` and `projection` may depend on `refs` (for `resolve` and
  `ref_of`), so nothing but review keeps them from calling a mutator, and the contract of `refs/mod.rs`
  says so. A sealed trait or a token type would not help, because anything in the crate can implement
  one or build the other, and a token defined in `state` would make `refs` depend on `state`.

`Layout` is the plain data that `open`, `persist` and `restore` take, and the only thing `refs` knows
about a document:

```rust
pub struct Layout { pub stories: Vec<StoryBlocks> }
pub struct StoryBlocks { pub part: Arc<str>, pub kind: StoryKind, pub blocks: Vec<BlockSpec> }
pub struct BlockSpec { pub key: BlockKey, pub kind: BlockKind, pub fingerprint: u64 }   // in blocks() order
```

`open` does not need fingerprints, and `state` leaves them zero there. `persist` and `restore` do,
and `state` computes them with `Story::fingerprints`.

## 5. Resolution

`resolve(claim, token: &str) -> Result<BlockKey, StetError>` checks in this order, and stops at the
first failure. It takes the token as the batch wrote it, so the claim is checked before the token is
read. `BlockRef::parse` is also public, for callers that need the parsed form, such as the scope guard.
`State::resolve` is the read path. `Tx::resolve` is the edit path, and adds the checks below it.

| Step | Condition | Code |
|---|---|---|
| 1 | `Claim::Implied` on the edit path (`Tx::resolve`) | `invalid_payload` |
| 2 | The claim's namespace is not the table's | `stale_ref` |
| 3 | The claim's version is above the document's | `stale_ref` |
| 4 | The claim's lineage is not the table's lineage for that version | `stale_ref` |
| 5 | The token does not parse | `invalid_payload` |
| 6 | The number is above the count of refs of that kind minted so far: this ref never existed | `not_found` |
| 7 | The slot is `Gone` | `stale_ref` |
| 8 | A `Base` claim, and the slot's `changed` is above the claim's version | `stale_ref` |
| 9 | The slot is `Live` | the block's key |

Steps 2 to 4 are the base check. They come first, so a batch from another document or another history
learns nothing about this one. Every message is a whole sentence, and every `stale_ref` here carries a
`next` hint that says to view the document again and rewrite the batch from the new view:

- `stale_ref`: "This batch was written for another document (namespace 3f9a0c11d2b4e577, not
  71c20e4b9a3d05f8). Refs do not carry over when a file is reopened. View the document again."
- `stale_ref`: "This batch was written for version 9 of a document that is at version 7, so it comes from
  a different copy. View the document again."
- `stale_ref`: "This batch was written for a different history of this document: another working copy,
  or another session opened from the same file. Its refs may name other blocks here. View the document
  again."
- `stale_ref`: "p31 was deleted in version 4."
- `stale_ref`: "p6 was merged into p5 in version 4."
- `stale_ref`: "p42 was changed in version 7, after version 5, which this batch was written for. View it
  again."
- `not_found`: "p9000 does not exist. This document's paragraphs run from p1 to p8152."

A merge message looks at its target once. If the target is gone too, the message adds "and p5 has
since been removed", and does not follow the chain further.

**`Tx::resolve` is stricter.** Inside a transaction the base table is still the pre-edit table, so the
transaction answers from its own declarations and checks attachment before it returns a key:

- A ref that an earlier operation of this transaction retired or merged away is `stale_ref` with the
  reason, for example "p6 was merged into p5 earlier in this batch". A ref that an earlier operation
  rebound or kept as a merge survivor resolves to its new key, so a split's second operation sees the
  first part where the agent expects it.
- The block must still be attached. The transaction walks the parent links of the node in its
  staged tree, at most the nesting depth (256), and requires that it ends at the document node. A
  block that an earlier operation of the batch detached is `stale_ref`: "p5 was deleted earlier in
  this batch". Without this check, a later operation would edit a detached node and the edit
  would silently go nowhere.
- The block's part must still be a story. A header or a footer that no section names any more,
  because an earlier operation of the batch accepted the deletion of the section break that named
  it, keeps its tree, and its blocks stay attached, but it is out of the document and the commit
  retires its refs: a block of it is `stale_ref`, "p142 is in word/footer2.xml, which no section of
  the document uses after an earlier operation of this batch". The receipt likewise leaves out the
  blocks and changes of such a part.

**Resolution never reveals a minted number early.** Refs are minted at commit (section 7), so no ref
exists for a block before it is committed.

## 6. What happens to refs

The rule: **a ref follows its element. It is retired when the element leaves its part's tree.** A
ref's lifetime is the element's lifetime, never the block's content, tracked status or position.
A block "removed from the document" means an untracked deletion, an accepted deletion, a rejected
insertion, or the absorbed block of a merge.

| Event | Effect on refs | Who acts |
|---|---|---|
| `open` | p, t and r numbers are minted in story order, then document order, so the same bytes always give the same numbers | `state` |
| Edit text, format, style, list level, set a property | None, except the block's `changed` version. It is the same element | `ops` touches |
| Insert a paragraph, tracked or not | A new element. It gets the next number of its kind at commit, which is larger than every number issued so far, wherever the block sits in the document | audit |
| Delete a paragraph, **tracked** | None. The element stays in the tree, with its content in a `w:del` and, when a paragraph follows it and it keeps no section break, a deleted mark. The block is touched, and the paragraph after a changed mark is stamped. The ref still resolves | `ops` touches |
| Delete a paragraph, **untracked** | The element is detached, so the ref becomes `Gone(Deleted)` | audit |
| Accept a deletion | The element is detached, so `Gone(Deleted)` | audit |
| Reject an insertion | The inserted element is detached. `revisions` declares `retire(r, Rejected)`, and without the declaration the audit records `Deleted` | `revisions` declares |
| Reject a deletion | None. The block is restored, and touched | `revisions` touches |
| Split p5 at an offset | **The original ref names the first part, and the new block, the second, gets a new ref.** `revisions` should keep the original element for the first part, so there is nothing to declare. If it keeps the original element for the second part instead, it declares `rebind(p5, first-part element)`, and the old element is minted as the new block | audit, or `revisions` declares |
| Merge p6 into p5, untracked | One declaration, `merge(p5, p6, element)`, where the element is whichever of the two the compiler kept. p5 names it and p6 becomes `Gone(MergedInto(p5))` | `revisions` declares |
| Merge, tracked | Both elements stay in the tree, with nothing declared, until the deleted mark is accepted. The accept joins into the first element: p5's element survives, p6's properties and content move into it, and `revisions` declares `merge(p5, p6, into: p5's element)`. p5 keeps its ref and its element, and p6 is `Gone(MergedInto(p5))` | `revisions` declares |
| Merge three blocks | Two `merge` declarations, each naming a block by its current element. Accepting two consecutive deleted marks, in p5, p6 and p7, joins into the first element, so `revisions` declares `merge(p6, p7, into: e6)` and then `merge(p5, p6, into: e5)`. p5 names e5 at the end, p6 and p7 are `Gone(MergedInto(p5))`, and e6 and e7 are gone from the tree. The other order and the other element are as valid. The audit checks the composition, section 7 | `revisions` declares |
| Insert or delete a table row | The same as paragraphs, with `r` refs. A tracked row deletion keeps the row | |
| Delete a table, untracked | The table node is detached. The audit finds the table, its rows and every paragraph in them gone, and retires all of them | audit |
| A move | See below | |
| `export` then `resume` | The table is persisted and restored. Every number, live or gone, every stamp and the whole lineage are the same | `refs` |
| Reopen saved bytes | A new namespace. Every old ref is `stale_ref` | `refs` |

**Tracked deletion keeps the ref live.** The table answers one question, "is this element still in
the document", and a tracked-deleted paragraph is in the package until the deletion is accepted. The
alternatives are worse. Retiring a ref at tracked deletion would force a way to revive it on reject.
Hiding it would make the table depend on revision state that `revisions` owns. So the agent can
still resolve p5 after deleting it with tracking, and the block shows in the redline view and not in
the final view. A block that left the document is `stale_ref`.

**Which paragraphs are tracked-deleted.** A **tracked-deleted block** is a paragraph whose mark is
deleted (`w:del`, or `w:moveFrom`) and whose `Final` text is empty, or one that sits in a deleted row or
cell, so it is absent from the final view (`revisions.md` 3.2.5 has the definition). An operation that
would edit the text or the properties of one is refused with `readonly_region`. That refusal belongs to
`revisions` and `ops`, and it is a different mechanism from a tombstone. Two neighbours
are not tracked-deleted blocks, and they stay live and editable:

- A paragraph whose **content** is all deleted but whose **mark** is live. It is the last paragraph of
  a container, which must end in a paragraph (a cell), or a paragraph that keeps a section break, which
  `revisions` leaves with its mark so that the section still ends. New text goes after the deleted text.
- A paragraph whose mark is deleted but that still has live text. It is a pending merge, which joins the
  next paragraph in the final view, and its text can be edited.

In all three shapes the element is in the tree, so the ref is live and resolves. What `state` adds is
the stamps. A paragraph whose mark revision changes changes what the paragraph after it is part of, so
the audit stamps that paragraph (section 7, step 6), and a paragraph whose content is deleted and whose
mark is live changes no block but itself. The tests of `state/tests_deleted.rs` run the three shapes.

**Moves.** The operation list has no move, so there is no ref event for one:

- A pre-existing `w:moveFrom` or `w:moveTo` paragraph is an ordinary block with an ordinary ref.
- `compare` writes a moved paragraph as a deletion and an insertion, so the old ref stays on the old
  paragraph (deleted by a tracked change) and the new paragraph gets a new ref.
- If a later operation moves an element within its part through the editor (detach, then insert),
  the node id is unchanged and the ref follows with no action. If a move is built from a clone, the
  clone has a new id, and the compiler must declare `rebind(old ref, clone)`, or the move reads as a
  deletion and an insertion.
- A block does not move between parts. That is a deletion in one story and an insertion in another.

**What the agent sees.** Receipts list each touched block with its ref after the edit, and carry the
new base token. A split receipt says the original ref is the first part, and shows the new ref beside
it. A merge receipt says which ref survived. These are `projection` and `ops` concerns, and the table
gives them the facts through the `Committed` report (section 10.4).

## 7. Minting, declarations and the commit-time audit

**Refs are minted at commit, never inside a `Tx`, and never in a dry run.** The alternative, where
`insert` calls `allocate` as it creates the element, fails a promise. A dry run's receipt would show
`p301`, the dry run would be discarded, and a different edit could then commit `p301` to another block.
An agent still holding the dry run's `p301` would hit the wrong paragraph with no error. Minting at
commit means a ref exists only for a block that is in a committed state, so there is nothing to
reuse. Dry runs and discards never touch the counters, so "the document is unchanged" is true
without any undo step.

It costs nothing that an operation needs. A batch is written blind, so no operation can name a block
that an earlier operation of the same batch created. The scope guard names existing blocks. A receipt
is built after commit and reads the new refs from the `Committed` report. A dry run's receipt lists a
new block without a ref, identified by its position relative to existing refs.

**The declarations.** The trees say which elements are gone and which are new. They cannot say which
identity survives a split or a merge, or why a block went. The operations say that through three
declarations on the `Tx`, and `touch` records which blocks changed:

```rust
Tx::retire(&BlockKey, Retired)                      // Retired::Deleted or Retired::Rejected
Tx::rebind(from: &BlockKey, to: BlockKey)           // the ref of `from` now names the element `to`
Tx::merge(survivor: &BlockKey, absorbed: &BlockKey, into: BlockKey)
Tx::touch(&BlockKey)                                // this block changed
```

They name blocks by key, which is what an operation holds after `resolve`. The table turns a key into
its ref.

- **`merge`** says that two blocks became one, and which element is the result. `into` is the
  survivor's element, the absorbed block's element, or a new element with no ref. The survivor's ref
  names `into`, and the absorbed ref becomes `Gone(MergedInto(survivor))`. The element that is not
  `into` must be gone from the tree when the commit is audited. Because the rule is about the end
  state and not about an order of steps, it expresses both ways a compiler can build a merge: keep the
  first paragraph's element, which is what `revisions` does when it accepts a deleted paragraph mark
  (the first element survives and takes the later paragraph's properties and content), or keep the
  second's, as Word's own accept does, or make a new one. **A merge of three blocks is two `merge`
  declarations**, and each names a block by its current element. For two consecutive deleted marks,
  `revisions` declares `merge(p6, p7, into: e6)`, after which the ref p6 names e6, and then
  `merge(p5, p6, into: e5)`, with p6's key being e6, which is what `Tx::resolve("p6")` returns at that
  point. The other order, `merge(p5, p6, into: e6)` and then `merge(p5, p7, into: e7)`, is just as
  valid. The audit checks the composition of all the declarations (step 3 below) and not each one
  alone, so an `into` that a later merge absorbs is valid. If a ref that was merged
  earlier in the same commit is absorbed in turn, its tombstone is retargeted to the final survivor, so
  no chain of merges forms inside one version.
- **`rebind`** is for a split that keeps the original element for the second part. The target must be
  an element with no ref. The old element, if it is still in the tree, is minted as a new block.
- **`retire`** is for the reason only. A plain deletion declares nothing, because the audit sees the
  element gone and records `Deleted`. Rejecting an insertion declares `Rejected`.
- **`touch`** records that an existing block **changed in place**. It is what the scope guard reads, and
  what the commit stamps into the block's `changed` version. `ops` and `revisions` call it for every
  existing block whose own bytes change. A paragraph owns all of its XML. A table owns its
  `w:tblPr`, `w:tblGrid` and other children that are not rows, and the list of its rows. A row owns its
  `w:trPr`, `w:tblPrEx`, the `w:tcPr` of each of its cells, and the list of its cells
  (`Story::own_parts`, section 2). So an edit inside a cell touches the paragraphs in it and not the
  table or the row. **Inserting, removing or reordering a row changes the list of rows, which the table
  owns, so the operation touches the table, and a cell does the same to its row.** `touch` takes any
  block, and nothing in it refuses a table whose rows changed: the guard sees the key in `touched()`,
  and what the guard asks of the scope for such a touch is `ops`'s decision. `touch` need not name a
  block that is new, removed, moved or rebound, and need not name a block whose text changes without
  its bytes changing: the audit finds those from the trees (step 6). A block that is new, rebound or
  the survivor of a merge is stamped without a call.

**The audit** runs inside `Tx::commit`, over the affected parts: every staged part that is a story, every
part whose story status changed (a new comments part, a removed header), and nothing else. A staged
styles part is not scanned.

1. Scan each affected part. A part that was a story is refreshed against the base's `Story`, the one
   the `Tx` began from, with `Story::refresh`, so the audit has the blocks that moved, the blocks
   whose wrappers changed and the paragraphs whose field start changed. `refresh` reports nothing when `prev` never ran the field
   pass, and a state that was just opened or resumed has not, so the audit runs the base story's pass
   first. It costs one pass per story and session (about 11 ms on the largest body), because the new
   story carries the index on. Without it a stale batch could edit, by text offset, a paragraph whose
   field start this commit changed. A part that is new as a story is scanned. A part that is no
   longer a story scans as empty. A story that a declaration names is scanned too, whether or not
   anything else changed it, because a declaration can bind a ref to an element of a part that no
   operation edited.
2. On a clone of the table, apply the declarations in order. Each is checked against the table as the
   earlier ones left it: `retire` and `merge` need live refs, `rebind` needs a ref that is live and a
   target with no ref.
3. **Check the composed result of the declarations** against the scans, before anything is
   tombstoned or minted:
   - Every ref that a `rebind` or `merge` moved, and that is still live, names a block of the ref's
     kind in a scan, or an element that is no longer in the document. It is the **final** element that
     counts, so a ref moved twice is checked once, and an element that a later declaration absorbed
     is not required to stay. An element that a later operation deleted (a merged block that is
     deleted in the same batch) is not a contradiction: step 4 retires the ref like any other block
     that left. An element that is in the document and is not a block, or is a block of another
     kind, or is in a part that is not a story, is a contradiction.
   - Every element that a declaration released as gone, and that no later declaration took back, is
     not in a scan. `retire` releases its block's element. `merge` releases the survivor's current
     element and the absorbed block's element, whichever of them is not `into`. `rebind` and `merge`
     take back the element they bind a ref to. `rebind` does not release the old element, which may
     stay in the tree as the new block of a split.

   A check of each declaration alone would refuse valid input. In `merge(p6, p7, into: e6)` followed
   by `merge(p5, p6, into: e5)`, the first `into`, e6, is gone by the end, because the second merge
   absorbed it and kept e5. The same is true of a `rebind` whose target a later `merge` absorbs.
4. For every live ref in an affected part, in slot order, whose node is not in the scan, `tombstone` it.
   The reason is the declared one, or `Deleted`. The refs that merges of this commit retired into a
   ref that is tombstoned here get the same reason, since the block they were merged into went in the
   same commit. So no tombstone of one version points at another of the same version, and `restore`
   can keep its rule that a merge target is live or was retired in a later version.
5. For every block in the scan that has no ref, in document order, parts in story order, `allocate`
   the next number of its kind.
6. Stamp the version: set `changed` to the new version for
   - the touched blocks that are still live, the new blocks, the merge survivors and the rebind
     targets;
   - the blocks that `refresh` reports as moved, those whose wrappers changed (`context_changed`: wrapped
     into or out of a content control or a revision, or in a row or cell that gained or lost a
     revision) and the paragraphs whose field start changed (`field_starts_changed`), since their text
     or position changes though their bytes do not;
   - the tables and rows whose own parts changed in a way that the trees show: a row or a cell was
     inserted, removed or reordered, or a node of the block's own properties was added or removed. The
     audit compares `Story::own_parts` of each table and row that is in both stories, in the base's
     tree and in the new one. So a stale batch that names a table whose rows changed is refused though
     no operation touched the table. An edit inside one of the property nodes, such as an attribute of
     `w:tblPr` or `w:trPr`, is not visible in the lists and stays the operation's `touch`;
   - the paragraph after every paragraph whose mark revision changed. The audit compares the `MARK_*`
     flags of each paragraph in the base scan with the new scan, by `NodeId`, and a paragraph that is
     new or removed and has any `MARK_*` flag counts as changed. The next paragraph in the story is
     stamped, because the join of the two depends on the mark, and a stale batch that edited it by
     offset must be refused.

Because step 5 follows document order and story order, the numbers depend on the committed trees and
not on the order the operations ran in. Because steps 4 and 5 follow the trees, the invariant "the
live refs are exactly the blocks in the trees, one to one" holds after every commit, **whatever the
operations did.** An operation that forgets to declare something cannot leave a ref on a detached
node or a block with no ref. The worst a forgotten declaration costs is a worse message ("p6 was
deleted" instead of "p6 was merged into p5"), a lost identity (a split that comes out the wrong way
round), or a block whose `changed` version was not raised. The conformance suite closes each of
those: the identity oracle (section 16) checks identity, and an oracle over every operation
compares the serialized bytes of each existing block's own parts before and after, and requires every
block whose own parts changed to have been touched. New, removed and moved blocks are outside that
comparison, and the audit finds them from the trees. A second oracle runs `refresh` against a fresh
scan after every operation. The scope guard needs the same property of `touch`.

A declaration that contradicts the trees is a bug in the caller, not an input error: a `retire` of a ref
whose node is still attached, a `merge` whose released element is still attached after the composition,
a `rebind` or `merge` target that is not a block of the ref's kind, or one that already has a ref. It fails the commit with `internal`, which changes nothing, and
the message names the declaration. `internal` is a new error code in its own exit class, 7, whose hint
says to report the bug and never to try another edit. Callers must be able to tell their own
validation failures from ours.

## 8. Persistence

`RefTable::persist(&self, layout: &Layout) -> Vec<u8>` and
`RefTable::restore(bytes: &[u8], layout: &Layout, original: Option<&[u8]>, limits: &Limits) -> Result<RefTable, StetError>`.

### 8.1 The format

A line-oriented text format, `stet-refs 1`. Every line ends in a line feed. Fields are separated by one
space. It is plain ASCII, so there is nothing to escape except part names.

```
stet-refs 1
namespace 3f9a0c11d2b4e577 opened
scan 1 1
version 2
lineage 2
51ac09f2e8b34d11
7be0c4a5d1f93268
counts 6 1 2
stories 2
story word/document.xml body 6
t1 9f3a0c11d2b4e577
r1 0123456789abcdef 2
p1 a3c4e1f2b5d60789
r2 f00dfacecafe0123
p2 5b6c7d8e9f001122 1
p3 0badc0de12345678
story word/header1.xml header 1
p6 c0ffee00deadbeef 2
gone 2
p4 d 1
p5 m p3 2
end
```

- `namespace` gives the id and its origin: `opened`, or for a derived namespace
  `derived <label> <n> <n parent ids> <extra as hex, or ->`. The label is at most 32 characters of
  `[a-z0-9-]`, there are at most 4 parents, and the extra is at most 256 bytes.
- `scan` holds the scan version and the fingerprint version the map was written with. `version` is the
  state version, and `lineage` lists `L1` to `Lv`, so `L0` is the namespace and is not repeated.
- `counts` is the number minted so far per kind, live and gone together, so that a hostile count is
  known before any allocation.
- Each `story` line gives the part name (percent-encoded: every byte outside `[A-Za-z0-9._/-]` becomes
  `%XX` in upper-case hex), the story kind and the number of live blocks. Exactly that many lines
  follow, one per live block in `blocks()` order: the ref, the fingerprint as 16 lower-case hex digits,
  and the version the block last changed in, left out when it is 0.
- `gone` lists the retired refs, sorted by kind (p, t, r) and then number. `d` is deleted, `x` is
  rejected, and `m` followed by a ref is merged into that ref. The last field is the version that
  retired it. There are no ranges, because a range would let a short line stand for four billion refs.
- `end` is the last line, and nothing follows it. A truncated blob has no `end`.

The format has **exactly one encoding for a table**: lower-case hex, canonical decimal, one space,
sorted tombstones, stories in layout order, a stamp only when it is not 0. Section 8.4 turns that into
the main safety check.

Size: 24 bytes per live block, so 0.44 MB for the 18,260-block document, plus 17 bytes per version.
The complement is already dominated by the package bytes.

### 8.2 Determinism

`persist` has no hidden inputs. The stories come from the layout, the refs from slots in order, the
tombstones sorted. The time is not read, and no map is iterated. The same table and layout give the
same bytes, and the property test persists a table, restores it and persists it again.

### 8.3 Versioning

The leading integer is the blob's format version, and it is bumped for every change, because the
canonical-form rule below leaves no room for optional records. A newer integer returns
`unsupported_version`. An older one is read by a function kept for that version, as `ARCHITECTURE.md`
says of complements. A blob that does not start with `stet-refs ` at all is `corrupt_complement`.

The `scan` line is checked next. **A working copy that records another scan version or fingerprint
version than this release's is `unsupported_version`**, with a message that names both. It is not
`corrupt_complement`, because nothing is damaged: this release would number the blocks differently.
The two versions change only in a major release (section 2), so within a major version this never
happens. A minor release that needed to change the scan would have to make `Story::scan` take the
recorded version and keep the old rules.

`document` stores the blob as one member of the complement, with its own complement version, so this
integer versions the ref map and nothing else.

### 8.4 Hostile input

The agent can write to the complement, so `restore` trusts nothing. The input is bytes, and the rules
are, in this order:

1. **Size before parsing.** The blob may not exceed `64 * max_refs + 65,536` bytes. `counts` must sum
   to at most `max_refs` and to at most `bytes.len() / 4`, since a ref's own line is at least five
   bytes, and `version` is at most `max_refs`. Vectors are sized from the checked count, never from a
   number the blob claims, so a blob cannot make us allocate more than a small multiple of its own
   size.
2. **Magic and versions** (section 8.3): `corrupt_complement` or `unsupported_version`.
3. **Strict parsing, line by line.** A line longer than 4,096 bytes, a missing or extra field, a number
   with a leading zero, upper-case hex, a bad percent escape, a byte outside printable ASCII, a
   missing `end` or any byte after it is `corrupt_complement`, and the message gives the 1-based
   line number.
4. **The namespace.** A derived namespace is recomputed from its recorded label, parents and extra
   and must match. An opened one must equal `of_package(original, recorded scan version)`, and
   `original`, the original package bytes in the complement, is required for it. A forged namespace
   line therefore fails. (An agent that rewrites the original bytes as well has made a different
   working copy, which is its own business.)
5. **The history.** `lineage` has exactly `version` entries. Every `changed` is at most `version`, and
   every retirement version is between 1 and `version`.
6. **Exact alignment with the layout.** The layout is built by `state` from the freshly parsed head
   package. The stories must be the same parts, kinds and order. Each story must have the same number
   of blocks, each block the same kind as its ref's letter, and each fingerprint must match. Any
   difference is `corrupt_complement`, with the first ref that disagrees and what changed
   ("p42 is block 41 of word/document.xml, and its text is not the text it had when this working copy
   was exported").
7. **Table consistency.** Per kind, the live and gone numbers together are exactly 1 to the count,
   once each. The counts, the story totals and the live and gone lists agree. A merge tombstone does
   not point at itself, its target is a ref of the table, and the target is live or was retired in a
   strictly later version, so a chain of merges cannot loop.
8. **Canonical form.** After parsing, `restore` builds the table, calls `persist` with the same layout
   and requires the result to equal the input byte for byte. This one check rejects every non-canonical
   spelling the line parser did not already catch, and it is the property the fuzz target asserts: if
   `restore` accepts x, then `persist(restore(x)) == x`.

Every failure is `corrupt_complement`, except the versions in rule 2, which can be
`unsupported_version`, and a table or history that would exceed the cap, which is `limit_exceeded`.
Nothing panics, and every length and sum is a checked operation.

### 8.5 Export and resume

Fingerprints cover text only, so two empty paragraphs with different styles swapped in the head
package, or two ref-map lines with equal fingerprints swapped, would pass the alignment. The
complement therefore also records the SHA-256 of the head package bytes and of the ref map, and
`resume` checks them first. They are consistency checks, not a defence against an agent that
rewrites all of it.

```rust
pub struct Exported { pub head: Vec<u8>, pub refs: Vec<u8>, pub base: BaseToken, pub head_sha256: [u8; 32], pub refs_sha256: [u8; 32] }
State::export(&self) -> Result<Exported, StetError>
State::resume(head: Vec<u8>, refs: &[u8], recorded: &Recorded, original: Option<&[u8]>, limits: &Limits) -> Result<State, StetError>
```

`Recorded` holds the base token and the two digests that `document` read from the complement.
`resume`:

1. Checks the two digests. A mismatch is `corrupt_complement`.
2. Opens the head bytes, scans every story, computes the fingerprints and builds a `Layout`. A head
   package that does not open, or whose scan fails, is `corrupt_complement` with the cause in the
   message, because the original opened fine. A limit violation stays `limit_exceeded`.
3. Calls `restore`, which applies section 8.4.
4. Checks that the restored namespace, version and lineage equal the recorded base token.

After `resume` the counters, the stamps and the lineage continue where they were, so the next new
block gets the number it would have had in the unbroken session, and a batch written before the
export is still valid after it, subject to the same checks. `document` owns the file layout of the
complement. This document owns what is recorded in it: the blob, the base token, the two digests, and
the original package bytes that an opened namespace is checked against.

## 9. `State`

```rust
pub(crate) struct State {
    package: Package,                   // bytes of every part, as of open or resume
    limits: Limits,
    index: Arc<PartIndex>,              // canonical names, ASCII-lower-cased lookup, PartId by package order
    slots: Vec<PartSlot>,               // one per part: the lazily parsed tree and its generation
    package_epoch: u64,                 // bumped by every add_part call, in this state's history
    main: PartId,                       // the main document part, whose number never changes
    refs: RefTable,                     // namespace, version, lineage, slots
    derived: RefCell<Derived>,          // section 12
}
struct PartSlot { name: Arc<str>, cell: Arc<OnceLock<Result<SpanTree, StetError>>>, generation: Generation }
```

A `PartId` is the part's index in the package order, with added parts after. It is never reused, so it
is safe in cache keys. The package is "as of open or resume": a tree that was edited and committed is
not written into it until `to_package`.

### 9.1 `open`

`State::open(bytes: Vec<u8>, limits: &Limits) -> Result<State, StetError>`:

1. Hash the bytes into the namespace (3 ms per MiB, section 15).
2. `Package::open`. The main part is `Package::main_part`. A package without one is `malformed_input`.
   So is one with no `[Content_Types].xml` or no `_rels/.rels`, which the container reads because it
   keeps any ZIP byte for byte, and so is one whose `[Content_Types].xml` is not XML: the part is read
   once here, because the first `add_part` would otherwise fail on it long after the document opened.
   The corpus index and the testkit fixtures agree (`state/tests_fixtures.rs` runs every fixture
   through `State::open`).
3. Number the parts in package order. One `PartSlot` each, none parsed.
4. Parse the main part and its relationships part, build `Rels`, and call `wordml::stories`.
5. Parse every story part. A part that does not parse fails the open with `malformed_input` and a message
   that starts with the part's name.
6. Scan every story and call `RefTable::open`. The `Story` values go into the derived cache.

**Open parses the stories and nothing else.** Refs are assigned at open for every story, so every
story must be scanned, and a scan needs a tree. Styles, numbering, settings, themes, comment
extensions, people, fonts and every other part stay as compressed bytes until something asks. The
story parts other than the body are small. The Harvey LAB median is two of them at 4 KB, the largest is
105 KB, and the 872-page handbook has 56 header and footer parts that parse in 1.4 ms together.

`State::open_in(namespace, origin, bytes, limits)` takes the namespace explicitly, for `compare`.

### 9.2 Parts are parsed lazily, through `&self`

`PartsView::tree(part) -> Result<&SpanTree, StetError>` finds the slot and fills its `OnceLock` on the
first call. The error of a failed parse is stored and returned each time, so a corrupt part costs one
parse. The cell lives in an `Arc`, so the next `State` after a commit shares a tree that was parsed in
the one before. `view` takes `&Document`, so this is what lets a read parse what it needs without
`&mut`.

A part name that is not in the package is a caller bug, not an input error, and is `internal`. A
caller that asks about a part that may legitimately be missing, such as `styles.xml`, uses
`contains` or `tree_opt`. A relationship that points at a missing part never reaches `tree`, because
`Rels` marks it `Invalid`. The slot's canonical name is what goes into every `BlockKey`. Hot callers
hold a `PartId` from `PartsView::id(name)`, since a name lookup lower-cases the name.

### 9.3 Relationships as plain data

`state/parts.rs` turns `Package::relationships(source)` into `wordml::Rels`: the id, the type URI, and
a `RelTarget`. An internal target is resolved with `opc::resolve_target`, so `media/image%201.png`
becomes `media/image 1.png`, and then matched against the part index. A match gives the part's name
spelled as the entry spells it, and a target that names no part is `Invalid`. An external target
stays a string. So a dangling relationship, or one whose target differs from the entry only in case,
never fails an open. `Rels` for a source part are a derived cache over that part's `.rels` part and
over the package epoch, since a source with no relationships part yet has no part to depend on.

### 9.4 `save` and `export`

`State::to_package(&self) -> Result<Package, StetError>` clones the `Package` (cheap, since the input
and the inflated parts are shared), serializes each edited tree and calls `replace_part`. If no part
was edited, the package is the one passed to `open` or `resume`, and `write` returns those bytes,
which is law L1. An edited tree is serialized on every call and the bytes are handed to
`replace_part`, one part at a time, and no copy is kept: serializing the largest body takes about a
millisecond, and a cached copy would cost the size of the part in memory for the life of the state.

A tree is never rebased: re-parsing a serialized tree would renumber its nodes, and `NodeId` must stay
stable for the life of a `Document`. The cost is that a tree's `added` buffer grows with the edits made
to it, which is bounded by the edits themselves.

Commit also serializes each edited part once, to check the result against `Limits::max_entry_bytes`
(the bound `Package::replace_part` enforces) so that `save` cannot fail after a successful edit, and
to hash it for the lineage. The bytes are dropped afterward. Serializing an edited large part takes
about 1 ms (section 15), so the check costs nothing measurable.

## 10. `Tx`

```rust
pub(crate) struct Tx<'s> {
    base: &'s State,
    staged: BTreeMap<PartId, Staged>,       // a part's cloned tree and the edit count it had when cloned
    index: Arc<PartIndex>,                  // the base's, with the parts add_part made
    added: Option<Added>,                   // the package after add_part, the number of calls, and the raw set: parts whose bytes the calls changed, each with the number of the call that last wrote it
    intents: Vec<Intent>,                   // the declarations of section 7, in order
    touched: Vec<BlockKey>,                 // in call order, without duplicates (and a set for the test)
    overlay: Overlay,                       // what the declarations mean for Tx::resolve
    derived: RefCell<Derived>,              // entries built during the transaction
    mint: IdMint,                           // section 13
}
```

### 10.1 Copy-on-write, per part

`Tx::part_mut(&mut self, part: &str) -> Result<&mut SpanTree, StetError>`:

- The first call for a part parses it if needed, clones the tree out of the shared cell and puts the clone
  in `staged`. Later calls return the same clone. A part nobody asks for mutably is never copied, and
  an untouched part of the next `State` is the same `Arc` as before.
- A part's generation is the pair (epoch, edits). Asking for a mutable borrow does not change it.
  Changing the tree does, because `edits` is `SpanTree::edit_count`, which moves with every change the
  editor makes (it counts marks, not calls, so it is compared for equality and never added up). So a
  read through a mutable borrow keeps every cache valid (section 12). The count says that a tree has
  changed, and never that two trees are equal: two clones of one base that each made one different edit
  have the same count and different content. That is why the generation is the pair (epoch, edits), and
  why nothing compares a tree by its count alone. Creating a node moves the count though no document
  changed, so a transaction that creates a node and never puts it in the tree commits a new version for
  no change in the document. That is harmless, and `ops` should not create what it does not use.
- A part that `save` could not replace cannot be staged: an encrypted part, or one stored with a
  method other than stored or deflate, is `unsupported` when it is read, and a tree needs a read. So the
  refusal comes before the edit, and a `Document` can always be saved, with no new `Package` function.
  The one other limit on replacing a part, its size, is checked at commit (section 10.4).
  `part_mut` also refuses `[Content_Types].xml` and the relationship parts, which change only through
  `add_part`.

**What is cloned.** `SpanTree::clone` copies a pointer for each chunk of the node and attribute arenas,
the `added` buffer and the namespace table, and shares the source bytes and the chunks (`package-and-xml.md`
section 2.2). An edit copies a chunk when it changes something in it. Cloning a tree and changing 50
paragraphs takes 2.8 ms on the densest input measured (928,000 nodes) and 0.4 ms on a 300-page contract,
where copying whole arenas took 5.7 and 0.7 ms. A paragraph edit in a 300-page document therefore pays
one such clone, and the other parts pay nothing. Node ids survive: a clone has the same ids for the same
nodes, so `NodeId`, and with it every `BlockKey`, means the same node in the staged tree and the base tree.

**Memory.** Copy-on-write used to hold two copies of the arenas of each staged part until the transaction
ended, and that was the memory risk of this design. The nodes and attributes are now stored in chunks that a
clone shares, so a staged part costs the chunk pointers and the chunks that its edits write, and the
second copy of the arenas is gone (`package-and-xml.md` section 2.2 and "Memory of the span tree"). The
budget is 4 MiB + the file + 8 times the XML. The `state_*` benchmarks hold the real `State` to
it (`cargo xtask bench --filter state`, `bench/state.rs`) by counting the bytes that the allocator is asked
for at the highest point of a run, which is the same on every run. The edit is the fifty writes with a read
of the story after each, and the commit. Peak MiB on the five benchmark inputs, from the baseline of this
platform (`crates/stetdoc-testkit/bench/baseline.tsv`):

| Input (main part, MiB) | Budget | Open | Edit | Export | Resume | Highest, of budget |
|---|---:|---:|---:|---:|---:|---:|
| `contract-300p` (3.3) | 30.8 | 12.6 | 18.2 | 17.9 | 13.3 | 59% |
| `dense-contract-300p` (7.1) | 61.6 | 38.0 | 47.2 | 49.1 | 39.3 | 80% |
| `worst-blocks-12mb` (10.8) | 90.2 | 26.0 | 40.9 | 41.5 | 27.3 | 46% |
| `node-dense-18mb` (17.2) | 141.5 | 81.5 | 102.3 | 106.4 | 83.0 | 75% |
| `fields-13mb` (12.4) | 103.3 | 34.8 | 52.2 | 52.6 | 36.4 | 51% |

Every operation of `state` is inside the budget on every input, with the export the highest because it
holds the written part next to the tree. The edit holds, besides the tree and the part, the base `Story`
and the refreshed one (about 0.4 MB each for 18,000 blocks), the cloned ref table (0.8 MB), and the
commit's serialized copy of each edited part, which it hashes and drops (`State` keeps no serialized
bytes). Before the chunks, cloning a tree, changing 50 paragraphs and writing the part took 206.9 MiB on
`node-dense-18mb` and 92.5 on `dense-contract-300p`, which was over the budget, and took 97.4 and 44.9
after.

**The probes.** `state/tests_probe.rs` builds two synthetic bodies of 15,935 paragraphs and 2,304 table
rows (18,273 blocks), one with a long run in each paragraph and one with eleven short runs, which is as dense
as `node-dense-18mb` and far denser than any document in the corpora (the largest Harvey LAB body is 4.5
MB). It has four ignored tests to run one at a time under `/usr/bin/time -l` (the module comment says
how). They read the resident size, which is a cross-check and not the measure of the budget: it includes the
test's own scaffolding (the body text and the buffers that built the package), and macOS counts the pages
that `realloc` moved a vector to only when they are written again. In a release build on the Apple M1 Pro,
in decimal megabytes: the light body (10.7 MB of XML, budget 90 MB) peaks at 54 MB open only and 66 MB for
the whole edit, and the dense one (18.1 MB, budget 150 MB) at 147 and 163 MB. Before the chunks the dense
body's whole edit was 216 MB and its open-only peak 114 MB. The open-only figures rose with the chunks
because chunks are never moved, so the resident size reads what the tree holds, where a vector that had
moved read low (`package-and-xml.md`, "The rise in the peak of `open` that `state` saw", has the stages).

### 10.2 `add_part`

`Tx::add_part(NewPart) -> Result<String, StetError>` clones the package once and delegates to
`Package::add_part`, which is atomic and returns the new relationship id. The clone is kept for
commit. A part added this way has a new `PartId` after the existing ones. It is parsed lazily like any
other. It is needed by `comments` and by `compare`'s import of media.

`add_part` also rewrites `[Content_Types].xml` and the relationships part of the source, creating the
latter if there was none. So it puts both in `raw` (the raw set), bumps the package epoch, and at commit
gives each of them a fresh, unparsed cell and a new epoch. Without that, the `Rels` cache and the parsed relationships
tree would stay stale after comments are added, and the audit would never see that the story list
changed. A part in `raw` is read, inside the transaction, from the transaction's package.

**Every call counts, not the first one.** A transaction may call `add_part` several times, and `comments`
does: `comments.xml`, `commentsExtended.xml`, `commentsIds.xml` and `people.xml` in one edit. The
transaction counts the calls. The package epoch is the base's plus the number of calls. A part that a
call wrote (added, or rewritten as the content types and the source's relationships are) has the epoch
that the base had for it, plus the number of the call that wrote it last, and a part that no base has
counts as epoch 0 there, so its epoch is the number of its call (1 for a part added by the first call,
2 for one added by the second). A part that a second call rewrites takes a number that it did not have
after the first. Whatever the transaction cached between the two calls (the story list, a `Rels`, a
story of a part that was just added) is then stale after the second, and the commit gives the next
`State` the same numbers, so what the transaction built after its last call is valid in the new state
and nothing built before it is. A call that the package refuses changes none of it and does not count. Counting only the first call, as the
first code did, left the `Rels` of the source with one relationship where two existed, minted no ref for
the comment paragraph, and wrote a `refs` blob that `resume` refused with `corrupt_complement`.

### 10.2.1 `add_relationship`, `content_type` and `opened_bytes`

`Tx::add_relationship(source, rel_type, target) -> Result<String, StetError>` adds a relationship with
`TargetMode="External"` from `source` (empty for the package) and names no part: the hyperlinks of
content that `compare` imports. It calls `Package::add_external_relationship`, which is atomic like
`add_part`, rewrites the source's relationships part (making it, with its content type, if there is
none) and returns the new id. The transaction handles the result as `add_part` does: the rewritten
parts get a fresh cell and a new epoch, the call counts, and the package epoch moves. A `source` that
is not a part is `invalid_payload` and nothing changes. The target is written as given and is not
checked as a URL.

`PartsView::content_type(part) -> Result<Option<String>, StetError>` is the content type that the
package this view reads registers for the part, by override or by the default for its extension, or
`None`. It is what check's rule P1 (every part has a content type) asks. It reads
`[Content_Types].xml` on each call.

`State::opened_bytes() -> usize` is the size in bytes of the package as it was opened, which no commit
changes, for the limits and receipts of `document`. `State::opened_source() -> Bytes` is the shared handle to those
bytes (`Package::opened`): the input of `open`, or the head of a resumed complement. `document` keeps it as the
original of a document it opened when the first edit installs a state, so that edit copies nothing (`document-and-ops.md` 7.4).

### 10.3 Declarations and touches

`Tx::retire`, `rebind`, `merge` and `touch` push onto `intents` and `touched` and take effect for
`Tx::resolve` at once (section 5). Nothing touches the ref table until commit applies them to a clone
of it (section 7). The scope guard reads `Tx::touched()`. The conformance tests read both lists:
they assert which declarations an operation made, and that every changed block was touched.

### 10.4 Commit

`Tx::commit(self) -> Result<(State, Committed), StetError>`:

1. If no staged tree has changed, no part was added, and nothing was declared or touched, return a
   clone of the base and `Committed::unchanged()`. The version does not move, so readable files and
   base tokens stay current. A staged tree has changed when its `edit_count` is not the count it had
   when the transaction cloned it. It is not `is_dirty`, which no edit ever clears: a part that was
   edited in an earlier commit, and only borrowed in this one, is not edited now, or the version and the
   lineage would move for a transaction that changed nothing, and the readable files would go stale
   and cost a hash for no change. A part that was borrowed mutably and not changed is dropped from the
   result and keeps the base's cell.
2. Choose the affected parts (section 7) and, in story order, scan each one's staged tree: a part that
   was a story is refreshed against the base's `Story`, which gives the blocks that moved, the blocks
   whose wrappers changed and the paragraphs whose field start changed. Compute the new story list if
   the main part or its relationships changed.
3. Run the audit of section 7 on a clone of the table. This is where every fallible step happens: a
   scan, the table cap, a contradictory declaration.
4. Serialize each edited part once, check its size, and hash it. The parts that `add_part` added or
   rewrote (the raw set: `[Content_Types].xml` and the relationships parts) are hashed as they are, each
   one explicitly, and a part that was added and then edited is hashed once, after its edits. Then drop
   the bytes. Keeping them would add a copy of the part to the peak memory, and a second
   serialization at `save` costs about 1 ms.
5. Form the commit's digest: the SHA-256 of the names and hashes of every part the commit changed (the
   edited trees, the added parts and the raw set), in `PartId` order, and of the canonical text of the
   audit's events. Call `advance` on the table, which appends the next
   lineage and raises the version. If the history is at the cap, this is `limit_exceeded`.
6. Build the next `State`: the edited trees go into fresh cells with their epochs, the unchanged
   slots are shared `Arc`s, and the table is the clone with everything applied. The derived caches
   carry over (section 12).

Steps 1 to 5 can fail, and they build values that are discarded on failure. Step 6 only moves values
and cannot. So **commit is atomic by construction**: on any error nothing was published, and the base
`State` was never written to.

```rust
pub struct Committed {
    pub base: BaseToken,                              // the new state's token
    pub minted: Vec<(BlockKey, BlockRef)>,            // in the order they were minted
    pub retired: Vec<(BlockRef, Reason)>,
    pub rebound: Vec<(BlockRef, BlockKey)>,
    pub merged: Vec<(BlockRef, BlockRef)>,            // survivor, absorbed
}
```

`Document` replaces its `State` with the first value. The receipt reads the second.

### 10.5 Discard

`Tx::discard(self)` drops the transaction: the staged trees, the declarations, the added package, the
derived entries it built and its id minter. It writes nothing to the document: no part, ref, token or
version of the base changed. In particular, a cache entry built through the transaction's view lives in
the transaction's store and never in the base's, so a cache builder with too narrow a dependency list
cannot leak the data of a dry run into the document. A dry run is `commit` skipped, so it costs the same
as a real edit up to that point and leaves the document exactly as it was. The conformance test compares
`export` bytes, the base token and the ref counters before and after.

One thing does reach the base from a commit that fails or is a dry run, and it is a cache. The audit
refreshes each story it scans from the base's story, so it reads the base's story list, its stories
and the field pass of each through the base's own caches (a view over the base, not over the
transaction), and a commit that does not finish leaves what it built there. Each holds what the first
read of it would build from the committed parts, so filling it changes nothing a caller can see, and a
later read finds it valid. Most of these reads are hits: `open` and `resume` build the story list and
every story for the table, and a commit seeds the next state with the stories it scanned. What is left
to fill is the field pass of a base story, once per story and session (about 11 ms on the 12 MB body).
The alternative was a store local to the commit, which would repeat that pass, and the scan of any
story the base lacks, for every dry run that comes before the real edit, so an agent that previews
every batch would pay it twice.

## 11. How readable files carry their base

Readable files record the base token of the state they were rendered from.
`State::check_files_base(&self, recorded: &BaseToken) -> Result<(), StetError>` returns `stale_files`
unless namespace, version and lineage all equal the current state's. The message says which differs: a
different namespace, a different history, an older version or a newer one. A newer version can only
come from files copied between working copies, and the right answer is the same. It is the only
comparison, so `projection::files` cannot forget a case. After it passes, the operations that
`Edit::files` derives are at the current version, and `projection` gives them `Claim::Base` with the
current token, so no `changed` check can refuse them.

- A fresh `open` is version 0. Each commit that changed something adds 1. `resume` restores the
  version from the complement, so it keeps counting.
- A commit that stages nothing leaves the version alone. A batch that makes no change therefore does
  not invalidate the readable files that were just exported.

## 12. Derived caches

Later modules cache things computed from parts: the block index (`Story`), the style cascade, numbering,
the defined-term index, the `Rels` of a part, the sets of durable ids. They need a mechanism that is
correct without anyone remembering to call it. A concrete type, `PartsView`, carries it. It is not a
trait: `ARCHITECTURE.md` allows exactly one seam in the core, and a view with a base and an optional
overlay has no second adapter.

**Per-part generation.** A part's generation is the pair (epoch, edits). The epoch counts replacements
of the part's tree object: a part parsed again from new bytes gets a new epoch. `edits` is the tree's
`SpanTree::edit_count`. A pseudo-part, `Dep::Package`, stands for the set of parts and their
relationships, and its generation moves with every `add_part` (section 10.2).

**A cache entry records the generations of the parts it was computed from**, and is valid exactly when
every one is unchanged. There is no invalidation pass and no list of subscribers: a stale entry is
recognized by comparing numbers on read.

```rust
pub enum Dep { Part(PartId), Package }
pub(crate) struct PartsView<'a> { base: &'a State, tx: Option<&'a Tx<'a>> }   // State::parts() and Tx::parts()
impl PartsView<'_> {
    pub fn id(&self, name: &str) -> Option<PartId>;
    pub fn contains(&self, name: &str) -> bool;
    pub fn tree(&self, part: PartId) -> Result<&SpanTree, StetError>;
    pub fn generation(&self, dep: Dep) -> Generation;
    pub fn derived<T: Send + Sync + 'static>(
        &self, variant: u64, deps: &[Dep], build: &dyn Fn(Option<&T>) -> Result<T, StetError>,
    ) -> Result<Arc<T>, StetError>;
    pub fn story_specs(&self) -> Result<Arc<StoryList>, StetError>;       // the stories in story order
    pub fn story(&self, part: PartId) -> Result<Arc<Story>, StetError>;
    pub fn rels(&self, source: PartId) -> Result<Arc<Rels>, StetError>;
    pub fn ids(&self, kind: IdKind) -> Result<Arc<IdSet>, StetError>;      // the ids a document uses
    pub fn layout(&self, fingerprints: bool) -> Result<Layout, StetError>; // for RefTable::{open, persist, restore}
    pub fn is_attached(&self, key: &BlockKey) -> Result<bool, StetError>;  // is the element in its part's document
    pub fn ref_of(&self, key: &BlockKey) -> Option<BlockRef>;              // the live ref of a block, as the view's declarations leave it
    pub fn related_part(&self, source: PartId, is_type: &dyn Fn(&str) -> bool) -> Result<Option<PartId>, StetError>;
}
```

- **`ref_of`** gives the live ref of a block for `projection`, which shows refs in views and receipts.
  A view of a committed state answers from its table. A view of a transaction answers as the
  transaction's declarations leave it, so a block that the transaction created has none (refs are
  minted at commit, and a dry run's receipt lists a new block without one, next to the ref of the block
  it follows), and a ref that a `rebind` or `merge` moved is on the element it names now.
- **The key is the type, the variant and the dependencies.** An entry is found by `TypeId` of `T`, a
  caller-chosen `variant`, and the sorted list of dependency part ids. Two `Story` values for two
  parts have different dependencies, so they have different keys. A position in the story list is
  never a key, because positions shift when a header is added or removed. The `variant` is for
  parameters that are not parts: a story's kind is one, because the kind decides how a part scans.
- **`PartsView` over a `Tx` answers from the transaction's store first, then from the base's.** A base
  entry is used only if every dependency has the same generation in the transaction as it had when
  the entry was built, which is true when none of its parts was edited. A miss builds the value
  and stores it **in the transaction's store**, never in the base's.
- **Commit** promotes the transaction's entries whose generations match the new state's into the new
  `State`, and keeps the base's entries that are still valid. **Discard** drops the transaction's
  store with the transaction. So caches are rebuilt per part: an edit to a paragraph in a header
  leaves the body's `Story` and the style cascade alone.
- **A builder receives the stale previous value**, if there is one for the same key. It comes only
  from this transaction's own store or from the base as it was when the transaction began, never from
  an older state or another transaction. It may refresh from it instead of rebuilding. The refresh must give exactly what a build from scratch
  gives. This is the hook for `Story::refresh`, and it is how the re-scan cost of section 15.3 is cut
  if `wordml` takes it.
- A builder may call `derived` itself (the cascade builds on the definitions). The store is not
  borrowed during a build.
- **The contract is purity.** A builder must be a function of the parts in `deps` and nothing else. The
  test suite checks it from outside: after every edit of a random sequence, and after every discard and
  every dry run, every cached value must equal a rebuild from scratch (section 16).

The store is a `RefCell`, so a `Document` is not `Sync` (section 14).

## 13. Identifiers and determinism

New identifiers come from inputs. The rules, by kind:

| Identifier | Rule | Owner |
|---|---|---|
| Revision and comment `w:id` | the highest existing id plus one, from one counter per transaction | `state/ids.rs`, `Tx::next_annotation_id` |
| `w14:paraId`, `w14:textId`, `w16cid:durableId` | the hash below, bumped on collision | `state/ids.rs` |
| Ref numbers | the next number of the kind, in document order, at commit | `state/commit.rs` |
| Relationship id | `Relationships::next_id` | `package` |
| New ZIP entry timestamps | copied from the main part's entry | `package` |

**The hash.** `Tx::mint_id(kind: IdKind, part: &str) -> Result<u32, StetError>` computes
`SHA-256(domain tag, namespace, new version, kind, part name, ordinal)`, with each field
length-prefixed. Here the new version is the base version plus one, and the ordinal counts the ids of
this kind already minted in this part by this transaction. The first four bytes, little endian,
masked with `0x7FFFFFFF`, are the candidate. `w14:paraId` must be below `0x80000000`, and zero is
treated as taken.

"Position" in the architecture's wording is the position of the request: the part and the ordinal. It
is not a byte offset, because new blocks may not have a byte offset yet. Operations run in a
deterministic order, so the ordinal is deterministic. A dry run and the real edit at the same
version mint the same ids. Ids minted in different commits, or in different documents, differ.

**Collisions.** If the candidate is already used, add one (wrapping within `1..=0x7FFFFFFF`) until it is
free. The used set is what `wordml::existing_ids` reports over `wordml::id_parts`: for `paraId` every story
part and `commentsExtended.xml` and `commentsIds.xml`, for `textId` every story part (the comment
extension parts have no `w14:textId`), for `durableId`
`commentsIds.xml` and `commentsExtensible.xml`. It reads both branches of an `mc:AlternateContent`,
because Word repeats a text box's ids in the fallback. The set is a derived cache over those parts,
computed from the base state once per transaction, and the transaction's own mints are kept on the
side. Ids freed by a deletion are not reused. Running out of the 2^31 ids is `limit_exceeded`.

**Annotation ids.** `Tx::next_annotation_id(&mut self) -> Result<u32, StetError>` makes the `w:id` of a
revision, a comment, a bookmark or a range marker. `revisions` and `comments` share it, because two
counters would hand out the same number. The first call of a transaction gives the highest `w:id` that
the transaction's view of the document uses, plus one, or 0 when it uses none. Each later call gives
one more. The highest id is the largest in `PartsView::ids(IdKind::AnnotationId)`, which reads the `w:id`
of every element of every story part, only values that are a non-negative decimal that fits a `u32`. The
transaction's own view is the one asked, so the parts that `add_part` brought in and the edits staged
before the first call count, and it costs nothing when no story part is staged, because the base's cached
set is still valid. After the first call the counter is a number and nothing is read again. A part that
is added after the first call takes its annotation ids from the counter, or has none. That keeps the
range below exact: a counter that skipped past ids that such a part carried would leave them inside it.

`Tx::minted_annotation_ids(&self) -> Range<u32>` is the range from the first id the transaction made to
one past the last, and an empty range when it made none. Every id in it was made by this transaction, and
no other id is in it, so an element whose `w:id` is in the range got that id from this transaction,
whichever operation, or `Compiler`, made it, and `revisions` uses that to find what a batch created
without keeping a list of its own. Some of those ids are on copies of revisions that already existed
(below). The counter is a field of the transaction, so a dry run and the real edit at the same base
make the same ids, and a dropped transaction leaves nothing behind. The next transaction reads the
committed trees again. Past 2,147,483,647, the largest `w:id` Word reads, the next call is
`limit_exceeded`, nothing has changed, and the range does not move. A document that already uses an id
above that has no id left for a new annotation. `mint_id(AnnotationId, ..)` stays `internal` and says so.

**Copies.** Not every id in the range is on a new revision. A split of a `w:ins` or `w:del` gives its tail
a new id, and a carve gives the copy of a pending `w:rPrChange` one, and those revisions already
existed, so the compiler must never extend or edit them in place. One compiler makes the copy and
another, over the same transaction (an operation after a `resolve` in the same batch), has to know, so
the note lives on the transaction. `Tx::note_copied_annotation_id(id) -> Result<(), StetError>` records an
id that the transaction made, in call order, and `Tx::copied_annotation_ids() -> &[u32]` lists them. An
id that the transaction did not make is `internal` and is not noted. The list is a note for `revisions`:
it has no effect on the commit, the audit or the digest, and it goes with the transaction. `state` does
not know what a copy is, only that these ids are in its range and are noted.
**Only `revisions` decides whether a new paragraph gets a `paraId` at all.** The rule: it
gets one if the paragraph it follows has one. Real documents differ sharply. One of the 42,003 Harvey
LAB documents has any `w14:paraId`, because python-docx writes none. The 872-page handbook has one on
every paragraph (8,152 of 8,152). A clone of a paragraph carries its source's `paraId`, so the
compiler must mint a fresh one for each cloned paragraph. The `check` rule "paraId is unique" is the
backstop and fails an edit that duplicates one. It must ignore `mc:Fallback` copies: Word repeats a text
box's `paraId`s there, and five of the curated documents do.

**The determinism rules** for both modules, all tested:

- No output is derived from iteration order of a `HashMap` or `HashSet`. Anything ordered is a `Vec`
  built in document or story order, a `BTreeMap`, or sorted. The staged parts are a `BTreeMap`.
- No clock, no random number generator, no address, no `DefaultHasher` or `RandomState` value.
- A hash that is persisted or minted is SHA-256 or the frozen FNV-1a of `wordml`.
- The same bytes, options and edits give the same `save`, `export`, namespace, lineage, refs and ids.

## 14. Concurrency

`Document` is `Send` and not `Sync`. `State` is `Send` because everything in it is: `Arc`s of `Send + Sync`
data, `OnceLock`s, a `Package`, and a `RefCell` of `Send` values. It is not `Sync` because of the
`RefCell` that holds the derived caches. That is deliberate, not incidental: later modules cache
through `&self` with interior mutability, and promising `Sync` would forbid it. Sharing the part
cells across `State` versions with `Arc` needs them to be `Sync`, which is why they are `OnceLock`
and not `OnceCell`.

A test asserts `State: Send` and `State: !Sync` (with the usual ambiguity trick, as the core cannot
use `static_assertions`). The napi binding calls a document from one thread at a time, and a host
that cares about its event loop runs the call in a worker.

## 15. Performance

### 15.1 Measurements

Release build, Apple M1 Pro, other work running, with a throwaway harness that is not in the
repository. All times are milliseconds. "Scan" is a throwaway version of the block scan that walks
the tree without descending into paragraphs, and "fingerprints" hashes the decoded text of every
paragraph with FNV-1a, as `wordml` will. The first table is one run per document.

| Document | docx | Main part | Paragraphs, tables, rows | Story parts (bytes) | SHA-256 of the file | Inflate | Parse | Scan | Fingerprints | Tree clone | Mint refs | Parse stories |
|---|---:|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `bug65649.docx` (POI) | 475 KB | 12.58 MB | 15,935, 21, 2,304 | 2 (15 KB) | 1.4 | 11.9 | 38.9 | 3.9 | 5.0 | 3.8 | 0.8 | 0.1 |
| `handbook-872p.docx` | 483 KB | 5.26 MB | 8,152, 2, 4 | 56 (199 KB) | 1.4 | 6.9 | 14.6 | 0.6 | 2.8 | 1.0 | 0.4 | 1.4 |
| `WC-BodyBookmarks-Before.docx` | 1.93 MB | 2.85 MB | 3,112, 0, 0 | 5 (425 KB) | 5.6 | 5.9 | 7.0 | 0.2 | 2.9 | 0.4 | 0.1 | 1.6 |
| LAB `alvarez-opt-in-consent-record-index` | 124 KB | 4.54 MB | 14,563, 41, 1,274 | 1 (3 KB) | 0.4 | 4.1 | 14.7 | 1.8 | 2.3 | 1.2 | 1.1 | 0.03 |

A second run measured what the design adds to a commit. A transaction clones the tree, edits one text
node, and a block scan follows. Then it serializes and hashes the result for the lineage:

| Document | Clone | One edit, then a block scan | 50 of those | Serialize | SHA-256 of the output |
|---|---:|---:|---:|---:|---:|
| `bug65649.docx` | 5.0 | 1.9 | 92.8 | 0.5 (12.58 MB) | 36.0 |
| `handbook-872p.docx` | 1.2 | 0.2 | 10.3 | 0.9 (5.26 MB) | 14.7 |

The ref table for the 18,260 blocks of `bug65649` (12-byte slots, 16-byte map entries): clone and drop
0.02 ms warm and 0.13 ms the first time, a resolve that parses the token and builds the key 10 ns,
and the audit's probe of every live block against the set of nodes in a scan 0.32 ms.

Over the 42,003 Harvey LAB documents that parse (six have the known bare `&` and are refused):

| | median | 90th | 99th | maximum |
|---|---:|---:|---:|---:|
| Blocks (paragraphs, tables, rows) | 487 | 1,764 | 3,820 | 15,878 |
| Story parts besides the body | 2 | 2 | 2 | 6 |
| Bytes of those parts | 4.1 KB | 4.2 KB | 4.7 KB | 105 KB |
| Main part | 231 KB | 644 KB | 1.22 MB | 4.54 MB |
| Parse of the main part (ms) | 0.55 | 1.79 | 3.97 | 100.7 (an outlier: the largest main part parses in 15 ms) |
| Block scan (ms) | 0.03 | 0.13 | 0.35 | 42.8 (an outlier of the same kind) |

Rows are 12.4% of all blocks (4.0 million of 32.4 million), paragraphs 86.2% and tables 1.4%. The
curated set (289 of its 306 documents measured, the rest being refusals) tops out at 5,608 paragraphs
in a 1.31 MB main part. SHA-256 runs at about 330 MB/s (64 MiB in 198 ms) in the crate's portable
path on this machine, and FNV-1a at 700 MB/s.

### 15.2 Against the budgets

| Budget | Plan | Estimate for the largest document (`bug65649`) |
|---|---|---|
| `open` ≤ 300 ms | hash the bytes, open the container, inflate and parse the body and the other stories, scan them all, mint the refs. Nothing else is parsed | 1.4 + 0.3 + 11.9 + 38.9 + 0.1 + 3.9 + 0.8 = **57 ms**, and about **67 ms** if the field pass of `wordml` (about 10 ms there) runs eagerly. The handbook takes 25 ms. This is four to twelve times under budget, with room for a slower runner |
| `edit`, 50 operations ≤ 1 s | copy-on-write of the parts touched, the operations, validation, then commit | The part of this budget that belongs to `state`: the clone 5.0 ms, 50 write-then-read cycles 93 ms (section 15.3), and a commit of scan 1.9 ms, audit 0.3 ms, serialize 0.5 ms, lineage hash 36 ms and table clone 0.02 ms. About **135 ms**, and about 145 ms if the field pass runs at commit. The 300-page handbook: about **30 ms**. The rest of the 1 s is for `ops` and `check`, whose cost is `check`'s per-unit validation of the dirty units, much cheaper than whole-part validation |
| `save` ≤ 1 s | serialize the edited parts (about 1 ms) and recompress (13 to 41 ms for the largest documents, `package-and-xml.md` section 8) | **about 45 ms** |
| `export` ≤ 1 s | the same as `save`, plus fingerprints (5 ms), the blob (about 3 ms) and the two SHA-256 digests of the head bytes and the blob (about 3 ms) | **about 55 ms** |
| `resume` | the same as `open`, plus fingerprints, `restore` with its canonical re-serialization (about 5 ms) and the digests | about **70 ms** |
| `view` ≤ 200 ms | the story index and ref lookups are cached and O(1) | not `state`'s, but `ref_of` is one hash probe per block |
| Peak memory | table 0.8 MB, copy-on-write that shares the chunks of the arenas (section 10.1), the `Story` indexes | at most **80%** of the budget on the five benchmark inputs, by the requested bytes that `state_*` counts (the export of `dense-contract-300p` is the highest). Section 10.1 has them |

**Measured through the code.** The estimates above were made with a throwaway harness before the code
existed. The `state_*` benchmarks time the real `State` (`cargo xtask bench --filter state`,
`bench/state.rs`): `state_open`, `state_writes` (fifty write-then-read cycles in one transaction, dropped),
`state_edit` (those and the commit), `state_save`, `state_export` and `state_resume`. The writes are plain
text edits in fifty blocks spread through the main part, each followed by the read of the story that the
write made stale, which is the shape of a batch. The table is the baseline of this platform (release
build, Apple M-series, the fastest of several runs), in milliseconds:

| Input (blocks) | Open | 50 writes and reads | Commit | Save | Export | Resume |
|---|---:|---:|---:|---:|---:|---:|
| `contract-300p` | 15 | 31 | 13 | 29 | 34 | 20 |
| `dense-contract-300p` | 42 | 49 | 32 | 55 | 66 | 51 |
| `worst-blocks-12mb` (18,273) | 25 | 102 | 42 | 20 | 37 | 43 |
| `fields-13mb` | 33 | 100 | 60 | 33 | 53 | 54 |
| `node-dense-18mb` (18,273) | 66 | 111 | 71 | 30 | 50 | 87 |

The commit is `state_edit` less `state_writes`, since a commit cannot be a benchmark of its own: the
transaction it ends borrows the state it was begun on. Every budget holds by a wide margin. The
50-operation edit takes 44 ms on the 300-page contract and 182 ms on the densest input, against the 1 s,
and leaves more than 800 ms for `ops` and `check`, and `open` is at most 66 ms against the 300. That is
under the 300 ms that section 15.3 sets for what `state` and `wordml` do in a batch on every input.

The numbers have a history that explains the design. A first version of the measurement had 226 ms for
the writes on the worst body, because each write-then-read cost 4.5 ms: the write makes the story list
stale, and building the list scanned the body a second time to find the section breaks that name the
headers and footers. `wordml::stories_of` takes the body's own `Story`, which the list's builder now gets
from the cache, so the list no longer scans the body, and a cycle costs about 2 ms where it cost 4.5. Another
run measured 405 ms for a dense body before that change, and a later run measured 274 ms for the
writes on it, 136 ms after the change and 158 ms with the chunked arenas of `xml`, which read a little
slower because a lookup is a chunk number and a position and the scan reads every node. The cost that
remains is the scan, which grows with the nodes and not with the blocks. If `ops` and `check` need more
than what the table leaves, the structural-edit counter of section 17 is the next remedy.

**Hashing the whole package** costs 3 ms per MiB of file, and a file is compressed. The 12.6 MB body of
`bug65649` sits in a 475 KB file that hashes in 1.4 ms, so for a typical document the hash is far
cheaper than hashing the inflated XML would be (38 ms for that body). A realistic contract file of 1 to 5 MiB
pays 3 to 15 ms. It only becomes large for a package that carries many
times its XML in media: 50 MiB takes about 150 ms of the 300 ms budget, and the 512 MiB limit about
1.6 s (an estimate of 0.3 s where the crate uses SHA instructions). That tail is a bounded, linear pass
that allocates nothing and costs less than inflating the same file. `ARCHITECTURE.md` says so in its
Performance notes. The alternative, hashing only what a ref depends on (the story parts and the
relationships that choose them), would be independent of media and would cost more on every ordinary
document. "Derived from the opened bytes" is simplest as the whole package.

**The lineage hash** is the largest single cost of a commit on the 12.6 MB body: 36 ms. It is paid once
per commit, and a commit is one `edit`, not one operation. On the 300-page class it is 4 to 15 ms.

### 15.3 Re-scans inside a transaction

The first drafts of the two designs did not count this. A batch in which every operation writes and the next reads
makes every operation re-scan the part, because the write changed the tree and the cached `Story` is
stale. On `bug65649` a write followed by a block scan is 1.9 ms, so 50 operations cost 93 ms. That is
the block scan alone. If `wordml`'s field pass also ran in full for each read it would add about 10 ms
per operation, 0.5 s in all, which is why the seam text in section 2 makes the pass lazy and says
whether it is incremental. The remedies, in the order they help:

1. **A mutable borrow that changes nothing keeps the caches.** The generation moves only when
   `SpanTree::edit_count` does. Every operation borrows the part, and the reads in between change nothing.
2. **The previous `Story` goes to the builder**, together with the nodes of this part that
   `Tx::touched()` lists, and `wordml`'s `Story::refresh` carries the field index forward. A text edit
   inside a paragraph changes no block, so a refresh is cheap when no touched block has field
   characters. `refresh` finds new, removed and moved blocks itself, so `touched` covers only edits
   inside existing blocks.
3. **The field pass is lazy.** It runs when a paragraph's text is first read.

The `wordml` design measured the case on the Council AI Act draft repeated to 13.1 MB (15,220 blocks):
a scan takes 1.7 ms and a field pass 11.3 ms, and 50 interleaved operations (one text edit, a scan and a
field pass each) took 659 ms against 86 ms with the scan alone. That is the cost remedies 2 and 3 remove.

`state_writes` (section 15.2) measures a 50-operation interleaved batch, and `document_edit_ops`
measures it with `ops` and `revisions` (`document-and-ops.md` 7.4). The threshold is 300 ms for everything `state` and
`wordml` do in the batch.

## 16. How it is tested

Tests sit at the interface where possible. `refs` and `state` have invariants of their own, so they
also have direct tests, and rule 5 of `ARCHITECTURE.md` allows it, together with `wordml`'s story scan
and text model. `refs` tests need only `wordml::testing`, never `xml`.

**Resolution** (`refs`). One test per row of the table in section 5, including the order of checks (a
wrong namespace with a malformed token is still `stale_ref`, and `Implied` on the edit path is
`invalid_payload` before anything else). The grammar is tested with every non-canonical spelling
and with each boundary (`p1`, `p4294967295`, `p4294967296`). Message text is asserted for each
tombstone reason, and for a merge whose target is gone too.

**Forks and claims** (`refs`, `state`). Two states opened from the same bytes make different inserts and
get different lineages, so a claim from one is `stale_ref` in the other, at the same version and at
others, and a claim from version 9 in a document at version 7 is `stale_ref`. Two states that make
byte-identical edits get equal lineages and refs, and a claim passes in either. A claim at version 3
passes for a block last changed in version 2 and fails for one changed in version 4. A claim names a
ref minted after its version and is refused. All of it again after `export` and `resume`, and the
lineage of the next commit after `resume` equals the lineage in an unbroken session.

**Identity oracle** (`state`). A model checker runs random sequences of edits against a `State` and an
independent model of block identity: a map from a name to an element, updated by the rules in
section 6. It covers a split that keeps the original element for the first part and one that keeps it
for the second, and a merge in both directions, tracked and untracked: keep the first element, keep the
second, and build a new one. It also covers **merges of three blocks in both orders** (`merge(p5, p6)`
then `merge(p5, p7)`, and `merge(p6, p7)` then `merge(p5, p6)`), keeping the first, the middle and the
last element and building a new one, which is the case where an `into` is absorbed by a later
declaration, and a `rebind` whose target a later `merge` absorbs. Each of them requires the composed
check of section 7 to accept it, the final refs to name the elements the model says, and every absorbed
ref to say which ref it was merged into. After every commit the checker requires, for every ref ever issued, that
`resolve` returns either the block the model says it names or an error with the right reason, and
never another block. This is the test of "can never silently point at the wrong paragraph". The edits
are made through the `xml` editor directly (insert, detach, clone, move) plus the declarations, so it
does not depend on `revisions`. A second family feeds the commit a contradictory declaration and
requires `internal` and an unchanged state.

**Touch coverage.** For each operation of the conformance suite, the serialized bytes of every
existing block's **own parts** before and after are compared (a paragraph owns all its bytes, a table
and a row own what `Story::own_parts` says, section 2), and every block whose own parts changed must be
in `Tx::touched()`. New, removed and moved blocks are outside the comparison, because they have no
before, no after, or the same bytes, and the audit finds them from the trees. A second oracle, run after
every operation, calls `Story::refresh` against a fresh scan and requires the same story, the same field
index and the same reported lists. The shapes that a `touched` that trusted its own list would get wrong
are in it by name: a split inside a field result, where the new paragraph holds the field's end, and an
untracked delete of the paragraph that holds a field's end or separate. The same check backs the scope
guard.


**Table invariants** (`refs`). Property tests over random sequences of allocate, tombstone, rebind and
merge: numbers stay dense, the map stays one to one, a gone ref never comes back, counters and
versions never decrease, and a merge chain never loops.

**Atomicity** (`state`). A failpoint harness runs a scripted edit and injects an error at each fallible
step in turn: every `part_mut`, `add_part`, scan, audit step, size check, hash and cap check. After each
failure the base `State` must equal what it was, where equality is the bytes of `to_package`, the
`export` blob, the base token, every part's generation and the ref table. A second test discards a
`Tx` after arbitrary staged edits and requires the same. A third runs a dry run on the corpus and
compares `save` and `export` bytes before and after. A test also checks that the base tree of a part is
untouched after a staged edit to its clone, and that an unstaged part is the same `Arc` before and
after commit.

**`NodeId` stability.** A test clones a parsed tree, applies a mix of edits through the editor (set
text, insert, detach, clone a subtree) and requires that every node id that existed before names the
same node, with the same name and parent, afterwards, in the clone and the original. It guards the
assumption in section 2 in `state`'s own tests, whatever `xml`'s test says.

**Persistence** (`refs`). A round trip on the table of every corpus document, and after random edits.
Canonical form: `persist(restore(persist(t))) == persist(t)`. The hostile suite: truncation at every
length; a single-byte flip at every position of a small blob and of a corpus blob; each rule of
section 8.4 with a dedicated blob (a count of four billion, a missing `end`, a duplicate ref, a gap in
the numbers, a merge to a ref that does not exist, to itself, and in a loop, a wrong kind letter, a
forged namespace, a derived namespace whose inputs were edited, a lineage of the wrong length, a stamp
above the version, a bad percent escape, a trailing byte, a line of 100,000 bytes, a version from the
future, and a scan version this release does not read); and a mismatched layout (a story removed, a
block's text changed, two blocks swapped, a table where a paragraph was). Every one must return the
right code and none may allocate more than a small multiple of the blob. A fuzz target that
feeds `restore` with mutated blobs and asserts the canonical-form property on every accepted input is
not implemented.
At the `state` level, `resume` is tested with two empty paragraphs of different styles swapped in the
head package and with two ref-map lines swapped, and both must fail on the digests.

**Determinism.** The same bytes, options and edits are run twenty times in one process and in two
threads (each `HashMap` has its own seed, so any iteration-order leak shows up) and compared on `save`
bytes, `export` bytes, base token, refs and minted ids. Opening a file twice gives equal namespaces and
equal refs. Opening the bytes of an edited `save` gives a different namespace. `mint_id` is tested
for collision handling by pre-loading the used set with the first 10,000 candidates, and for the
`mc:Fallback` repeat.

**Cache coherence.** After each edit of a random sequence, and after every discard and every dry run,
every cached value (`Story`, `Rels`, the id sets and a test cache built over two parts) is compared
with a rebuild from scratch. A dependency declared too narrowly in a test cache must make this fail,
which proves the check can see it. A second test removes the first of two headers and requires the
second's `Story` to be its own, which a key by position would get wrong. A third adds a part with
`add_part` and requires the `Rels` of the source and the story list to change, and the content types
tree to be fresh. A refresh from a stale value is compared with a scan from scratch, with a `touched` that covers only the
edits inside existing blocks and deliberately omits every new, removed and moved block, so that a refresh
that trusted `touched` for the shape of the story would fail. A test cache records the previous value it
is given and requires that it came from this transaction's own store or from the base as it was when the
transaction began.

**Corpus laws** (in `state`'s tests over the fetched corpus, `state/tests_corpus.rs`, listed in
`xtask/core-io-exceptions.txt`). For every document that opens: the unedited `save` equals the input
(L1), the number of live refs equals the number of scan entries, `persist` then `restore` reproduces
the table, two opens give the same blob, every live ref resolves to an element of the right kind, and a
`Tx` that stages every part and discards leaves `save` unchanged.

**Concurrency.** `State: Send` and `State: !Sync`, asserted at compile time by a test.

**Performance.** A timing test records open, commit, `to_package`, `export`, `resume` and the
50-operation interleaved batch on the corpus documents with the most blocks and on a synthetic body
of the worst measured shape (section 15.2 has its numbers). The core may not read a clock, so the test
cannot live in `stetdoc-core`. It is the `state_*` benchmarks, which `cargo xtask bench` runs against
the baseline of the platform. The same harness re-measures the peak-memory table, and the `document_*`
rows measure the full edit path. A unit test asserts the shape that
makes `open` cheap: opening a document parses the stories and nothing else (the slot of every other
part is still unparsed afterward).

## 17. What other modules rely on, decisions, and what is open

**What this design changed elsewhere:**

- `ARCHITECTURE.md`: the resolution rules, which separate a block removed from the document
  (`stale_ref`) from a tracked-deleted one (kept, and `readonly_region` for a text edit); the base
  token and the rule that every operation batch names it, in working copies too, with `Implied` only
  for `inspect` and `Edit::files`; refs minted at commit and "no operation can name a block created
  earlier in the same batch"; the complement (ordinal and fingerprint instead of a block path, the
  lineage, the digests, the recorded versions); the Errors table (`internal`, exit class 7); rule 5
  (direct unit tests also for `wordml`'s story scan and text model, `refs` and `state`); the
  cache wording; and the Performance notes (copy-on-write, the package hash for very large
  packages, and the memory of a staged part).
- `GLOSSARY.md`: **ref**, **ref namespace**, **state version**, **complement**, **base token**,
  **lineage**, **tombstone** and **part generation**.
- The `internal` error code in `api/error.rs` through `error_codes!`, with exit class 7, and an
  addendum to decision 0001.
- `api/limits.rs`: `max_refs: u32` (default 1,048,576, which is 57 times the largest
  document measured and 66 times the largest of the 42,003 Harvey LAB documents). It bounds the
  table, the history and `restore`.
- `xml`: a `NodeId` is stable for the life of a tree and its clones, with a test, and
  `SpanTree::edit_count`, a counter that moves with every change the editor makes and with no
  call that changes nothing, and that a clone keeps (`xml/tree.rs`, `xml/edit.rs`, tests in
  `xml/tests_count.rs`).
- `wordml`, agreed in section 2: `SCAN_VERSION`, `FINGERPRINT_VERSION`,
  `IdKind`, `existing_ids` and `id_parts`, the lazy field pass, `Story::refresh` returning a
  `Refreshed`, `Story::own_parts`, and the edit count that a `Story` records.

**What other modules rely on:**

- **`revisions` and `ops`.** Take every revision and comment `w:id` from `Tx::next_annotation_id`
  and find what a batch created with `Tx::minted_annotation_ids()` and `Tx::copied_annotation_ids()`
  (section 13). Call `Tx::touch` for every existing block whose own bytes change (a
  paragraph's XML, a table's or a row's own parts, section 2), and for no block that an edit only
  inside a cell reaches through its table or row. A block that is new, removed or moved needs no call,
  and neither does the paragraph after one whose mark revision changed: the audit stamps those. Declare `retire(r, Rejected)` when an insertion is rejected, and `merge` for every
  merge, whichever element is kept. A split should keep the original element for the first part. Refuse
  a text edit to a tracked-deleted block with `readonly_region`. Resolve every operation's ref with
  `Tx::resolve` and the batch's claim. Mint a fresh `paraId` for each cloned paragraph, and make the
  uniqueness rule ignore `mc:Fallback`.
- **`comments`.** `Tx::add_part` is the way to add `comments.xml`. The anchors of a comment can span
  paragraphs.
- **`projection` and `document`.** Views, receipts and readable files carry the base token, and so
  does every operation batch, in a working copy too. `document` stores in the complement the blob, the base token, the two
  digests and the original package bytes, and checks nothing about them that `State::resume` checks.
  The unit of public text offsets is `document`'s: `wordml` counts Unicode scalar values, and
  `Edit::offsets` converts UTF-16 code units for napi callers (`document-and-ops.md` 2.2).
- **`styles` and `numbering`.** `state` may depend on both (`ARCHITECTURE.md`), and owns the cached
  accessors `State::cascade()` and `State::numbers(story, mode)`, which those modules write over `PartsView::derived`.
  Their builders are pure functions of the parts they read: the styles, theme and settings parts for the
  cascade, found through the main part's relationships with `PartsView::related_part`, and the story,
  numbering and styles parts for the numbers. An entry whose part is absent also depends on
  `Dep::Package`, so adding the part with `add_part` makes it stale. A numbers entry depends on the whole
  story part, so any edit to the story rebuilds it, measured at 3 to 20 ms per story. A batch that
  reads the numbers after each of 50 writes would pay that 50 times, and the builder receives the stale
  previous value for a refresh if that is ever needed. Both are on `PartsView`, so a check inside a
  transaction that staged the styles part sees the staged cascade, and on `State` as thin wrappers.
- **`compare`.** The namespace of a redline is `NamespaceId::derived`, and its origin is recorded in
  the ref map so that `resume` can verify it. A relationship that has no part (an external
  hyperlink) is added with `Tx::add_relationship` (section 10.2).

**Decisions:**

- **Every `add_part` call counts** (section 10.2). Counting only the first call left a stale story
  list and a stale `Rels` after the second, and the commit minted no ref for the new story. The generations
  move with each call.
- **An edited part is one whose edit count moved since the transaction cloned it**, not one that is
  dirty. A transaction that only borrows a part that an earlier commit edited changes nothing
  (section 10.4).
- **`merged_now` belongs to the commit** and not to the table (`Merged`, section 4). The base's caches
  that a commit that fails fills are allowed, and the contract says so (section 10.5).
- A tracked deletion keeps its ref, and a text edit to such a block is `readonly_region`.
- Refs are minted at commit, so a dry run's receipt shows new blocks without refs.
- The complement records an ordinal and a fingerprint instead of a block path. Counters are per kind.
- The namespace is a hash of the whole package, and its cost for very large packages is stated in the
  Performance notes.
- An internal contradiction is the new `internal` code, exit class 7, and never `validation_failed`.
- Operation batches always name the base token.
- A merge of three blocks is two `merge` declarations, and the audit checks their composition (section 7
  step 3), not each one alone.
- `touch` covers edits inside existing blocks, by what each block owns. New, removed and moved blocks,
  the blocks whose wrappers changed, the paragraphs whose field start changed, and the paragraph after
  a changed mark revision are found and stamped by the audit.
- **The timing test** is the `state_*` benchmarks (`bench/state.rs`), because the core may not read a
  clock. `cargo xtask bench` runs them against the baseline of the platform (section 15.2).
- **The corpus manifest** records `poi:test-data/integration/stress025.docx` as refused at the
  `document` stage. Its package is sound, but its main part is a theme manager, not a `w:document`, so
  `State::open` refuses it with `malformed_input`.
- **Memory headroom** (section 10.1) was the risk of copy-on-write on the largest bodies, and the chunked
  arenas of `xml` closed it: a clone shares the chunks, and every operation of `state` is at most 80% of
  the budget on the five benchmark inputs, by the requested bytes that the `state_*` benchmarks count.
  The probes of `state/tests_probe.rs` read resident size, which macOS reports low for moved pages and
  which holds the test's scaffolding, so they are a cross-check.
- **`insert_before` and `detach`** (`package-and-xml.md` section 7) use the previous-sibling link of
  `xml` nodes, so a `compare` that inserts thousands of paragraphs into one list does not walk the list
  for each one.

**Still open:**

- **The re-scan cost of an interleaved batch** (section 15.3) is 2.2 ms per write-then-read on the
  12 MB body and 3.2 ms on the dense one, measured through the code (section 15.2), and `ops` adds
  its own reads. A counter in
  `xml` of structural edits (insert, detach), which a text edit does not bump, would let a story survive
  a text edit with no scan, but `wordml` wants a story refreshed on every edit, so it would need changes
  in both modules. It is not needed for the budget.
- **Rebasing a tree** would shrink the `added` buffer of a long session, but it renumbers nodes, so it
  would need the table remapped through the scan, as `restore` does. It is not needed for any measured
  workload.

## 18. Dependencies

| Crate | Used for | Licence | Notes |
|---|---|---|---|
| `sha2` | SHA-256 for the namespace id, the lineage, durable ids and the complement's digests | MIT or Apache-2.0 | Already a workspace dependency (corpus fetch). The crate uses `unsafe` for CPU feature detection and is widely used and fuzzed, which rule 6 of `ARCHITECTURE.md` allows. Pure Rust, so the digest is the same on every platform |

No other dependency is added. The persistence format is hand-written text, so `refs` needs no
serialization crate, and the canonical-form rule is easier to state and to check than it would be for
a general serializer.
