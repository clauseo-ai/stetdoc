# Design: comments and notes

Design notes for reading and writing comments (with their threads) and for notes (footnotes and
endnotes). `ARCHITECTURE.md` is the map. This document says where the code lives, what Word writes, what each
operation compiles to, what is refused, and what the other modules provide. Terms follow `GLOSSARY.md`.
Evidence comes from the Word-authored and other documents of the curated corpus (306 documents; 20 have a comments
part, 13 of them with at least one comment, and 14 have footnote references and 10 endnote
references; fetched with `cargo xtask corpus fetch`) and from
one document outside the curated set, `RP050-Deleted-Footnote.docx` (Open-Xml-PowerTools, commit 5881422, SHA-256
`ff9b28d2f0bc…`), which Word wrote. Document names below are the corpus file names without the hash. No Word was
available, so every claim about what Word writes is a claim about files that Word, or another named producer, wrote.

## 1. The decisions to review first

1. **Two modules.** `threads` reads (the comment parts, the anchors in the stories, the threads, and an audit of how
   consistent the four comment parts are). `comments` writes (add, reply, resolve and reopen, edit, delete, the
   cleanup after another operation removed text, and the deletion of notes). `projection`, `check` and `ops` read through
   `threads`. Only `ops` calls `comments`. The split keeps `projection` and `check` from depending on `revisions`.
2. **Comment anchors go through the compiler's `place_markers` and `remove_markers`, and nothing else.** `comments` writes the comment
   parts itself. It builds comment paragraphs and the rows of the extension parts with the `xml` editor, because they
   hold no revisions and nothing in them is a diff of existing text. `ARCHITECTURE.md` rule 2 (one path for edits) is
   about the document's stories. The sentence is added to rule 2 (section 15).
3. **One id counter.** A comment's `w:id` comes from `Tx::next_annotation_id`, the counter that revisions and
   bookmarks use. The corpus shows that Word keeps comment ids apart from revision ids (2 of the 13 documents with comments have a comment id
   that equals a bookmark or revision id), so sharing is allowed but not required. Sharing is safe, and it is
   what `state` and `revisions` agree on. Ids in comment parts are not dense: documents have comment ids 5, 53, 518 and
   31, 35, 36, and 2 or 8 alone.
4. **A thread is the comment and every comment whose `paraIdParent` chain reaches it.** It is resolved when its root
   has `w15:done="1"`. A new reply always names the root as its parent. Resolving writes `done` on every comment of the
   thread, as the Word-written `word-authored-threaded-comments.docx` does (a parent and its reply, both
   `done="1"`).
5. **Only the author may edit or delete a comment.** "Own" means the comment's `w:author` equals the batch's author,
   byte for byte. Replying and resolving are open to anyone, since the agent's job is to answer a counterparty's
   comments. A refusal is `readonly_region`, the code `revisions` already uses for another author's pending change. No new code.
6. **The extension parts that exist are kept consistent, and `commentsExtended` is created when a comment is
   added to a document that has none.** `commentsIds`, `commentsExtensible` and `people` are kept up to date when they
   exist and are not created. Of the 13 documents with comments, 7 have `commentsExtended`, 4 have `commentsIds`, 1
   has `commentsExtensible` and 3 have `people` (4 more documents have a `people` part and no comments part). The alternative, writing all four as Word 365 does, was question 4 and was decided against (section 15).
7. **A comment survives the removal of its text, and `reconcile` is the safety net.** `revisions` hoists range markers and comment
   references and never removes either (`revisions.md` 3.6.5, and `cleanup.rs`), so an untracked deletion, an accepted deletion or
   a rejected insertion leaves the comment at the place where its text was, with an empty range, and `comments::reconcile`
   says so (`anchor_emptied`). If a reference run does go by another route, nothing in Word shows the comment, so `reconcile`
   (section 4.7) removes that comment and its own markers and rows. Comments already stranded in the input are left alone.
   **As built.** An untracked removal of a row or a table hoists the same way, so a comment anchored in a removed row
   keeps its markers and its reference run, in the neighbouring paragraph.
8. **Notes: text edits and `delete-note` in 1.0, insert-note later.** A note's paragraphs are blocks of the
   notes story, so `replace` on them is the tracked text edit of `revisions` and needs no code in `comments`. `delete-note` is built on the compiler's primitives (section 6).
   Inserting a note is designed in section 6 and is not implemented (section 15).

## 2. What Word writes: the comment model

### 2.1 The parts

| Part | Root and rows | Relationship type | Content type suffix | In the 13 corpus documents with comments |
|---|---|---|---|---|
| `word/comments.xml` | `w:comments` of `w:comment w:id w:author w:date w:initials`, each holding `w:p` | `officeDocument/2006/relationships/comments` | `wordprocessingml.comments+xml` | 13 |
| `word/commentsExtended.xml` | `w15:commentsEx` of `w15:commentEx w15:paraId [w15:paraIdParent] w15:done` | `microsoft.com/office/2011/relationships/commentsExtended` | `wordprocessingml.commentsExtended+xml` | 7 |
| `word/commentsIds.xml` | `w16cid:commentsIds` of `w16cid:commentId w16cid:paraId w16cid:durableId` | `microsoft.com/office/2016/09/relationships/commentsIds` | `wordprocessingml.commentsIds+xml` | 4 |
| `word/commentsExtensible.xml` | `w16cex:commentsExtensible` of `w16cex:commentExtensible w16cex:durableId [w16cex:dateUtc]` | `microsoft.com/office/2018/08/relationships/commentsExtensible` | `wordprocessingml.commentsExtensible+xml` | 1 |
| `word/people.xml` | `w15:people` of `w15:person w15:author` holding `w15:presenceInfo w15:providerId w15:userId` | `microsoft.com/office/2011/relationships/people` | `wordprocessingml.people+xml` | 3 |

All five hang off the main document part. `schema::PartKind` has the relationship type and default path of each, and
`wordml::id_parts` already finds them by relationship type. The content types are `application/vnd.openxmlformats-officedocument.` plus the suffix.
A **Strict** document (`Strict01`) has its comments part in the Strict `w` namespace and its relationship to it of the type `http://purl.oclc.org/ooxml/officeDocument/relationships/comments`. A created part copies the flavour of the main part (the `w` namespace and the relationship type prefix), and a test checks it against `Strict01`.
`comments/parts.rs` holds the five strings (a `PartKind::content_type` in `schema` would remove them).

