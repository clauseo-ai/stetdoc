# Design: `package`, `xml` and the L1 corpus harness

Design notes for `package` and `xml`. `ARCHITECTURE.md` is the map and the module contracts in
`package/mod.rs` and `xml/mod.rs` are the interface; this document explains why they are shaped as
they are, and records the measurements behind the limits and budgets. Terms follow
`GLOSSARY.md`: package, part, entry, span tree, clean and dirty. "Byte span" means a range of
offsets into a buffer.

This document covers:

1. the byte-identity model for the ZIP layer, and what a change rewrites
2. the span tree: node model, dirty tracking, serializer, namespaces
3. encodings
4. limits and hostile input
5. how `open` stays within 300 ms
6. the L1 and L2 harness
7. decisions that changed `ARCHITECTURE.md`, and what is still open
8. measurements and corpus results
9. dependencies

## 1. The ZIP layer (`package`)

### 1.1 The model: entries as they lie, and the structure the other readers see

The input is kept as one immutable, shared buffer (`Bytes`). Nothing in it is ever rewritten in
place. `open` partitions the buffer and records, for each entry, the byte ranges that belong to
it.

```
[ prefix ][ entry 0 ][ entry 1 ] ... [ entry n-1 ][ central directory ][ ZIP64 record + locator ][ EOCD + comment ][ tail ]
```

The rule that shapes the model is that **a package says what the other readers of the same file
say**. Python's `zipfile`, `unzip` and `bsdtar` agree with each other on where everything is, and a
reader that goes through the file from the front (`bsdtar` from a pipe, a streaming unzipper) sees
it in another order. A file where those views differ shows one archive to some readers and another
to the rest, so the package either reads it as the seekable readers do or refuses it. Two LibreOffice
test files found this out: `two-zips.docx` and `zip64-eocd.docx` were each opened here as one
archive while Python, `unzip` and `bsdtar` read another, and a file written after an edit broke
them. The structure that follows is what that rule leaves.

- **prefix**: every byte before the first local header (a self-extractor stub, say). Copied
  verbatim. It cannot start with `PK`: a reader that goes through the file from the front takes
  whatever starts at byte 0 for the archive, so a prefix that is itself an archive (a file that is
  one archive followed by another, as `two-zips.docx` is) would be read as one thing there and as
  another by the readers that look for the end record.
- **entry i**: the bytes from the entry's local header up to the next entry's local header (entries
  ordered by offset, not by central directory order). The last entry ends where the central
  directory begins. An entry is its local header (with its own extra field), its compressed data
  and its data descriptor if it has one, and **nothing lies between two entries, or between the
  last entry and the central directory**. Bytes there belong to no entry. A seekable reader
  skips them and a reader that goes through the file in order reads whatever is in them, which can
  be a hidden `junk.xml` or a second `word/document.xml` (`inner-gap.docx`). So an archive with
  them is refused. Nothing is dropped on a rewrite, because there is nothing to drop.
- **central directory**: the central records in their original order. Each record's raw bytes
  (46 fixed bytes, name, extra field, comment) are kept as a range. It **ends where the end record
  begins, or where the ZIP64 end record begins when there is one**, and starts at that point minus
  its size. That is how the other readers find it. The offset stored in the end record is used for
  one thing, which is to learn how far the archive is shifted by bytes in front of it, and every
  entry's offset is moved by the same amount. Taking the stored offset at its word instead is how
  `two-zips.docx` was read as its first archive.
- **ZIP64 end record and locator**: the 56 and 20 bytes before the end record, when the locator is
  there. Copied verbatim except for the ZIP64 fields in section 1.3. A locator right before the end
  record makes the record part of the archive for the readers that look for it, and they take the
  counts, size and offset from it whether or not the end record's own fields are all ones, while
  `unzip` takes them from the end record when they are not. So the record has to be at the place
  every reader looks (the 56 bytes before the locator, where Python looks, and where the locator
  says, where `unzip` and `bsdtar` look), has to be 44 bytes long (extensible data is not
  supported), and has to agree with the end record in every field the end record fills in. A record
  that does not is refused, which is what `zip64-eocd.docx` is: a second directory and a second
  ZIP64 record hidden in the comment of its last central record.
- **EOCD + comment**: the 22-byte record plus the archive comment. The comment is
  preserved. The EOCD is found by scanning backwards for `PK\5\6`, as Python's `zipfile`,
  `unzip` and `bsdtar` do: **the last signature in the final 64 KiB and 22 bytes is the end
  record, and it is the only one tried.** So a fake signature inside the comment does win. If the
  last record does not describe an archive that reads (a directory that does not end where it
  begins, entries that overlap, a comment length that runs past the end of the file), the file is
  refused, and an earlier end record is not used instead. A LibreOffice test file (`overlap.docx`)
  holds a good archive followed by a second directory with its own end record: falling back to the
  first would open it as one archive where the other tools see, and refuse, the second. A fake end
  record behind a stub that says the archive is empty opens with no entries, which is what those
  tools show for the same file.
- **tail**: bytes after the comment. Copied verbatim.

The entry's data range comes from the central record (sizes are authoritative there),
located by reading the local header's name and extra lengths. The central and local
records are **never merged**: local extra fields and central extra fields differ in
practice (Office writes a `0xA220` padding extra only in local headers, and the local
and central time fields differ in some Word files), and each is carried as it was.

### 1.2 What `write` does

- **Nothing changed.** `write` returns the input bytes. This is the fast path of law L1.
- **Always the same machinery.** The rebuild path is also exercised with an empty edit
  set in every test (`write_rebuilt`), and must reproduce the input exactly. So L1 is
  proven for the code that L2 depends on, not only for the shortcut.
- **Rebuild.** Output is assembled in this order:
  1. the prefix, copied
  2. each entry in local-offset order. An untouched entry is copied as one slice:
     local header, compressed bytes and descriptor exactly as found. A replaced entry gets
     a new local header and data (section 1.3)
  3. new entries (from `add_part`), appended after the last entry
  4. the central directory, in the original order, with new entries at the end. Each
     record is copied verbatim, then **patched** only where a value changed
  5. the ZIP64 end record and locator if there are any, the EOCD (entry count, central directory
     size and offset patched, comment kept) and the tail

### 1.3 What changes when one entry changes

Replacing part P, with everything else untouched:

| Where | What changes |
|---|---|
| P's local header | New CRC-32, compressed size and uncompressed size. Name, version needed, time and date, the extra field and the general-purpose flags are kept, except flag bits 1 and 2 (compression level), which are cleared because the data is recompressed |
| P's data | Recompressed with the same method P used (deflate or stored). Deflate runs at a fixed level with a deterministic pure-Rust encoder, so the same input gives the same bytes on every platform |
| P's data descriptor | If P had bit 3 set, it keeps bit 3 and gets a descriptor in the same shape (with or without signature, 32-bit or 64-bit sizes). The local header then carries zero for CRC and sizes, as streaming writers do |
| P's central record | The same fields as the local header, plus the local header offset if P moved |
| Every later entry | Copied byte for byte. Only its **offset** moves |
| Later central records | Copied verbatim except the 4-byte local header offset (or the 8-byte value in the ZIP64 extra when the field is `0xFFFFFFFF`) |
| End record | Central directory offset and size, and the entry count if parts were added. A field that holds `0xFFFF` or `0xFFFFFFFF` stays as it is, because the ZIP64 record holds its value |
| ZIP64 end record | When there is one: the entry count (twice), the directory size and the directory offset, always, whether or not the end record's own fields are all ones |

The ZIP64 rule: whatever an entry used, it keeps. If the original local header had a
`0x0001` extra that holds both sizes (16 bytes), the replacement has one with the new 64-bit
sizes and sentinel `0xFFFFFFFF` fields; a shorter one is not treated as ZIP64, because writing
16 bytes into it would run over the field after it. If the central record had one, its fields are
patched in the order the format defines (uncompressed size, compressed size, offset, disk), only
for fields whose 32-bit value is a sentinel. A file that was not ZIP64 and would need to be (an
offset above 4 GiB or more than 65,535 entries) fails with `limit_exceeded`; the default limits
make that unreachable in practice.

