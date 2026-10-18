# Design: `styles` and `numbering`

Design notes for two modules: `styles`, which says what formatting a run or paragraph
really has and whether its text is hidden, and `numbering`, which says what clause number Word shows
next to a paragraph. [`ARCHITECTURE.md`](../../ARCHITECTURE.md) is the map and the `//!` contracts in
`styles/mod.rs` and `numbering/mod.rs` are the interfaces. This document explains why they are
shaped as they are, what real documents ask of them, how they are cached, and how they are checked
against something other than themselves. Terms follow [`GLOSSARY.md`](../../GLOSSARY.md): clause number,
effective formatting, style cascade, story, mode, revision. The definitions these modules read
(`StyleDefs`, `NumberingDefs`, `Props`) are described in [`wordml.md`](wordml.md), section 6.

Section 9.1 says what is built and what is not.

This document covers:

1. what real documents contain, from the corpus
2. the interfaces, and where they differ from the one-line contracts
3. `styles`: the cascade, toggle properties, tables, themes, hidden text, caching
4. `numbering`: facts, list identity, the counter model, labels, modes
5. number formats
6. performance
7. the oracle, and the disagreements to expect
8. the test plan
9. build order, changes to other documents, what other modules provide, open questions

## The decisions that matter most

1. **Clause numbers are computed for a whole story at once, so the contract line changes** (2.2, 4).
   A paragraph's number depends on every paragraph before it in its story, so
   `clause_number(&Paragraph) -> Option<String>` cannot be a function of one paragraph. The interface is
   `StoryNumbers::build(facts, defs, cascade, mode)`, built once per story and mode, with
   `clause_number(node)` as the lookup. It stays pure: the same facts and definitions always give the
   same table. `ARCHITECTURE.md` needs that one line amended.
2. **Counters belong to a list, not to a `w:num`** (4.3). The key is the `abstractNum` that supplies the
   levels, after `numStyleLink` is followed. Two `w:num` elements over one `abstractNum` share counters
   unless a `w:startOverride` restarts one, which is how Word behaves (python-docx's restart recipe
   depends on it). `startOverride` applies when a `w:num` first appears at a level and at every later
   restart of that level, which is what ISO 29500 says. OpenXmlPowerTools keeps one "already used" flag per
   `numId`, so it gets a second overridden level wrong; we keep a set of the (`numId`, level) pairs
   already started.
3. **`numbering` is pure over plain facts, and the modes are data** (4.1, 4.6). `wordml` hands over one
   `NumFact` per paragraph that already says which modes the paragraph exists in (paragraph mark, row and
   wrapper revisions, the join rule) and what its properties were before a `pPrChange`. The counter
   algorithm never sees XML, so its direct unit tests build facts by hand and need no document.
4. **The cascade is resolved per style, once, and a run costs a lookup** (3.1, 3.7). `Cascade::build`
   resolves every style's merged properties and a small flag set for the toggle properties. `hidden` for a
   run is two table reads and a scan of the run's own `w:rPr`, with no allocation. The full `Effective`
   with sources is built only on request, for `inspect`.
5. **Toggle properties are exclusive-or across table style, paragraph style and character style, and
   nowhere else** (3.3). A kind that says false contributes no toggle. Inside one `basedOn` chain the
   nearest definition wins. Direct formatting is absolute. The twelve properties are the ones ISO 29500
   calls toggles; `dstrike` is not one. This is ISO 29500 and Word's documented behaviour, and it
   **differs from the OpenXmlPowerTools reference**, which exclusive-ors only the table style against
   the rest (3.3, register U1).
6. **Hidden text is three different things** (3.6). `vanish` hides text. `webHidden` hides it only in a
   web view, so it is reported under its own reason and is not hidden in print. `specVanish` is not about
   text at all: on a paragraph mark it means the paragraph never ends there, so it joins the next one.
   "Looks hidden" camouflage (white text, 2 pt text) is a separate signal, `looks_hidden`,
   and never a `HiddenReason`.
7. **The oracle is three independent things, none of them Word on this Mac** (7). Word's own cached
   results already in the corpus (twelve `REF` fields in one Word document, and table-of-contents
   lines), LibreOffice's `ListLabelString` read through UNO in a container, and Docxodus's
   `ListItemRetriever` read as a reference algorithm. The disagreements expected between LibreOffice and
   Word are listed by class, so a failure is either a bug or a named class.

## 1. What real documents contain

Counted with a throwaway script (lxml, not committed) over the curated corpus fetched into a private
directory under `/tmp` (deleted afterwards). **Curated** is the 241 readable documents of the curated set
that open and are not Harvey LAB. It was chosen to cover features, so it shows which shapes exist and not
how often. **LAB** is the same 2,801-document sample of Harvey LAB that [`wordml.md`](wordml.md) uses (every
15th document in path order, all written by python-docx). Counts are occurrences, and documents in
brackets. How often a feature appears in the wider population was counted separately and is not
repeated here.

### 1.1 Numbering

| Feature | Curated | LAB sample |
|---|---|---|
| Documents with `numbering.xml` | 95 | 2,800 |
| Documents with at least one paragraph that is actually numbered | 57 | 6 |
| Numbered paragraphs: body, table cell, header or footer, text box | 7,785 (57), 27 (5), 1 (1), 4 (1) | 369 (6), 0, 0, 0 |
| Numbered through direct `w:numPr`, through the paragraph style | 7,479 (50), 307 (13) | 369 (6), 0 |
| `w:abstractNum`, `w:num` | 2,268 (87), 2,414 (87) | 25,242, 25,261 |
| Documents with 100 or more numbered paragraphs | 5 (the largest: 5,096, then 961, 766, 243, 112) | none |
| `w:lvlOverride` with `w:startOverride`, with a replacement `w:lvl` | 1,797 (19), 12 (2) | 963 (6), 0 |
| Two `w:num` that are both used and share one `abstractNum` | 4 (4) | 3 (3), all bullets or lettered lists with a `startOverride` each |
| `w:isLgl` on a level, on a level that a paragraph uses | 173 (3), none | none |
| `w:lvlRestart`, and its value | 7 (2), all `0` (never restart) | none |
| `w:numStyleLink`, `w:styleLink` | 2 (1), 9 (6), no paragraph reaches a list through a `numStyleLink` | none |
| Levels linked to a paragraph style (`w:lvl/w:pStyle`) | 207 (27) | 16,764 (2,794), none of them reached by a numbered paragraph |
| Styles that carry `w:numPr`; style `numId` 0 | 279 (54); 15 (7) | 19,564 (2,800) |
| Paragraph `numId` 0 over a numbered style, over an unnumbered one | 35 (6), 7 (5) | none |
| `w:multiLevelType`: hybrid, multilevel, single, absent | 1,929, 157, 120, 62 | single 25,146, multilevel 96 |
| Levels with no `w:start`, of which counting levels | 731 (12), 21 (3) | 72 (6) |
| Levels whose text has one placeholder, none, two or more | 3,902, 3,677, about 300 | 14,762, 11,248, none |
| `w:suff` on used levels: space, tab, nothing | 1,433, 478, 4 | tab only |
| Paragraph `ilvl` beyond the levels the list defines | 3 (2) | none |
| Numbered paragraphs whose mark is a tracked deletion, insertion | 1 (1), 1 (1) | none |
| Numbered paragraphs with a `w:pPrChange`, or a `numPr` inside one | none | none |
| A list's first paragraph is deeper than level 1, a jump of more than one level | 7 (6), 9 (4) | not measured |
| Paragraphs that go back to a shallower level | 193 (22) | not measured |

What follows:

- **Numbering is rare in LAB and common in Word documents.** Only 6 of 2,801 LAB documents number a
  paragraph, and the lists are short, so LAB is a smoke test. The hard cases are Word-authored contracts:
  the EU AI Act drafts (961 and 766 numbered paragraphs) and the open-agreements templates (112) are the
  nearest contract-sized fixtures. The census's own gap list says the same: no corpus document combines
  hundreds of paragraphs, `isLgl`, `lvlRestart` and a counterparty redline.
- **Tracked changes that touch numbering are almost absent from the corpus** (one deleted mark, one
  inserted mark, no `pPrChange` on a numbered paragraph). Everything section 4.6 says about modes is
  therefore specified from Word's documented behaviour and checked on hand-built fixtures, not on corpus
  documents.
- **Style-based numbering is the common case for headings** (307 paragraphs in 13 documents, and every
  numbered heading of the Word document `bookmark.docx`). A heading style carries `numId`, and its level
  comes from the level that names the style, so section 4.2 gives that rule its own place.
- **The format list is long and used shallowly.** Defined levels use about 60 distinct `w:numFmt` values,
  but 59 of them come from one probe document (`word-page-number-formats.docx`, written by docx.js, one
  list per format, 5,096 numbered paragraphs). Outside it, the formats on levels that paragraphs use are
  `decimal`, `lowerLetter`, `bullet`, `lowerRoman`, `upperRoman`, `none`, `upperLetter`, `ordinal`,
  `cardinalText`, `ordinalText`, `decimalZero` and `decimalEnclosedCircle`. Section 5 tiers the formats
  accordingly. There is no `custom` format anywhere in the curated set.
- **`w:numberingChange` is not paragraph numbering.** It sits on the `fldChar` of a `LISTNUM` field (3
  documents in the wider census) and records a revision of that field's result. `LISTNUM`, `AUTONUM` and
  `SEQ` fields (4 documents) give their numbers as field results, which `wordml` already reads as text.
  Neither is a clause number.
- **Word's own computed numbers exist in the corpus, in small supply.** One Word document, the Open XML
  SDK test file `bookmark.docx`, has 12 `REF` fields (11 with `\w`, one with `\r`) whose cached results are Word's numbers
  (`26.4`, `26.3.1`, `26.7.1.9`), all for headings numbered through styles. Nine curated documents
  have both a table of contents and numbered paragraphs, and a TOC line holds the number Word showed as
  plain text. Section 7.2 uses both.

### 1.2 Styles

| Feature | Curated | LAB sample |
|---|---|---|
| Styles, by type: paragraph, character, table, numbering | 2,930 (223), 2,278 (196), 1,633 (184), 192 (163) | 100,776, 75,726, 279,406, 2,794 |
| Longest `basedOn` chain | 10 (one document); typically 2 to 3 | 3 or 4 |
| Table styles with conditional formatting (`w:tblStylePr`) | 1,302 (29); 23 documents set bold or italic in one | 273,818 (2,800) |
| Tables that use a table style, `w:tblLook`, `w:cnfStyle` | 44 (44), 172 (55), 323 (7) | 5 (5), not measured |
| `w:tblLook` written as a hex value, as attributes | 172 (55), 125 (46) | not measured |
| Style `rPr` toggles: bold, italic, small caps, caps | 718 (101), 605 (112), 81 (36), 67 (15) | 41,982, 47,576, 8,382 |
| Style `rPr` with `vanish`, `specVanish`, `webHidden` | 6 (4), 2 (1), 2 (1) | none |
| Document defaults `rPr` that carry a toggle | bold 2, italic 2, vanish 1 (a handful of documents) | none |
| Run uses a character style that sets `vanish`; paragraph uses a style that does | 5 (2); 1 (1) | none |
| Direct `vanish`, `webHidden`, with `w:val` false | 34 (5), 553 (7), 18 (2) | not measured |
| `vanish` on a paragraph mark, `specVanish` on a mark | 3 (1), none | not measured |
| Direct run size of 2 pt or less | 35 (5) | not measured |
| Styles that use theme fonts, theme colours | 1,185 (65), 1,426 (73) | 100,722, 243,204 (2,800) |
| Documents with a theme part, with `w:clrSchemeMapping` in settings | 200, 155 | 2,800 |
| Runs with a `w:rStyle` | 1,472 (47) | not measured |
| `w:rPrChange`, `w:pPrChange` | 607 (8), 103 (6) | not measured |

What follows:

- **Hidden text is rare and mostly comes from `webHidden`.** 553 of the 590 direct hidden-text elements
  are `webHidden`, in 7 documents. Word writes it on the page-number runs of a table of contents, which
  show in print. Treating `webHidden` as hidden would hide every TOC page number, so it gets its own
  reason (3.6).
- **Hidden text through a style is real but small** (6 styles, 7 uses), and it is the case a
  run-level check that ignores styles gets wrong. The cascade is needed for it.
- **Toggle properties matter in practice**: bold and italic are set in more than a hundred documents'
  styles, and 23 documents set them in a table style's conditional formats, where a table style, a
  paragraph style and a run style can disagree.
- **Themes are universal in Word and python-docx output, and almost every colour and font in a style
  comes through one.** Word also writes the resolved `w:val` beside `w:themeColor`, so a missing theme
  degrades to the cached value and not to nothing.
- **`cnfStyle` is far rarer than `tblLook`** (7 documents against 55), and ISO 29500 calls it an
  optimization for consumers. The conditional region is computed from `tblLook` and the cell's position
  (3.4), and `cnfStyle` is used only as a cross-check.

## 2. The interfaces

### 2.1 `styles`

```rust
// styles: immutable, Send + Sync, built from definitions. No cache of its own.
pub struct Cascade { /* merged per-style tables, theme, colour map */ }
impl Cascade {
    pub fn build(defs: &StyleDefs, theme: Option<&SpanTree>, settings: Option<&SpanTree>) -> Cascade;
    pub fn effective(&self, site: &Site<'_>) -> Effective;            // the contract's `effective`
    pub fn hidden(&self, site: &Site<'_>) -> Option<HiddenReason>;    // the contract's `hidden`: Word's meaning only
    pub fn looks_hidden(&self, site: &Site<'_>) -> Option<LooksHidden>; // text a reader cannot see, though Word shows it
    pub fn mark_effect(&self, site: &Site<'_>) -> MarkEffect;
    pub fn style_numbering(&self, style_id: &str) -> StyleNumbering;  // for `numbering`, section 4.2
}

// Where a piece of text is. Built by the caller from wordml's accessors; borrows the tree.
pub struct Site<'a> {
    pub style: Option<&'a str>,           // w:pStyle, None for the default paragraph style
    pub ppr: Option<PropsRef<'a>>,        // direct w:pPr (in Original mode, the old snapshot)
    pub mark: Option<PropsRef<'a>>,       // w:pPr/w:rPr: the paragraph mark
    pub table: Option<&'a TableSite>,     // wordml::TableSite: innermost table only (style, look, row, column, span, cnf); borrowed, so a site allocates nothing
    pub level: Option<LevelSite<'a>>,     // the numbering level's w:pPr and where numbering came from
    pub numbering_removed: bool,          // the paragraph's own w:numPr says numId 0: the indent its styles give goes
    pub cell_fill: Option<&'a str>,       // the cell's own w:tcPr/w:shd/@w:fill, for looks_hidden
    pub background: Option<&'a str>,      // the document's w:background/@w:color, for looks_hidden
    pub run: Option<RunSite<'a>>,         // None means "the paragraph itself (its mark)"
}
pub struct RunSite<'a> { pub style: Option<&'a str> /* w:rStyle */, pub direct: Option<PropsRef<'a>>, pub complex_script: bool /* size is szCs, not sz */ }
pub struct LevelSite<'a> { pub props: &'a Props, pub num_id: i64, pub ilvl: u8, pub from_style: bool }

pub struct Effective { /* entries sorted by name */ }
pub struct Entry { pub prop: Prop, pub source: Source }
impl Effective {
    pub fn get(&self, name: &str) -> Option<&Entry>;
    pub fn on(&self, name: &str) -> bool;                // an on/off property, false when absent
    pub fn val(&self, name: &str) -> Option<&str>;
    pub fn font(&self, slot: FontSlot) -> Option<String>; // theme resolved
    pub fn color(&self) -> Option<Rgb>;                   // theme resolved
    pub fn iter(&self) -> impl Iterator<Item = &Entry>;
}
pub enum Source {
    Default, TableStyle { id: Box<str>, region: Option<TableRegion> },
    Numbering { num_id: i64, ilvl: u8 }, ParagraphStyle { id: Box<str> },
    CharacterStyle { id: Box<str> }, Direct,
    Toggle(Vec<(Source, bool)>),   // the levels that set a toggle property and what each said
}
pub enum HiddenReason { Vanish, WebHidden }
pub enum LooksHidden { TinyText, WhiteText }
pub enum MarkEffect { Normal, Hidden, JoinsNext /* specVanish */ }
```