What links the rows:

- The **last paragraph of a comment** has a `w14:paraId`. [MS-DOCX] defines `w15:commentEx/@paraId` as the `paraId` of the last paragraph of the comment, and no corpus comment has two paragraphs, so the keying of a multi-paragraph comment rests on the specification. Tests use a hand-built two-paragraph fixture, and the Word checklist (section 13) asks Word to open one. That value is the key of the comment's row in
  `commentsExtended` and in `commentsIds` (`officeimo` `word-authored-threaded-comments`: comment 0, paragraph `317E22E5`, in all three).
  A comment with several paragraphs is keyed by the last. Documents written by Word 2010 or later give every comment paragraph a `paraId`. 4 of the 13 documents have no `paraId`
  anywhere in their comments (LibreOffice and library-written files), and they have no extension parts either. No corpus comment has more than one paragraph.
- `w15:paraIdParent` names the **last paragraph id of the parent**. A reply is any comment with it. A chain
  exists in exactly one corpus document, the Open XML SDK test file `Invalid_Word15Comments` (tier `word-authored`, and its name says it is invalid): comment 53 names 52, which names 43, which has no parent. `Comment042` has one reply and no chain. So a thread root is found by
  following parents, and the reader tolerates a parent that is missing or a loop (both end the chain at that comment).
- **The `w15` namespace has two spellings in the corpus.** `Comment042` and `Invalid_Word15Comments`, written by Word 2013's preview in 2011, use `http://schemas.microsoft.com/office/word/2010/11/wordml` for `w15`, and Word reads it as `w15`. `threads`, `wordml::existing_ids` and the schema tables read both: the schema maps the preview namespace to `w15` (`PREVIEW_ALIASES`, from `xtask/src/codegen/spec.rs`), so its elements and attributes are validated as `w15` (W5 in section 9).
- `w16cid:durableId` is the same value as the `w16cex:durableId` of the same comment. It is a hash Word keeps for the life of the comment.
  It is a different id space from `w:id` and from the `w:num` durable ids in `numbering.xml`
  (`wordml::existing_ids(CommentDurableId)` already keeps those apart).
- `w15:done` is `0` or `1` per row. In `Invalid_Word15Comments`, 9 of 64 are `1`, and a reply may differ from its root
  (comment 36 is `1`, its reply 37 is `0`, its reply 38 is `1`). Word's own reading is the root's value. In the
  `threaded-comments` document both parent and reply are `1`.
- `w:date` is local time with a `Z`, and `commentsExtensible` carries the true `dateUtc` (`18:07:00Z` against `16:07:00Z`).
  The rule of `revisions` is that a date is the caller's wall clock time with seconds and `Z`, and that no `dateUtc` is written. `comments` follows it, so a
  new `commentExtensible` row has a `durableId` and no `dateUtc` (section 15). The attribute is optional in the schema.
- `people.xml` has one `w15:person` per author, with `presenceInfo providerId="None" userId="<author>"` for a local account
  (3 of 5 corpus documents), or `AD` and `Windows Live` ids for signed-in accounts. A new author gets `None`.

### 2.2 The anchors in the story

The shape Word writes for one comment (`word-authored-threaded-comments`, and the same in `Comment042`, `Strict01` and
`Formatted_elements`):

```xml
<w:commentRangeStart w:id="0"/><w:r>...text...</w:r><w:commentRangeEnd w:id="0"/>
<w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="0"/></w:r>
```

- The **reference run** follows the end marker. A reply has its own start, end and reference, beside its parent's: the
  document reads `start0 start1 text end0 ref0 end1 ref1`. The ranges of a thread have the same extent, and their
  order among themselves does not matter to Word.
- A range can span paragraphs (4 ranges in 2 curated documents, for example comment 2 of the `threaded-comments`
  document starts at the end of one paragraph and ends in the next). The reference run is where the range ends. A marker may also sit
  between blocks (`wordml.md` section 3.9). A range may be empty (a point comment).
- A range inside tracked changes keeps its markers as siblings of the runs in `w:ins` and `w:del`
  (`Comment042` has one marker group inside a revision). The compiler's `place_markers` and hoisting own this.
- A comment may have no markers at all (`rprchange_closed` has 3 comments and no marker or reference in any part).
  `Anchors` reports it as `start: None`. Word shows no such comment. The reader lists it as **unanchored** and keeps it.

### 2.3 The comment paragraphs

```xml
<w:comment w:id="0" w:author="..." w:date="2026-06-29T18:07:00Z" w:initials="OWC">
  <w:p w14:paraId="317E22E5" w14:textId="5068671C" ...><w:pPr><w:pStyle w:val="CommentText"/></w:pPr>
    <w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:annotationRef/></w:r>
    <w:r><w:t>text</w:t></w:r></w:p></w:comment>
```

The first paragraph starts with an `annotationRef` run. A comment with no text still has it (`Comment042` comment 31). The **style
ids are localised**: `Testocommento` and `Rimandocommento` (Italian), `Kommentartext` and `Kommentarzeichen` (German). The
names are not: `annotation text` and `annotation reference`. `comments` finds a style by name with `StyleDefs::by_name`, never by id.
When a document has neither (the files that LibreOffice and the libraries wrote often do not), the new comment is written with the same two styles that Word adds on first use, copied from the
Word-written `Strict01`: `annotation reference` (a character style, `w:basedOn DefaultParagraphFont`, `uiPriority 99`, `semiHidden`,
`unhideWhenUsed`, size 16) and `annotation text` (a paragraph style, `basedOn Normal`, size 20). The linked `Comment Text Char` style, the `w:link` to it and the `Balloon Text` style are left out. A document does not need them to open, and `comments` adds no character style that no run uses. It adds the two styles with
`Tx::part_mut(styles)` and `schema::insert_in_order`.

### 2.4 What Word writes for each action