**The ZIP64 end record is patched whenever the archive has one.** A reader that finds a ZIP64
locator trusts the record over the end record, so a record that keeps the old directory offset
makes the output unreadable to Python's `zipfile`, `unzip` and `bsdtar`, even though the end
record is right. Writers that use ZIP64 only for an entry that needs it leave real values in the
end record, so this is not a rare shape. The tests read what they write with a second reader that
follows those tools' rules (`package/reference_reader.rs`), and a check outside the repository
ran `unzip -t`, `bsdtar -tf` and `zipfile.testzip` on 208 rewritten archives (the builder's ZIP64
shapes and 200 corpus documents after a replacement and `add_part`) with no failure.

**Encrypted parts cannot be replaced.** An entry whose central or local flags have bit 0
(encryption), bit 6 (strong encryption) or bit 13 (masked local header) set is `unsupported` for
`replace_part`, as it is for `part`: writing plain bytes under its flags would make an archive no
reader can open. Its raw bytes are still copied when something else changes.

Other rules for the rewritten bytes:
- **Offset base.** Some archives have a prefix and store central-directory offsets
  relative to the first local header. `open` records the delta between stored and real
  offsets (the shift of the archive, which is also how it finds the central directory, section
  1.1) and applies it again when it writes, to the entry offsets, the directory offset and the
  offset in the ZIP64 locator.
- **New entries** (`add_part`). A new local header and central record are cloned from the
  **main document part's** records (found through the `officeDocument` relationship in
  `_rels/.rels`, falling back to `word/document.xml`). From the clone we take the version
  fields, time and date, host and external attributes. The name, sizes, CRC and offset
  are new, the extra field and comment are empty, and bit 11 (UTF-8 name) is set only
  for non-ASCII names. So a new entry's timestamp is copied, never read from a clock.
  The method is deflate.
- **`add_part` is all or nothing.** It builds the new part, the new content types, and the new
  relationships (a changed `.rels` part, or a new one) before it changes anything, and then
  applies them. A malformed `.rels` or a content type that contradicts an override is an error with
  the package as it was, not a package with a part but no relationship.
- **Relationship targets are URI references.** A target such as `media/image%201.png` names the
  part `media/image 1.png`. `resolve_target` decodes `%XX` (UTF-8), counts an escaped dot as a dot,
  drops a fragment, and refuses what cannot be a part name once decoded (an escaped slash or
  backslash, a control character, a bad escape, bytes that are not UTF-8). The target `add_part`
  writes for a new relationship encodes the same way.
- **Content types and relationships.** `add_part` also registers the part. Both parts are XML,
  so the edit goes through the span tree: one `Override` or `Relationship` element is appended as
  the root's last child, in the root's own namespace and prefix. Only the root's end tag and the
  new element are written, and the rest of the part keeps its bytes, prefixes, whitespace, BOM and
  encoding (a UTF-16 part stays UTF-16). The edit goes through `replace_part`, so the rules above
  apply to it. OPC parts have no element-order constraints, so none of this uses
  `schema::insert_in_order`. This is why `package` may depend on `xml`.
- **Entries that are not rewritten are not decompressed.** A replaced or added part never
  needs its old bytes inflated, and a 2 GB media part costs nothing at open or write.

### 1.4 Reading parts

`part(name)` is lazy: it inflates the entry, checks the size and CRC-32 against the central
record, and caches the result as shared `Bytes` for the life of the package (so a span tree and
the package share one buffer, with no copy).

- **One allocation.** The inflater writes straight into a buffer of the declared size, which then
  becomes the shared bytes. There is no growth and no second copy.
- **Lying sizes cannot make us allocate.** Deflate cannot expand by more than about 1032 to 1, so a
  declared size above `1032 * compressed + 64` is refused before anything is allocated. A stream
  that wants more output than declared, or ends short, is refused, and the CRC must match.
- **Ratio limit at read time.** For parts of at least 16 MiB, a ratio above `max_compression_ratio`
  is `limit_exceeded` when the part is read. It is not checked at `open`, because `open` inflates
  nothing, and a package must not be refused for an entry it never reads. The corpus has a real
  document with a 47 MB `docProps/app.xml` at 682 to 1 that must still open.
- A part replaced by `replace_part` returns the new bytes.

Failures are `malformed_input` (bad CRC, size mismatch, corrupt stream, data out of range),
`limit_exceeded` (ratio) or `unsupported` (encrypted entries, which is bit 0, 6 or 13 of either
flags field, and methods other than stored and deflate). None of them stops `open`, because an unread part of an unsupported kind is still
copied raw.

### 1.5 The entry-name and structure checks at `open`

`open` is the place where hostile structure is rejected, with `malformed_input` unless noted:

- not a ZIP (no EOCD), truncated central directory, multi-disk archive. The end record is the last
  `PK\5\6` in the file, and its comment has to fit inside the file
- a central directory that does not end where the end record (or the ZIP64 end record) begins, a
  ZIP64 locator whose record is not where every reader looks or disagrees with the end record, a
  ZIP64 record with extensible data (section 1.1)
- bytes that belong to no entry, between two entries or between the last one and the central
  directory, and bytes in front of the first entry that start with `PK` (section 1.1)
- an OLE compound file signature (`D0 CF 11 E0`) is `unsupported`, not `malformed_input`: it is
  either a password-protected `.docx` or a legacy `.doc`, and stetdoc reads only unencrypted
  `.docx`. The hint says to remove the password or save as `.docx` in Word. The OLE directory is
  not read to tell the two apart
- central directory or any entry offset outside the buffer, an entry count larger than the
  records present, compressed data running past the buffer
- two central records with the same local header offset, or entries whose byte ranges
  overlap (this is what makes overlapping-entry ZIP bombs fail)
- local header name different from the central directory name (the classic parser
  confusion attack), and likewise a local header whose compression method differs from the
  central record's, since a reader that takes the method from one and a reader that takes it from
  the other would inflate the entry differently (a LibreOffice test file, `dd-deflated.docx`, says
  stored in one and deflated in the other)
- duplicate names, including names that collide under ASCII case folding. OPC compares part
  names ignoring ASCII case, so `part`, `contains` and `replace_part` do too, and a new part cannot
  take a name an entry has in another case
- path traversal and unsafe names: `..` components, a leading `/`, a drive letter,
  backslashes, NUL or other control characters, names over 1,024 bytes, `.` components.
  Directory entries (a trailing `/`) are allowed. They are entries, copied like the others, but not
  parts: they are not listed, cannot be read or replaced, and a new part cannot take their name
- limits (section 4), which return `limit_exceeded`

## 2. The span tree (`xml`)

### 2.1 Why a hand-written parser

The tree is built by a small scanner in `xml/scan.rs` rather than `quick-xml`. Reasons:

- Every token must carry exact byte offsets, including whitespace inside tags, quote
  characters, and the text between attributes. A general reader gives events, and we would
  rebuild offsets from its buffer position.
- Limits (depth, attributes per element) and the DOCTYPE rule must be enforced inside the
  scan, not after it.
- BOM and UTF-16 handling (section 3) is outside what such a reader offers by default.
- No `unsafe`, no entity expansion code path at all, and one fewer dependency.

The scanner is iterative (an explicit stack), so depth never touches the Rust stack. The
serializer is iterative too. `memchr` finds the next `<`, `>` and quote.

### 2.2 Node model

One arena per tree, addressed by `NodeId(u32)` and stored in chunks (below). Node 0 is the **document
node**, whose children are everything in the file after the BOM: the XML declaration (a
processing-instruction node), whitespace, comments, processing instructions and the root
element. Every byte of the source belongs to exactly one node, so there are no hidden gaps.

A node stores:

| Field | Meaning |
|---|---|
| `kind` | document, element, text, CDATA, comment, processing instruction |
| `span` | for elements, from the `<` of the start tag to the `>` of the end tag (or of the self-closing tag); for the other kinds, the node's literal bytes |
| `open_end`, `close_start` | for elements, where the start tag ends and the end tag begins. For a self-closing element `close_start == span.end` |
| `attrs_end` | where the attribute area of the start tag ends, so the tail (`>`, ` >`, `/>`) can be reproduced |
| `name` | the qualified name span, and the prefix length |
| `ns` | an index into the tree's table of namespace URIs |
| attributes | a range in the attribute arena. Each attribute stores `full` (from the whitespace before the name to the closing quote), `value` (inside the quotes), the name span, the prefix length and the namespace index |
| links | parent, first child, last child, next sibling and previous sibling |
| `flags` | self-closing, new, tag-dirty, children-dirty, subtree-dirty, declares-namespaces, and name-not-chosen-yet (see section 2.5) |