`Site` has one `run: Option<RunSite>`. With a run it describes that run, and without one it describes the
paragraph itself, so `effective` returns the paragraph's properties and `hidden` reports on its mark. That
keeps the two functions of the contract and adds one type. `mark_effect(&Site) -> MarkEffect` is the third
small function, because a joined paragraph is a structure question, not a hiding one (3.6).

`Effective` is generic on purpose: it holds `Prop` values, so any property in `w:rPr` or `w:pPr` has an
answer and a source, which is what `inspect` shows ("effective formatting and where each value comes
from"). Typed reads exist only for the few that need resolution (fonts and colours, through the theme) and
for on/off properties. Adding a property to the cascade is not a code change unless it merges
attribute by attribute (3.2).

### 2.2 `numbering`

```rust
// numbering: immutable, Send + Sync. Pure over its inputs.
pub struct StoryNumbers { /* items in document order, index by NodeId, notes */ }
impl StoryNumbers {
    pub fn build(facts: &[NumFact], defs: &NumberingDefs, cascade: &Cascade, mode: Mode) -> StoryNumbers;
    pub fn item(&self, paragraph: NodeId) -> Option<&ListItem>;
    pub fn clause_number(&self, paragraph: NodeId) -> Option<&str>;   // the contract's `clause_number`
    pub fn full_context(&self, paragraph: NodeId) -> Option<&str>;    // "1.2(a)", for cross-references
    pub fn find(&self, text: &str) -> impl Iterator<Item = NodeId>;   // reverse lookup, document order (4.4)
    pub fn notes(&self) -> &[Note];                                   // what was approximate, each once (surfaced by projection, 2.4)
    pub fn notes_at(&self, paragraph: NodeId) -> impl Iterator<Item = &Note>; // the notes that arose for this paragraph
    pub fn with_list_num_fields(self, present: bool) -> StoryNumbers; // the caller found LISTNUM fields: adds `ListNumField`
}
pub struct ListItem {
    pub level: u8, pub num_id: i64, pub list: ListId /* the abstractNum that supplies the levels */,
    pub kind: ItemKind, pub suffix: Suff,
    pub ghost: bool,   // Redline only: the paragraph is a deletion, and the number is where it would fall
    pub from_style: bool,       // the numbering came from the paragraph style, not from a direct numPr (3.1)
    pub label_hidden: bool,     // the level's own rPr has vanish: Word hides the label (none in the corpus)
}
pub enum ItemKind { Counted { text: String }, Bullet { glyph: String }, Plain }
pub enum Note {
    UnsupportedFormat { fmt: Box<str> }, MissingLevel { num_id: i64, ilvl: u8 }, StyleLinkCycle { style: Box<str> },
    ListNumField,         // the story mixes LISTNUM fields with numbered paragraphs, and Word counts them in one list (story-wide: no paragraph)
    UnresolvedNumId { num_id: i64 },
}
```

From `wordml` (section 9.3) `numbering` receives `NumFact` values and never reads
XML:

```rust
pub struct NumFact {
    pub entry: usize, pub node: NodeId,
    pub style_id: Option<Box<str>>, pub num_pr: Option<NumPr>,       // direct, current
    pub old: Option<OldPPr>,                                        // present iff a w:pPrChange exists
    pub mark_revs: MarkRevs,                                        // every revision on the paragraph mark
    pub presence: Presence,                                         // Always | FinalOnly | OriginalOnly | Never
    pub empty_section_break: bool,
}
pub struct OldPPr { pub style_id: Option<Box<str>>, pub num_pr: Option<NumPr> }
```

**Where this differs from `ARCHITECTURE.md`.** The module table says `clause_number(&Paragraph) ->
Option<String>`, and the `//!` contract says "pure: the same definitions and paragraph always give the same
clause number". Both hold only if "the paragraph" means its whole story, because counters depend on earlier
paragraphs. The fix is to say so: the interface is `StoryNumbers::{build, clause_number, full_context}`,
and the invariant becomes "the same facts, definitions and mode always give the same table". Nothing else
in the architecture changes, because `projection`, `index` and `compare` already hold a `State` and read
per-story products from it.

Clause numbers are text in `StoryNumbers`, not stored in the package and not part of any text range:
`compare` excludes them from comparison text, and `projection` marks them read-only
(`readonly_region`).

### 2.3 Dependencies and where the caches live

`styles` depends on `api`, `xml` and `wordml`, and `numbering` on `api`, `wordml` and `styles`, as the
table says, and neither needs more. The one `xml` type `styles` names is `SpanTree`, to read the theme
and the settings (`Cascade::build`). `numbering` names `Mode` (`api`), `NodeId`, `NumFact` and
`NumberingDefs` (`wordml`) and `Cascade` (`styles`), and no XML type, because `wordml` now re-exports
`SpanTree` and builds the facts.

Neither module can see `state`, so neither caches. Both are **builders** for `PartsView::derived` (design
in [`refs-and-state.md`](refs-and-state.md), section 12):

| Product | Type | Dependencies | `variant` | Cost (estimate, section 6) |
|---|---|---|---|---|
| Style definitions | `Arc<StyleDefs>` | the styles part, `Dep::Package` | 0 | parse of one part (the largest curated parts are 765 KB and 1 MB) |
| Numbering definitions | `Arc<NumberingDefs>` | the numbering part, `Dep::Package` | 0 | parse of one part |
| Cascade | `Arc<Cascade>` | the styles part, the theme part, the settings part, `Dep::Package` | 0 | 1 to 3 ms |
| Story numbers | `Arc<StoryNumbers>` | the story part, the numbering part, the styles part, `Dep::Package` | the mode: Final 0, Original 1, Redline 2 | 3 to 20 ms (more with the story's field pass, see below) |

A builder must be a function of the parts in its dependency list and nothing else, and these are. The
numbering part, the styles part and the theme part are reached through `document.xml.rels` by
`PartsView::related_part`, so a change to a relationship must also invalidate the entry: every list carries
`Dep::Package`, whose generation moves when a part or a relationship changes (a cached `Rels` depends on it
too). The price is a rebuild of the cascade (1 to 3 ms) and of a story's numbers when a part is added, which
happens once per comment thread and not per edit. The `NumFact` list is read from the `Story` (itself a derived
value of the story part) and the tree, so the story part is the only story dependency. The cost of an edit
follows: a text edit in the body rebuilds the body's `StoryNumbers`, not the cascade and not another story's
numbers; a style edit rebuilds the cascade and every `StoryNumbers`, because numbering reads style links.
Nothing is invalidated by hand.

**Where the accessors live (decided in review).** `projection`, `index` and
`compare` each need "the `StoryNumbers` of story S in mode M" and "the `Cascade`", and the dependency lists
above are knowledge that must not be copied three times. So `state` depends on `styles` and `numbering` and
owns the accessors, in `state/cascade.rs`. They are defined on `PartsView`, so that a check inside a `Tx`
that staged `styles.xml` sees the staged cascade, and `State::{cascade, numbers}` are one-line wrappers:

```rust
impl<'a> PartsView<'a> {   // state/cascade.rs, built
    pub fn style_defs(&self) -> Result<Arc<StyleDefs>, StetError>;
    pub fn numbering_defs(&self) -> Result<Arc<NumberingDefs>, StetError>;
    pub fn cascade(&self) -> Result<Arc<Cascade>, StetError>;
    pub fn numbers(&self, story: PartId, mode: Mode) -> Result<Arc<StoryNumbers>, StetError>;
    pub fn cascade_deps(&self) -> Result<Vec<Dep>, StetError>;                    // what `cascade` is built under
    pub fn numbers_deps(&self, story: PartId) -> Result<Vec<Dep>, StetError>;     // what `numbers` is built under, in every mode
    pub fn sites(&self, story: PartId, mode: Mode) -> Result<SiteHandle<'a>, StetError>;                          // state/sites.rs
    pub fn site(&self, story: PartId, node: NodeId, run: Option<RunKey>, mode: Mode) -> Result<SiteHandle<'a>, StetError>;
}
```

**One list of dependencies for each product, exported (`cascade_deps`, `numbers_deps`).** The list a builder
caches under and the list that `PartsView` gives to a caller are the same function, so that a cache built
*from* a product can name the parts the product was built from. `index::of` is the first such cache: it lists
`numbers_deps(story)` and the cascade's, so an edit to the styles part or the numbering part invalidates a
clause label. A test pins this (`state::tests_cascade::the_builders_are_cached_under_the_exported_lists`): it asks
`derived` for the same type, variant and exported list with a builder that fails, and gets the value the
accessor built. A second test pins what each part invalidates and what it does not
(`what_a_part_changes_is_rebuilt_and_nothing_else`): the theme and the settings rebuild the cascade and not the
numbers, the numbering part rebuilds the numbers and not the cascade, the styles part rebuilds both, and a text
edit rebuilds only its own story's numbers. `numbers` and `numbers_deps` take the story's `PartId`, which
`projection` and `index` get from `PartsView::id`; `Dep` and `PartId` are re-exported from `state` for them.

`numbers` takes the story's `PartId` and gets the story from `view.story(part)` and the tree from
`view.tree(part)`, so the dependency on the story part holds. The parts are found with
`view.related_part(main, &|rel_type| ..)`, whose matching closure (Transitional and Strict relationship types
of styles, numbering, theme and settings) lives there too. The Allowed dependencies row for `state`
is `api`, `package`, `xml`, `wordml`, `refs`, `styles`, `numbering`, and the graph stays acyclic because
neither module depends on `state`.

**`PartsView::site` and `sites` (S8).** Building a `Site` needs knowledge from three owners: the paragraph's style, `pPr`,
mark and table site (`wordml`), the numbering level and whether it came from the style (`numbering`), and in
Original mode the revision snapshot of the direct properties (`wordml`). Left to callers, `projection`,
`inspect` and `ops::format` would each repeat it, so `state`, which owns both products, builds it once:
`sites(story, mode)` returns a `SiteHandle` that holds the story, the tree and the cascade, and `site_for(node,
run)` moves it to a paragraph, or to one run of it, and lends a `Site<'_>`; `site(story, node, run, mode)` is the
same for a caller that asks once, and gives a handle already moved (`handle.site()` lends where it is). A run is
a `RunKey`: an item of the paragraph's text map and whether its text is complex script, which only the caller
knows. `Effective` and `hidden` take the `Site`, and no caller assembles one by hand. The handle is built for a
loop over a story and is cheap in it:

- The paragraph is read once while `node` stays the same, so a loop over the runs of a paragraph reads its
  style, direct properties, mark, the shading of its cell and its place in a table once. `site_for` takes
  `&mut self`, because the handle keeps what it read about the paragraph, and the `Site` borrows the handle
  until it is dropped.
- The story's table sites (`Story::table_sites`) are computed in one pass on the first paragraph that is in a
  table, and never for a story with no table: a per-paragraph `table_site` call reads the whole table each time.
- The story's numbers are built on the first **paragraph** site (no run), the only site that reads the numbering
  level (`numbering::level_site`); a run's own properties never read the level, so a loop over runs builds no
  numbers. `level_site` finds the level the way the counters do (the `w:num`'s replacement level, a
  `numStyleLink`, then the nearest lower level), through the same function.
- In `Original` mode the paragraph's direct properties, its mark's and each run's are what a tracked formatting
  change replaced (`w:pPrChange/w:pPr`, `w:rPrChange/w:rPr`; a snapshot with no `w:pStyle` means the default
  style), and in `Final` and `Redline` they are as written. These reads, the shading of the cell around the
  paragraph (`w:tcPr/w:shd/@w:fill` of the nearest `w:tc`), the page colour (`w:background/@w:color` of the main
  part) and a paragraph's `numPr` of 0 are functions of `styles` (`site.rs`), unit-tested there over small trees.
- A node that is not a paragraph is a caller bug and is `internal`.

**The numbers also run the story's field pass.** `ListNumField` needs to know whether the story has a `LISTNUM`
field, and `NumFact` does not say, so the builder asks `Story::fields`, which is the story's own pass, run once and
kept with the `Story` that `PartsView::story` caches. A story whose text has been read already pays nothing; the
first numbers read of a story nobody has read text from pays the field pass (about 4.5 ms at 5 MB, from `wordml`).

**Numbers are computed lazily, only on read (decided in review).** Nothing in the edit path reads them: a
text edit makes its story's `StoryNumbers` stale and nothing rebuilds it until a reader asks, and the readers
are views, `check`'s summary, compare and index, after the batch. If an operation ever needs a clause number
mid-batch (a target given as "clause 4.2"), the targets are resolved once before the batch runs, not per
write: a batch of 50 writes with a read after each on the largest body would cost 50 rebuilds, about a second
at the 20 ms estimate, which is the whole edit budget. If measurement shows numbers on the hot path, the
remedy is an incremental refresh through `PartsView::touched(part)`, which `state` offers, with the builder
receiving the stale previous value (4.7). The benchmark gate for it is in 8.5.

### 2.4 Where the numbering notes go

`check` may not depend on `numbering`, and does not. `StoryNumbers::notes()` and the `Cascade`'s issues are read
by `projection` and by `document`, which may depend on both modules, and surfaced as warnings in the view and in
the receipt ("this clause number may not match Word: unsupported format `hindiVowels`"). `check` reports the
definition-level problems that `wordml`'s `StyleIssue` and `NumberingIssue` already carry and says nothing
about numbering. This keeps the Allowed dependencies table as it is for `check`.

## 3. `styles`

### 3.1 The cascade, in order

ISO 29500-1 17.7.2 (the style hierarchy) gives the order, lowest priority first, and the glossary uses the
same words:

1. document defaults (`w:docDefaults`)
2. the table style, with its conditional formats (3.4)
3. the numbering level (paragraph properties only)
4. the paragraph style, through its `w:basedOn` chain
5. the character style, through its chain
6. direct formatting

**Two corrections from Word's observed behaviour and from OpenXmlPowerTools' `FormattingAssembler`,
which is the reference for the order and whose code we do not use.**

- *Where the numbering level sits depends on where the numbering came from.* Numbering set directly on
  the paragraph (`w:numPr` in its `pPr`) applies the level's `w:ind` above the paragraph style, so the
  level's indent beats the style's. Numbering that comes from the paragraph style applies it below the
  style, so the style's own indent beats the level's. That is what Word does, and it is why `LevelSite`
  has `from_style`. Only `w:ind` is taken from the level's `pPr`, as the reference does. (Unverified
  claim: tabs from the level. Register in 7.5, U7.)
- *A paragraph whose own `numPr` says `numId` 0 drops the indent that comes from its style.* The reference
  removes `w:ind` that comes from the style hierarchy for such a paragraph (`IsZeroNumId`), because the
  style's indent was written to go with its numbering. The paragraph's direct `w:ind` stays. A style whose
  numbering does not resolve (a missing `w:num`) keeps its indent, as the reference does. Register in 7.5, U19.
- *Paragraph properties and run properties use different level sets.* Run properties of the numbering
  level (`rPr` inside `w:lvl`) format the label only and never reach the paragraph's text, so the run
  cascade has no numbering step. The paragraph mark takes the paragraph's run properties from the
  paragraph style and from its own `pPr/rPr`.

**Run properties.** `rPrDefault`, then the table style's `rPr` (whole table, then the matching conditional
formats), then the paragraph style chain's `rPr`, then the character style chain's `rPr`, then the run's
`w:rPr`. **Paragraph properties.** `pPrDefault`, the table style's `pPr`, the numbering level where it
sits above, the paragraph style chain's `pPr`, the numbering level where it sits below, then direct
`pPr`. **Original mode** reads the direct values from the revision snapshots: a run's `rPrChange` holds the
complete previous direct formatting, and a paragraph's `pPrChange` the complete previous direct paragraph
properties. A snapshot replaces the direct formatting, it is not merged into it. The caller passes the
snapshot as `RunSite::direct` or `Site::ppr`, and `styles` does not know about modes. **Style definitions
that themselves carry a `w:rPrChange` or `w:pPrChange` are read as they stand in every mode**: a revision of
a style's definition is not undone in Original, because no producer we know writes one and Word shows
the current definition. A `Note` is the place to say so if one appears.