| Action | `comments.xml` | `commentsExtended` | `commentsIds` | `commentsExtensible` | `people` | Story |
|---|---|---|---|---|---|---|
| New comment | a `w:comment`, last in the part, paragraph with a new `paraId` and `textId` | a row `paraId`, `done="0"` | a row with the `paraId` and a new `durableId` | a row with the `durableId` | a `w15:person` if the author has none | start, end and reference run |
| Reply | the same, by the replying author | a row with `paraIdParent` = the root's last `paraId`, and `done` equal to the root's | as above | as above | as above | start, end and reference run beside the root's |
| Resolve | no change | `done="1"` on the root and on each reply | none | none | none | none |
| Reopen | no change | `done="0"` on all of them | none | none | none | none |
| Edit text (own) | the paragraphs of that comment are rebuilt, `w:date` is kept, the last paragraph keeps its `paraId` and gets `textId` `77777777` | none (the last paragraph keeps its `paraId`) | none | none | none | none |
| Delete | the `w:comment` goes | its row goes | its row goes | its row goes | stays | the three elements go |

The edit row is a design choice, not an observation. Word does not change a comment's date when its text is edited, so neither does `comments`.
`comments` never changes an author or a date of an existing comment.

## 3. Reading: `threads`

Pure functions over trees, read only, cheap when a document has no comments (one relationship lookup).

```rust
pub struct Threads { /* comments in the order of comments.xml; threads in the order of their roots */ }
pub struct Comment {
    pub id: i64,                       // w:id
    pub unit: usize,                   // index into the comments Story::units(), so projection can find its blocks
    pub author: Option<String>, pub initials: Option<String>, pub date: Option<String>,   // as written
    pub para_id: Option<u32>,          // of the last paragraph
    pub parent: Option<i64>,           // the comment id of the parent, if the row names one that exists
    pub resolved: bool,                // this row's done
    pub durable_id: Option<u32>,
    pub anchor: Anchor,
}
pub struct Anchor {
    pub story: Option<Arc<str>>,       // part that holds the markers; None when unanchored
    pub start: Option<Loc>, pub end: Option<Loc>, pub reference: Option<Loc>,   // wordml::Loc
    pub blocks: Vec<BlockKey>,         // the paragraphs the range touches, first to last (empty if unanchored)
}
pub struct Thread { pub root: i64, pub replies: Vec<i64> /* in file order */, pub resolved: bool /* the root's */ }

impl Threads {
    pub fn of(parts: &PartsView<'_>) -> Result<Arc<Threads>, StetError>;      // cached with PartsView::derived
    pub fn comments(&self) -> &[Comment];
    pub fn threads(&self) -> &[Thread];
    pub fn comment(&self, id: i64) -> Option<&Comment>;
    pub fn thread_of(&self, id: i64) -> Option<&Thread>;
    pub fn covered_text(&self, parts: &PartsView<'_>, id: i64, mode: Mode, limit: usize) -> Option<String>;
    pub fn counts(&self) -> Counts;                                            // total, resolved threads, unanchored
}
pub fn audit(parts: &PartsView<'_>) -> Result<Vec<Finding>, StetError>;       // section 3.2
```

How it reads:

- **Parts** by relationship type from the main part (`PartsView::related_part`). A missing extension part means no rows, and
  every comment is open with no parent.
- **Comments** from `Story::units()` of the comments story (it already has `id`, author, initials and date), and the
  `w14:paraId` of the last paragraph read from its element (`Paragraph::id()` is the node, not the `paraId`).
- **Rows** by `paraId`, from the three extension parts. A row whose `paraId` names no comment is ignored by the reader and
  reported by `audit`.
- **Anchors** from `Story::anchors(tree).comments` of every story, since comments can be anchored in a note (Word allows it, though no corpus document does). A
  comment id found in two stories takes the first. `blocks` come from `Loc::paragraph` and `Story::index_of`, plus every
  paragraph between a start and an end in different paragraphs (walking `Story::blocks`). `covered_text` uses `TextMap::offset_of(mode, node, 0)`
  as `wordml.md` 3.9 says, and shows `limit` characters.
- **A marker between blocks** (282 of 1,993 in the curated set) starts the blocks after it and ends the blocks before the next one, and `blocks` also lists the paragraph that holds the reference run, which can lie after a range that ends between blocks. `blocks` and `covered_text` share one rule for the range: a range between blocks covers the blocks between its markers whole, and two markers with no block between them cover the empty string.
- **`Comment::parent` is an id and is given only when the parent's `w:id` is unique.** When two comments share the id, the parent is `None`, so climbing by id always ends and never lands on the wrong comment. Threads are built from indexes and stay exact (`thread_of`).
- The cache depends on every story part, the comment parts and `Dep::Package`. It is rebuilt after a body edit, once per read
  and only when comments exist (the walk is the 11 ms at 13 MB that `wordml.md` measures, and a document with comments is
  rarely that large).

### 3.2 The audit

`audit` returns findings with a stable `code`, the comment id or part, and a sentence. `check` uses the set of
findings before and after a batch under the no-new-violations rule, so a finding already in the input is reported and does not block
(`rprchange_closed` has unanchored comments, `wordprocessingml-comprehensive` has comments with no rows and `Invalid_Word15Comments` has 65 comments and 64 rows; these are the corpus documents that have findings).
Findings in two classes: **errors** (a state Word may repair) and **notes** (legal, worth saying).

| Code | Class | When |
|---|---|---|
| `comment_id_duplicate` | error | two comments with one `w:id` (Word rule W3, per kind). An operation that names a duplicated id is `ambiguous_match` |
| `marker_unpaired` | error | a `commentRangeStart` with no end or an end with no start (Word rule W7) |
| `marker_without_comment` | error | a range marker or reference whose id is no comment |
| `reference_missing` | error | a comment with a start and an end and no `commentReference` |
| `reference_duplicate` | error | two references to one id |
| `unanchored` | note | a comment with no marker and no reference |
| `row_without_comment` | error | a row in any extension part whose `paraId` names no comment |
| `row_missing` | note | a comment with a `paraId` and no row in a part that exists (Word reads it as open and with no parent) |
| `para_id_missing` | note | a comment whose last paragraph has no `paraId` (it cannot be threaded) |
| `para_id_duplicate` | error | two comment paragraphs, or a comment paragraph and a story paragraph, with one `paraId` |
| `para_id_range` | note | a `paraId` at or above `0x80000000`, which the schema forbids and `contract-redlined` has |
| `parent_missing` | note | `paraIdParent` names no comment |
| `thread_cycle` | error | parents form a loop |
| `durable_mismatch` | error | a `commentsIds` row and its `commentsExtensible` row disagree, or a `durableId` is used twice |
| `part_unrelated` | error | an extension part exists but the main part has no relationship to it |