Spans are `u32` offsets into a **virtual buffer** made of the source followed by an
append-only `added` buffer. Original content is a span below `source.len()`. Anything an
edit creates, such as new text, new attribute text or a new tag name, is appended to
`added` and addressed above that line. One `bytes(span)` accessor hides the difference,
so a moved clean node and a brand-new node serialize through the same path.

Memory is about 60 bytes per node plus 28 per attribute. The tree is `Clone`; the source is
shared by `Arc`, and so are the chunks of the arenas until a clone changes one. The namespace table
interns each URI once (a hash map from URI to a 16-bit index, so a part can name at most 65,535).

**The arenas are chunked.** The nodes and the attributes are stored in chunks (`xml/arena.rs`), 512
nodes (30 KB) or 4,096 attributes (115 KB) to a chunk, and an index is a chunk number and a position
in it. The parser fills chunks that it owns outright, and then puts each behind an `Arc` without
moving anything (`freeze`: the last chunk gives back the room it did not fill, so a small part is
not a chunk big). `SpanTree::clone` copies a pointer for each chunk, which is 1,800 of them for the
928,000 nodes of the densest input. An edit copies a chunk when it changes something in it
(`Arc::make_mut`): the node it changes, the elements above it that it marks, and the neighbours
that an insert or a detach relinks. A node is read before it is written, so a mark that is already
there copies nothing. The attributes of an element lie next to each other in one chunk, which is why
an element has at most 4,096 of them whatever `max_xml_attributes` says (65,534 before, the most a
16-bit count holds), and a list that does not fit in the room left in a chunk goes to the next one.

There are two reasons. A clone is cheap, which `state` needs to stage an edit without copying a
part: the densest input, cloned, with 50 paragraphs changed and written, held 97 MiB where two whole
arenas held 207. And a chunk is allocated whole and nothing is copied to make room, so a tree holds
what it uses and at most a chunk more. A `Vec` that grows by doubling ends with up to twice what it
needs, and a parsed part is never grown again: the arenas of the densest document were asked for
6.9 bytes for each byte of XML when 4.0 were in use. The room that was never written is not in the
resident size of a process, but the speed and memory benchmarks (`ARCHITECTURE.md#performance`) count the bytes the allocator is asked
for, and counted it: two of the five inputs were over the memory budget for a tree held while its
part is written (section 8, which also has where the bytes go).

**A `NodeId` is never reused or renumbered.** The arena only grows: `detach` unlinks a node and
leaves it, with everything under it, where it was in the arena, so its id and the ids of its
descendants still name them, and it can be inserted again. A node made later gets the next free
index, never one that was given out, and nothing compacts the arena. `SpanTree::clone` shares the
chunks of the arena, so a clone has the same ids for the same nodes, and from then on each of the two hands out
ids on its own (an id made after the clone means something in one tree only). `wordml` relies on
this to keep a handle to a paragraph across edits and across the copy `state` stages. A test
(`tests_nodes.rs`) detaches, inserts, clones and churns, and checks that no id comes back.

**Siblings are linked both ways.** A node knows its previous and its next sibling, so `detach` and
`insert_before` take the same time whatever the length of the list of children: 10,000 moves in a
list of 50,000 children took 4 to 8.7 seconds when each had to walk the list to find its
neighbour, and take milliseconds now (a test with a 5 second bound runs them in a debug build).

An edit gives an element a new list of attributes at the end of the arena, or in place of its old
list if that is already last, so changing one element over and over does not grow the arena. The
editor also keeps to the limits the part was parsed under (attributes per element, nodes plus
attributes, nesting depth, namespace declarations in scope), so a part the editor wrote can be
parsed again with the same limits. An insert that would nest elements deeper than `max_xml_depth`
is `limit_exceeded`, and nothing has changed: the depth at the parent plus the height of the
subtree is checked before the node is linked, where the parent is in the document or not (when it
is not, the check is a lower bound, because the subtree can only end up deeper than it is). A
200-deep copy put under the leaf of a 200-deep part used to succeed and write a part that could
not be parsed again.

### 2.3 Dirty tracking

Three flags on elements, and `subtree-dirty` on any node: **an edit replaces the span of a text node
and marks the node, and the serializer reads the span whatever the flag says**.

- `tag-dirty`: this element's own start tag has to be regenerated (an attribute was set
  or removed, a namespace was declared, or a self-closing element gained children).
- `children-dirty`: the child list changed (insert, remove, move).
- `subtree-dirty`: this element or something below it changed. It is set on the edited
  node and on every ancestor. The walk upwards stops at the first ancestor that already
  has it set, so marking is O(depth) once per path.

A node that has none of the flags is **clean**. A clean node serializes as one slice of the
source. A moved or cloned clean node stays clean and keeps its original bytes. It is the
parent that becomes dirty.

`NodeRef::is_dirty` reports `subtree-dirty`, and it means that what is written for the node can
differ from its source bytes. That includes a text node that an edit made meet another text node
(`detach` of what lay between two, or an insert of text beside text), because the writer joins text
that meets and can write either of the two differently (section 2.6). Both nodes are marked when
they meet, so **a node that is not dirty is written as it was read**, with no exception for the
join. The marking is free: the two nodes are the ones the edit relinks, whose chunk it copies anyway.
The testkit's fuzz law, that every clean node of an edited tree is in the part it wrote byte
for byte, used to allow a text node beside text to be written in the joined forms; it no longer
needs to, and the model test of text edits (`tests_joins.rs`) holds the strict form after every
step.

**The edit count.** The flags say that a tree has changed, and never say that it has changed
again, because nothing clears them. A cache that was built from a tree needs to know that the tree
is still the one it was built from, so `SpanTree::edit_count` moves when the editor changes the tree.
It moves in `touch` (every edit that changes a node, and the marking it makes) and in `push` (every
node an edit creates), which are the two places an edit passes through, and nowhere else. It counts
marks and not calls: `set_text` marks two nodes, and a `detach` between two text nodes three, so a
caller compares it for equality and never adds calls up. A call that changes nothing does not move it:
a refused edit, a `remove_attr` of an attribute that is not there, a `detach` of a node that is already
detached, a `bind_namespace` that finds the binding. A read through an `Editor` does not move it
either. Creating a node moves it though no document changed, which is harmless for a cache. A clone
starts at the count of the tree it was cloned from, and counts on its own after. It says that a tree
has changed since a count was read, and never that two trees are equal: two clones of one tree that
each make one different edit have the same count and different content. So `state` makes the pair
(epoch, edit count) the generation of a part, and `wordml`'s `Story` records the count of the tree it
was scanned from. Only the `Editor` methods promise any of this: `mark_all_tags_dirty`, which test and
fuzz builds use, changes bytes without moving the count.

### 2.4 The serializer

`serialize` writes into one `Vec<u8>` using an explicit stack.

- A tree with no dirty node returns the original buffer (for a UTF-16 part, the original
  UTF-16 bytes).
- For a **clean** element the output is `source[span]`: a single copy.
- For a **dirty** element:
  - **Start tag.** If it is not tag-dirty, the original bytes `source[span.start..open_end]`
    are copied. If it is, we write `<`, the original name bytes, then each current
    attribute in order (original attributes as their `full` bytes, so quote style, spacing
    and entity references stay; a changed attribute keeps its name, spacing and quote and
    gets a new escaped value; a new attribute is ` name="value"`), then the original tail.
    A self-closing tail that now has children becomes `>` with its inner whitespace kept.
  - **Children.** Each child is serialized by the same rule, in order. There is no text
    "between" children, because whitespace is a text node. This is how whitespace and
    mixed content are preserved.
  - **End tag.** The original end tag bytes (`</w:p >` stays `</w:p >`). A formerly
    self-closing element that gained children gets `</name>`. An element that lost all
    children stays paired, so `<a>x</a>` becomes `<a></a>`, never `<a/>`.
- A **new** element has no source bytes. It writes `<name`, its attributes, then `/>` if it
  has no children, or `>`, children and `</name>`.