A `basedOn` chain is read with `StyleDefs::chain`, which is cycle safe, bounded at 64 and stops at a
dangling id. A style that names a missing parent simply has no parent, and `check` reports the issue
(`StyleIssue::BasedOnMissing`). A duplicate style id uses the first definition, as `StyleDefs::get` does.

### 3.2 How properties combine

The default rule is replacement: the property element at the higher level replaces the one at the lower
level. Four kinds of property do not follow it.

| Kind | Properties | Rule |
|---|---|---|
| Toggle | `b`, `bCs`, `i`, `iCs`, `caps`, `smallCaps`, `strike`, `emboss`, `imprint`, `outline`, `shadow`, `vanish` | exclusive or across levels (3.3) |
| Merge by attribute | `rFonts`, `lang`, `ind`, `spacing`, `numPr` | each attribute is its own property |
| Merge by child | `pBdr`, `tcBorders`, `tblBorders`, `tblCellMar`, `tcMar`, `tabs` | each side, or each tab stop by position, is its own property; a tab with `val="clear"` removes the lower one |
| Exclusive pair | `ind` `firstLine` and `hanging`; `ind` `left` and `start`; `ind` `right` and `end` | setting one at a higher level drops the other from lower levels |

Notes:

- `rFonts`: a higher level that writes `ascii` or `asciiTheme` replaces both of the lower level's `ascii` and
  `asciiTheme` (one slot, two spellings), and likewise for `hAnsi`, `eastAsia` and `cs`. Within one element
  a theme attribute beats the explicit one. (To be pinned by a fixture against LibreOffice, section 8.)
- `numPr`: for the cascade a direct `numPr` replaces the style's as a unit, as the reference does. A direct
  `<w:numPr><w:ilvl w:val="2"/></w:numPr>` with no `numId` therefore makes the paragraph "not a list item",
  and a direct `numId` with no `ilvl` is level 0. We considered letting `numId` and `ilvl` inherit
  separately, which is what the schema's shape suggests, but nothing in the corpus tells the two apart (one
  paragraph in one document has a `numPr` without `numId`) and the reference is the one that has been run
  against Word documents, so we follow it (register U8). The cascade's `Effective` still lists the style's
  `numPr` as a separate entry with its own source.
- `ind` is the only paragraph property the numbering level contributes (3.1).
- `Source` is per property element, and per attribute for the merge-by-attribute kind (an `Entry` for
  `rFonts` lists which level supplied each attribute).

The rule table is data in one file (`styles/merge.rs`), so a new rule is one row, not a new branch.

### 3.3 Toggle properties

ISO 29500-1 17.7.3 defines toggle properties, and each of the twelve in the table above says "this
formatting property is a toggle property (17.7.3)" in its own entry (checked against the Open XML SDK
documentation, which quotes the standard). `dstrike`, `webHidden`, `specVanish`, `noProof`, `color`, `sz`
and `u` are not toggles. The rule as implemented:

1. Resolve each style *kind* on its own. Inside the paragraph style's `basedOn` chain, and inside the
   character style's chain, the nearest style that mentions the property gives that kind's value. The
   chain is not exclusive-ored: a `Heading1` based on `Normal` that both say bold is bold.
2. The three kind values are the table style's (after its conditional formats, 3.4), the paragraph
   style's and the character style's. A kind that does not mention the property contributes nothing, and a
   kind that says false (`w:val` of `0`, `false` or `off`) contributes false, which leaves the others
   unchanged under exclusive-or. "False means no toggle."
3. The result is the document default's value (false when absent) exclusive-ored with the values of the
   kinds that mention the property. Bold in the paragraph style and in the character style gives not bold.
4. A direct `w:b` on the run is absolute and replaces the result, true or false, whatever the styles say.
   A `w:val` of `0`, `false` or `off` is false, and no `w:val` is true (`Props::on_off`).

**This differs from the OpenXmlPowerTools reference, and the difference is real.** In
`FormattingAssembler.cs`, `ToggleMergeRunProps` is called once (line 2841), and only to combine the table
style's `rPr` with the rest. The paragraph style against the character style is a plain
`MergeStyleElement`, so the character style replaces the paragraph style's value and there is no
exclusive-or. Inside `ToggleMergeRunProps` an explicit false at the higher level wins, where under
exclusive-or it would leave the lower value as it is. So a bold character style in a bold heading stays bold in
the reference and shows regular weight in Word. The census finds 60 runs in 3 curated documents with a paragraph
style and a character style that both set a toggle and no direct override, among them Word's own
`bookmark.docx`, and LibreOffice's `tdf154370.docx`, which is the test file for exactly this
(a paragraph style and a character style both setting bold, italic, caps, emboss and strike, and a hidden
pair `Hidden1` over `Hidden`). The decision (taken in review) is to keep exclusive-or across the three kinds,
which is what ISO 29500 and Word's documented behaviour say, and to register the deviation (U1). It matters to
`hidden`: under the reference `Hidden1` over `Hidden` stays hidden, and under ISO it does not.

**Two readers, one rule, pinned by a test.** `hidden` folds a set of flag bits (`toggle.rs`, no allocation, once
per run) and `effective` folds a list of entries that carry their sources (`effective.rs`). They are two
implementations of the four steps above, and they must never differ on `vanish`, because a view that hides
text on one answer while `inspect` says it is shown on the other would be a bug nobody could see. The test
`styles::tests_agree::hidden_and_effective_agree_on_the_toggle_rule_and_both_match_it` sets all twelve toggles at every
level the rule reads (the defaults, the table style and its first-row format, a paragraph style and the style it is
based on, a character style and its base, and the run) in 400 random combinations of mention and value, and checks
`effective` for every toggle and `hidden` for `vanish` against a plain statement of the rule, and the two against each
other. Replacing the exclusive-or with an or in either implementation fails it in the first rounds.

**What is unverified and why.** The wording of 17.7.3 itself was not available offline in the session that
wrote this design: that each of the twelve properties is a toggle property is quoted from the standard in the
Open XML SDK documentation, but the algorithm of 17.7.3 is taken from Word's documented behaviour. Two parts
have no direct evidence: "no exclusive-or inside a chain" (U1b) and "document defaults are the starting value, not
a fourth term" (U2). `tdf154370.docx` is LibreOffice's document for the first claim (LibreOffice, whose DOCX import is
believed to match Word here, is its oracle). Synthetic twins in `styles::tests_cascade` and `styles::tests_hidden` pin
the rule today, and the document itself is read by the totality law only, so asserting what it shows waits for the
LibreOffice rung (7.3), and the Word checklist has the rest. A document default that sets a
toggle is in a handful of curated documents, so U2 matters little in practice, and the fixture records whichever
way the oracle goes.

`Source::Toggle` lists the kinds that set the property and what each said, so `inspect` can explain "not
bold: bold from the paragraph style Heading1, bold from the character style Strong".

### 3.4 Table styles

A table style contributes `pPr` and `rPr` (and cell and table properties, which `styles` passes through as
`Effective` entries but the first release does not use). It is found from `TableSite::style_id` and its
own `basedOn` chain. The conditional formats in `StyleDef::conditional` apply, per ISO 29500-1 for
`tblStylePr`, "in the following order, so subsequent formats override previous ones": whole table, banded
columns (odd then even), banded rows (odd then even), first row and last row, first column and last
column, then the top left, top right, bottom left and bottom right cells. **The reference applies a
different order**: whole table, banded columns, banded rows, then first column and last column, then first row and last row,
then the corner cells (`TableStyleOverrideTypes`). So in a cell where the first row meets the first column,
the standard lets the first column win and the reference lets the first row win. Decision (taken in review:
where ISO's listed order and the reference disagree and neither has corpus evidence, follow the reference,
which has been run against Word documents): **we apply the reference's order**, banding first, then columns, then
rows, then corners. A table style with a header row that is bold and white and a first column that is bold and
dark is the case, and Word's tables put the header row's look on the corner cell. The register has the row
(U20) and a fixture for the meeting cell, and the Word checklist has it.

Which formats match a cell comes from `w:tblLook` and the cell's position (`TableSite`: row and column
index and counts), not from `w:cnfStyle`, which the standard calls an optimization: `firstRow`, `lastRow`,
`firstColumn` and `lastColumn` apply only when their `tblLook` flag is set, and `noHBand` and `noVBand`
turn banding off. The default look applies banding but not first and last. The band size comes from
`w:tblStyleRowBandSize` and `w:tblStyleColBandSize`, default 1. Only the innermost table counts, because
Word does not inherit a table style into a nested table. A merged cell uses the grid position of its
first column. **`cnfStyle` is a free oracle.** Word writes it on rows and cells (7 curated documents), and it
is Word's own answer for which conditional formats apply. For every Word-authored document the corpus test
asserts that the regions computed from `tblLook` and the position equal the set named by the `cnfStyle` of the
cell (and the row), and a difference is a bug here, not a note. For documents from other producers a
disagreement is a note for `check`, and the computed regions win.

**What the oracle showed when it was built** (`state/tests_cascade_corpus.rs`, over the curated corpus): 100 cells
with a `cnfStyle` in Word documents, and the computed regions equal Word's in all but one document. Three facts about
what Word writes shaped the comparison. Word splits the flags: the row-level ones (first and last row, the
horizontal bands) sit on the row's `trPr/cnfStyle` and the column-level ones on the cell's, so the recorded set is the
union of the two. Word records only the regions that the table style defines a format for (a style that formats
the odd band and not the even one gets `oddHBand` on its odd rows and nothing on the even ones), so the comparison
uses the regions that apply and that the style defines (`Cascade::applicable_regions`). And in the corner flags of
the 12-character `w:val` the top right comes before the top left (`Formatted_elements.docx` has
`firstRowFirstColumn="1"` with the value `001000000100`), where ISO 29500 lists the top left first. The one document
that differs, `Corrupted_footnotes.docx`, has a second table whose odd-band flag sits on the even rows, where the first
table of the same document with the same style is consistent: Word kept a stale value after an edit. It is a named
exception in the test with its reason, capped at 40 cells.

The table level matters for hidden text and for bold and italic in table headers (23 curated documents
set one in a conditional format). It is the lowest-priority part of the build (section 9) and the cascade
works without it: a `Site` with `table: None` skips the step.

### 3.5 Theme fonts and colours

`Theme::read(&SpanTree)` reads `a:theme/a:themeElements`: the major and minor font schemes (Latin, East
Asian and complex script typefaces) and the twelve colours of the colour scheme. `w:clrSchemeMapping` in
settings maps the `bg1`, `tx1`, `bg2` and `tx2` names to the scheme (155 curated documents have it), with
the standard mapping when it is absent. A `w:rFonts` slot that has `asciiTheme="minorHAnsi"` resolves to the
minor Latin typeface, and `majorEastAsia`, `minorBidi` and the like likewise. A `w:color` with
`w:themeColor` resolves to the scheme colour, then applies `w:themeTint` (towards white) and
`w:themeShade` (towards black) as Word does, by **scaling the luminance in HSL space** (the colour is converted
to hue, saturation and luminance, the luminance is multiplied by the shade, or moved towards 1 by the tint,
and the result is converted back). Tinting in RGB is wrong more often: over 2,408 colours with a tint or shade
in the whole corpus the cached `w:val` equals the RGB result for 654 exactly and 1,064 within one per channel,
and the plain HSL result for 918 exactly and 2,311 (96%) within one. **Word's own rounding is not
reproduced exactly.** It works in an integer HLS space that is not published, and a search over quantisations
found the closest formula: hue to 1/255 and saturation to 1/256, the arithmetic in floating point, and the
result truncated (`Rgb::tinted`). Against the Word-saved documents of the curated set (the manifest's `producer`
is Microsoft Word and `word_fp` is `y`, 111 documents) it gives: 1,363 plain theme colours, **all exactly equal**
to the cached value; 1,236 tinted or shaded colours, 1,101 (89%) exact and **none more than two** apart in a
channel. An earlier draft of this test accepted 5% misses and counted python-docx's and Google Docs' files as
Word's; those 15 misses came from files Word did not save, whose cached accents are the old Office theme, and
filtering by the manifest removed them. The corpus test (`state/tests_cascade_corpus.rs`) asserts zero plain
misses, zero tinted colours over two, **at least 1,101 exact** (the count the formula gives, so a change to the hue or
saturation rounding in `Rgb::tinted` fails: without the saturation step the count is 1,096), and floors on all the
counts, so a worse formula fails. It compares every `w:color` and `w:shd` fill that carries both a theme attribute and a cached
value. Word writes the resolved `w:val` beside
`w:themeColor`, and a missing theme part means the cached `w:val` is used and the entry carries `theme:
unresolved`. `Effective::font(slot)` and `color()` do this, and nothing else in the
cascade knows about themes. `w:hint` and the per-character choice between the Latin and East Asian font are
not modelled: `font(slot)` answers for a slot, and picking the slot for a character is a rendering
decision. **`w:themeFontLang` in settings** (the language of the East Asian and complex script text) is read:
when a theme slot's typeface is empty in the font scheme (`a:ea` or `a:cs` with no typeface, which is common),
Word takes the typeface from the scheme's per-script `a:font` list for that language, so `Cascade::build` keeps
the list and `font(slot)` consults it. The same kind of corpus test compares resolved fonts with the explicit
attribute Word writes beside a theme attribute: in the review's measurement all 202 resolvable cases agreed
with the theme, and 55 more hit an empty `ea` or `cs` typeface, which the language list resolves.