The rules for ids and ranges are the comment half of the Word rules W3 and W7. `check` takes them from `audit`, so there is one reader.

## 4. Writing: the operations

`ops` parses an operation, resolves its refs, and calls one function of `comments`. Every function takes `&mut Compiler` (it
needs `place_markers` and `remove_markers`, and through the compiler the `Tx`), a `RevStamp` (author and date) and plain
parameters, and returns a `CommentEffect`:

```rust
pub enum Action { Added, Replied, Resolved, Reopened, Edited, Deleted, Removed /* by reconcile */ }
pub struct CommentEffect {
    pub action: Action, pub id: i64, pub thread: i64, pub author: String,
    pub block: Option<BlockKey>,         // the anchored paragraph, for the receipt and the scope guard
    pub warnings: Vec<CommentWarning>,   // code and sentence: `already_resolved`, `reply_to_resolved`, `anchor_emptied`, ...
}
```

The operation shapes (defined in `api/op.rs`). Comment ids are `c3`, the `w:id`, as in the readable files:

| Operation | Fields | Calls |
|---|---|---|
| `comment` | `ref`, optional `find` (a unique text range of that paragraph; the whole paragraph when absent), `text`, optional `initials` | `comments::add` |
| `reply` | `comment`, `text` | `comments::reply` |
| `resolve` | `comment`, optional `reopen: true` | `comments::set_resolved` |
| `edit-comment` | `comment`, `text` (approved, section 15) | `comments::edit` |
| `delete-comment` | `comment` | `comments::delete` |

`text` is one paragraph per line (`\n` separates paragraphs; there is no line break inside a paragraph, and `\t` is a tab). An empty text
is `invalid_payload`. `initials` default to the first letter of each word of the author, in upper case, at most four.

### 4.1 `add`

Before any write: the block is in the body story and is not a deleted block, otherwise `unsupported` (a header, footer, note, comment
or text box: Word cannot comment in a header, and the others are a later release) or `readonly_region` (a tracked-deleted block).
Then, in this order, so that a refusal leaves nothing half done (the `Tx` makes it atomic anyway):

1. `id = tx.next_annotation_id()`.
2. `place_markers(block, range, spec)`. `spec.start` is `w:commentRangeStart w:id`, `spec.end` is the matching end, and
   `spec.reference` is the run `w:r { w:rPr { w:rStyle <annotation reference> }, w:commentReference w:id }`, the style id from
   `by_name`, and no `rStyle` when the style is absent and the add of it failed (it does not). The range is the `find` match, or `0..len`
   for a whole paragraph. A range that cuts into a field is `readonly_region` by the compiler's rule. The compiler touches the paragraph.
3. `ensure_parts`: create what is missing with `Tx::add_part` and a skeleton, `related_from` the main part. The order is
   `comments.xml`, then `commentsExtended.xml`. When `add` creates `commentsExtended.xml`, every existing comment that has a `paraId` gets a row with `done="0"` at that moment, as in 4.3, so the new part describes the thread set. The skeletons are a root element with the namespaces it needs and `mc:Ignorable`
   (`w`, `mc`, `w14`, and `w15` or `w16cid` or `w16cex`). The `xml` editor adds declarations on the root when new content needs them.
4. `ensure_styles` (section 2.3).
5. The comment: `w:comment` appended to `w:comments`, its paragraphs built whole (`pStyle` by name, the `annotationRef` run first,
   one run per paragraph with `xml:space="preserve"` where the text has leading or trailing spaces and a `w:tab` for `\t`), `paraId` from
   `tx.mint_id(ParaId, comments_part)`, `textId` `77777777` as the compiler writes for new paragraphs.
6. The rows: `commentEx` (`done="0"`), then `commentId` with `durableId` from `tx.mint_id(CommentDurableId, ids_part)`, then
   `commentExtensible`, then the `people` entry, each only in a part that exists.

The new comment paragraphs are blocks of the comments story. Their refs are minted at commit, and no operation names one.

### 4.2 `reply`