- **Escaping of new content.** Text escapes `&`, `<`, `>` and `\r` (as `&#13;`, so it
  survives a parse). Attribute values escape `&`, `<`, `>`, the quote in use, and tab,
  newline and carriage return as character references. Non-ASCII characters stay literal
  UTF-8. New attributes are double-quoted. **A character XML cannot carry is refused**, whether
  written literally or as a reference: U+0000 to U+0008, U+000B, U+000C, U+000E to U+001F, U+FFFE and
  U+FFFF are `invalid_payload`, because Word does not open a part that holds one. The escape
  functions return a `Result` for that, and a refused edit changes nothing. The names the editor
  creates (elements, attributes, prefixes) must be XML names of the fifth edition, so a name with
  a space or a symbol is refused too.

The unit test that carries most of the weight is the **all-dirty round trip**: mark every
element tag-dirty, serialize, and require the exact input. It proves that the dirty
path reproduces whitespace, quotes, entity references and tails byte for byte, for every
document, and not only that the clean shortcut works.

### 2.5 Namespaces

**Reading.** Names are resolved while parsing, with one stack of bindings for each prefix and a
single undo list. A start tag pushes what it declares, and the end tag pops exactly that, so a scope
costs time in proportion to what it declares and a lookup costs one hash probe, whatever is in
scope. (Scanning the whole scope for every name instead makes a part of a few thousand
declarations took minutes. Section 8 has the numbers.) URIs are interned through a hash map, and a
four-entry cache of recent prefix lookups, valid until a binding changes, keeps the usual case
(one declaration site, a handful of prefixes) as fast as before. An unbound prefix is
`malformed_input`. Prefixes, declaration order and where declarations sit are never touched,
because they are just bytes in clean tags.

**New names.** For a new element or attribute the caller names a namespace URI and a local name,
and the tree chooses the prefix **when the node is inserted**, because which prefix is right depends
on what is in scope where the node ends up. Until then `NodeRef::name` is the local name.

1. Walk up from the insertion point, which counts when it is an element, and take the nearest
   declaration of the URI whose prefix is not declared again, to something else, between that
   declaration and the insertion point. A default-namespace match gives an element a bare name.
   Attributes never use the default namespace. The `xml` prefix needs no declaration.
2. If nothing is in scope, declare the namespace on the root, as Word does, under a prefix that
   no declaration between the root and the insertion point uses, so the root's declaration reaches
   the insertion point. If the root already binds the URI but a closer declaration hides that, declare
   it on the new node itself under a fresh prefix, which hides nothing.
3. A namespace the caller wants ignorable, or under a particular prefix, is declared first with
   `bind_namespace(uri, hint, ignorable)`. That is for what the caller has to decide: the tree does
   not know which namespaces are extension namespaces, because `schema` owns that. With
   `ignorable` the prefix is appended to `mc:Ignorable`, and `mc` itself is declared if missing.

A subtree built while detached is resolved when it is inserted into the document, as a whole, so a
new element under a new parent is chosen against the scope of its final position. An element with
no namespace is refused where a default namespace is in scope. A refused or failed insert leaves the
node detached.

**L2 and the root.** Declaring a namespace on the root changes the root's start tag. Law L2 says
what an edit may change: the bytes of the edited elements only, except that the part's root start
tag may gain the namespace declarations and `mc:Ignorable` tokens that new content needs. Nothing
else outside the edited elements changes, and a declaration is added after the last existing
one, so the rest of the tag keeps its bytes.

**Moving nodes.** A node whose names are already written keeps its bytes when it moves. That is
safe when every prefix it uses means the same at the new position, which is always true when only
the root declares namespaces (all Word output). The tree records at parse time whether any element
below the root declares one, and when one does, an insert checks every written name in the
inserted subtree against the declarations inside the subtree and above the new parent. Where a
prefix, or the default namespace, would mean something else, the namespace it had is declared on the
highest element of the subtree that writes it (`xmlns:q="..."`, or `xmlns="..."` for the default
namespace), so that element and everything under it read as before. Only a prefix that cannot be
declared (`xml` or `xmlns` bound to something else, or one element needing a prefix for two
namespaces) is refused with `unsupported`, and then nothing is declared. The check follows
declarations that the subtree carries with it, so a subtree that declares its own prefixes needs no
more, and it runs on the whole subtree when it is finally inserted into the document, whether or not
it was built inside a new element first. Accepting a merge in a document that binds one prefix to
different namespaces in sibling paragraphs is the case that needs it.

### 2.6 Text

Text nodes keep raw bytes, with entity references undecoded. `text()` decodes on demand
(five predefined entities and numeric references; a named entity other than the five is
rejected at parse, because DTDs are rejected, so no other entity can be defined). It reports text
as an XML parser does: a carriage return line feed pair and a lone carriage return are each one
line feed, in text, CDATA and comments, but a character written as a reference (`&#13;`) stays what
it is. An attribute value is read the same way, and a tab, line feed or carriage return written
literally in it is a space. Text that the editor writes escapes these characters as references so
that they survive a read. `replace_text_range` counts positions in the characters `text()`
reports.
`set_text` replaces a whole node. `replace_text_range` splices a range of decoded
characters, and keeps the raw bytes (including `&#xA0;` and friends) outside the range, so
an edit inside one `w:t` does not change how its other characters are written.

**Text that touches text.** The bytes of two pieces of text that each read right can read
differently once they touch, and an edit makes them touch: removing the element between two text
nodes (`x\r`, `<b/>`, `\ny`), putting text beside text, or a splice that keeps the bytes on both
sides of a range it removes. A carriage return that ends the first and a line feed that starts the
second read as one line break, where the tree has two, because a carriage return by itself reads as
a line feed. The serializer writes the carriage return, and every one right before it, as the line
feed it reads as, so the tree's text and the text of the part it writes are the same. Both nodes
are dirty from the moment they meet (section 2.3), so this is a change to nodes that the edit
marked and law L2 allows it, and `text()` of the earlier node does not change. The same join writes the `>` of the later node as `&gt;`
when the earlier one ends in `]]`, because `]]>` is not allowed in content. `replace_text_range`
joins what it keeps and what it adds the same way (`text::join_bytes` does both). Writing a
carriage return as a reference (`&#13;`) instead would have kept it as a carriage return, which is
another character than the line feed the tree reads.

**`]]>` in text is refused.** XML allows `]]>` only to end a CDATA section, and a `>` that follows
`]]` in content has to be written `&gt;`. The parser used to read it and now refuses it
(`malformed_input`, reason `xml.cdata_end_in_text`), as the independent parser the model test uses
does. A search of 51,831 documents that open (every XML part of the corpora and the generated cases)
found none with `]]>` in a text node, so the refusal costs no document, and it keeps the two sides
of the join honest: no text node has the three characters, so no edit makes them meet by accident
and the join's `&gt;` is the only way they are ever written. The check is a search for `>` in each
text node, which most do not have.

**A document stays a document.** What stands outside the root element is limited, because a part
that breaks these rules does not parse. A text node that is a child of the document node can hold
only white space, written as itself (a reference is not allowed there either, so a carriage return
is written as it is and reads as a line feed): `set_text` and `replace_text_range` return
`invalid_payload` for anything else, and so does an insert of a text node that is not white space
under the document, whether it was made or copied from inside an element. A CDATA section cannot go
there. The XML declaration cannot be inserted anywhere, and nothing can be inserted before it, because
it is only allowed at the very start. The root element cannot be detached, because a document has
exactly one and an insert refuses a second. Comments and processing instructions can go anywhere. A
test (`tests_joins.rs`) edits random documents full of carriage returns, line feeds, `]]` and `>`
by detaching, moving, copying, inserting and splicing, and after every edit that is accepted,
parses what `serialize` returns and requires the same text and the same elements as the tree has.
It found the rule about the XML declaration, the one about what can go before it, and a case of
two carriage returns before a line feed.

## 3. Encodings

- **UTF-8 without BOM**: the common case. The source buffer is validated as UTF-8 once, so
  text accessors can return `&str` with no per-call check and no `unsafe`.
- **UTF-8 BOM** (`EF BB BF`): the BOM is not a node. The tree records its length, and the
  serializer writes the same bytes first.
- **UTF-16**: detected from a BOM (`FF FE`, `FE FF`) or from the first four bytes of `<?`
  (`3C 00 3F 00`, `00 3C 00 3F`). Without a BOM, the part must begin with an XML declaration
  that names UTF-16 (XML 1.0 section 4.3.3 requires a BOM otherwise); a processing instruction
  such as `<?xmen?>` is not enough, since an edit can remove it and the written part would no
  longer be detected as UTF-16. Anything else is `malformed_input`. The part is transcoded once to UTF-8, which becomes the
  tree's source, and the original bytes are kept. If nothing is dirty, `serialize` returns the
  original bytes. If something is, the UTF-8 result is transcoded back, with the original
  byte order and with a BOM if the original had one. UTF-16 and UTF-8 convert without loss
  for valid text, so every clean span comes back as the same code units. A lone surrogate or
  an odd byte count is `malformed_input`.