### 3.6 Hidden text

`hidden(site)` answers for a run, or for the paragraph mark when `site.run` is `None`. It uses the
effective value of the property, so a style can hide a run and a direct `w:vanish w:val="0"` can unhide it.

`hidden` has Word's meaning only.

| Reason | Rule | Hidden in a view? |
|---|---|---|
| `Vanish` | effective `vanish` is true (toggle rules of 3.3) | yes: ISO 29500 says the text "shall be hidden from display" |
| `WebHidden` | effective `webHidden` is true | not by default: it affects the web view only and "should not affect a normal paginated view" |

`looks_hidden` is a separate function and a separate answer, not a `HiddenReason`. It reports text that
Word shows and a reader cannot see, which is the known way to plant instructions in a document that an
agent will read and a person will not:

| Signal | Rule |
|---|---|
| `TinyText` | effective `sz` of 4 half-points (2 pt) or less on a run that has text |
| `WhiteText` | effective colour is white or a near-white (theme resolved), and no background behind the run is a different colour |

Tiny text is judged by `sz` for ordinary text and by `szCs` for complex script text (`RunSite::complex_script`,
which the caller sets from the text), so a run whose Arabic is full size is not flagged for the 2 pt size of its
Latin letters. "A different colour" is read, from the last level that sets each kind (the document defaults, the
table style, the paragraph style, the character style, direct formatting), from the run's highlight, its `w:shd`,
its paragraph's `w:shd`, its cell's `w:shd`
**and the table style's cell shading for the cell's region** (a table style that puts white text on an accent
fill through a conditional `tcPr/shd`, such as a header row, would otherwise be flagged on every built-in table),
and from the document's `w:background` for the page. An unknown or auto background counts as white
("white text on a white or absent background"). **White means every channel at or above
`0xF8`** (`Rgb::is_near_white`), for the text and for a fill alike: `F8F8F8` text cannot be told from white on a
screen, so it hides text as well as `FFFFFF` does, and a fill that near white is not a background that makes the text
visible. The line is one channel below `0xF8`: `F7F8F8` is a visible colour. Known limit: only `w:fill` is read, so
a shading **pattern** whose colour is the pattern's own (`w:val="solid"` or `pct40` with `w:color="auto"` over
`w:fill="auto"`) counts as no background, and white text on it is flagged although it is readable. The curated
corpus has one such document (26 of the 34 `WhiteText` runs: `complex1_NOR.docx`, a test of Word's shading levels). It is one comparison each on values the cascade has already computed, and
`projection` decides how to show it (a flag on the inline, and a line in `check`). It never changes
what `hidden` says, so a view that hides `Vanish` text does not hide `WhiteText`, and tells the agent it is
there. Census: 35 direct runs of 2 pt or less in 5 curated documents.

`specVanish` is not in the table because it is not about text. ISO 29500 says it "specifies that the given
run shall always behave as if it is hidden" but "shall only be used to specify that a paragraph mark shall
never be used to break the end of a paragraph for display", and "if applied to any other run, it can be
ignored". So `mark_effect` returns `JoinsNext` for a mark whose effective `specVanish` is true, and
`Hidden` for a mark with effective `vanish`. A joined paragraph is a structural fact that `projection`
shows (Word uses it so that a style applied to part of a paragraph still feeds the table of contents), and
nothing in a run's own `hidden` answer depends on it. Only a mark can be `JoinsNext`. Numbering counts a
`JoinsNext` paragraph as a paragraph, which is an unverified choice (register in 7.5, U4); the corpus has no
`specVanish` on a mark, so it is a corner for a hand-built fixture.

Why the split. `ARCHITECTURE.md` lists `vanish`, `specVanish` and `webHidden` together
as hidden. They cannot be treated the same way: hiding `webHidden` text hides the page numbers of a
table of contents (553 direct `webHidden` runs in 7 curated documents), and treating `specVanish` as a text
rule would drop the first half of a run-in heading. `projection` renders `Hidden` generically from the
reason, so it can choose: the default view hides `Vanish` and, separately, flags what `looks_hidden`
reports.

Where each reason applies in a view: text in a *deleted* run is hidden or not by its formatting like any
other, so a hidden deletion stays hidden in the redline. A hidden paragraph mark hides the paragraph's
number as well. A paragraph is "hidden" for a projection only if its mark is hidden and every run is
hidden or the paragraph is empty, and that is `projection`'s decision from the two answers.

### 3.7 Lazy, cached and cheap

`Cascade::build` resolves, once per styles generation:

- for each paragraph style and each character style: the merged `pPr` and `rPr` along the chain over
  the document defaults, as `Props`, and a `Flags` word with two bits for each of the twelve toggle
  properties plus `webHidden`, `specVanish`, a "size is small" bit and a "colour is white" bit (set, and
  value), and the style's `outlineLvl` and effective numbering reference (`style_numbering`). That is about 560 styles in the worst curated document and 164 in a LAB one, so the build is
  an arithmetic mix of a few hundred small merges (an estimate of 1 to 3 ms; the benchmark in 8.5 checks it).
- for each table style: the same, with the conditional formats kept apart so that a cell picks them by
  region.
- the theme and the colour map.

It is eager, not lazy, on purpose: a lazy memo inside a `Send + Sync` value needs a lock, and an eager
table needs none. `state` provides the laziness, since the cascade is built on first read by
`derived`, not at `open`.

Per request:

- `outline_level(site) -> Option<u8>` is the cheap path the outline view needs for every paragraph (16,000 of
  them): the paragraph style's resolved `outlineLvl` from the table, overridden by the direct `pPr` read through
  `PropsRef`, with no merge and no allocation. The outline is not served by `effective`, which would allocate
  16,000 times. (A heading's level also comes from its built-in name in some documents, which is `projection`'s
  rule over `StyleDefs`.)
- `hidden(site)` for a run reads the paragraph style's `Flags`, the table style's `Flags` if there is a
  table, the run style's `Flags`, and the direct `w:rPr` (`PropsRef`, a short scan, no allocation), then
  combines the toggle bits. There is no `Props` merge and no allocation on this path, because
  `projection` calls it for every run of the page it renders.
- `effective(site)` merges the resolved `Props` of each level that applies and applies the direct
  properties, and allocates. It runs for `inspect` and for the `format` operation's "what is the value now"
  check, not per run in a view.
- A given (paragraph style, run style, table style and region) combination is a small key. If a profile
  shows `effective` on a hot path, a per-`Cascade` memo keyed by it can be added behind the same interface
  with a lock, since `Effective` is plain data. It is not built unless a benchmark asks for it.

## 4. `numbering`

### 4.1 Input: one fact per paragraph

`numbering` receives `&[NumFact]` for one story, in entry order (the order `Story::scan` lists blocks),
and builds numbers for the paragraphs among them. The facts carry only what numbering needs, from
`Story::num_facts(tree)`, which reads each paragraph's `pPr` once with no text model and no field pass.
The fields that matter:

- `style_id` and `num_pr`: the direct `w:pStyle` and `w:numPr`. The paragraph style's own numbering is
  looked up here, through `Cascade::style_numbering`, so `wordml` does not follow `basedOn`.
- `old`: present exactly when the paragraph has a `w:pPrChange`, holding the old style and old `numPr`.
  `old: Some(OldPPr { style_id: None, .. })` means the old properties had no style (so the default
  paragraph style), which is different from `old: None`.
- `presence`: whether the paragraph exists in Final, in Original, in both or in neither, after
  applying the paragraph mark's revision, the join rule (a deleted mark on the last paragraph of a cell
  or story is ignored, as Word ignores it), and every enclosing row, cell, block-level `w:ins`, `w:del`
  and `w:move*` wrapper. This is the same wrapper rule as the text model, so the two cannot disagree.
  `Never` is an insertion inside a deletion.
- `empty_section_break`: the paragraph has a `w:sectPr` in its `pPr` and no `w:t` below it. Open XML
  PowerTools treats such a paragraph as not a list item. Word may show a number for it, and this is an
  unverified rule (register in 7.5, U5); the LibreOffice oracle will tell us what the other major renderer
  does.

Each story is numbered on its own: the body, each header and footer, the footnotes, the endnotes and the
comments. A part is one story, so the paragraphs of all footnotes form one run, and a header shared by
two sections is numbered once. Tables and block-level content controls are transparent: a list
continues across a table, and a paragraph in a cell is part of the story's sequence in document order.
**Text boxes are not numbered in 1.0.** Their paragraphs have no entry and no ref (`wordml.md`, 3.8). A
text box's list would be a story of its own (the reference also counts each text box on its own), and
the one curated document with a numbered paragraph in a text box has four of them. The projection shows
text-box content read-only inside the host paragraph's placeholder, and does not show a clause number
there.

### 4.2 Binding a paragraph to a list

For each paragraph, in the mode being built:

1. **Pick the properties.** In `Final` and `Redline` use `style_id` and `num_pr`. In `Original`, if
   `old` is `Some`, use `old.style_id` and `old.num_pr`; otherwise the current ones.
2. **Direct `numPr` wins over the style, as a unit.** A `numId` of `0` means no numbering, and it beats
   numbering that a style supplies (35 paragraphs in 6 curated documents rely on this); such a paragraph also
   drops the indent its style carries (3.1, register U19). A direct `numPr` with no `numId` makes the paragraph
   not a list item, as the reference does (register U8). A direct `numPr` with a `numId` and no `ilvl` is level 0.
   A `numId` that names no `w:num` makes the paragraph unnumbered, with a `Note`.
3. **Otherwise the style supplies the numbering.** `Cascade::style_numbering(style)` walks the paragraph
   style's `basedOn` chain and takes the nearest `numPr`, child by child (`numId` and `ilvl` separately). A
   style `numId` of `0` removes the numbering that the parent style would give. The paragraph with no
   `pStyle` uses the default paragraph style.
4. **The level.** Direct `ilvl` if the paragraph has a direct `numPr` with one. Otherwise, when the
   numbering comes from a style, the level of the `abstractNum` whose `w:lvl/w:pStyle` names the paragraph's
   style: ISO 29500 says that when a paragraph style includes a numbering definition, any `ilvl` in its
   `numPr` "shall be ignored, and instead" the `pStyle` on the level specifies the level. If no level names
   the style, the style's own `ilvl`, then 0. The match is on the paragraph's own style id: a derived style whose
   *parent* is the one a level names is not matched (searching up the `basedOn` chain would change 4 paragraphs,
   all in LibreOffice's `tdf141966`, and is register U18). Word writes both consistently, so the two rules agree on Word's
   own files, and the rule matters for generators that omit the style's `ilvl` (heading 2 would be level 0
   without it). A fixture with the two disagreeing pins the order, and the oracle checks it (7.3).
5. **The list.** `NumberingDefs::level(num_id, ilvl)` gives the level with the overrides applied. If the
   `abstractNum` has a `numStyleLink`, follow it: style S (type `numbering`) has a `numPr` with a `numId` M,
   and M's `abstractNum` (with `styleLink` S) supplies the levels, with at most 8 hops and a `Note` on a cycle.
   The levels come from, in order: the `w:num`'s own `lvlOverride` with a `w:lvl`, the first `abstractNum`'s
   own levels, then M's override, then M's `abstractNum`. The start comes from the `startOverride` of the
   `w:num`, then M's, then the level's `w:start`. The **list key** is the `abstractNum` that the `w:num` itself
   names, as in the reference (`ListItemInfo.AbstractNumId` is the main one, not the linked one), so two `w:num`
   over one `abstractNum` that has a `numStyleLink` share counters, and two `w:num` over two such `abstractNum`
   that reach the same numbering style do not. That is the reference's rule, nothing in the corpus tells it apart
   from sharing (no paragraph reaches a list through a `numStyleLink`), and it is simpler: register U13.
6. **A level the list does not define** takes the nearest lower defined level's format and text (the
   reference does this), with a `Note::MissingLevel`. 3 paragraphs in 2 curated documents. `multiLevelType`
   is a hint and is not used to force level 0: no paragraph in the corpus has `ilvl` above 0 in a
   `singleLevel` list, so the reference's rule has no evidence behind it.

A paragraph that does not exist in the mode (`presence` says so) is skipped, with the ghost exception for
Redline in 4.6.

### 4.3 The counter model

State is per story and mode, and per **list key**:

```text
ListState {
    value:   [i64; 9]        // the current counter of each level
    entered: [bool; 9]       // the level has a value since the last restart
}
// and, once per story and mode (not per list):
started: HashSet<(i64 /* numId */, u8 /* level */)>   // pairs of (w:num, level) that have begun numbering
```

For a paragraph at level `L` of `w:num` N in list K:

1. **Restarts of deeper levels.** For each level `D > L`, the level restarts, that is `entered[D] = false`,
   unless its `lvlRestart` says otherwise. `lvlRestart` is a one-based level: omitted is the same as `D`
   (restart after any shallower level, which ISO 29500 states as "restarts each time the previous
   numbering level is used"); `0` never restarts; `n` restarts only after a paragraph at one-based level
   `n` or shallower, that is when `L + 1 <= n`; and a value above `D` is ignored. The standard's own
   example is a level 3 with `lvlRestart` `0` that counts `i`, `ii`, then keeps counting `iii`, `iv`
   after later level 1 and level 2 paragraphs. That example is a test.
2. **The level itself.**
   - If `entered[L]` is false, or N has a `startOverride` at `L` and the pair (N, L) is not in `started` (the
     first use of this `w:num` at this level), the value is set to the level's start (with N's override, 4.2
     item 5).
   - Otherwise the value is `value[L] + 1`.
   - Then `entered[L]` is true, and (N, L) is added to `started`.

   A pair, once started, stays started for the story and mode. The first draft remembered only the `w:num` that
   last numbered the level, which restarts an overridden `w:num` every time another `w:num` of the list has
   been in between: with `w:num` A (override 1) and B over one `abstractNum`, the paragraphs A, B, A would
   number 1, 2, 1. ISO 29500, the reference and the draft's own prose all say 1, 2, 3, and `shared_list_continue`
   has that case.

   `startOverride` is "used when this level initially starts in a document, as well as whenever it is
   restarted via the properties set in the lvlRestart element" (ISO 29500, `startOverride`). Our model
   makes that exact: the override replaces the start every time the level (re)starts in N, and the first
   appearance of N at a level is a start. It also covers the usual way a lawyer restarts a list in Word:
   a new `w:num` over the same `abstractNum` with an override on each level. A `w:num` without an override,
   used after another `w:num` of the same list, continues the count. The reference keeps a single "already
   used" flag per `numId` for all levels, so with two overridden levels it applies only one; the set is
   per (`numId`, level) here (7.3, class LO4, register U12).
3. **Shallower levels that never had a value.** A level above `L` with `entered` false (a list whose first
   paragraph is at level 2, 7 paragraphs in 6 curated documents) is shown with its start value and then
   counts as entered with that value, so the next paragraph at that level continues from it. The start it shows
   is the one the level would get if it were entered by N, so N's `startOverride` for that level applies. This is
   the reference's rule. It is also the rule that gives `1.1.1` for a list that starts at level 2, and the
   oracle checks it.
4. **A value is a signed 64-bit integer, saturating.** A `w:start` or `w:startOverride` can be any `i64` (and
   negative). Formats that cannot show a value (4.4, section 5) fall back to decimal.