`comment` names any comment of a thread. The parent is the thread's root. Steps: as `add`, with two differences. The markers go **beside the
root's** (`place_markers_beside`, section 9), which handles a root whose range spans paragraphs. And the row has
`paraIdParent` = the root's last `paraId` and `done` equal to the root's. If the root has no `paraId`, `comments` first gives its last paragraph one (and
`textId`; the part's root start tag gains `mc:Ignorable` and the `w14` declaration through the `xml` editor, Word checklist item 4), because a row cannot name a parent without it. This is an edit of a comment paragraph that already exists, and its bytes change
only by the two attributes. If the root has no row in an existing `commentsExtended`, `comments` adds one. A reply to a resolved thread succeeds and
carries the warning `reply_to_resolved`: Word's behaviour here could not be observed, so the thread stays as it was.

A reply to a thread whose root is unanchored (no markers) is `unsupported`: with no range there is nothing to anchor beside, and a comment with no reference is not shown by Word.

### 4.3 `set_resolved`

Sets `done` on the row of every comment of the thread. Rows that do not exist are created, which creates `commentsExtended` if
the document has none (every existing comment that has a `paraId` gets a row with `done="0"` at that moment, so the new part describes the
whole thread set, and comments with no `paraId` are left without rows). Resolving a resolved thread is a no-op with
`already_resolved`. Touching no document block, it changes no block: the scope guard cannot see it (section 8).

### 4.4 `edit`

Own comments only. The comment's paragraphs are rebuilt from the new text: the last new paragraph keeps the old last paragraph's `paraId` (so the
rows keep their key) and gets `textId` `77777777`, other paragraphs get new ids, and `w:date` is kept. The change is not tracked, since the comment text is not
document content and Word has no revision shape for editing a comment. A comment with replies by others can still be edited.

### 4.5 `delete`

Own comments only. What goes is the comment **and its descendants** (its replies, and their replies if the file chains them). Deleting a root deletes the thread, and deleting a reply or a mid-chain comment deletes it and what hangs below it. All of them must be the author's own: any other author's comment among them is `readonly_region`: "c3 has a reply by EW. Reply to it, or ask EW." Steps: `remove_markers` for the ids of the removed comments, then remove the
`w:comment` elements and the rows by `paraId` and `durableId`. `people` stays. When the thread had the comments part's last comment, the empty
part stays, since `Tx` cannot remove a part and an empty `w:comments` is valid.

### 4.6 What is refused

| Code | When |
|---|---|
| `not_found` | a comment id that no comment has. A comment in a deleted thread: the same |
| `invalid_payload` | a malformed id, empty text, a `find` that is empty, text with a character XML cannot carry |
| `ambiguous_match` | `find` matches more than once in the paragraph (the rule of `ops` for `find`, same as `replace`) |
| `readonly_region` | `edit` or `delete` of another author's comment; `delete` of a thread with another author's reply; a range that cuts into a field (the compiler's rule); a tracked-deleted block |
| `unsupported` | a target in a header, footer, note, comment or text box; adding a comment to a table or a row (they are not paragraphs: name a paragraph in a cell) |
| `stale_ref` | the paragraph ref is stale (the rule of `ops`, not of `comments`) |
| `limit_exceeded` | the annotation counter is exhausted (from `state`) |

### 4.7 `reconcile`

`comments::reconcile(tx)` runs once at the end of `ops::apply`, before `check`, and only when a comments part exists. It takes nothing from `ops`:
it reads the base from the `Tx` itself (`Tx::base()`). The set it examines is the comments that had a reference run in the base, plus the comments
that `add` and `reply` made earlier in the same batch (the `Tx` knows them from `minted_annotation_ids` and the effects `comments` returned). It reads `Threads` of the staged state, and for each comment in the set that has no
reference now, it deletes **that comment and only its own markers**, as `delete` does but without the author rule, and returns a `Removed` effect with the warning "c5 was removed because the text it was
attached to was removed". Markers that no removed comment owns are never touched. A reply whose root is removed while its own reference survives is re-parented to the nearest surviving ancestor, or becomes a root: its row's `paraIdParent` changes or goes, and
nothing else about it does. A comment whose range is empty after a tracked deletion keeps its reference and stays (`anchor_emptied`, a warning). A comment that was stranded in the input is not in the set and is not touched.
The marker removals of `reconcile` are exempt from the scope guard and are listed in the receipt (section 8).

## 5. Interaction with `revisions` and with the other parts

- **Tracking.** Comment operations are never tracked, whatever the batch's track mode (`revisions`: markers "never tracked"). An untracked batch
  writes the same bytes.
- **Accept and reject** leave comments alone. `revisions` hoists a range marker and a comment reference out of a removed region (`cleanup.rs`), so a comment whose reference run is inside a `w:ins` that is rejected, or inside a `w:del` that is accepted, stays at the place where the text was, with an empty range, and `reconcile` warns `anchor_emptied` (`Comment042` has reference runs inside `w:ins`: `reject all` there leaves its comments as points). An earlier version of this design said such a comment was deleted, as Word does; the rule of `revisions` that a start never loses its end decides it, and nothing is lost.
- **A reference run at the edge of a range** is not covered by a whole-paragraph `replace`: `revisions` guarantees that `replace_text(0..len)` leaves a `commentReference` run that sits after the range end where it is, so a comment survives a rewrite of the text it is attached to (a test covers it).
- **Text model.** Range markers and reference runs have no width, so adding or removing a comment changes the text of no mode (law C5).
- **`paraId` uniqueness.** `comments` mints with `mint_id(ParaId, part)`, which skips every id that `existing_ids` reports in the story parts and in
  `commentsExtended` and `commentsIds`.
- **Refs of comment paragraphs.** They are blocks of the `Comments` story, so `state` mints refs for them. Agents are never shown them.

## 6. Notes

**Reading.** `wordml` already lists the units of the notes stories, skips the separator notes, and the text model has `NoteRef` and `NoteMark` atoms.
Notes are named by an **ordinal** (the position of the note's first reference, counted per kind, as `projection` shows it in `{fn3}`). The ordinal is computed in `wordml`
so that `projection` and `ops` share one rule. `comments` adds no reader.

**Editing a note's text** is `replace` on the paragraphs of the note, the tracked text edit of `revisions`. Nothing is needed from `comments`. Evidence for a tracked edit inside a note:
`tdf121784` and `w-note-endnote`, each with `w:ins` runs in the note's paragraph.

**What Word writes.** `RP050-Deleted-Footnote` is the Word-written shape for a deleted note. The body has the reference run inside the `w:del` with its text
(`w:del{ r(delText), r{rStyle FootnoteReference, footnoteReference id=1}, r(delText) }`). The note keeps its element, **its paragraph's mark has `w:rPr/w:del`**, and every run, the
`footnoteRef` run included, is inside one `w:del`. A tracked insertion is written by LibreOffice (`tdf121784`) and by a library, OfficeAgent (`w-note-endnote`), as the
reference run in a `w:ins` in the body, and in the note a `w:ins` around the `footnoteRef` run and one around the text run, with no inserted
paragraph mark. No Word-written insertion exists in the corpus, so insert-note would rest on those two (risk R3), which is one reason it is not implemented.

**Operation in 1.0: `delete-note`.** The note is named by kind and ordinal. It compiles to the compiler's primitives and one cleanup call:

| Step | Call |
|---|---|
| the reference | `replace_text(block, range of the NoteRef atom, "")`, tracked: the compiler writes RP050's `w:del` around the reference run |
| the note's paragraphs | the **whole-note variant of `delete_block`** in `revisions`: when the whole note goes, the last paragraph's mark is deleted too, as RP050 has it. `delete_block` alone keeps the last mark of a container (`revisions.md` 3.2.2), which is not RP050's shape |
| accept, or an untracked delete | the note cleanup of `revisions` removes the note element once nothing refers to it (`revisions.md` 3.6.4, called from `resolve` step 5). It lives in `revisions`, since `comments` depends on it and not the reverse. `comments` calls it for an untracked `delete-note` (`Compiler::remove_unreferenced_notes`) |

A note that has no reference (an unreferenced note) is deleted with the whole-note variant alone. Refused: a note in a header, footer or comment (`unsupported`), a note ordinal that does not exist (`not_found`), a note whose paragraph is already a tracked deletion by another author (`readonly_region`, the compiler's rule).
`delete-note` is `unsupported` through `notes.md` edits (`projection` refuses a removed note line). `delete_note` touches no comment part. It lives in `comments/notes.rs`, and the `comments` contract says "comments, and the deletion of notes", because the two share the style-by-name and part-finding helpers.

**Not implemented: `note` (insert).** It needs a new compiler primitive `insert_run(block, at, RunSpec)`, a notes part with the two separators and the `settings.xml` list when none exists (`<w:footnotePr><w:footnote w:id="-1"/><w:footnote w:id="0"/></w:footnotePr>`), the note styles by name
(`footnote text`, `footnote reference`, localised as `Textodenotaderodap` and `Refdenotaderodap` in `Corrupted footnotes`), and a new id, **the highest id of that kind plus one, at least 1** (not the annotation counter). It is not built in 1.0.

## 7. What `projection` and `check` and receipts need

**`projection`** (`projection-and-index.md` ask W4): `Threads` gives, for each comment, the thread id, `parent`, `resolved`, author, initials, date, anchor locations, anchor blocks, the unit index
for its blocks, and `covered_text` for the 100 characters of `comments.md`. `Threads::counts` gives the summary line ("4 comments, 1 resolved"). `Scope::Comments(filter)` is built on `threads()`.
Comment marks in the body text are the projection's own, from `mark_id`.

**Receipts (`ops`).** Each comment effect gives the action, the comment id as `c3`, the thread, the author, the anchored block (to be turned into a ref by `PartsView::ref_of`), and warnings. Added comments
are listed with the id so the next operation of the same agent can name them, which a ref cannot do within a batch. A reply and a resolve name the thread's root as well.

**`check`.** `threads::audit` supplies the comment findings of section 3.2 for the before and after sets of the no-new-violations rule. It also supplies the W3 (comment ids unique per kind) and W7 (comment ranges paired)
that `revisions.md` section 7.3 assigns to `check`.

## 8. The scope guard

A comment operation's touched blocks are these: `add` and `reply` touch the paragraphs that hold their new markers (the compiler's `place_markers` does), and the cleanup of a removed comment's markers is exempt from the guard and listed in the receipt. The guard must
**ignore blocks in the comments story**, whose paragraphs `comments` creates and rewrites: they are not part of any `--scope` an agent can name. `resolve` and `edit` change no document block, so touching an
anchored block would stamp it changed for nothing and turn old refs to it `stale_ref`. For them `ops` checks the scope itself: the operation is in scope when every block in `Comment::anchor.blocks` is declared. An unanchored comment is in scope
always. `reconcile` and `delete` remove markers from paragraphs the agent may not have named. For `delete`, the anchored blocks must be in scope like any other comment operation, and for `reconcile` they are exempt, because the text edit that caused them was itself in scope. `ops` implements both.

## 9. What this module uses from other modules

From `revisions`:
- `place_markers_beside(&mut self, comment_id: i64, spec: &MarkerSpec) -> Result<Effect, StetError>`: the start goes immediately after the start marker of `comment_id`, the end after its
  reference run (after its end marker when it has none), the reference run after that end. It works when the existing range spans paragraphs. Without it a reply to a spanning comment could not be anchored with `place_markers`, whose range is one paragraph's.
- `remove_markers_in_part`, which removes markers by part and ids: the markers and reference runs of an id can be in two paragraphs, or between blocks (282 of 1,993 range markers in the curated set are). A block parameter cannot reach those.
- Hoisting. `delete_block` untracked, and the untracked removal of a row or a table, hoist range markers and comment reference runs as an accepted deletion does (`revisions.md` 3.6.5 and 3.4). The markers stand between blocks, and when a comment's reference run is among them they all go into the neighbouring paragraph, in order.
- Nothing in `Effect`. `comments` finds lost references by comparing `Threads` of the base and of the staged state.
- A `commentReference` run at the edge of a range is not covered by a whole-paragraph `replace_text(0..len)`, so the comment survives a rewrite of its text.
- The whole-note variant of `delete_block`: when every paragraph of a note goes, the last paragraph's mark is deleted too (RP050; section 6).
- The note cleanup (remove a note once nothing refers to it), callable on its own as `Compiler::remove_unreferenced_notes`, for an untracked `delete-note`.

From `state`: **S7** the staged `PartsView` of a `Tx` gives a fresh `Story` for a part made by `add_part` and then edited with `part_mut`, and a `derived` entry keyed by every story part (`refs-and-state.md` 10.2).
`reconcile` reads the base with `Tx::base()` and the made ids with `minted_annotation_ids`. `mint_id` after `add_part` and `next_annotation_id` complete what it needs.

From `wordml`: the note ordinal (section 6). **W5:** `existing_ids` accepts `http://schemas.microsoft.com/office/word/2010/11/wordml` as the `w15` namespace (`paraId` in `commentEx`), and the schema maps it to `w15`; two corpus documents use it. `Paragraph::id()` and `Story::units()` give the rest.

`projection` uses `Threads` for W4 and the `wordml` note ordinal (a note is named by ordinal for `delete-note`).

`ops` dispatches `comment`, `reply`, `resolve` (with `reopen`), `edit-comment`, `delete-comment` and `delete-note`; calls `comments::reconcile(tx)` at the end of `apply` (no arguments); applies the scope guard as in section 8; and builds receipts as in section 7, including the comments that `reconcile` removed and the markers it removed. Capabilities show `edit-comment`, `delete-comment`, `resolve` and the note deletion as `trackable` only where they are (`delete-note` is tracked, the comment operations are not). New lines in `comments.md` stay `readonly_region` (`projection` does that). An operation that names a duplicated `w:id` is `ambiguous_match`.

**The handle.** `comments` writes parts through `revisions::handle::PartWriter`, obtained with `Compiler::parts_mut()`. It reads the transaction, writes only a part the main part relates to as a comment part (`comments`, `commentsExtended`, `commentsIds`, `commentsExtensible`), the people part or the style sheet (the comments part is a story of `wordml` and is allowed), adds only one of those, related from the main part with that kind's relationship type and content type (which writes their content type and relationship), and mints ids. It exists so that no module can write a story past the compiler. `delete_note` reads whether the batch is tracked from the compiler (`Compiler::tracked`), and takes out an unreferenced note with `Compiler::remove_unreferenced_notes`.

## 10. Laws and tests

Comments are not tracked, so the reject-all law does not apply (`ARCHITECTURE.md`: "comment operations are checked against their own laws"). Those laws. Several are weaker than byte identity on purpose, because the operations rebuild paragraphs, add attributes and split runs (`place_markers` carves runs and `remove_markers` does not join them), and keep the styles and `people` entries they added:

| Law | Statement |
|---|---|
| C1 Total | `Threads::of` and `audit` succeed on every corpus document and never change a byte |
| C2 Counts | On the corpus, comments, threads, resolved threads and unanchored comments equal an independent count by regular expression over the parts (the test harness computes it) |
| C3 No new findings | After any batch of comment operations, on **every** document, the set of `audit` findings has nothing that the input's set did not have. For each comment, the `paraId` and `durableId` of its rows exist in every extension part that exists |
| C4 Own only | A property test with two authors: `edit` and `delete` by the second of the first's comments is `readonly_region` and changes nothing |
| C5 Text untouched | Adding, replying, resolving, editing and deleting change no block's text in `Final`, `Original` or `Redline` (the text of a block, not its comment paragraphs: those are what `edit` changes) |
| C6 Locality | A comment operation changes only: in the anchored paragraph, its markers, its reference run and the runs that `place_markers` split to place them; in the comment parts, the elements it appends, removes or rebuilds, and the attributes `done`, `paraId`, `textId` and `paraIdParent` it sets; the parts it adds, with the rows of `[Content_Types].xml` and of the main relationships; elements appended to `styles.xml`; `w15:person` entries appended to `people.xml`. Every other element is byte-identical (L2) |
| C7 Delete undoes add | `add` then `delete` of the same comment leaves every block's text, the set of comments and threads, and every element outside the anchored paragraph, the comment parts and `styles.xml` as they were. It does **not** restore bytes: the anchored paragraph keeps its split runs, created parts stay and are empty, and the styles and the `people` entry that `add` wrote stay |
| C8 Resolve | `resolve` twice equals `resolve` once, byte for byte (the second is a no-op with a warning). `resolve` then `reopen` gives the same threads, resolved flags and anchors as the input. It is not byte-identical when `resolve` had to create rows or a `commentsExtended` part, or when a thread's rows had different `done` values to begin with |
| C9 Determinism and dry run | The same batch on the same base gives the same bytes, ids included. A dry run changes nothing |
| C10 Stability | `export`, `resume`, then the same operation gives the same bytes as without the round trip. A second unpack and pack gives the first bytes (L4) |
| C11 Reconcile | After random text edits, deletions, accepts and rejects on documents with comments (and with comments added in the same batch), every comment of the examined set that lost its reference run is removed, and the ones `revisions` hoisted stay with an empty range and a warning; every removed comment's own markers are gone; markers of no removed comment are untouched; every surviving comment keeps its id, author, date, text, anchor and resolved flag. The one thing that may change on a survivor is its parent, when its parent was removed (re-parented to the nearest surviving ancestor, or a root) |
| C12 Validity | The output validates (L6), and the Word rules W3 and W7 and the comment findings hold as C3 says |
| C13 Read after write | `Threads::of` on the state after an operation shows what the operation did: the new comment with its author, date, text, parent and resolved flag, an anchor whose `covered_text` equals the `find` text, and a thread with the right members. `resolve` shows resolved and `delete` shows no trace |

Tests, at the module interface and through `Document`:

- `threads`: the three-comment threaded document (two threads, one resolved, a spanning range), `Comment042` (one reply, an empty comment, reference runs inside a `w:ins`), `Invalid_Word15Comments` (the one chain in the corpus, a missing row), a LibreOffice document (no `paraId`, no rows), `contract-redlined` (`paraId` at or above `0x80000000`), and
  `rprchange_closed` (unanchored comments), each against hand-written expectations. A synthetic document for a parent loop, a missing parent, a duplicated `w:id` and a **two-paragraph comment** (keyed by its last paragraph).
- `comments`: the fixture builder (`stetdoc_testkit::fixtures`) makes documents with no comment parts, with `comments.xml` only, with all five, with localised style ids, with no comment styles and a Strict one. Add, reply, resolve, reopen, edit, delete on each. The expected result is asserted
  by element (not by snapshot), and the shapes of section 2 are asserted against the Word-written files above.
- A comment survives `replace` of the whole paragraph it is attached to, and a comment whose reference run is inside a `w:ins` is deleted by `reject all` (`Comment042`).
- The laws C3, C5, C6, C8, C9, C11, C13 run on the Word-authored and the other-producers tiers with generated batches. Generators use only `view` output (a find range from the text, a comment id from `comments.md`).
- Part creation: `add_part` is called up to four times in one `Tx`. The test asserts the `Rels` and the story list after each.
- `delete-note`: `RP050` as the shape oracle (reference run in `w:del`, note paragraphs and mark deleted), then accept (the note is gone, and the settings list is unchanged), then reject (the document is as before), and an untracked deletion.

## 11. Module layout

```text
threads/     mod.rs        //! contract, Threads, Comment, Thread, Counts                    ~120
             parts.rs      which part is which; the rows of the extension parts             ~200
             anchors.rs    the anchors of every story, anchor blocks, covered_text          ~200
             audit.rs      Finding and the rules of 3.2                                     ~280
             tests_*.rs    one file per fixture family                                      each under 500
comments/    mod.rs        //! contract, CommentEffect, Action, the entry functions         ~120
             parts.rs      create a missing part (skeleton, add_part), find a part          ~220
             rows.rs       add, find, remove and set rows in the four extension parts       ~280
             body.rs       build and rebuild the paragraphs of a comment                    ~200
             styles.rs     find a style by name, add the two comment styles                 ~180
             add.rs        add and reply                                                    ~260
             resolve.rs    set_resolved                                                     ~100
             delete.rs     delete, and the shared removal of a thread                       ~200
             edit.rs       edit                                                             ~120
             reconcile.rs  the cleanup after other operations                               ~220
             notes.rs      delete-note                                                      ~150
             tests_*.rs
```

Every file under 600 lines. `mod.rs` of each opens with its `//!` contract.

**Dependencies** (`ARCHITECTURE.md` Allowed dependencies):

| Module | May depend on |
|---|---|
| `threads` | `api`, `xml`, `schema`, `wordml`, `state` (`schema` for the relationship types of the comment parts) |
| `comments` | `api`, `xml`, `schema`, `wordml`, `state`, `revisions`, `threads` |
| `check` | adds `threads` |
| `projection` | adds `threads` |
| `ops` | adds `comments`, `threads` |

Neither `threads` nor `comments` is named by `state`, `revisions` or `wordml`, so the table stays acyclic. The module table has two rows: `threads` (read the comment parts and the threads, audit them: `Threads::{of, comments, threads, comment, thread_of, covered_text, counts}`, `audit`; start at `threads/mod.rs`)
and `comments` (write comments, and delete notes: `add`, `reply`, `set_resolved`, `edit`, `delete`, `reconcile`, `delete_note`; start at `comments/mod.rs`). `GLOSSARY.md` has the terms root, resolved, reference run and durable id under "Comment, reply, thread".

## 12. Plan

| Step | Content | Done when |
|---|---|---|
| 1 | `threads` read side: `parts`, `anchors`, `Threads::of`, `covered_text`, `counts` | C1 and C2 on the corpus; the threaded fixtures |
| 2 | `threads::audit` | findings on the corpus equal the expected set (the documents named in 3.2 and nothing else) |
| 3 | `comments::parts`, `rows`, `body`, `styles` | part creation tests with up to four `add_part` calls, and the Strict flavour |
| 4 | `add`, `reply` (needs `place_markers_beside` and `remove_markers_in_part`) | C3, C5, C6, C9, C13 on generated batches |
| 5 | `resolve`, `delete`, `edit` | C4, C7, C8 |
| 6 | `reconcile` | C11 with the compiler of `revisions` |
| 7 | `delete-note` (needs the whole-note `delete_block` and the note cleanup) | the note tests |
| 8 | the laws through `Document`, and the docs checklist | `cargo xtask check` |

## 13. The Word checklist

What only a person with Word can settle (decision [0005](../decisions/0005-word-oracle.md)). Each item is a document that `comments` writes and Word opens:

1. A document with a new `commentsExtended` and no `commentsIds` or `commentsExtensible` opens with no repair, and its threads and resolved state show.
2. A reply added to a resolved thread, written with `done="1"`, shows as resolved. Record whether Word reopens the thread (R2).
3. A comments part left as an empty `w:comments` after the last comment is deleted opens with no repair.
4. A LibreOffice `comments.xml` that gains `w14:paraId` (a reply to a comment with no `paraId`) and has no `mc:Ignorable` opens with no repair.
5. A new `people.xml` entry with `presenceInfo providerId="None" userId="<author>"` opens, and the author's name shows.
6. Adding a comment to a Strict document (`Strict01`-like) with the Strict `w` namespace and the purl comments relationship type opens.
7. A point comment (empty range) opens and shows at its position.
8. A reply anchored beside a root whose range spans paragraphs (start1 after start0, end1 and ref1 after ref0) shows as one thread.
9. `durableId` values at the top of the range (`0x7FFFFFFF`) are accepted. If not, `comments` masks them to `1..=0x7FFFFFFE` (an unverified belief that Word wants a value strictly below `0x7FFFFFFF`).
10. The `annotation reference` and `annotation text` styles added by `comments` (no `Comment Text Char`, no `w:link`) open, and the comment looks like Word's own.
11. A comment whose text was edited by `comments` (date kept, last `paraId` kept, `textId` reset) opens and still threads.
12. A two-paragraph comment written by `comments`, keyed by its last paragraph, threads.
13. A deleted note in RP050 shape, as `comments` writes it (reference run in `w:del`, note paragraphs and mark deleted): accept removes the note, reject restores it, and no repair.

## 14. Risks

- **R1. Word is the only judge.** The shapes follow Word-written files, but whether Word accepts a reply, a resolved thread, a missing `dateUtc` and a new `commentsExtended` with no `commentsIds` is for the checklist above.
- **R2. Reply to a resolved thread.** Word may reopen the thread, and `comments` leaves it resolved with a warning. Checklist item 2 settles it.
- **R3. A tracked note insertion** (not implemented) rests on LibreOffice and one library. If Word also writes an inserted paragraph mark, the note insertion changes by one element.
- **R4. Comments left with an empty range after a deletion** stay, as the hoisting in `revisions` decides. A lawyer may expect Word's behaviour, which deletes the comment with its text. The alternative is for `reconcile` to remove a comment whose range emptied, which loses a review comment silently; this design keeps it and says so.
- **R5. Comment text is not tracked.** A lawyer who turns Track Changes on and edits a comment gets a tracked edit in Word. The `edit` of `comments` does not, by design. If Word's behaviour differs, `edit` is the only operation that has to change.
- **R6. A document where comments hang on a note or a text box** is read, and not written to, in this release.

## 15. Decisions

These five questions were settled in review of the first version of this document:

1. Notes in 1.0: text edits plus `delete-note` on the compiler's primitives. Insert-note is not implemented.
2. `edit-comment` and `reopen` (a flag of `resolve`) are in.
3. `ARCHITECTURE.md` rule 2 gets a sentence: the comment parts are written by `comments`, and the document's stories only by the compiler.
4. Existing extension parts are maintained, and only `commentsExtended` is created, when needed.
5. No `dateUtc` on new rows.