- **Declared encoding**: must agree with the detected one. UTF-8 bytes accept no declaration
  or `UTF-8`; UTF-16 bytes accept none, `UTF-16`, `UTF-16LE` or `UTF-16BE` matching the byte
  order. `us-ascii` is accepted when every byte is ASCII (a real producer writes it), and then
  edits write non-ASCII characters as character references so the part stays ASCII. Anything
  else, such as `windows-1252`, is `malformed_input`, since OPC allows only UTF-8 and UTF-16.
  UTF-32 byte marks are `malformed_input`.
- **Line endings** are text bytes and are never normalised in the tree or on write. A `\r\n` after
  the declaration is a whitespace text node that is copied. What `text()` reports is normalised
  (section 2.6).

## 4. Limits and hostile input

`Limits` (in `api`) holds the bounds below. `OpenOptions` carries it. All are checked before
the allocation they guard.

| Limit | Default | Checked |
|---|---|---|
| `max_archive_bytes` | 512 MiB | before anything is read |
| `max_entries` | 10,000 | from the EOCD count, before the central directory is read |
| `max_entry_bytes` | 256 MiB | declared uncompressed size of each entry, at open |
| `max_total_bytes` | 1 GiB | sum of declared uncompressed sizes, at open |
| `max_compression_ratio` | 500 | per part, when it is read, for parts of at least 16 MiB |
| `max_xml_depth` | 256 | during parse (see below) |
| `max_xml_attributes` | 1,024 | per element, during parse, and never more than 4,096 whatever the limit (section 2.2) |
| `max_xml_namespaces_in_scope` | 1,024 | namespace declarations in scope at one element, during parse (see below) |
| `max_xml_nodes` | 5,000,000 | nodes plus attributes in one part, during parse (see below) |

**The node default is 5,000,000, from the corpus and from memory.** The span tree takes about 60
bytes for each node or attribute, however few input bytes it came from, so without a bound a 256 MiB
part of `<a/>` would ask for several gigabytes. The largest real part in the corpus has 876,891 nodes
and attributes (a 12.6 MB `document.xml`), so 5,000,000 is almost six times that, and caps the arena
at about 300 MB.

**The default of 1,024 declarations in scope is from the corpus too.** The count is every
declaration on an element and its ancestors, including one that a closer declaration of the same
prefix hides, because that is what a lookup in the editor walks past. Over the corpus a good
document has at most 74 in scope at once and 99 percent have at most 35. Parsing is linear whatever
the count (section 2.5), but the editor's lookups are not: with 250,000 declarations in scope an
insert took 33 ms and 200 `set_attr` calls took 3.9 seconds. At the limit a lookup walks at most
1,024 declarations and the plain attributes of the elements that carry them. The editor counts
the declarations it adds against the same limit, as an upper bound (one more than the most there
were), so that what it writes can be parsed again. An insert counts what the subtree
brings with it as well: the declarations in scope at the new parent, plus the most that are in scope
at an element of the subtree when only the subtree's own declarations count (a copy of an element
that declares namespaces, or a node moved from somewhere else, brings its declarations along). If
that is over the limit the insert is `limit_exceeded` and nothing changes. Only `declare_on` used
to count, so copies put inside one another reached 256 levels of 1,024 declarations, which is the
slowdown the limit is there to prevent. Where the parent is not in the document yet, the count
waits for the moment the whole subtree is put in, when it is first measured against what is above
it.

**The depth default is 256, from the corpus.** Over 51,784 documents (every XML part of every
document in the local corpora), the deepest element nesting in a valid document is 69 levels (a
nested-table document), the next are 65, 53 and 45, and 99.99 percent are at most 45. One document
nests 15,005 levels (POI's `deep-table-cell.docx`, a denial-of-service test). 256 is 3.7 times the
deepest real document and 17 times shallower than the attack. The limit exists for the code that
runs on the tree after parsing, which is written with explicit stacks too, but a depth bound still
keeps work and memory per path small. **Nothing recurses on input depth.** The scanner, the
serializer, the editor's subtree walks, text extraction and the node arena (chunks of nodes, so drop
is not recursive) are all iterative. A unit test parses a document 150,000 levels deep at a raised
limit, serializes it, clones it, edits it and drops it in a debug build on a 2 MiB test stack, which
a recursive parser would overflow.

Inflation is bounded by the **declared** size, not trusted from it (section 1.4): a stream that
wants more is `malformed_input`, a stream that ends short is `malformed_input`, the CRC must
match, and a declared size that deflate could not produce from the compressed size is refused
before it is allocated.

XML: any DOCTYPE is rejected with `malformed_input`, with no attempt to read its entity
declarations, so entity expansion and external entities cannot happen. So is a character that XML
1.0 does not allow anywhere in the part (a control character other than tab, line feed and carriage
return, written literally, and U+FFFE and U+FFFF), found by one pass over the part, and an XML
declaration whose version is not `1.0`: XML 1.1 allows control characters that 1.0 does not, and
Word refuses both. A part the editor wrote can always be parsed again, because the editor refuses
the same characters. A named entity other
than the five predefined ones is rejected. Duplicate attributes (the same name, or two prefixes
for one namespace with the same local name), mismatched tags, text outside the root, an
unterminated construct, a `<` inside an attribute value, `]]>` in text and an unbound namespace
prefix are `malformed_input`.

Failures are `StetError` values with a stable `ErrorCode`, a message and a `next` hint. Every XML
message starts with the name of the part (`SpanTree::parse` takes it), such as
`word/document.xml: malformed XML at byte 8131: end tag </w:t> does not match <w:p>`, and an
offset in a UTF-16 part is an offset in the UTF-8 form the parser works on, which the message says:
`... at byte 7 of the UTF-8 form of this UTF-16 part`. Container messages name the entry. Nothing in either module panics: no `unwrap`, `expect`,
indexing that can go out of range (reads go through checked accessors) or arithmetic that
can overflow (`checked_*` on every offset computed from input).

## 5. Staying within the 300 ms `open` budget

`Package::open` does no decompression and no XML parsing. It reads the end record and walks the
central directory, then reads each local header (30 bytes each) to verify names and place the data.
Cost is linear in the number of entries, not in the size of the file: 0.05 to 0.3 ms for the
documents below, and 75 microseconds at the 99th percentile over 42,009 LAB documents.

The rest of `Document::open` is the block scan of `word/document.xml`, and that is where the budget
goes. What the scan costs is in section 8. The design choices that keep it small:

- **Inflate once, into the final buffer.** The decoder writes into one allocation of the declared
  size, which becomes the shared buffer. There is no growth and no copy.
- **One pass over the bytes.** `memchr` finds the next `<`, `>` and quote. Names are spans, not
  strings. There is one arena for nodes and one for attributes, sized from the input length.
- **No work for what is not read.** Other parts (styles, numbering, headers, comments) are inflated
  and parsed only when asked for. Media parts are never inflated unless a caller asks.
- **Namespace resolution** is one hash probe for each prefixed name, and usually a hit in a cache
  of the last four answers (section 2.5). It does not depend on how many declarations are in scope.

Peak memory is the one budget this design cannot meet by cleverness. XML compresses roughly 10 to
1 for Word output and 30 to 40 to 1 for python-docx output, so the inflated main part is already
about 11 times the file for the 872-page handbook below, before any tree exists, and the tree adds
about as much again. Section 7 asks for the budget to say what it measures.

## 6. The harness

The container and span tree laws are tests of `package` and `xml` inside the core:
`package/corpus_laws.rs` and `xml/corpus_laws.rs`. The repository commits no third-party documents.
`cargo xtask corpus fetch` downloads the curated corpus by pinned hash into a cache,
and the tests read it from the directories in `STETDOC_CORPUS_DIR` (separated by `:`; `sets/curated`
is used when present, or `sets/<name>` from `STETDOC_CORPUS_SET`, else the directory itself is
walked). With the variable unset they report a skip and pass. The corpus is part of
`cargo xtask check`, which always sets it, so the skip is not a way out of CI. The files are
listed in `xtask/core-io-exceptions.txt` because they read files. `xml` may not depend on `package`,
so its test reads the parts of each document with a small reader of its own (it follows ZIP64
records), which also makes it an independent second reader of the container.