A **bullet** or `none` level has no counter that matters, but it participates in restarts like any other
level, because a counting level below it restarts when it appears.

State is a map from list key to `ListState`, with a `HashMap` of at most the number of lists used. One pass
over the facts is O(paragraphs x levels), at most nine steps each.

### 4.4 Labels

`lvlText` is read as: `%1` to `%9` is a placeholder for the counter of one-based level `k`; `%%` is a
percent sign; any other `%` is a literal percent sign (the reference tokenises the two characters after
every percent, which only differs on malformed text, 2 occurrences in 1 curated document). A placeholder
for a level **deeper than the paragraph's own** renders nothing, per ISO 29500 ("any number of a level
higher than this level shall be ignored"). The reference clamps it to the last counter instead, so this is
a named expected disagreement (7.3, class LO9) and an early check for the oracle.

Each placeholder is formatted with the **format of the level it names**, not the current level's. If the
current level has `isLgl`, every placeholder in its text is formatted as decimal, whatever the named
level's format, except `none` and `bullet` (ISO 29500: "all numbering levels present in the lvlTxt element
shall be converted to their decimal equivalents when they are displayed in this level"). The reference
keeps `decimalZero` under `isLgl`; ISO 29500 does not, and the oracle has to settle it (class LO12). A level
with no placeholder at all and a counting format (181 levels in 22 curated documents) has the literal text
as its label, which is usually empty.

The result is `text`, **trimmed of whitespace at both ends** (the suffix is kept in `ListItem.suffix`, not in
the text, so the trailing `.` of `1.` stays and a trailing space in `lvlText` goes). `kind` is:

- `Counted { text }` when the level's format is a counting one and `text` is not empty
- `Bullet { glyph }` when the format is `bullet`, with the level text as it is (the glyph is often a
  private-use code of a symbol font, U+F0B7 for a round bullet, and it is passed through; mapping it is
  `projection`'s choice)
- `Plain` for `none`, and for a counting level whose label is empty

`clause_number` returns `Some` only for `Counted`. `full_context` is the label with the labels of its ancestors,
and is what Word's `REF \w` shows. It is built as Word's own cross-reference does: the label of the paragraph and,
for each shallower level of the same list that the paragraph's label does not already contain, that level's label,
with **trailing periods only** stripped from each part (not other punctuation: stripping all of it would turn
`(a)` into `(a` and `1(a)(i)` into `1(a(i`). `Heading1`'s label is `1.` and `REF \w` shows `26` without the period, so
the period goes from the full-context form and stays in the plain label. When a level's label already contains its
ancestors (`%1.%2.%3`, as in `bookmark.docx`) the label is the answer and nothing is prepended. When a label does
not (`%3` alone, `(%3)`), the parts are joined as they are, with no separator, and whether that matches Word is
unverified and untested by the corpus: register U16, and the form falls back to the plain label if the list
shape is unusual. The ancestor labels are **rendered again from the counters as they are now**, not remembered
from when the ancestor was last numbered: a level that never restarts (`lvlRestart` 0) keeps its old value under
a new parent, so the paragraphs level 0, 1, 0, 2 must give `2.1(a)` for the last, not `1.1(a)` from the first
parent (a review finding, now a test). The twelve cached `REF` results of `bookmark.docx` are the test of the common shape (7.2).
`\r` (relative context) is a different switch and shows the label relative to the referring paragraph. It
matched `\w` in that document only because every label there contains its ancestors, so the evidence check does
not use it as a test of `full_context` (7.2).

**`find(text)`** is an exact match on the `full_context` form with trailing periods stripped, with the plain label
(trailing periods stripped) as a fallback, in document order. It does not parse "clause 4.2" prose, and it does not
compose context across lists: in contracts the `(a)` list is often a separate `abstractNum` from the clause
headings, so `\w` gives `(a)` alone while a lawyer writes `4.2(a)`. Composing across lists, by walking back to the
nearest paragraph of a shallower list, belongs to `index`, which owns the reference graph. This matters to
the "clause 4.2" resolver in `index` and the readable `(a)` display in `projection`.

**Suffix and justification** are carried, not applied: the tab, space or nothing after the number is for
whoever renders the paragraph. **Right-to-left and locale.** Counters are the same in every language.
Text formats are English: `cardinalText` and `ordinalText` use the English words, because the reference
and Word choose the language from the label's `w:lang`, and doing it per language is out of scope for 1.0
(section 5).

### 4.5 Style linked lists, in one example

`bookmark.docx` is the model case. `Heading1` carries `w:numPr` with `numId` 47 and no `ilvl`; `Heading2`
carries `ilvl` 1 and `numId` 47. `w:num` 47 points at `abstractNum` 41, whose levels are named `Heading1`
to `Heading4` through `w:pStyle` and whose texts are `%1.`, `%1.%2`, `%1.%2.%3`. A paragraph in `Heading3`
has no direct `numPr`, so item 3 of 4.2 finds `numId` 47 in the style, item 4 finds level 2 by the
level's `pStyle`, item 5 finds list 41, and the counters give `26.3.1`: the level-0 heading has `w:start` 26 in
this document (there is one level-0 heading), the level-1 heading is the third, and so on. That chain is the
first fixture and the first Word-evidence check.

### 4.6 What a tracked change does to numbering, by mode

Word ties numbering to the paragraph mark. A paragraph whose mark is a tracked deletion merges into the
next paragraph when the revision is accepted, and the numbers of everything after it follow the accepted
text. The table is the model, and each row says what `presence` and `old` give.

| Situation | Final | Original | Redline |
|---|---|---|---|
| Paragraph with an inserted mark (`w:rPr/w:ins`), or moved in | numbered normally | does not exist: no number, advances nothing | numbered as in Final |
| Paragraph with a deleted mark (`w:rPr/w:del`), or moved away | does not exist: no number, advances nothing | numbered normally | **ghost**: shows the number the counters hold at its position, advances nothing |
| Paragraph with a deleted mark but not all its content deleted | its text joins the next paragraph, which is numbered as itself | numbered normally | ghost, as above |
| A deleted mark on the last paragraph of a cell or story | the mark is ignored: it exists and is numbered | numbered | numbered |
| Paragraph in a row with `w:trPr/w:ins` (or `cellIns`) | numbered normally | does not exist | numbered as in Final |
| Paragraph in a row with `w:trPr/w:del` (or `cellDel`) | does not exist | numbered normally | ghost |
| Tracked change of paragraph properties (`pPrChange`) | numbering from the current `pStyle` and `numPr` | numbering from `old` | numbering from the current properties |
| Tracked change of the list only (a `numPr` appears or disappears in a `pPrChange`) | the new list or none | the old list or none | the new one |
| Insertion inside a deletion (`Never`) | does not exist | does not exist | no number |

**The ghost rule.** For a paragraph that exists only in `Original`, Redline computes its number as if it
were a normal paragraph at that position in the list that `Final` produces, and then **restores the
counters**. The paragraph shows a number and the next live paragraph shows the number it would show
without it, so the two can display the same number: this is what Word's All Markup view does, and it is
how Docxodus (`TrackedChangesNumberingTests`) models it. A ghost has `ListItem.ghost` set so a format
can mark it as a deletion.

**Provenance and confidence.** The rule that a deleted mark advances nothing in Final, and that an
inserted mark advances nothing in Original, follows from "the paragraph merges into the next one when the
change is accepted" and is the behaviour the reference models. Which paragraph's properties survive a merge,
which is the numbering of the merged paragraph, is wordml's open question 5 and needs Word. We take the
following paragraph's properties (so the next paragraph shows its own number), which is also what the
reference does, and no corpus document tests it (one curated document has one deleted and one inserted
mark on numbered paragraphs). Confidence: medium for Final and Original, low for the ghost number in
Redline. The synthetic fixtures of section 8 pin the behaviour, and the Word oracle (section 7.4) is the
only way to confirm it. `compare` accepts revisions before it aligns and so reads `Final`; `projection`
reads the mode the view asks for.

**Other tracked and numbering-adjacent shapes that are not computed.** A `w:numPr/w:ins` (the numbering itself
inserted as a tracked change) is legal in the schema and is not interpreted: the paragraph is numbered from its
current `numPr`, and **no note says so**, because `NumFact` does not carry the element and `numbering` cannot see
the tree (none in the corpus; a `num_pr_revision` flag on `NumFact` would let a `Note` say it, and is a request to
`wordml` if a document ever needs it). `LISTNUM` fields advance the counters of the list they name in Word, and a
story that mixes them with numbered paragraphs gets `Note::ListNumField`, since the field results are text and are
not folded into the counters. The caller finds the fields (`PartsView::numbers` asks `Story::fields`) and says so
with `StoryNumbers::with_list_num_fields`; the corpus has one story that gets it. A level whose own `rPr`
has `vanish` hides its label in Word, and `ListItem.label_hidden` is set from the level's `rPr` (no document in the
corpus does it). Docxodus has a "continuation" rule (`ListItemRetriever.cs`, lines 1199 to 1250) that renders a
deeper-level item whose counter equals the parent's plus one as a continuation of a flat list, using level 0's
text (so `4.` and not `3.4`). It is a heuristic that is in neither the standard nor any Word evidence we have,
and we deliberately do not take it (register U22).

### 4.7 Cost of an edit

`StoryNumbers` is rebuilt from scratch for a story whose part changed, **and only when it is read** (2.3). A
tracked text edit changes no numbering fact, but it changes the part, so the table goes stale; the next reader
rebuilds it, at an estimated 3 to 20 ms for a 300-page story. Nothing in the edit path reads numbers per
operation, so the 50-operation budget is not touched, and a target given as a clause number is resolved once
before the batch.

Two remedies exist and neither is built until the gate in 8.5 asks for it. The cheap one: `StoryNumbers` keeps a
fingerprint of the facts it was built from, inside an `Arc` of its payload, so a builder that receives the stale
previous value (`derived` hands it over) recomputes the facts (5 to 15 ms of the cost, the part that reads XML),
compares, and returns the old table when no fact changed, which is every text-only edit. The other is
incremental: the builder saves the counter state every 256 paragraphs and resumes from the first paragraph whose
fact changed, using `PartsView::touched(part)` to know where, with the rule that a refreshed table must equal a
rebuild from scratch, which is `state`'s coherence test.

## 5. Number formats

The format of a level is `NumFmt` from `wordml`, with `Other(name)` for the rest. `numbering/format.rs`
maps a value to text with one function per family. The formats group by evidence, and a lower tier is
built only after the one above it passes its oracle.

| Tier | Formats | Evidence | Rule |
|---|---|---|---|
| 1, Word-verified | `decimal`, `decimalZero`, `upperRoman`, `lowerRoman`, `upperLetter`, `lowerLetter`, `ordinal`, `cardinalText`, `ordinalText`, `bullet`, `none` | all real documents | exact |
| 2, enclosed and width | `decimalEnclosedCircle`, `decimalEnclosedParen`, `decimalEnclosedFullstop`, `decimalFullWidth`, `decimalFullWidth2`, `decimalHalfWidth`, `numberInDash`, `chicago`, `hex` | `decimalEnclosedCircle` in 3 curated documents; others in the probe | Unicode tables |
| 3, scripts | the CJK counting and legal formats (`chineseCounting`, `chineseCountingThousand`, `chineseLegalSimplified`, `japaneseCounting`, `japaneseLegal`, `japaneseDigitalTenThousand`, `ideograph*`, `taiwanese*`, `koreanCounting`, `koreanLegal`, `koreanDigital`, `koreanDigital2`, `ganada`, `chosung`, `aiueo*`, `iroha*`, `vietnameseCounting`), Cyrillic (`russianLower`, `russianUpper`), Hebrew, Arabic, Thai, Hindi (`hindiVowels`, `hindiConsonants`, `hindiNumbers`, `hindiCounting`), `bahtText`, `dollarText` | the probe document only; the census names `hindi*` as the gap that matters most | table driven, in `format_scripts.rs` |
| `custom` | a `w:format` such as `001, 002, 003, ...` | none in the corpus | `w:format` of the digits pattern, else the format named by `val` (ISO 29500) |

Rules inside tier 1 (each one is a table-driven unit test, with values to 1,000 and at the edges):

- `decimal`: the number, negative as `-3`. `decimalZero`: two digits, `01` to `09`, then `10`.
- `upperRoman`, `lowerRoman`: standard subtractive numerals up to 3,999. Beyond that, and for zero and
  negative values, the level shows the decimal number, and the table records a `Note`. That Word does the same
  above 3,999 is unverified (register U21). The reference prints up to 10 `M`s, a named disagreement (class LO10).
- `upperLetter`, `lowerLetter`: `a` to `z`, then `aa`, `bb` to `zz` (the letter repeated, not base 26), so
  27 is `aa`, 28 is `bb` and 53 is `aaa`, which is Word's rule. The reference wraps at 780 and so do
  we, and zero and negative show decimal.
- `ordinal`: `1st`, `2nd`, `3rd`, `4th`, with `11th`, `12th`, `13th`, and `111th`.
- `cardinalText`, `ordinalText`: English words (`One`, `Twenty-one`, `Twenty-first`) with a capital
  first letter, as the reference writes them, up to 99,999. A value outside that range, zero or a negative
  shows decimal. The first letter is capitalised and the rest is lower case, which is a claim to be
  checked: the fixture `LIR001` to `LIR020` documents of Docxodus are lists in every one of these formats
  with paragraph text `1`, `2`, `3` and so on up to 3,500 (from the OpenXmlPowerTools test set, with no stored labels), which gives
  us input but not expected values, so the expected values come from the oracle (7.3).
- `bullet` and `none` have no counter text (4.4).

**Unknown formats** (a name not in the table, or a tier not yet built) show decimal, which is what Word
does for a value it cannot render, and the table records `Note::UnsupportedFormat`. `check` reports the
notes, so an agent is told "this clause number may not match Word" instead of being given a wrong one
silently.

**Language.** The text formats and the scripts depend on `w:lang` in Word (the reference has `fr-FR`,
`ru-RU`, `sv-SE`, `tr-TR` and `zh-CN` variants). 1.0 renders the English text formats and the script
formats in their own script, and ignores `w:lang`. The `Note` records that a document-language other than
English would change `cardinalText` and `ordinalText`, and the cost of adding a language is one function.

## 6. Performance

The budgets are `open` 300 ms, `view` 200 ms and `edit` of 50 operations 1 s on a 300-page contract (about
12,000 to 16,000 paragraphs in one part). Neither module is on `open`'s path: both are derived values,
built on first read. These are estimates from the sizes involved, not measurements, and each gets a
benchmark and a gate before the module is called done (8.5).

| Step | Work | Estimate | Where it counts |
|---|---|---|---|
| Parse and read `styles.xml` and `numbering.xml` into `StyleDefs` and `NumberingDefs`, and parse the theme and settings | the largest curated parts are 765 KB and 1 MB | 5 to 20 ms (an estimate from `xml` parse speed; the benchmark measures it on those two parts) | first `view`, and again after a style or numbering edit |
| `Cascade::build` | about 560 styles at most, an eager merge over each chain, 12 toggles each | 1 to 3 ms | first `view`, then after a style edit |
| `Story::num_facts` | one read of each paragraph's `pPr`: first child, `pStyle`, `numPr`, `pPrChange`, the mark flag | about 0.3 to 1 microsecond a paragraph, so 5 to 15 ms for 16,000 | each rebuild of a story's numbers |
| `StoryNumbers::build` | one pass, nine levels at most, one string per numbered paragraph | 2 to 5 ms for 8,000 numbered paragraphs | same |
| `hidden` for one page | a few hundred runs, two table reads and a scan of a short `rPr` each | well under 1 ms | each `view` |
| Memory | a `Props` per style (kilobytes), about 100 bytes per numbered paragraph | 1 to 2 MB at the extreme | |