**A named corpus has to be whole, or the tests fail.** Each directory needs the `COMPLETE` marker
that the fetch writes last, and at least one document. A variable that is set but names nothing, a
file that cannot be read, and a mismatch between the files and `INDEX.tsv` (a document the index
does not list, or a listed file that is missing) are failures. Only the variable being unset skips.

**The index.** `INDEX.tsv` is read by column name (`path`, `expect`, and `stage` and `part` if there
are any), so extra columns and their order do not matter. A document it expects to be refused has
to be refused as it says, and **not earlier than it says**: a document that `open` refuses while the
index puts the refusal at `read` used to pass because the code matched, and now fails, because the
index is then wrong about where this layer meets it.

| `stage` | The container law requires |
|---|---|
| `open` | `Package::open` returns exactly the code in `expect` |
| `read` | `open` succeeds and a part is refused with that code when it is read; one of the parts `part` names, if it names any |
| `xml` | `open` succeeds and an XML part is refused with that code when it is parsed; one of the parts `part` names, if it names any, and it reads; the span tree law needs the same of a part it parses |
| `document` | `open` succeeds, and the document gets the laws of a good one: the refusal is from layers above |
| none | any of `open`, `read` and `xml` |

The container law reads every part, so it meets every refusal at `read` and `xml` wherever it is.
A layer that reads only some parts meets a refusal only if it reads one of the named parts, and
sees an `ok` document otherwise (`Expect::seen_by_reading` in the testkit). The span tree law reads
the parts named `.xml` and `.rels`, and is not asked for a refusal in a part it does not parse.

Without an index, a document under a directory named `hostile` has to fail cleanly: an error from
`open`, or a document that opens and whose parts each read or return an error, and never a panic.
Every other document, and every one the index expects to be `ok`, runs:

| Check | What it proves |
|---|---|
| `write` equals the input (byte comparison, which is stronger than comparing SHA-256) | the shortcut |
| `write_rebuilt` equals the input | entry copy, central directory and end-record patching with nothing changed |
| every XML part and every part up to 256 KiB, replaced with its own bytes, then `write`: the result opens, those parts read back the same, every other entry is raw-identical, every other central record differs at most in its offset field | the replacement path, and locality for it |
| the first `w:t` of the main part changed by editing its bytes, then `replace_part` and `write`: the same locality | the one-edit L2 smoke |
| the other reader (`reference_reader`, which follows Python's `zipfile` and shares no code with the crate's reader) reads the input, the archive written after replacing every part, and the archive written after one edit, and finds the same parts in the same order with the same sizes and CRC-32 values as this crate does; after the edit, only the main part differs | a package that opens here and reads differently there is one that Word, `unzip` or `bsdtar` could show something else, and an edit of it could leave a file they refuse |
| every XML part that parses: `serialize` equals the input, and still does after every start tag is marked dirty | the clean path and the dirty path |
| every XML part that does not parse is refused with `malformed_input` or `limit_exceeded`, in a message that starts with the part's name | no other code, no panic |
| for a good document outside the hostile tier, the main part (the target of the `officeDocument` relationship, else `word/document.xml`) is found, read and parses | a document cannot pass the span tree law by being one the test cannot read |

The unit tests build the ZIP structures themselves (`package/testzip.rs`, a raw writer that controls
every field): data descriptors in all four shapes, ZIP64 in entries and end records, comments,
prefixes with absolute and relative offsets, central order differing from local order, extra
fields that differ between the local and central record, and hostile structure. What `write` produces is
read back with a second reader written to Python's `zipfile` rules (`package/reference_reader.rs`),
which is how a stale ZIP64 record was found: this crate's own reader ignored the disagreement. The
same reader is the oracle of a sweep (`package/tests_locate.rs`): 4,000 archives, made by mutating
the end record, the ZIP64 record and locator, the central records, and by inserting, deleting and
appending bytes at every boundary and appending a second archive or a fake record, have to be
read to the same entries by both readers whenever this crate opens them, and an edit of them has to
leave a file the reference reader reads. The sweep fails on the code that read the stored offset at
its word, and on the code that skipped an end record whose comment ran past the end of the file;
the corpus laws did not, because they only reopened what they wrote with this crate's own reader,
which is why both shapes reached review. Two other sweeps, one for archives and one for XML parts,
truncate at every length and flip bytes; they must never panic, and a tree that parses must still
round trip. The sweep found one arithmetic overflow, now fixed.

The cross-module L2 (edit through the span tree, write through the package) belongs to `Document`
and is checked through it by the testkit's laws (`stetdoc-testkit/src/laws/locality.rs`). Before `Document` existed, a
throwaway run (not in the repository) did it by hand on 44,293 documents with 0 violations.

## 7. Decisions that changed `ARCHITECTURE.md`, and what is open

Settled in review, and applied in the same change as the code:

1. **`package` may depend on `xml`.** Content types and relationships are XML, and editing them
   through the span tree preserves their bytes and handles UTF-16 for free. The Allowed dependencies
   table says `api`, `xml` for `package`, and stays acyclic.
2. **`Limits` is in `api`**, because `OpenOptions` carries it. The shared byte buffer, `Bytes`, is
   not in `api`: it lives in `xml`, which holds the source of a part, and `package` hands the same
   buffer to the tree. It wraps `Arc<Vec<u8>>`, so a `Vec<u8>` becomes shared bytes without copying.
3. **The memory budget is relative to what is inflated and parsed**: 4 MiB plus the file size plus 8
   times the uncompressed size of the XML parts parsed (section 8). `ARCHITECTURE.md`'s Performance
   table says so.
4. **Password-protected documents are `unsupported`**, with the message and hint in section 1.5.
5. **Strictness**, each refusal accepted: a bare `&` is `malformed_input` when the part is parsed;
   `./` entry names and names that differ only in ASCII case are `malformed_input` at `open` (a part
   visible under two names would let stetdoc and Word disagree about which is the document); depth
   above 256 is `limit_exceeded`; a bad CRC is `malformed_input` when the part is read, and untouched
   entries are still copied raw.
6. **The compression-ratio limit is checked when a part is read**, not at `open`. Absolute caps on
   one part and on the total stay at `open`.

Settled in review, and applied with the code that follows:

7. **New names get their prefix at insertion** (section 2.5), and law L2 says the root start tag may
   gain the namespace declarations and `mc:Ignorable` tokens that new content needs. The wording is
   in `GLOSSARY.md` and the `xml` contract.
8. **`xml` refuses what XML cannot carry**: the escape functions return a `Result`, and a forbidden
   character is `invalid_payload`. A namespace declaration cannot be removed.
9. **A dependency may use `unsafe` inside** if it is widely used, maintained and fuzzed, as `memchr`
   is, which settles the question this document used to leave open. `ARCHITECTURE.md` rule 6 says so.
   Each new dependency is justified in the change that adds it and passes `cargo deny`.
10. **Relationship data reaches `wordml` through `state`**, which depends on `package` and `wordml`
    and passes plain data down. `wordml` never names `package`.

Settled in the second review, and applied with the code that follows:

11. **The container is read as the other readers read it** (section 1.1): the central directory ends
    where the end record or the ZIP64 record begins, the stored offset only gives the shift, a ZIP64
    locator needs a record that agrees, the last signature is the end record and its comment fits,
    entries lie end to end, and a prefix does not start with `PK`. Each rule refuses something that
    the other readers read in more than one way. Across the 48,666 documents of the manifest that
    can be fetched it refuses three, all of them LibreOffice test files built to be ambiguous.
12. **A previous-sibling link** (4 bytes a node), so moves are constant time (section 2.2).
13. **`max_xml_namespaces_in_scope`**, 1,024, and a depth check on every insert (sections 2.2 and 4).
14. **A `NodeId` is never reused or renumbered**, and a clone of a tree keeps the ids (section 2.2),
    as `wordml` relies on.

Still open:

- **Part names compare ignoring ASCII case only**, as OPC says. A package with two names that differ
  only in a non-ASCII case (`é` and `É`) opens, and its parts are separate.

## 8. Measurements

Release build, Apple M-series laptop, other work running, median of 9 runs, measured with a throwaway benchmark that is not in the repository. All times in milliseconds.

| Document | docx | main part | blocks | open | inflate | parse | block scan | edit one run and serialize | replace and write (recompress) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| unstructured `handbook-872p.docx` (872 pages) | 483 KB | 5.3 MB | 8,154 | 0.21 | 8.0 | 16.0 | 0.2 | 0.7 | 32 |
| eu-ai-act proposal (Oct 2022) | 3.5 MB | 5.2 MB | 3,214 | 0.30 | 6.9 | 17.3 | 0.1 | 0.7 | 38 |
| POI `bug65649.docx` | 475 KB | 12.6 MB | 15,956 | 0.05 | 11.3 | 35.4 | 4.6 | 1.2 | 39 |
| LAB `alvarez-opt-in-consent-record-index` (largest LAB) | 124 KB | 4.5 MB | n/a | 0.05 | 3.8 | 12.7 | 1.9 | 0.5 | 13 |
| docxodus `WC-BodyBookmarks-Before.docx` | 1.9 MB | 2.8 MB | 3,112 | 0.08 | 6.0 | 7.5 | 0.1 | 0.3 | 41 |

- **Open budget.** Container open, inflate and parse together are about 24 ms for the 872-page
  handbook and 47 ms for the 12.6 MB main part. A 300-page contract is a third of the handbook.
  The 300 ms budget has a margin of six to twelve.
- **Save budget.** Writing an unchanged package is a copy (0.01 ms for most, 0.07 ms for a 3.5 MB
  file). Writing after one edit recompresses the main part and costs 13 to 41 ms, so one edit and
  one save of the largest documents is well inside the 1 s budget.
- **Peak memory** (open, inflate and parse the main part, nothing else, measured as the peak
  resident size of a process that does only that, with vector arenas and then with chunked ones):
  22.4 and 21.6 MB for the 483 KB handbook, 46.5 and 55.0 MB for the 475 KB POI `bug65649.docx`,
  12.3 and 8.8 MB for POI `bug59058.docx`. A process that opens a 573-byte document peaks at 3.3 MB.
  The inflated main part alone is 10.9 and 26.5 times the file for the first two, so a budget of 10
  times the file cannot hold for well-compressed XML. The budget is therefore 4 MiB + the file size +
  8 times the uncompressed size of the XML parts parsed. Against it the tightest case,
  `bug59058.docx` (a 1.35 MB main part with 40,000 nodes), used 81% with vector arenas and uses 58%
  with chunked ones, the handbook used 48% and uses 46%, and `bug65649.docx` used 44% and uses 52%.
  The chunks are why the small part got smaller: a vector of nodes was started at a guess that is
  too big for it, and the last chunk gives back what it did not fill. The large part reads higher
  because the resident size of a process counts the pages that a vector moved by `realloc` only
  when they are written again (see the next subsection), so the earlier figure was low, and the
  chunked one is what the tree holds. Over 72
  random documents of the corpus whose main part is at least 300 KB, the worst was 78% of the budget
  and is 70%, and the median was 73% and is 64%. The sibling link adds 4 bytes to each node of 56: the
  same three documents, measured in the same test binary with the code from before it, used 21.8,
  45.1 and 12.3 MB, so the link costs 2.0%, 2.9% and nothing.
- **Throughput.** Parse runs at 330 to 440 MB/s of XML including node construction, inflate at
  450 to 650 MB/s, `serialize` of an unedited tree is a copy.

After the namespace changes, parse medians are 13.9 ms for the handbook and 37.4 ms for POI
`bug65649.docx`, against 16.0 and 35.4 above: the same within noise. With the sibling link and the
checks of the second review, the same two parts parse in 14.7 and 40.2 ms in a session where the
code from before them took 15.0 and 39.5 ms.

### Memory of the span tree

The speed and memory benchmarks (`cargo xtask bench`, release build, Apple M-series) count the bytes that
the allocator is asked for at the highest point of a run, which is the same on every run. The peak
of a benchmark is the part it was parsed from, one tree, and whatever the run adds. The budget is 4
MiB + the file + 8 times the XML. MiB, before the chunked arenas and after:

| Input (main part) | Budget | Parse | Write after one edit | Clone, change 50 paragraphs, write |
|---|---:|---:|---:|---:|
| `contract-300p` (3.3) | 30.8 | 16.3 to 11.6 | 19.6 to 14.8 | 27.8 to 16.6 |
| `dense-contract-300p` (7.1) | 61.6 | 56.4 to 36.0 | **63.7** to 43.2 | 92.5 to 44.9 |
| `worst-blocks-12mb` (10.8) | 90.2 | 32.0 to 23.4 | 42.9 to 34.2 | 55.4 to 35.8 |
| `node-dense-18mb` (17.2) | 141.5 | 128.3 to 78.5 | **145.7** to 95.9 | 206.9 to 97.4 |
| `fields-13mb` (12.4) | 103.3 | 55.3 to 51.5 | 68.0 to 64.1 | 106.9 to 65.6 |

The two figures in bold were over the budget, and the benchmarks of the writer on them were known
gaps. The nodes and attributes were vectors that grow by doubling, and a vector that has stopped
growing holds up to twice what it uses: the arenas of `dense-contract-300p` held 6.9 bytes for each
byte of XML when 4.0 were in use. With chunks the room that is asked for is the room that is used.

**Where the bytes go** when a tree is held and its part is written (MiB), on the two densest inputs,
after the change:

| | `dense-contract-300p` | `node-dense-18mb` |
|---|---:|---:|
| the part it was parsed from | 7.1 | 17.2 |
| nodes, 60 bytes each | 24.3 (424,971) | 53.1 (928,207) |
| attributes, 28 bytes each | 4.5 (166,580) | 8.0 (300,301) |
| the part that is written | 7.1 | 17.2 |
| **together** | **43.0**, 6.05 times the XML | **95.6**, 5.56 times the XML |
| budget | 61.6, 70% used | 141.5, 68% used |

The nodes are 56% of the first column and the tree 4.05 times the XML, so the options were these.
Fewer bytes for each node would do the most, but an element, nine nodes in ten on the contracts,
uses all of its 60 bytes, and a text node (10 to 32% of the nodes, in the five inputs) needs 22: two
sizes would save 6 to 20% of the nodes at the cost of a branch on every read. Writing the part into
the deflater as it is made would save the last row, about a sixth of the total, and would change
what `replace_part` takes, which is the contract of `package`. Chunks closed the gap and are what
the clone needed, so neither was done, and both are still there if a budget is missed again.

**What a parse costs in time.** A chunked arena does a little more for each push and each read: a
push is about 25 instructions more and a lookup about 15 (a chunk number, a position in it, and for
a frozen tree a pointer to follow). Two changes in the parser take most of it back: the attributes of
a start tag are stored in the arena as they are read, and a node is linked to the one before it through
the element that is open, so that neither reads the parent. A parse of one part in a new process, the
fastest of 15, in milliseconds before and after: 8.5 and 8.5 (`contract-300p`), 27.0 and 27.7,
13.0 and 13.6, 48.1 and 50.9, 35.9 and 37.5, which is from none to six per cent. Writing a part with
every start tag dirty, which reads every node in turn, is 12 to 19% slower (16.8 to 18.9 ms on
`node-dense-18mb`), and writing after one edit, which copies clean spans, takes 0.03 to 0.3 ms
longer than its 0.2 to 2 ms. Cloning a tree and changing 50 paragraphs takes 0.66 and then 0.39 ms on
`contract-300p` and 5.7 and then 2.8 ms on `node-dense-18mb`, because the clone shares the chunks
where it copied a vector of 928,000 nodes.

**The rise in the peak of `open` that `state` saw.** The peak resident size of opening the two
bodies of `state/tests_probe.rs` (`/usr/bin/time -l`, MB) rose from 28 and 90 to 42 and 114 when
`state` merged this module, and the cause is not in the module. The same process that reads a
prepared part and parses it, at each stage of this work:

| | worst-blocks (11.3 MB of XML) | node-dense (18.0 MB) | dense-contract (7.5 MB) |
|---|---:|---:|---:|
| end of the first review | 26.5 | 50.1 | 24.4 |
| before the sibling link | 26.7 | 50.2 | 24.6 |
| the sibling link and the limit on declarations in scope | 27.4 | 52.1 | 25.4 |
| the end of the second review | 27.4 | 52.0 | 25.3 |
| the module as merged | 27.6 | 52.2 | 25.5 |
| chunked arenas | 28.3 | 89.5 | 42.7 |