**Measured once the first implementation existed** (a throwaway test, release build, this Mac, not committed
because the core may not read a clock): a 20,000-paragraph body with 16,000 numbered paragraphs across three
levels and 561 styles in chains of at most ten. Parsing the body took 8 to 10 ms (`xml`), the block scan 1 ms,
`StyleDefs::read` 0.6 ms, `Cascade::build` 2.0 ms, `Story::num_facts` 10 to 11 ms (`wordml`), and
`StoryNumbers::build` 6 to 7 ms. The first draft of the counters took 25.6 ms because it looked each level up in
the definitions for every paragraph; one table of the nine levels per `w:num`, with the text split into pieces
once, cut it to a quarter. Facts plus build are 17 to 18 ms against the 25 ms gate below, and the cascade is
well under its 5 ms gate. A 561-style chain, each style based on the one before, took 13.6 ms to resolve
(quadratic in the depth), which is a hostile shape: real chains reach ten. The benchmark gates stay as written,
to run in the testkit, which may read a clock.

So the cost of a cold `view` on a 300-page contract is the parse of the definitions and the two builds, about 20 to
60 ms, against a 200 ms budget, and a warm `view` pays nothing for either. The worst case that matters is a numbered story
rebuilt on every commit of a 50-operation batch: one rebuild per commit, not per operation, which `ops`
guarantees by not asking for numbers (4.7). The incremental rebuild is the escape hatch, and the gate that
would trigger it: **`StoryNumbers::build` plus `num_facts` on a 20,000-paragraph story must take 25 ms or
less** (12% of the `view` budget), measured on the concatenated-bodies document the census recommends
for 20,000-paragraph benchmarks, and `Cascade::build` on the 561-style document must take 5 ms or less.
A module that blows its budget is redesigned behind its interface, not worked around by callers.

## 7. The oracle

Word is not installed on the machine that builds this, and the numbers must match Word's. There is no
single independent source of truth that can be run on every machine, so the plan is a ladder, from
the most authoritative and least available down to the most available and least authoritative. Every
rung is independent of the Rust implementation.

### 7.1 What was checked about the candidates