The sibling link is the 4 bytes a node (a twentieth of the tree) and its cost, 0.7 to 2 MB, is all the
module added; the tracking of namespaces added nothing that shows. The rest is how the resident size is
counted. On macOS the pages that `realloc` moves a large vector to are not counted until they are
written again, so a parser that fills a vector and has it moved twice, as this one did, reads far
below what the tree holds: the process above reports 26 MB resident with 80 MB of tree in it, and a
program of 20 lines that fills two vectors the same way reads 24 MB for 82 MB (47 MB if the vectors
start empty). It depends on where the vector had to move, which is why a change of 4 bytes in the
size of a node can move it. Chunks are written once and stay, so their resident size is what they
hold, and with them the same probes of `state` read 54 and 147 MB for opening, and 66 and 163 MB for
the whole edit, where they read 43 and 116 for opening and 66 and 213 for the whole edit. The peak
that decides the budget, the whole edit, fell on the dense body by 50 MB, because the second copy of
the arenas is not made. The count of requested bytes that `cargo xtask bench` makes is not affected
by any of this, and is the figure the budget is held to.

### Namespace-heavy parts

Parts built to make name resolution slow, parsed in a release build (median of 9; the scanning
column is the median of 3 of a build that scanned every binding in scope for each name and searched
every URI seen for each new one, which is the obvious way to do it and the one this replaced).

| Shape | Input | Nodes and attributes | Scanning the scope | Binding stacks |
|---|---:|---:|---:|---:|
| 65 elements, each declaring 1,000 distinct URIs (65,000 of the 65,535 a part may name) | 1.5 MB | 65,132 | 3.03 s | 17.6 ms |
| 100 nested levels, each declaring the same 1,024 prefixes again (102,400 in scope) | 2.4 MB | 102,701 | 3.42 s | 19.5 ms |
| 254 nested scopes of 1,024 declarations, the deepest allowed | 6.1 MB | 260,859 | 8.52 s | 45.4 ms |
| 2,400 elements, each declaring 1,000 prefixes (2.4 million nodes and attributes) | 56.7 MB | 2,407,202 | not run | 581 ms |

The cost with binding stacks is about 240 ns for each node or attribute in the worst shape, so a
part at the default limit of 5,000,000 takes about 1.2 s. Scanning the scope grows with the square
of the scope, which puts the same input near 14 minutes. Unit tests in `xml/tests_scope.rs` parse the
first three shapes, and one more than a part may name, on another thread with a 20 second limit
(the core may not read a clock, and a thread with a timeout needs only `Duration`). They take about
a second in a debug build and would not finish in 20 seconds with a scan of the scope. The second
and third shapes have 102,400 and 260,159 declarations in scope, which the limit of 1,024 refuses
(section 4), so those tests parse them with the limit raised, to show that parsing is linear
without it; with the default limit the same parts are refused at the second level. Other tests
fill a root with 1,024 declarations through the editor and check what the editor does at the limit.

Corpus run times (5 threads): 42,009 LAB documents (2.7 GB) through every L1 check in 265 seconds.

### Results

What `cargo xtask check` runs is the curated set. The larger runs below used the same two laws
(`package/corpus_laws.rs`, `xml/corpus_laws.rs`) on directories assembled for the purpose, and the
one-edit run and the check with outside tools used throwaway tests that are not in the repository.

**The curated set** (306 documents, with the engine-level expectations of its `INDEX.tsv`: 287
`ok` and 19 refusals, 12 of them at `open`, 5 at `xml` and 2 at `document`): both laws pass.

**The whole manifest** (48,727 rows: 48,655 expected `ok`, and 72 refusals, 45 at `open`, 3 at
`read`, 17 at `xml` and 7 at `document`; 61 rows have no download location, so 48,666 documents can
be fetched). Every document was found by its SHA-256 in the local caches and linked into one
directory with an index built from the manifest, so each is checked against what the manifest
expects of it, including the stage and the part. Both laws pass on all 48,666 that can be fetched,
and on the 61 that cannot, which were in the local copy: 324 seconds for the container law with four
threads, which includes the other
reader (law 5 in the table above) on the input and on two rewrites of every document. The corpus
holds, among others, the 42,009 Harvey LAB documents, 4,402 produced by Word, and the LibreOffice,
POI, Open XML SDK, Aspose, Tika and python-docx test files. Before the second review's fixes the
same run, without the other reader, passed too, and three of its documents were read here as another
archive than the other readers read: `two-zips.docx`, `zip64-eocd.docx` and `inner-gap.docx`. They
are `hostile`, `malformed_input` at `open` in the manifest now, and they are the only three
documents of the 48,666 that the rules of section 1.1 refuse.

**The 176 fixtures** of `stetdoc_testkit::fixtures`, with the expectation each carries: 173 passed
both laws. Three were `ok` in the testkit and are refused at `open` here: `zip-gap-between-entries`
and `zip-gap-before-cd` (bytes that belong to no entry) and `zip-comment-fake-end-record` (the fake
end record in the comment is the last signature, Python and `bsdtar` refuse that file, `unzip` exits
with an error after it compensates, and only a reader from a pipe sees the archive). The testkit now
judges all three `hostile`, `malformed_input` at `open`, so all 176 pass.

**Directories without an index** (every document expected to be good unless it is under `hostile`):
52,067 documents of public fixtures, the corpus sources and the generated cases. The container law
fails on 53, and each is a document that has to be refused at `open`: 18 OLE compound files
(`unsupported`, how a password-protected document is stored), 11 that are not ZIP archives or are
cut off before their end record, 6 whose central directory does not end where the end record
begins, 5 with a local header method or name that differs from the central directory's, 2 that
are missing a local header, 2 with names that differ only in case, a `./word/document.xml` entry
name, `overlap.docx`, and the five that the second review refuses: `two-zips.docx`,
`zip64-eocd.docx`, `inner-gap.docx` and two generated cases with bytes between entries. The span
tree law fails on 60 documents with no index to tell it what to expect: those that its own reader
cannot read either, ODF and other files named `.docx` that have no main part, and documents whose
main part is malformed or has a DOCTYPE or too deep a nesting.

**L2**: one `w:t` changed through the span tree in each of 51,902 documents (50,735 had a `w:t` with
text and were edited), the part written through the package: 0 violations. In every UTF-8 part the
new bytes are the old bytes with that one text node replaced, byte for byte, and every other entry
is raw-identical. This was run again after the changes to the serializer's text joins and to what
`set_text` accepts.

**Output read by other tools**: 305 archives written after replacing the main part and adding a
part with a relationship (every 170th document of the corpus) passed Python's `zipfile.testzip`,
`unzip -t`, `bsdtar -tf` and `bsdtar` reading from a pipe, which lists the same entries as the
seekable readers do. In the first review the builder's ZIP64 shapes were checked the same way.

**Unit tests**: 157 in `package` and `xml`, including the mutation sweeps, the hostile namespace
shapes, a randomized test that inserts new elements and attributes into random documents with
nested and redeclared prefixes and reads the output again (400 documents, ten insertions each),
the sweep of 4,000 mutated archives against the other reader, tests that each gate of the corpus
laws fails when it should, the tests of the sibling links, the depth rule, the declarations an
insert brings and the stability of `NodeId`, the chunks of the arenas (what a clone shares, which
chunk an edit copies, lists that cross a chunk), and the model of text edits (800 random edit
sequences of 60 steps over five documents full of carriage returns, line feeds, `]]` and `>`, each
accepted step checked by writing the tree and reading it back, and by requiring that every node
that is not dirty is in the part written, byte for byte, with no exception for text that meets
text).

## 9. Dependencies

| Crate | Used for | Licence | Notes |
|---|---|---|---|
| `miniz_oxide` | inflate and deflate | MIT or Zlib or Apache-2.0 | `#![forbid(unsafe_code)]`; pure Rust, so output is the same on every platform. Shared with `flate2`, which the fixture builder uses, so the workspace has one version |
| `adler2` | pulled in by `miniz_oxide` | 0BSD or MIT or Apache-2.0 | |
| `memchr` | finding `<`, `>` and quotes | MIT or Unlicense | uses `unsafe` internally for SIMD |

CRC-32 is written here (slice-by-8, safe code). No ZIP crate is used, because reading and writing
the container ourselves is what makes byte identity possible.