| Candidate | State on this machine | Verdict |
|---|---|---|
| Word | not installed | the ground truth, available to a human or a Windows runner only; section 7.4 |
| LibreOffice in a container | not run when this was written | practical, no listening port needed (question 1 of 9.4) |
| Docxodus `ListItemRetriever` (C#, MIT, in `/tmp/oss/docxodus`) | no `dotnet` on the machine | read as a reference; running it is not practical here (7.3) |
| pandoc | not installed, and it does not compute list labels | no value for labels: it emits the list structure and `start`, not Word's text |

### 7.2 Word's own results, already in the corpus (no tool needed)

Word writes the numbers it computed into the documents it saves, in two places. Both checks obey four rules, from
the review, so that they test Word and not a lookalike:

1. **Only documents that Word saved count.** The manifest's `producer` must be Microsoft Word and its `word_fp`
   must be `y` (113 curated documents, 111 of them readable here). `docProps/app.xml` is not enough: python-docx's
   template and other tools write Word-like strings. `edge/smallFracOnlyOffice.docx` has two `REF \r` fields with
   OnlyOffice's cached results (`7.1.2.62`) that point at bookmarks that do not exist, and would otherwise be
   counted. The same filter applies to the `cnfStyle` and theme-colour checks of 3.4 and 3.5.
2. **Dangling bookmarks are counted and skipped.** A field whose bookmark names no paragraph is a note with a
   count, not a failure and not a silent pass.
3. **The TOC check is two-sided.** A heading with a number must have that number in its TOC line, and a heading
   with none (for instance `numId` 0) must have none. Skipping unnumbered headings would hide the false positives
   that matter.
4. **The number is the TOC line minus the heading's own text,** not the first token before a tab: `content.docx` has
   ten `Heading1` paragraphs with `numId` 0 whose literal text `Appendix A:` plus a tab looks like a number.

- **`REF ... \n`, `\r`, `\w` fields.** The cached result of a cross-reference to a numbered paragraph is the number Word
  showed when it last updated the field. In the curated set, `bookmark.docx` (Open XML SDK test files, authored in
  Word) has 12 `REF` fields, 11 with `\w \h` and one with `\r \h`, whose results are Word's numbers, all for headings
  numbered through styles (`26.4`, `26.3.1`, `26.7.1.9`). The check resolves each field's bookmark to its paragraph
  and compares the cached result with that paragraph's `full_context` in `Final` for `\w`. `\n` is the label
  without trailing periods, and `\r` is relative context, so neither is a test of `full_context`: `\r` agreed
  with it in `bookmark.docx` only because every label there contains its ancestors. It needs no new code beyond the
  field index `wordml` already has.
- **Table of contents lines.** A TOC entry for a numbered heading holds the number as text before the heading
  text (`1.1<tab>Title`). Nine curated documents have a TOC and numbered paragraphs. The check finds the target
  heading through its `PAGEREF` bookmark, removes the heading's own text from the line, and compares what is
  left with the `clause_number` of that paragraph (two-sided, rule 3).

The check is now a test (`state/tests_numbers_corpus.rs`) and passes on the curated corpus: 111 documents that Word saved (by the manifest), all 11 `REF \\w` results equal the full-context number (one `\\r` field is skipped, none dangles), and 123 table-of-contents lines are checked (93 for numbered headings, 30 for unnumbered ones) with no difference, 10 more skipped because the heading text is not in the line. The review reproduced it first with a throwaway script: all 12 `REF` results of `bookmark.docx`,
60 of 60 TOC lines of `content.docx` (50 numbered up to `5.2.20`, ten unnumbered), 8 of 8 in
`DML_unique_ID_compare.docx` (one heading skips a level and Word shows `1.1.1`, which is the evidence for the rule
of 4.3 item 3 and against showing `1.0.1`), and 11 unnumbered headings in three other documents.

This is small (one document with 12 exact data points, and a few TOCs), but it is real Word output, it runs
everywhere, and it tests the cases that matter most for lawyers: style-linked multilevel headings with
`%1.%2.%3` texts. The test lists which documents it used and fails if the count of checked fields falls below the
known number, so a change in the corpus cannot silently empty it. The same extraction runs over the whole of
whatever corpus is fetched, so as Word-authored contracts arrive (7.6) the check grows without a code change.

### 7.3 LibreOffice, read through UNO

LibreOffice computes list labels itself and exposes each paragraph's label as the `ListLabelString` property
of the paragraph. This rung is written so that it can be added later with no change to the design: the runner
only *produces* golden files (a file format fixed here), the tests only *read* them, and a missing golden is
reported, never silently skipped. **The numbering runner and its goldens are not implemented**, so for clause
numbers the ladder is 7.2 alone. (`cargo xtask oracle libreoffice` exists, but it compares text after accepting
and rejecting changes, not list labels.) The plan:

- A script, run once per corpus change and not per test (`xtask oracle numbering`, or a runner in
  `stetdoc-testkit`, which the architecture already reserves for oracle runners), starts a container with
  `--network none` and the corpus mounted read-only. Inside, `soffice --headless --invisible --norestore
  --accept="pipe,name=stet;urp;"` runs and a Python script using the `uno` module connects through the
  **named pipe**, not a TCP port. It loads each `.docx`, walks the paragraphs of the body, headers, footers
  and footnotes in story order, and writes one row per paragraph: document hash, story, a hash of the
  paragraph's text, its `w14:paraId` if it has one, the paragraph ordinal, and `ListLabelString`. **Paragraphs
  are paired by text hash plus `w14:paraId`, with the ordinal as a fallback**, because LibreOffice's model differs
  from ours where ordinals break (headers per page style, frames, deleted paragraphs). The first runner covers the
  **body only**; headers, footers and footnotes follow once the body pairs reliably.
- The result is a **golden file**, one small TSV per document keyed by the SHA-256 the corpus manifest
  already records, committed under `crates/stetdoc-core/src/numbering/testdata/` (text only; the documents
  themselves are never committed). The tests compare our `clause_number` against the golden, so they run
  everywhere and need no LibreOffice. The oracle skips locally, as the architecture says, only when a
  golden is missing for a document, and the check refuses to pass if the number of compared paragraphs falls.
  Tool and image versions are recorded in the golden's header, and regenerating is an explicit act.
- A Debian image with `libreoffice-writer-nogui` and `python3-uno` is enough, so the image is built from an
  `apt` layer with no application code in it.

**The disagreements to expect** between LibreOffice and Word, each of which is a named class in a small
table in the test (`oracle_divergences.tsv`), so that every difference is either a bug or a class:

| Class | What differs | Expected winner |
|---|---|---|
| LO1 | `w:num` instances over one `abstractNum`: LibreOffice may keep each `numId` as its own list and restart where Word continues | Word (python-docx's restart recipe depends on continuing); the four curated documents where it applies are inspected by hand |
| LO2 | any tracked paragraph mark, row or `pPrChange` (a changes-aware list model differs) | none from LibreOffice: fixtures and Word only (4.6) |
| LO3 | `w:start` omitted: ISO 29500 says 0, LibreOffice may start at 1 | ISO 29500, which `wordml` already follows |
| LO4 | more than one `startOverride` in one `w:num`, or an override on a level other than the paragraph's first | the standard and Word; the reference is wrong here too |
| LO5 | formats LibreOffice maps differently: `ordinal`, `chicago`, `hex`, `numberInDash`, the Hebrew, Thai, Hindi and CJK formats | per format; accept agreement of two of Word-known tables (the reference's), ICU and LibreOffice, and record the rest |
| LO6 | the style-link level: style `numPr` with an `ilvl` that disagrees with the level's `pStyle` | ISO 29500 (the level's `pStyle`); pinned by a fixture |
| LO7 | a direct `numPr` with no `numId`: we say not a list item (as the reference); LibreOffice may inherit the style's `numId` | unknown; recorded (U8) |
| LO8 | `empty_section_break` paragraphs, hidden paragraphs, `specVanish` joins | unknown: recorded as LibreOffice's answer, not asserted |
| LO9 | `%k` for a level deeper than the paragraph's own: ISO 29500 renders nothing, the reference clamps | ISO 29500 |
| LO10 | roman numerals above 3,999, letters beyond 780, spelled-out numbers above 99,999 | decimal fallback, as documented in section 5 |
| LO11 | numbering that points at missing definitions, a `numId` with no `w:num`, a level outside 0 to 8 | none: LibreOffice repairs, we report a `Note` and show no number |
| LO12 | `isLgl` with `decimalZero` | unknown |
| LO13 | bullets: LibreOffice returns the glyph, we return `Bullet { glyph }` | compare glyphs only for non-private-use code points |
| LO14 | text-box lists | out of scope in 1.0 |
| LO15 | `lvlRestart` other than 0 (a restart after a named level) | the standard; LibreOffice's support is partial |
| LO16 | `isLgl` at all: older LibreOffice versions are believed to ignore it | the standard |
| LO17 | heading lists that LibreOffice maps to its outline numbering (`tdf141966` is that torture test) | Word; the golden records LibreOffice's answer only as a divergence |
| LO18 | footnote and endnote stories (one story per part for us, LibreOffice may restart per note) | unknown; recorded |
| LO19 | lists whose first paragraph is deeper than level 0 (`1.1.1` against `0.0.1`) | Word, per the evidence of 7.2 |

If LibreOffice is unavailable the golden files still run. If a golden is missing for a document (a new
corpus entry), the test names the document and says how to regenerate it, and does not skip silently.

**Docxodus's `ListItemRetriever`** (about 1,400 lines, plus per-language formatters) is read as the
reference algorithm, as the architecture says. It is not run: it needs .NET and `DocumentFormat.OpenXml`,
neither of which is here, and a C# runner would add a toolchain to the test setup for one rung that LibreOffice
already covers better. What is taken from it is the list of cases in section 4 (the five Word behaviours
that the tests name: shared lists, restarts, the deleted mark, single-level lists, empty section-break
paragraphs) and the format tables of section 5. What differs from it on purpose is in the class table
above (LO1, LO4, LO6, LO7, LO9, LO10). If someone with .NET wants a second reference column, the same golden
format takes it.

### 7.4 Word, when someone has it

A checklist (a `.md` beside the fixtures, generated from the register in 7.5) lists about 22 hand-built fixtures, one
per "Word" row of the register, that each isolate one rule, with the expected number written next to each. A human, or a Windows runner, opens each in
Word with All Markup, Simple Markup and Original, and records what Word shows. The answers become a golden
column of the same format as 7.3, so the confidence label of a rule changes to "verified" or the rule changes.
The architecture already says Word oracle tests run before each release. Until a person with Word runs it
(open question 2 of 9.4), `FIDELITY.md` says which rules are unverified and the checklist stays ready.

### 7.5 The register of unverified choices

Each row is a rule that no oracle available in the build environment can confirm. Each has tests that pin
what we do (so a change is deliberate; the "Fixture" column names them, paths under `crates/stetdoc-core/src`, and every one of those tests carries a `// Register: U..` line) and is on the Word checklist where the last column says so. **"Reference
says" is what OpenXmlPowerTools and Docxodus do** (`FormattingAssembler.cs`, `ListItemRetriever.cs`).
**"We do"** is this design. The rule for choosing, decided in review: where the standard's text or the corpus
supports a deviation we keep it; otherwise we follow the reference, and the basis column says which applied. The
first draft claimed the reference was silent on these rows. It is not, and several rows were wrong about it.

| ID | Rule | Reference says | We do | Basis | Fixture | Settles it | Word |
|---|---|---|---|---|---|---|---|
| U1a | Toggle properties across style kinds | exclusive-or only the table style against the rest; paragraph style against character style is a plain merge, an explicit false wins | exclusive-or across table, paragraph and character style; false is no toggle | **deviation, kept**: ISO 29500 and Word's documented behaviour; corpus has 60 runs in 3 documents that differ | `styles::tests_cascade::bold_follows_the_toggle_rule_over_every_combination`; `styles::tests_cascade::a_bold_character_style_in_a_bold_paragraph_style_is_not_bold`; `styles::tests_hidden::a_hidden_character_style_over_a_hidden_paragraph_style_shows_the_text`; `styles::tests_tables_theme::a_table_style_conditional_format_reaches_the_cell_text` | LibreOffice (believed to match Word); Word | yes |
| U1b | No exclusive-or inside one `basedOn` chain | the chain is rolled up with a plain merge | nearest style wins | same as the reference | `styles::tests_cascade::inside_one_based_on_chain_the_nearest_toggle_wins` | Word | yes |
| U2 | A toggle in the document defaults | the defaults are the lowest level of a plain merge | the defaults are the starting value that style kinds exclusive-or against | **deviation**: ISO reading, unverified; 1 to 2 curated documents | `styles::tests_cascade::bold_follows_the_toggle_rule_over_every_combination` | Word | yes |
| U3 | A deleted mark merges into the next paragraph, which numbers as itself (Final); an inserted mark does the same in Original | same | same | follows the reference | `numbering::tests_modes::a_deleted_mark_advances_nothing_in_final_and_is_a_ghost_in_redline`; `numbering::tests_modes::an_inserted_mark_advances_nothing_in_original`; `numbering::tests_modes::a_deleted_mark_on_the_last_paragraph_is_ignored`; `numbering::tests_modes::a_paragraph_in_a_deleted_row_is_absent_in_final`; `numbering::tests_modes::a_paragraph_property_change_is_read_from_the_old_properties_in_original` | Word only | yes (2) |
| U4 | A hidden paragraph and a `specVanish` paragraph advance the list | no rule, so they count | they count | follows the reference | `numbering::tests_register::a_hidden_paragraph_still_advances_the_list` | LibreOffice shows its answer; Word | yes (2) |
| U5 | An empty paragraph that only carries a section break | not a list item | not numbered | follows the reference | `numbering::tests_counters::an_empty_section_break_paragraph_is_not_numbered` | LibreOffice shows its answer; Word | yes |
| U6 | Level of a style-numbered paragraph | the style's `ilvl` (`ListItemRetriever.cs` 646 to 669) | the level whose `pStyle` names the style, then the style's `ilvl`, then 0 | **deviation, kept**: ISO text for `pStyle` and `numPr` ("any numbering level defined by the numPr element shall be ignored, and instead this element shall specify the numbering level"); differs on 1 corpus document (`tdf141966`) | `numbering::tests_counters::a_style_supplies_the_numbering_and_the_level_names_the_style`; `numbering::tests_counters::a_style_without_a_matching_level_uses_its_own_level` | LibreOffice; Word | yes |
| U7 | What the numbering level's `pPr` contributes | only `w:ind` | only `w:ind` | follows the reference | `styles::tests_cascade::only_the_indent_of_a_numbering_level_reaches_the_paragraph` | Word | yes |
| U8 | A direct `numPr` with no `numId` | not a list item (lines 421 to 427) | not a list item | follows the reference (changed from the first draft): no corpus evidence either way (one paragraph in one document), and the reference was run against Word documents | `numbering::tests_counters::direct_numbering_wins_over_the_style_as_a_unit` | Word | yes |
| U9 | A shallower level with no value yet | shows its start and counts as entered | same (with N's override as the start) | follows the reference; corpus evidence (`DML_unique_ID_compare.docx`: Word shows `1.1.1`, not `1.0.1`) | `numbering::tests_counters::a_list_that_starts_deep_shows_the_start_of_its_parents` | Word evidence (7.2) | no |
| U10 | A placeholder for a deeper level than the paragraph's own | clamps to the last counter (line 1336) | renders nothing | **deviation, kept**: ISO text for `lvlText` ("any number of a level higher than this level shall be ignored") | `numbering::tests_counters::a_placeholder_for_a_deeper_level_renders_nothing`; `numbering::tests_counters::the_iso_level_text_example` | LibreOffice; Word | yes |
| U11 | `isLgl` with `decimalZero` | keeps `decimalZero` (line 1350) | plain decimal | **deviation, kept**: ISO text for `isLgl` ("converted to their decimal equivalents") | `numbering::tests_counters::is_lgl_shows_every_level_as_decimal` | Word | yes |
| U12 | `startOverride` bookkeeping | one "already used" flag per `numId`, so a second overridden level is wrong | a set of (`numId`, level) pairs that have started; restart from the override whenever the pair is not yet started or the level restarts | **deviation, kept**: ISO text for `startOverride` and the A, B, A case, which gives 1, 2, 3 | `numbering::tests_counters::a_start_override_restarts_on_first_use`; `numbering::tests_counters::shared_list_continue_a_b_a`; `numbering::tests_counters::overrides_on_two_levels_each_apply` | LibreOffice for the override; Word for sharing | yes (2) |
| U13 | Counters of lists reached through `numStyleLink` | keyed by the `w:num`'s own `abstractNum` | same (changed from the first draft) | follows the reference; no paragraph in the corpus reaches a list this way | `numbering::tests_register::counters_of_style_linked_lists_are_keyed_by_the_nums_own_abstract_definition`; `numbering::tests_counters::num_style_link_takes_levels_from_the_linked_list` | Word | yes |
| U14 | `rFonts` theme attribute against explicit attribute in different levels | `FontMerge` per slot, with later `AdjustFontAttributes` | one slot with two spellings, the higher level replaces both | pinned by a fixture; theme and explicit agree on all 202 resolvable cases of the corpus | `styles::tests_cascade::fonts_merge_by_slot_and_a_theme_attribute_replaces_its_explicit_twin` | LibreOffice | no |
| U15 | The ghost number in Redline | same | same | follows the reference (`TrackedChangesNumberingTests`) | `numbering::tests_modes::a_deleted_mark_advances_nothing_in_final_and_is_a_ghost_in_redline`; `numbering::tests_modes::a_ghost_at_the_start_of_a_list_leaves_the_counters_empty`; `numbering::tests_modes::two_ghosts_in_a_row_show_the_same_number`; `numbering::tests_modes::a_ghost_between_two_levels_does_not_disturb_the_levels_below` | Word only | yes (2) |
| U16 | `full_context` across levels whose labels do not contain their ancestors | (no such function) | join the labels, trailing periods only stripped | the 12 cached `REF` results cover labels that contain their ancestors; the rest is unverified | `numbering::tests_counters::full_context_and_find`; `numbering::tests_register::full_context_shapes`; `numbering::tests_register::full_context_re_renders_ancestors_from_the_current_counters`; `state::tests_numbers_corpus::word_cached_numbers_equal_ours` | Word | yes |
| U17 | Where the numbering level's indent sits | above the paragraph style for a direct `numPr`, below for style numbering | same | follows the reference | `styles::tests_cascade::numbering_indent_sits_above_the_style_for_direct_numbering_and_below_for_style_numbering` | LibreOffice; Word | yes |
| U18 | A level's `pStyle` matched against a derived style | (not applicable: uses the style's `ilvl`) | the paragraph's own style id only | a search up the `basedOn` chain changes 4 paragraphs, all in `tdf141966` | `numbering::tests_register::a_style_is_matched_to_a_level_by_its_own_id_only` | LibreOffice | yes |
| U19 | `numId` 0 on a paragraph drops the indent that comes from its style | `IsZeroNumId` removes `w:ind` (line 2379) | same | follows the reference | `styles::tests_cascade::numbering_removed_drops_the_style_indent_but_not_the_direct_one` | Word | yes |
| U20 | Order of conditional table formats where the first row meets the first column | columns, then rows (`TableStyleOverrideTypes`, line 1973) | the reference's order | follows the reference; ISO's list gives rows before columns; no corpus evidence; Word's own `cnfStyle` (3.4) can only tell which regions apply, not their order | `styles::tests_tables_theme::table_regions_apply_in_the_references_order` | Word | yes |
| U21 | Roman numerals above 3,999 | prints up to ten `M` | decimal fallback | unverified; soft claim | `numbering::tests_format::roman_numerals` | Word | no |
| U22 | The "continuation" heuristic (a deeper item whose counter is the parent's plus one renders as a flat list with level 0's text) | applies it (lines 1199 to 1250) | does not | **deliberately not taken**: in neither the standard nor any Word evidence | `numbering::tests_register::the_continuation_heuristic_of_the_reference_is_not_applied` | Word | no |

The cascade rows (U1, U2, U14, U17, U20) matter little to clause numbers, and the others decide what a lawyer sees next
to a clause. The text formats' capitalisation and fallbacks (section 5) are pinned by tests but are not on the
checklist, since the tier-1 formats are the ones the Docxodus tables and LibreOffice both give.

### 7.6 More Word evidence, metamorphic checks and a second reference

Word evidence is the main lever without Word, and it grows with the corpus:

- **More Word-authored contracts.** Fetch, licence permitting, documents full of `REF \w \h` and tables of
  contents: NVCA model documents, UK Crown Commercial Service model contracts. The census names the lack of such
  documents as its largest numbering gap. The 7.2 extraction picks them up with no code change.
- **`STYLEREF \n` and `\w` in headers.** Headers and footers often show the current clause number (the running
  head of a contract), with a cached result per page: another Word-computed number.
- **Word-rendered PDFs published with the same file** (the EU Council drafts) give the printed labels. A PDF text
  extraction is a third, slower column; it is a one-off, not a test dependency.
- **Metamorphic tests**, which need no oracle: permuting `numId` and `abstractNumId` changes no label; splitting a
  `w:num` into two copies with no overrides changes no label; inserting unnumbered paragraphs changes none;
  converting the document to Strict changes none.
- **A second reference column.** superdoc's `numbering-manager.ts` (in `/tmp/oss`; Node is installed) can run
  over the same documents. Its model differs (an override scopes counters to its own `numId`), so it is a second
  column for LO1 and LO4 and not an authority.

## 8. The test plan

Rule 5 of `ARCHITECTURE.md` gives `numbering` direct unit tests, and `styles` is tested through the interface
(`Document`) and by unit tests of its pure merge rules. Nothing in
these tests needs an I/O exception except the oracle runner in the testkit.

### 8.1 `numbering`, direct unit tests (no XML)

Tests build `NumFact` lists and `NumberingDefs` by hand through small helpers, so a case is five lines.

- **Formats.** Each tier-1 format at 1 to 30, 98 to 102, 999, 1,000, 3,999, 4,000, 99,999, 100,000, 0,
  negative and the `i64` limits, with the expected text from the rules of section 5. Each tier-2 and
  tier-3 format against its table, with the table's source named in the test.
- **`lvlText`.** `%1.%2(%3)`, `StringA %2 StringB %1 StringC %3` (the ISO 29500 example, expected
  per the standard), `%%`, a lone `%`, a placeholder above 9, a placeholder for an undefined level, the
  empty text.
- **Counters.** Plain list; two levels with restart; three levels; the ISO 29500 `lvlRestart` example
  (`1) a) i) ii) 2) a) iii) iv)`); `lvlRestart` of `0`, of `1` and of a value above the level; a list that
  starts at level 2; a jump of two levels; a return to level 0; `w:start` absent (starts at 0), `w:start` 0,
  negative and huge; saturation at `i64::MAX`.
- **Lists.** Two `w:num` over one `abstractNum`, plain (continue) and with `startOverride` (restart);
  an override on level 0 and one on level 1 in the same `w:num` (the reference gets one wrong); an
  override that equals the abstract start; `lvlOverride` with a replacement `w:lvl`; `numId` 0 over a
  numbered style; a `numId` with no `w:num`; an `abstractNum` with no levels.
- **Style links.** Numbering from the style (level from the level's `pStyle`, from the style's `ilvl`,
  from neither, and the two disagreeing); `basedOn` chain with the `numPr` on the parent; a style `numId` of 0
  on the child; `numStyleLink` and `styleLink` with two `w:num`; a `numStyleLink` cycle (bounded, `Note`);
  a dangling style.
- **Modes.** Every row of 4.6, in Final, Original and Redline: an inserted mark, a deleted mark, a moved
  pair, a deleted mark on the last paragraph of a cell, rows and cells with revisions, a `pPrChange` that
  adds a `numPr`, removes one, changes the style, changes the level, two ghosts in a row, a ghost between
  two live paragraphs of one list, a ghost at the start of a list. The expected values in each are
  written out, with the rule's name.
- **Stories.** The same `numId` in the body, a header and a footnote, each with its own counters (the
  `NumFact` lists are separate); a table between two numbered paragraphs; one paragraph in a block
  content control.
- **`shared_list_continue`** has three cases: two `w:num` over one list with no override (the second continues); a
  second `w:num` with an override (it restarts); and the A, B, A case, where A has an override of 1 and B has none,
  which numbers **1, 2, 3**.
- **Properties, deterministic and hand-rolled** (no new dependency; `wordml` does the same in
  `tests_refresh_random`): a seeded generator makes random definitions (levels, starts, restarts, overrides,
  formats) and random fact lists, and checks that the build never panics, equals a second build,
  gives non-decreasing counters inside a restart-free run, and is monotone under appending a paragraph (the
  numbers of earlier paragraphs never change). It also checks the mode relation: a list with no revisions
  gives the same table in all three modes, and a list with only deleted-mark paragraphs gives, in
  Original, the table of the same list without the revision.
- **Hostile.** Nine levels, a `numStyleLink` cycle, a style `basedOn` cycle, a `lvlRestart` out of range,
  `i64::MIN` start, a million facts of one list (linear time, bounded memory), a lvlText of 64 KB.

### 8.2 `styles`, unit tests of the merge rules

- **Toggles, and the reference's difference.** `tdf154370.docx` (LibreOffice's `ParaStyle` and `CharStyle` with
  `BoldItalicCapsEmbossedStrike`, and `Hidden1` over `Hidden`) and its synthetic twins: a bold character style in a
  bold paragraph style is not bold, a hidden character style in a hidden paragraph style is not hidden, a direct
  `w:b` over both is bold; and the reference's rule written out as the "what OpenXmlPowerTools would say" column of
  the fixture's comment, so the deviation is visible (U1a).
- **Toggles.** The truth table over (default, table, paragraph, character, direct) with each of true,
  false and absent, for each of the twelve properties, with the rule of 3.3; one test for each property
  that is not a toggle (`dstrike`, `webHidden`, `specVanish`) showing replacement.
- **Merge kinds.** `rFonts` with the `ascii` and `asciiTheme` interplay, `ind` with `hanging` and
  `firstLine`, `spacing` by attribute, `tabs` by position with a `clear`, `pBdr` by side, `numPr` by
  child.
- **Levels.** The cascade order with every level setting a different `sz`, and the same with numbering
  from a paragraph and from a style (3.1); the Original-mode snapshot replacing direct formatting; a
  chain of nine styles; a dangling `basedOn`; a cycle; a duplicate id (the first wins).
- **Tables.** The cell where the first row meets the first column, in the reference's order (U20), and with
  `nwCell` defined; the conditional order (each pair of regions that could overlap), `tblLook` as hex and as
  attributes, banding on and off, band size 2, first column and first row meeting in a cell, a nested
  table (inner style only), a merged cell, `cnfStyle` disagreeing (a note).
- **Theme.** Each font slot, each of `bg1`, `tx1`, `accent1` to `accent6`, tint and shade, a missing
  theme (the cached `w:val`), a missing mapping (standard mapping).
- **Hidden.** `vanish` through the paragraph style, the character style, the table style and directly;
  a direct `vanish` of `0` over a hidden style; `webHidden` (reported separately); a `specVanish` mark
  (`JoinsNext`) and the same on a run (ignored); a hidden mark; `looks_hidden`: `TinyText` at 4 and 5 half-points,
  `WhiteText` on a white run, on a run over a white shading and over a coloured shading (not reported); and the real documents of the census (the 7 uses
  of a hidden style).

### 8.3 Interface and corpus tests

These run from `stetdoc-core` over the fetched corpus (`state/tests_cascade_corpus.rs` and
`state/tests_numbers_corpus.rs`, which find the documents through the testkit's `corpus` module).

- **`cnfStyle` as an oracle (3.4).** For every Word-authored document with `cnfStyle`, the regions computed from
  `tblLook` and position equal the set Word wrote, row and cell, with a floor on the count (7 curated documents).
- **Theme against Word's cached values (3.5).** Computed colour against the cached `w:val` (miss rate under 5%,
  within one per channel) and resolved font against the explicit attribute, with floors on the counts.
- **Totality.** Every document of the curated set that opens: numbering and cascade build for every
  story without panic, in all three modes, and every `Note` is one of a known set. 300 documents.
- **Word evidence** (7.2). The 12 `REF` fields and the TOC lines, with the counts guarded.
- **Golden comparison** (7.3). Every document that has a golden, every counted paragraph, with the
  divergence classes as the only allowed differences. The count of compared paragraphs and the count of
  excused ones are printed and have floors.
- **Law support.** `clause_number` is read-only in every projection (`readonly_region` on an edit that
  changes it), and compare's comparison text never contains one: the tests for both live with those
  modules, and this module's contribution is a `StoryNumbers` that has no text ranges.
- **Snapshots.** One snapshot per fixture of 8.1's modes table and per corpus contract (the EU AI Act
  drafts, the open-agreements template): the list of (ref, clause number) pairs. A snapshot changes only when
  the intent changes, never to fix a law.

### 8.4 Fixtures

Each fixture says what must happen, per `AGENTS.md`. They are built with `stetdoc_testkit::fixtures`, and
a short builder for numbering (an `abstractNum` with levels, `w:num` with overrides, paragraphs with
`numPr` or a style) belongs in the testkit's `parts` module, with one fixture per row of 4.6, per style-link case, per `lvlRestart` case, per format
family. The real documents that serve as fixtures, from the census, with LibreOffice's `tdf154370.docx` (toggles through
styles, `Hidden1` over `Hidden`) and `tdf141966.docx` (heading lists, derived styles, U6 and U18): `bookmark.docx` (style-linked
headings, 12 Word results), the EU AI Act drafts (961 and 766 numbered paragraphs, Word), the
open-agreements template (112), `docx-editor-numbering.docx` (shared list with five `w:num` and four
overrides), a LAB lettered list with a `startOverride` per `w:num`, and the docx.js probe for formats.
The counterparty redline with numbering, `isLgl` and `lvlRestart` is the census's own gap and the reason the
Word checklist of 7.4 exists.

### 8.5 Benchmarks and gates

A benchmark builds a 20,000-paragraph document by concatenating real bodies (the census's own method) with
about 8,000 numbered paragraphs and 561 styles and gates the figures of section 6. They run nightly with
regression alerts, as the architecture says. One more gate comes from the decision to compute numbers lazily
(2.3): **50 interleaved writes on the largest body, then one `numbers` read, must stay within the 1 s edit budget**
(so the writes must not trigger a rebuild each, and the single read costs one). If it fails, the remedy is the
incremental refresh through `PartsView::touched(part)` of 4.7.

## 9. Build order, other documents, what others must do, open questions

### 9.1 Build order

**Status.** Built and passing `cargo xtask check`: the numbering core (formats of
tiers 1 and 2, labels, bindings, counters, the three modes, full context and `find`, note locations, `level_site`;
unit tests including a seeded random test, the metamorphic checks and the register tests), the cascade (merge
rules, toggles, the three style kinds, table styles, themes, `hidden`, `looks_hidden` with the near-white rule,
`mark_effect`, `outline_level`, the reads that build a site; unit tests including the agreement test of 3.3),
`state/cascade.rs` and `state/sites.rs` (the accessors, the exported dependency lists, `SiteHandle`; tests over
small packages, including what each part invalidates, a `Tx` that staged the styles, the Strict relationship types
and `add_part`), the corpus laws of 7.2 and 3.4 to 3.5 in `state/tests_cascade_corpus.rs` and
`state/tests_numbers_corpus.rs`, which open every document through `State::open` and read it through the
accessors: totality over every item of every paragraph of **286 documents** in three modes (810 stories, 37,329
paragraphs, 85,925 runs, 7,916 numbered paragraphs, all of them with a level site; the floors sit just under those
counts, and nothing is skipped: `stress025.docx` is a refused document of the hostile tier and not in
the set), Word's cached `REF` and TOC numbers (111 Word-saved documents by the manifest), `cnfStyle`, theme colours
(at least 1,101 of 1,236 tinted colours exact) and fonts, and the benchmarks of 8.5 (`bench/numbering.rs`).
Not implemented: the tier 3 script formats (they show decimal with a note), the LibreOffice numbering runner and
goldens of 7.3, and the numbering checklist document of 7.4.

The build went in this order:

0. **The `state` side**: `state/cascade.rs` with the accessors and the dependency lists of 2.3,
   `state/sites.rs` with `SiteHandle`, and in `state/mod.rs` the two `mod` lines and the re-exports of `Dep`, `PartId`,
   `RunKey` and `SiteHandle` that the callers need to name the types in their signatures.
1. **Numbering core** (`format.rs`, `label.rs`, `counters.rs`), tier-1 formats, over hand-built facts and
   definitions, with the unit tests of 8.1 that need no other module. This is a pure module and needs
   nothing from `wordml` beyond `NumberingDefs`.
2. **Cascade core** (`merge.rs`, `toggle.rs`, `cascade.rs`, `hidden.rs`) over `StyleDefs` and `PropsRef`,
   with 8.2's merge, toggle and hidden tests, no tables and no theme.
3. **The adapters**: `NumFact` into `StoryNumbers` (4.1, 4.2, 4.6), `Cascade::style_numbering`, and the
   `Site` builders. Needs `wordml`'s `num_facts`, the `PropsRef` accessors and the re-export of `SpanTree`
   (9.3).
4. **Theme and tables**: `theme.rs`, `tables.rs`. Needs `TableSite`.
5. **Tiers 2 and 3 of the formats** and the script tables, as the oracle allows.
6. **Oracle tooling**: the Word-evidence checks (no tool), then the LibreOffice runner and goldens, then the
   Word checklist.

Each step passes `cargo xtask check`.

### 9.2 Changes to other documents

Made with the design, because the table and the code must change together:

- `ARCHITECTURE.md`: the module table row for `state` reads "Committed state, staging and
  derived caches", with `State::{cascade, numbers}` in its interface, and the Allowed dependencies row for `state` is `api`, `package`, `xml`, `wordml`, `refs`,
  `styles`, `numbering`, and the module table rows now read `Cascade::{effective, hidden, looks_hidden}` for `styles`,
  `StoryNumbers::{build, clause_number, full_context}` for `numbering`, and `State::{cascade, numbers}` is added to the
  `state` interface. The `//!` contracts of `state`, `styles` and `numbering` say the same.

With the code:

- "Where do I change X": "Add a hidden-text rule" points at `styles/hidden.rs`, and "Fix a numbering bug" at
  `numbering/counters.rs` (the counter rules) or `numbering/format.rs` (a number format).
- The "Start reading" lines of the two contracts become `styles/cascade.rs` and `numbering/counters.rs`.
- `GLOSSARY.md`: **Clause number** gains "the label exactly as Word shows it, without the suffix, and separately
  the full-context form `1.2(a)`"; new terms **list item** (a paragraph that Word numbers or bullets, with its
  level and list), **ghost number** (the number a deleted paragraph shows in a redline), **toggle property**
  and **looks hidden** (text that Word shows and a reader cannot see).
- `FIDELITY.md`: the register of 7.5, so that every unverified rule is stated where users look.
- `CHANGELOG.md`: a line under Unreleased when the first behaviour lands.

### 9.3 What other modules provide

`wordml` provides all of these:

1. `Story::num_facts(tree) -> Vec<NumFact>` with the shape of 2.2, `OldPPr`, and a four-valued `Presence` that is
   the text model's own wrapper rule (rows, cells and block wrappers). Text-box content is not listed. Built.
2. `Paragraph::ppr()`, `mark_props()`, `run_props(&Item)` and `run_style(&Item)`, public, as `PropsRef` and
   `Cow<str>`. They are methods on `Paragraph` because a text map holds no tree reference. Built.
3. `Story::table_site(tree, i)` and `Story::table_sites(tree)` (every entry, linear in a large table), giving a
   `TableSite { style_id, look: TblLook, row, row_count, column, column_count, span, v_merge, row_cnf, cnf }` for
   the innermost table only. `TblLook` uses the attributes if any is written and the hex `w:val` otherwise. A
   `w:p` standing directly in a row is opaque and has no entry. Built. `styles` takes one `TableSite` per
   paragraph, so `projection` should call `table_sites` once per story.
4. `NumFmt::Custom(Box<str>)` from `w:format`, read from the taken branch of the `mc:AlternateContent` of a level
   (a direct `numFmt` wins; `val="custom"` without a format is `Custom("")`). Built.
5. `pub use crate::xml::SpanTree;` in `wordml`. Built.

`state`: the two derived entries of 2.3 as `State::cascade()` and `State::numbers(story, mode)`, and
the dependency change. The testkit: the numbering fixture builder and the oracle runner of 7.3 (not implemented).
`projection`: reads `clause_number`, `hidden`, `looks_hidden` and `mark_effect`, and decides how to show
`WebHidden`, a `looks_hidden` signal, ghosts and joined paragraphs. `revisions`: nothing from these modules, but a
tracked list change it writes must agree with the `pPrChange` row of 4.6, so the two sets of tests share the
fixtures.

### 9.4 Decisions from the review round, and what is still open

Decided in review of the first version of this design:

1. **`state` owns the accessors** and depends on `styles` and `numbering` (2.3, 9.2).
2. **The LibreOffice oracle comes second** (a container from a Debian image). The ladder starts with the
   Word-evidence checks of 7.2, and the runner of 7.3 is written to be added without design changes.
3. **`hidden` reports Word's meaning only.** "Looks hidden" is `looks_hidden`, a separate function and a separate
   type, and `projection` decides how to show it (3.6).
4. **The 20-fixture Word checklist stays ready** (7.4), and the ghost number stays at low confidence, which
   `FIDELITY.md` says, until a person with Word runs it.
5. **The unverified choices follow the OpenXmlPowerTools reference, except where the standard or the corpus
   supports a deviation** (U1a, U6, U10, U11, U12 are kept; U8 and U13 follow the reference; the table order U20
   follows the reference), each pinned by a fixture and marked for the checklist (the register of 7.5).
6. **The interface change is accepted:** `clause_number(&Paragraph)` becomes `StoryNumbers::build` plus lookups.

Decided in the second review round: the toggle rule stays exclusive-or and
the claim about the reference is corrected (B1, 3.3); the register has "reference says" and "we do" columns (B2,
7.5); `startOverride` keeps a set of started pairs (B3, 4.3); S1 to S11 and the nits are in the sections they
touch; numbers are computed lazily on read, with a benchmark gate (2.3, 8.5); numbering notes reach users through
`projection` and `document`, and `check` stays free of `numbering` (2.4); `PartsView::site` is in 2.3.

Still open:

1. **Where does the LibreOffice container run?** It needs a container runtime, a Debian
   base image pulled (network), and the corpus mounted read-only. No port is bound and the container has no
   network after the pull.
2. **Is there a person with Word** for the checklist of 7.4?
3. **Section numbers.** ISO 29500 sections are given where the text of the standard was seen (17.7.3 toggle
   properties; 17.9.26 `start`); the rest are cited by element name, because the standard's PDF was not available
   offline, and the ISO text quoted here was read from the Open XML SDK documentation, which reproduces it. The
   wording of 17.7.3 itself is in the register (U1, U2).

## 10. Fidelity summary, for `FIDELITY.md`

What a reader of the projection can rely on, by how it was checked. **Exact** means the result is what Word
shows and has been checked against Word's own output. **Normalized** means the same information in a canonical
form. **Approximated** means close, with the difference stated. **Not supported** means the module says so
instead of guessing.

| Area | Exact | Normalized | Approximated | Not supported |
|---|---|---|---|---|
| Clause numbers (`numbering`) | Counters, restarts, `lvlRestart`, `startOverride`, shared lists, style-linked headings with `%1.%2.%3` texts, `isLgl`, in Final: 11 of 11 Word `REF \w` results and 93 numbered TOC lines (and 30 unnumbered ones) agree, with no difference | The label is trimmed and has no suffix; `full_context` strips trailing periods; a bullet has a glyph and no clause number | Redline ghost numbers, a deleted mark that merges into the next paragraph, `specVanish` and hidden paragraphs, empty section-break paragraphs: rules from the reference, unverified without Word (register U3 to U5, U15). `full_context` across lists with separate definitions is composed by `index`, not here | Text boxes (no ref in 1.0), `LISTNUM` fields (a `Note`), `w:numPr/w:ins` (not noticed: the facts do not carry it), the reference's "continuation" heuristic (deliberately not taken) |
| Number formats | `decimal`, `decimalZero`, Roman and letter forms, `ordinal`, `cardinalText`, `ordinalText`, `bullet`, `none`, the enclosed and width forms | A format outside its range (a Roman numeral above 3,999) shows decimal, with a `Note` | English words only; `chicago`, `hex`, `numberInDash` from tables | The script formats (CJK counting and legal, Hindi, Thai, Hebrew, Arabic, Cyrillic, `bahtText`, `dollarText`) show decimal with a `Note` until tier 3 is built |
| Effective formatting (`styles`) | Document defaults, `basedOn` chains, the toggle rule across table, paragraph and character style, merge by attribute and by child, table regions (the computed set equals Word's `cnfStyle` in 100 cells of Word documents, one stale table excepted), plain theme colours (1,363 of 1,363) and theme fonts (194 of 194) | Sources are per property element, per attribute for fonts, indents and spacing | Tinted and shaded theme colours: 89% exact, none more than two apart in a channel; the order of conditional formats where the first row meets the first column (U20) | Per-character font choice (`w:hint`), `cnfStyle` as input, a style definition's own `rPrChange` |
| Hidden text | `vanish` through every level of the cascade, `webHidden` as its own reason, `specVanish` as a join of paragraphs | | `looks_hidden` (white text, 2 pt or less) is a heuristic about what a reader cannot see, with the backgrounds from direct formatting, styles, the table style, the cell and the page | |

## References

- ISO/IEC 29500-1 (ECMA-376) as reproduced in the Open XML SDK documentation on Microsoft Learn: `b`,
  `vanish`, `specVanish`, `webHidden`, `dstrike` and the other run properties (which of them are toggle
  properties, 17.7.3); `lvlRestart`, `start`, `startOverride`, `lvlText`, `isLgl`, `pStyle` of a level,
  `numStyleLink`, `styleLink`, `suff`, `numFmt`, `lvlOverride`, `numId`, `numPr`; `tblStylePr` (the order
  of conditional formats), `tblLook`, `cnfStyle`.
- Open XML PowerTools and Docxodus, MIT: `ListItemRetriever.cs` (counters, restarts, the deleted mark, the
  format dispatch), `GetListItemText_*.cs` (the formats), `FormattingAssembler.cs` (cascade order, the toggle
  merge, where the numbering level sits), `TrackedChangesNumberingTests.cs` (the ghost rule).
- [`wordml.md`](wordml.md), sections 3.6, 4.6 and 6: the paragraph views, the modes, the definitions.
- [`refs-and-state.md`](refs-and-state.md), section 12: derived caches and generations.
