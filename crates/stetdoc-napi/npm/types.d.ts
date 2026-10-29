// The data types of stetdoc, as they cross the library in JSON.
// Generated from the core's json_schema() by `cargo xtask codegen`. Do not edit it by hand.

/** The changes of one author, with a count of each kind. */
export type AuthorRevisions = {
  /** The author, as the document writes it. A change with no author is `Unknown`. */
  author: string;
  /** The changes, in the order given. */
  changes: ChangeEntry[];
  /** How many changes of each kind, in the order of the kinds. */
  counts: [ChangeKind, number][];
};

/**
 * A list of operations that applies completely or not at all.
 *
 * It names the base token of the state it was written against, always, in a working copy as well, so that an old `p42`
 * can never silently hit a different paragraph. A batch is written blind: no operation can name a block that an earlier
 * operation of the batch created, and each operation sees the effect of the earlier ones. `Batch::parse` reads JSON or
 * block text, and `to_json` and `to_block` write them.
 */
export type Batch = {
  /** The base token that the batch was written against: the one the last view or receipt showed. */
  base: string;
  /** The operations, in order. A batch with none is `invalid_payload`. */
  ops: Op[];
};

/** A budget in tokens, by the estimate of design 4.4. Zero means no limit. */
export type Budget = number;

/** What this release can do: the operations, the formats, the codes and the default limits. */
export type Capabilities = {
  /** The version of the complement this release writes, such as `1.0`. It reads the versions up to its own. */
  complement_version: string;
  /** The error codes. */
  error_codes: ErrorInfo[];
  /** The finding codes of `check`. */
  finding_codes: FindingInfo[];
  /** The formats it reads and writes. */
  formats: FormatInfo[];
  /** The default limits. */
  limits: Limits;
  /** The operations. */
  ops: OpInfo[];
  /** The release. */
  version: string;
  /** The warning codes. */
  warning_codes: string[];
};

/** One revision, or one change, that an edit wrote. */
export type ChangeEntry = {
  /** The ref of the block, or none for a block that a dry run shows as new. */
  block?: string | null;
  /** The date as written into the document. */
  date: string;
  /** Every revision id of the change. */
  ids: number[];
  /** What the change is. */
  kind: ChangeKind;
};

/**
 * What a change is. A **change** is the unit that an agent thinks in: a move with both halves, a paragraph deletion with
 * its mark, an Enter pair, a split insertion with another author's change between its pieces.
 */
export type ChangeKind = "insertion" | "deletion" | "move" | "paragraph_inserted" | "paragraph_deleted" | "mark_inserted" | "mark_deleted" | "row_inserted" | "row_deleted" | "cell_inserted" | "cell_deleted" | "cell_merged" | "run_format" | "paragraph_format" | "section_format" | "table_format" | "row_format" | "cell_format" | "grid_change" | "numbering_change";

/** What `Document::check` reports. */
export type CheckReport = {
  /**
   * Every code that occurred and how many times, in the order of [`FindingCode::ALL`]. It
   * counts all the findings, and not only the ones in `findings`.
   */
  by_code: [FindingCode, number][];
  /**
   * The findings in the order they were found, violations first and then notes, at most
   * [`CheckReport::MAX_FINDINGS`] of them.
   */
  findings: Finding[];
  /** How many notes there are, all of them. */
  notes: number;
  /** True when there is no violation. Notes do not count. */
  ok: boolean;
  /** How many parts were examined. */
  parts: number;
  /**
   * How many parts were not: the ones the main part names that have no schema (an image, a
   * theme, a chart), and the ones that could not be read. Neither is listed, and the second is
   * a note.
   */
  unchecked: number;
  /** How many violations there are, all of them. */
  violations: number;
};

/** The result of `Document::check_pending`: everything `stetdoc check` prints, and its exit status. */
export type CheckResult = {
  /** The revisions already in the document, by author. */
  existing: AuthorRevisions[];
  /**
   * The exit status: 0, or 5 when `strict` and the report has a violation. A pending edit that would add a violation is the
   * error `validation_failed`, whose exit class is 5.
   */
  exit_status: number;
  /** The dry run of the pending files, which shows the revisions they would write. None when none are pending. */
  receipt?: Receipt | null;
  /** What `Document::check` found in the document as it is. */
  report: CheckReport;
  /** Whether any violation, new or old, fails the check. */
  strict: boolean;
};

/** How serious a finding is. */
export type Class = "violation" | "note";

/** What a comment operation did, or what `reconcile` removed. */
export type CommentEffect = {
  /** `added`, `replied`, `resolved`, `reopened`, `edited`, `deleted` or `removed` (by `reconcile`). */
  action: string;
  /** The comment's author. */
  author: string;
  /** The paragraphs that hold its markers. */
  blocks: string[];
  /** The comment, such as `c12`. A new comment is listed so that the next operation can name it. */
  id: string;
  /** The root of its thread. */
  thread: string;
  /** Things worth saying that are not errors. */
  warnings: Warning[];
};

/** Which comment threads a `Comments` view lists. */
export type CommentFilter = {
  /** Only threads started by this author, by code or name. */
  author?: string | null;
  /** Only resolved threads (`true`) or only open ones (`false`). */
  resolved?: boolean | null;
};

/** What `compare` takes. The stamp is required, because a redline has an author and a date. */
export type CompareOptions = {
  /** The format of the working copy of the redline. */
  format?: FormatId;
  /** The bounds on untrusted input, applied to both packages. */
  limits?: Limits;
  /** The author and the date of the revisions of the redline. */
  stamp: Stamp;
};

/** Where a page continues: `p188` (after that block), `change:14` or `comment:7`. Opaque to callers. */
export type Cursor = string;

export type ErrorCode = "unsupported" | "cross_block" | "readonly_region" | "scope_violation" | "io_error" | "invalid_payload" | "not_found" | "ambiguous_match" | "count_mismatch" | "stale_ref" | "stale_files" | "pending_edits" | "stale_target" | "validation_failed" | "malformed_input" | "limit_exceeded" | "corrupt_complement" | "unsupported_version" | "internal";

/** An error code with its exit status. */
export type ErrorInfo = {
  /** The code. */
  code: string;
  /** What it means. */
  description: string;
  /** The exit status of its class. */
  exit: number;
  /** The default hint for the next step. */
  next: string;
};

/** One field of an operation. */
export type FieldInfo = {
  /** For a choice, the words it may be. */
  choices: string[];
  /** What it means. */
  doc: string;
  /** The kind of value: `ref`, `text`, `number`, `bool`, `choice`, `list`, `number list` or `properties`. */
  kind: string;
  /** The key. */
  name: string;
  /** Whether the operation needs it. */
  required: boolean;
};

/** One thing a check found. */
export type Finding = {
  /**
   * The ref of the block it is in, when the block has one. A new block, a paragraph in a
   * text box and a problem at the level of the part have none.
   */
  block?: string | null;
  /** Whether it is a violation or a note. */
  class: Class;
  /** The stable code. */
  code: FindingCode;
  /** A sentence that says what is wrong. */
  message: string;
  /**
   * The part it is in, as the package names it. Empty for a finding about the document as a
   * whole, such as a revision id that two parts use.
   */
  part: string;
};

export type FindingCode = "schema_wrong_root" | "schema_unexpected_child" | "schema_unknown_element" | "schema_out_of_order" | "schema_non_canonical_order" | "schema_missing_child" | "schema_must_understand" | "mc_prefix_undeclared" | "schema_unexpected_text" | "schema_unknown_attribute" | "schema_missing_attribute" | "schema_invalid_attribute_value" | "text_in_deletion" | "deltext_outside_deletion" | "edge_space_without_preserve" | "revision_id_duplicate" | "bookmark_id_duplicate" | "revision_nested_in_itself" | "bookmark_unpaired" | "move_range_unpaired" | "move_name_unpaired" | "field_split_by_deletion" | "cell_without_final_paragraph" | "table_without_row" | "row_without_cell" | "container_without_block" | "empty_revision" | "empty_properties" | "part_without_content_type" | "relationship_target_missing" | "relationship_id_duplicate" | "comment_id_duplicate" | "marker_unpaired" | "marker_without_comment" | "reference_missing" | "reference_duplicate" | "unanchored" | "row_without_comment" | "row_missing" | "para_id_missing" | "para_id_duplicate" | "para_id_range" | "parent_missing" | "thread_cycle" | "durable_mismatch" | "part_unrelated" | "part_unreadable";

/** A finding code with its class. */
export type FindingInfo = {
  /** `violation` or `note`. */
  class: string;
  /** The code. */
  code: string;
  /** What it means. */
  description: string;
};

/** An own pending property change that an operation edited in place instead of writing a new one. */
export type FoldedEntry = {
  /** True when the edit brought the properties back to what they were and the change element was removed. */
  cancelled: boolean;
  /** The id of the change. */
  id: number;
};

/**
 * The id of a projection format, such as `stet/1`. A format with options is several ids, so one
 * string says which adapter wrote a file and which one must read it back.
 */
export type FormatId = string;

/** A projection format and the extension of its readable files. */
export type FormatInfo = {
  /** The extension of its readable files, such as `md`. A working copy holds the files of the directory that have it. */
  extension: string;
  /** The id, such as `stet/1`. */
  id: string;
};

/** One thing a query found. A query returns every hit and never chooses. */
export type Hit = {
  block: {
    /** The ref. */
    block: string;
    /** The terms this block defines. */
    defines: string[];
    /** The label composed across lists, such as `4.2(a)(ii)`. */
    full?: string | null;
    /** The references to this block. */
    inbound: Reference[];
    /** The label Word shows, such as `4.2`. */
    label?: string | null;
    /** The references this block makes. */
    outbound: Reference[];
    /** The story. */
    story: string;
    /** The block's text as the format writes it. */
    text: string;
  };
} | {
  clause: {
    /** The ref. */
    block: string;
    /** True when the composed label was built across lists. */
    composed: boolean;
    /** The composed label. */
    full: string;
    /** The label. */
    label: string;
    /** The rule that matched the query. */
    matched_by: string;
    /** The noun in front of it, such as `Section`. */
    noun?: string | null;
    /** `computed` (Word's numbering) or `typed` (the author typed it). */
    source: string;
    /** The story. */
    story: string;
    /** The start of its text. */
    text: string;
  };
} | {
  term: {
    /** Where it is defined. */
    definitions: Site[];
    /** `exact` or `case_insensitive`. */
    matched: string;
    /** True when more than one block defines it. */
    multiple_definitions: boolean;
    /** The term. */
    term: string;
    /** How many uses there are in all. */
    use_count: number;
    /** Where it is used. */
    uses: Site[];
  };
};

/** One line of an inspection: a label, its value and where the value comes from. */
export type InspectItem = {
  /** What the line is about, such as `style` or `spacing`. */
  label: string;
  /** Where the value comes from: `default`, a table style, numbering, a paragraph style or `direct`. */
  source?: string | null;
  /** Its value. */
  value: string;
};

/**
 * The answer to `Document::inspect`: effective formatting with its source for a block, or the structure of the document
 * for `styles`, `numbering` and `sections`.
 */
export type Inspection = {
  /** The base token of the state the answer describes. */
  base: string;
  /** The same lines as data. */
  items: InspectItem[];
  /** What was inspected, as asked. */
  target: string;
  /** The lines as text: what a surface prints. */
  text: string;
};

/** Bounds on untrusted input. `Default` is the production setting, and a partial JSON object keeps the default of every field it leaves out. */
export type Limits = {
  /** Size of the input package in bytes. */
  max_archive_bytes?: number;
  /**
   * Bytes of one batch, as JSON or as block text: a rewrite of every paragraph of the largest
   * document at 1 KB each.
   */
  max_batch_bytes?: number;
  /**
   * Operations in one batch, each paragraph of an `insert` counting as one, since each costs what an operation does.
   * For an edit of readable files, the blocks that the edit changes. The
   * budget is 50 operations in a second, the largest corpus document has 15,878 blocks, and on a
   * dense 28 MiB body an operation costs about 4 ms, so a batch at the limit takes about 20 s.
   */
  max_batch_ops?: number;
  /**
   * Uncompressed size divided by compressed size, per entry. Applies to entries whose
   * uncompressed size is at least 16 MiB.
   */
  max_compression_ratio?: number;
  /** Number of ZIP entries. */
  max_entries?: number;
  /** Declared uncompressed size of one entry. */
  max_entry_bytes?: number;
  /** Bytes of one text field of an operation: 300 pages of 3,000 characters is 0.9 MB. */
  max_op_text_bytes?: number;
  /**
   * Refs in one ref namespace, live and retired together, and committed versions of one
   * document. It bounds the ref table, the lineage and what `restore` accepts. The default is
   * 57 times the 18,260 blocks of the largest document measured (a 12.6 MB body), and 66 times
   * the 15,878 of the largest of 42,003 Harvey LAB documents.
   */
  max_refs?: number;
  /**
   * Entries of a scope. A scope is a few refs and ranges, and a range is one entry however many blocks
   * it covers, so this bounds what a hostile scope can make the guard allocate, at about 1 MB.
   */
  max_scope_refs?: number;
  /** Sum of the declared uncompressed sizes of all entries. */
  max_total_bytes?: number;
  /** Attributes on one element, including namespace declarations. */
  max_xml_attributes?: number;
  /** Element nesting depth in one XML part. */
  max_xml_depth?: number;
  /**
   * Namespace declarations in scope at one element: all those on the element and its ancestors,
   * counting a declaration that a closer one of the same prefix hides. Looking a name up costs
   * time in proportion to this, so it bounds the cost of an edit in a part that declares
   * thousands of namespaces down one path.
   */
  max_xml_namespaces_in_scope?: number;
  /**
   * Nodes plus attributes in one XML part. Bounds the memory a part can make the span tree
   * allocate: about 60 bytes for each, however few input bytes they take.
   */
  max_xml_nodes?: number;
};

/** The answer to a query, at one state. */
export type Lookup = {
  /** The base token of the state the answer describes. Every ref in it is a ref of that token. */
  base: string;
  /** Every hit, in the order block, clause, term. */
  hits: Hit[];
  /** The query as asked. */
  query: string;
  /** The answer as text, in the document's format: what a surface prints. */
  text?: string;
};

/**
 * One change of manual markup: the old text it struck through and the new text it underlined after it. Either may be empty:
 * a deletion underlines nothing and an insertion strikes nothing.
 */
export type ManualMark = {
  /** The ref of the paragraph, or none for a paragraph that a dry run shows as new. */
  block?: string | null;
  /** The old text, kept and struck through. */
  struck: string;
  /** The new text, underlined. */
  underlined: string;
};

/** How an edit shows the text it changes. */
export type Markup = "revisions" | "manual" | "manual_color";

/** How tracked changes appear in a view. */
export type Mode = "final" | "original" | "redline";

/** A paragraph that compare wrote as a deletion and an insertion, and lists as a move. */
export type MoveEntry = {
  /** The ref where it was. */
  from: string;
  /** The start of its text. */
  text: string;
  /** The ref where it went. */
  to: string;
};

export type NewParagraph = string | {
  style?: string | null;
  text: string;
};

/** One place a text is in a paragraph's Final text. */
export type Occurrence = {
  /** The ref of the paragraph. */
  block: string;
  /**
   * The full label of the clause the paragraph is, or whose own text it is, such as `11.1(a)`. A paragraph after a
   * list item, such as one after (g) with no number, is in the clause the list is in. A recital or another paragraph
   * under a heading that is not a clause is in none.
   */
  clause?: string | null;
  /** One past the last character of the match. */
  end: number;
  /**
   * When the paragraph is in no clause: the start of the nearest heading before it (a paragraph with an outline
   * level), such as `RECITALS`, cut at 50 characters.
   */
  heading?: string | null;
  /** True when only the second try matched: curly and straight quotes taken as the same, as an operation's `find` does. */
  loose: boolean;
  /** Which match of the text in the paragraph it is, counted from 1: the `occurrence` an operation names it with. */
  occurrence: number;
  /** The match with up to 30 characters on each side, the match in square brackets, on one line. */
  snippet: string;
  /** The first character of the match, as an offset in the paragraph's Final text. */
  start: number;
  /** The story: `body`, `notes`, `headers` or `comments`. */
  story: string;
};

/** Every place a text is, at one state. */
export type Occurrences = {
  /** The base token of the state the answer describes. */
  base: string;
  /**
   * The paragraphs where the text is only in text a tracked deletion removes, one entry each with its first match in
   * the Original text. They are not targets, and are listed so that a caller can say why the text was not found.
   */
  deleted: Occurrence[];
  /**
   * Every match in the Final text, in document order. Text that a tracked insertion added counts, and text that a
   * tracked deletion removed does not.
   */
  hits: Occurrence[];
  /** The text, as asked. */
  needle: string;
};

/** One edit instruction. See `Op::spec` for the table that describes it. */
export type Op = {
  /** One past the last character of the range, as an offset in the Final text. */
  end?: number | null;
  /** The text to find in the paragraph's Final text. It must match once, or `occurrence` says which match. */
  find?: string | null;
  /** Which match of `find` to use, counted from 1. */
  occurrence?: number | null;
  op: "replace";
  /** The paragraph. */
  ref: string;
  /** The first character of the range, as an offset in the Final text. */
  start?: number | null;
  /** The new text. */
  with: string;
} | {
  op: "insert";
  /** The paragraphs, each a text (and optionally a style) or a bare string. */
  paragraphs: NewParagraph[];
  /** Whether they go before or after it. */
  place: Place;
  /** The paragraph or table the new paragraphs go next to. */
  ref: string;
} | {
  /** One past the last character of the range, as an offset in the Final text. */
  end?: number | null;
  /** The text to find in the paragraph's Final text. It must match once, or `occurrence` says which match. */
  find?: string | null;
  /** Which match of `find` to use, counted from 1. */
  occurrence?: number | null;
  op: "delete";
  /** The paragraph, table or first paragraph of the range. */
  ref: string;
  /** The first character of the range, as an offset in the Final text. */
  start?: number | null;
  /** The last paragraph of a range that spans paragraphs. `start` is then an offset in `ref` and `end` an offset in `to`. */
  to?: string | null;
} | {
  /** Cut just after this text, which must match once. */
  after?: string | null;
  /** The offset of the cut in the Final text. */
  at?: number | null;
  /** Cut just before this text, which must match once. */
  before?: string | null;
  op: "split";
  /** The paragraph. */
  ref: string;
} | {
  op: "merge";
  /** The paragraph that stays. */
  ref: string;
  /** The paragraph that is joined into it. By default the next paragraph that is not a deleted block. */
  second?: string | null;
} | {
  /** Bold on or off. */
  bold?: boolean | null;
  /** The id or name of a character style. */
  char_style?: string | null;
  /** The names of formatting to remove. */
  clear?: string[] | null;
  /** The text colour, `RRGGBB` or `auto`. */
  color?: string | null;
  /** One past the last character of the range, as an offset in the Final text. */
  end?: number | null;
  /** The text to find in the paragraph's Final text. It must match once, or `occurrence` says which match. */
  find?: string | null;
  /** The font name. */
  font?: string | null;
  /** The highlight colour, by Word's name. */
  highlight?: string | null;
  /** Italic on or off. */
  italic?: boolean | null;
  /** Which match of `find` to use, counted from 1. */
  occurrence?: number | null;
  op: "format";
  /** The paragraph. */
  ref: string;
  /** The font size with a unit, such as `12pt`. */
  size?: string | null;
  /** The first character of the range, as an offset in the Final text. */
  start?: number | null;
  /** Strike through on or off. */
  strike?: boolean | null;
  /** The underline style: `single`, `double`, `none` and the others Word has. */
  underline?: string | null;
} | {
  /** `left`, `center`, `right` or `justify`. */
  align?: string | null;
  /** The names of properties to remove. */
  clear?: string[] | null;
  /** The indent of the first line. */
  first_line?: string | null;
  /** The hanging indent. */
  hanging?: string | null;
  /** The left indent. */
  indent_left?: string | null;
  /** The right indent. */
  indent_right?: string | null;
  /** Keep the lines together. */
  keep_lines?: boolean | null;
  /** Keep with the next paragraph. */
  keep_next?: boolean | null;
  /** The line spacing. */
  line_spacing?: string | null;
  op: "paragraph";
  /** Start a new page before the paragraph. */
  page_break_before?: boolean | null;
  /** The paragraph. */
  ref: string;
  /** The space after the paragraph. */
  space_after?: string | null;
  /** The space before the paragraph. */
  space_before?: string | null;
} | {
  op: "style";
  /** The paragraph. */
  ref: string;
  /** The style, by id or by name as the view shows it. */
  style: string;
} | {
  /** The new level, from 0. */
  level?: number | null;
  op: "list";
  /** The paragraph. */
  ref: string;
  /** Take the paragraph out of its list. */
  remove?: boolean | null;
  /** Restart the numbering at this number. */
  restart?: number | null;
} | {
  /** The cell of a row, from 1. */
  cell?: number | null;
  /** The names of properties to remove. */
  clear?: string[] | null;
  /** One past the last character of the range, as an offset in the Final text. */
  end?: number | null;
  /** The text to find in the paragraph's Final text. It must match once, or `occurrence` says which match. */
  find?: string | null;
  /** Which match of `find` to use, counted from 1. */
  occurrence?: number | null;
  op: "set";
  /** The properties to set. */
  props: PropSet[];
  /** The paragraph, table or row. A section is named by `section` instead. */
  ref?: string | null;
  /** The number of the section, as `inspect sections` shows it, from 1. */
  section?: number | null;
  /** The first character of the range, as an offset in the Final text. */
  start?: number | null;
  /** What to set it on: `paragraph`, `run`, `mark`, `section`, `table`, `row` or `cell`. By default from the kind of `ref`. */
  target?: string | null;
} | {
  /** Whether to insert a row or delete this one. */
  action: RowAction;
  /** For an insert, the text of each cell. A newline in a cell is a line break. */
  cells?: string[] | null;
  op: "row";
  /** For an insert, whether the new row goes before or after `ref`. After by default. */
  place?: Place | null;
  /** The row. */
  ref: string;
} | {
  /** One past the last character of the range, as an offset in the Final text. */
  end?: number | null;
  /** The text to find in the paragraph's Final text. It must match once, or `occurrence` says which match. */
  find?: string | null;
  /** The initials shown with the comment. By default the first letters of the author's words. */
  initials?: string | null;
  /** Which match of `find` to use, counted from 1. */
  occurrence?: number | null;
  op: "comment";
  /** The paragraph. */
  ref: string;
  /** The first character of the range, as an offset in the Final text. */
  start?: number | null;
  /** The text of the comment. */
  text: string;
} | {
  /** Any comment of the thread, such as `c3`. */
  comment: string;
  op: "reply";
  /** The text of the reply. */
  text: string;
} | {
  /** Any comment of the thread. */
  comment: string;
  op: "resolve";
  /** Reopen the thread instead of resolving it. */
  reopen?: boolean | null;
} | {
  /** The comment. */
  comment: string;
  op: "edit-comment";
  /** The new text. */
  text: string;
} | {
  /** The comment. */
  comment: string;
  op: "delete-comment";
} | {
  /** The note, such as `fn3` or `en2`: its kind and its ordinal. */
  note: string;
  op: "delete-note";
} | {
  /** Every revision. */
  all?: boolean | null;
  /** Every revision of this author, by name or by the short code the view shows. */
  author?: string | null;
  /** The revisions with these ids, the numbers after `#` in the view. A change is resolved whole. */
  ids?: number[] | null;
  op: "accept";
} | {
  /** Every revision. */
  all?: boolean | null;
  /** Every revision of this author, by name or by the short code the view shows. */
  author?: string | null;
  /** The revisions with these ids, the numbers after `#` in the view. A change is resolved whole. */
  ids?: number[] | null;
  op: "reject";
};

/** One operation. */
export type OpInfo = {
  /** An example, as JSON. */
  example: string;
  /** Its fields. */
  fields: FieldInfo[];
  /** The name. */
  name: string;
  /** The group of optional fields of which exactly one must be given. Empty when there is none. */
  one_of: string[];
  /** Whether rejecting what it wrote restores the original. */
  reversible: boolean;
  /** What it does. */
  summary: string;
  /** Whether it writes revisions. */
  trackable: boolean;
};

/** The result of one operation of a batch. */
export type OpResult = {
  /**
   * The refs minted for the blocks it added. Empty for a dry run, which issues no ref. A block it added that a later
   * operation joined an older block into carries the older block's ref, which is not listed.
   */
  added: string[];
  /** How many blocks it added: the same for a dry run as for the real edit. */
  added_count: number;
  /** The refs of the existing blocks it changed. */
  blocks: string[];
  /** False when it changed nothing. */
  changed: boolean;
  /** For a comment operation, the comment, such as `c12`. */
  comment?: string | null;
  /** The author's own pending property changes that it edited in place. */
  folded: FoldedEntry[];
  /** The zero-based index of the operation. */
  index: number;
  /** With manual markup, what it struck through and underlined, in the order of the text. */
  marked?: ManualMark[];
  /** Its name. */
  op: string;
  /** For an edit of files, the file and line it came from. */
  origin?: Origin | null;
  /** The author's own pending text, marks or rows that it removed outright. */
  removed_own: RemovedOwnEntry[];
  /** For `accept` and `reject`, what was resolved. It is shown on the first operation of a run. */
  resolution?: Resolution | null;
  /** The changes it wrote. */
  revisions: ChangeEntry[];
  /** The ids of other revisions that went with removed text. */
  swallowed: number[];
  /** Things worth saying about this operation. */
  warnings: Warning[];
};

/**
 * What `Document::open` and `Document::resume` take. Every field has a default, and a partial JSON object keeps the
 * default of each field it leaves out.
 */
export type OpenOptions = {
  /**
   * The format of the working copy that `export` writes, that receipts show blocks in and that `lookup` writes its text
   * in. A resumed document keeps the format recorded in its complement, and this applies to `open` only.
   */
  format?: FormatId;
  /** The bounds on untrusted input, which apply to `open`, `resume`, `compare`, readable files and batches. */
  limits?: Limits;
};

/** Where an operation came from, so that an error that names an operation can name a file and a line. */
export type Origin = {
  /** The second ref of a merge. */
  also?: string | null;
  /** The ref of the block the operation is about, when it has one. */
  block?: string | null;
  /** The file. */
  file: string;
  /** The line in the edited file, from 1. Zero when the operation comes from a removed line. */
  line: number;
};

/** Whether new paragraphs go before or after their anchor. */
export type Place = "before" | "after";

/** Where a page is in what it selects. */
export type Position = {
  /** The first block shown. */
  first?: string | null;
  /** The last block shown. */
  last?: string | null;
  /** An estimate of the tokens that remain after this page. */
  remaining_tokens: number;
  /** What was selected, such as `p1..p8152` or `outline`. */
  scope: string;
  /** How many blocks the page holds. */
  shown: number;
  /** How many blocks the scope holds. */
  total: number;
};

/** One property element for the generic setter: a name in the `w` namespace, an optional `w:val`, attributes and children. */
export type PropSet = {
  /** The other attributes, by local name. Values take human units where the schema has a unit. */
  attrs?: { [key: string]: string };
  /** Child elements, for the properties that have them (borders, tab stops). */
  children?: PropSet[];
  /** The element's local name, such as `spacing` or `jc`. A `w:` prefix is accepted. */
  name: string;
  /** The `w:val` attribute, for the properties that have only that. */
  val?: string | null;
};

/** The files of a working copy as written by a format, and the base token they were written at. */
export type ReadableFiles = {
  /** The base token of the state the files were rendered from. */
  base: string;
  /** `(name, text)` for each file, in the format's order. */
  files: [string, string][];
  /** The format that wrote them, which must read them back. */
  format: FormatId;
  /**
   * `(name, SHA-256 in hex)` of each file as it was written by the release that wrote it. Empty for files that
   * `render_files` just made. `WorkingCopy::pending_files` fills it from the manifest, so that an edit can tell that
   * another release wrote the files (release skew).
   */
  rendered?: [string, string][];
};

/** The receipt of an edit. */
export type Receipt = {
  /** The refs of the blocks the batch added. Empty for a dry run. */
  added: string[];
  /** How many blocks the batch added. */
  added_count: number;
  /** The base token after the edit. For a dry run, and for a batch that changed nothing, the current one. */
  base: string;
  /** The post-edit text of every block touched or added, at most 200. */
  blocks: RenderedBlock[];
  /** How many more blocks there were. */
  blocks_omitted: number;
  /** What the comment operations did, and what `reconcile` removed. */
  comments: CommentEffect[];
  /** True for a dry run, which changed nothing. */
  dry_run: boolean;
  /** How the edit showed the text it changed. Manual markup is direct formatting and no revision: tell the reader so. */
  markup?: Markup;
  /** Moved paragraphs, which only compare lists. Empty for an edit. */
  moves: MoveEntry[];
  /** One result for each operation, in order. */
  operations: OpResult[];
  /** The refs that are stale now, with the reason. */
  retired: Retired[];
  /** The revisions the batch wrote, by author. */
  revisions: AuthorRevisions[];
  /** The author, and the date as written into the document. */
  stamp: Stamp;
  /** What a surface shows an agent. */
  text: string;
  /** False for an untracked edit. */
  tracked: boolean;
  /** What `check` verified. */
  validation: Validation;
  /** Every warning, deduplicated by code, block and message. */
  warnings: Warning[];
};

/** One cross-reference: a block that cites something and what it resolved to. */
export type Reference = {
  /**
   * Every block it may mean, when there are several, or the candidates of an `outside` one. A `relative` one
   * lists its chosen block first and then the candidates it was chosen over.
   */
  candidates: string[];
  /** The ref of the citing block. */
  from: string;
  /** `resolved`, `ambiguous`, `relative`, `outside` or `unresolved`. */
  state: string;
  /** The citation as written. */
  text: string;
  /** The ref of the block it resolved to, when it resolved to exactly one or a `relative` reading chose one. */
  to?: string | null;
};

/** Own pending content that an operation removed outright: the author's earlier insertion, say. */
export type RemovedOwnEntry = {
  /** The id of the revision it was, when it had one. */
  id?: number | null;
  /** What was removed: `text`, `mark`, `paragraph` or `row`. */
  kind: string;
};

/** One block as a receipt shows it. */
export type RenderedBlock = {
  /** For a block with no ref, the ref of the block it follows. */
  anchor?: string | null;
  /** The block's ref, or `None` for a block that a dry run created and no commit has numbered. */
  ref?: string | null;
  /** The story it is in. */
  story: string;
  /** Its lines, as the readable file will hold them. */
  text: string;
};

/** What an `accept` or `reject` did. */
export type Resolution = {
  /** `accept` or `reject`. */
  action: string;
  /** Paragraphs that were joined: the one that stays and the one that went. */
  joined: [string, string][];
  /** The notes that were removed because their last reference went, such as `fn3`. */
  notes_removed: string[];
  /** The refs that went stale. */
  removed: Retired[];
  /** What was resolved, by author and kind. */
  resolved: ResolvedCount[];
  /** The ids of revisions that went with an element that was removed. */
  swallowed: number[];
  /** Things worth saying that are not errors. */
  warnings: Warning[];
};

/** How many revisions of one kind an `accept` or `reject` resolved, for one author. */
export type ResolvedCount = {
  /** The author. */
  author: string;
  /** How many revision elements. */
  count: number;
  /** `insertion`, `deletion`, `move` or `format`. */
  kind: string;
};

/** A ref that is stale now, and why. */
export type Retired = {
  /** `deleted`, `rejected` or `merged into p5`. */
  reason: string;
  /** The ref. */
  ref: string;
};

/** Which tracked changes a `Revisions` view lists. */
export type RevisionFilter = {
  /** Only changes by this author, by code or name. */
  author?: string | null;
  /**
   * In a view: 0 for one line for each change; otherwise the blocks that hold the kept changes whole, as the redline
   * shows them, with this many blocks on each side. `Document::revisions` ignores it.
   */
  context?: number;
  /**
   * Only changes whose inserted or deleted text contains one of these texts. Each is case-insensitive unless it has
   * an uppercase letter, as in a `Find`. Empty keeps every change. JSON takes one string or a list of them.
   */
  find?: string | string[];
  /**
   * Carry the whole inserted and deleted text of each change, up to 1,000 characters, and not 200; a view shows it
   * all and does not cut each line to 80 characters.
   */
  full?: boolean;
  /** Only changes of this kind: `insert`, `delete`, `move` or `format`. */
  kind?: string | null;
};

/** One change in the document. */
export type RevisionInfo = {
  /** Who made it, as the document writes the author, if it says. */
  author?: string | null;
  /** The ref of the first block the change touches. */
  block?: string | null;
  /** When, as the document writes the date, if it says. */
  date?: string | null;
  /** The text the change deletes, cut at 200 characters. */
  deleted?: string | null;
  /** Every revision id of the change, in document order. Accepting or rejecting any of them resolves the whole change. */
  ids: number[];
  /** The text the change inserts, cut at 200 characters. */
  inserted?: string | null;
  /** What the change is. */
  kind: ChangeKind;
  /** The story: `body`, `notes`, `headers` or `comments`. */
  story: string;
  /** True when `inserted` or `deleted` was cut. */
  text_truncated?: boolean;
};

/** What a `row` operation does. */
export type RowAction = "insert" | "delete";

/** A summary of a saved package. */
export type SaveReceipt = {
  /** The base token of the saved state. */
  base: string;
  /** False when the bytes are the input's, exactly. */
  edited: boolean;
  /** Every revision of the saved package. */
  revisions: RevisionInfo[];
  /** The SHA-256 of the bytes, in hex. */
  sha256: string;
};

/** What a view selects. */
export type Scope = "auto" | {
  outline: {
    /** The deepest level shown, or the deepest that fits. */
    depth?: number | null;
  };
} | {
  range: {
    /** The first ref. */
    from: string;
    /** The last ref. */
    to?: string | null;
  };
} | {
  clause: string | string[];
} | {
  find: {
    /**
     * 0 for one line for each matching paragraph; otherwise the matching blocks whole, with this many blocks on
     * each side.
     */
    context: number;
    /** Only the blocks from this ref on, as a `Range` starts. */
    from?: string | null;
    /**
     * The texts to find, each case-insensitive unless it has an uppercase letter. A block that holds any of them
     * matches. JSON takes one string or a list of them, as `needles` or `needle`.
     */
    needles: string | string[];
    /**
     * Only in this story, such as the headers and footers. A range is in one story already, so with `from` this
     * must be the story of `from`.
     */
    story?: StoryName | null;
    /** With `from`, only the blocks up to this ref; without it, to the end of the story of `from`. */
    to?: string | null;
  };
} | {
  revisions: RevisionFilter;
} | {
  comments: CommentFilter;
} | {
  story: StoryName;
};

/** A place in the document: a block, the story it is in, and what kind of thing was found there. */
export type Site = {
  /** The ref of the block. */
  block: string;
  /** What was found: `definition`, `parenthetical` or `use`. */
  kind: string;
  /** A short piece of the block's text around it. */
  snippet: string;
  /** The story: `body`, `notes`, `headers` or `comments`. */
  story: string;
};

/**
 * Who is editing, and when. It has no default, so it cannot be forgotten: every edit and every compare takes one.
 *
 * The date is the wall-clock time of the date the caller gives, `YYYY-MM-DDTHH:MM:SS` with an optional fraction and an
 * optional `Z` or offset. What is written into a document is its first 19 characters and `Z`, which is Word's own
 * convention (its `w:date` is the author's local time labelled `Z`). A caller that passes UTC gets UTC, and the CLI and
 * a typical host application pass local time. The core reads no clock, and it validates the stamp when the edit runs: an empty or
 * over-long author, or a date that is not a real date and time, is `invalid_payload`.
 */
export type Stamp = {
  /** The author of the revisions and comments, as Word shows it. At most 255 characters. */
  author: string;
  /** The wall-clock date and time, such as `2026-10-06T14:03:27`. */
  date: string;
};

/** What a failure looks like as data: `--json`, napi's `StetdocError` and the schema. */
export type StetError = {
  /** The stable code. */
  code: ErrorCode;
  /** The readable file the failure came from. */
  file?: string | null;
  /** The one-based line in `file`. */
  line?: number | null;
  /** The one-line message. */
  message: string;
  /** The hint for the next step. */
  next: string;
  /** The zero-based index of the failing operation. */
  op_index?: number | null;
};

/** One of the document's stories, by the name an agent uses. */
export type StoryName = "body" | "notes" | "headers" | "comments";

/** What `check` verified for an edit. */
export type Validation = {
  /** Up to ten problems that were already in the blocks the edit touched, so a later `check` will still show them. */
  carried: Finding[];
  /** How many such problems there are in all. */
  carried_total: number;
  /** Notes that are not problems. */
  notes: Finding[];
  /** How many parts were examined. */
  parts: number;
  /** How many units were validated. */
  units: number;
};

/** One page of a view. */
export type View = {
  /** The base token of the state the page describes. */
  base: string;
  /** The mode it is in. */
  mode: Mode;
  /** Where the next page starts, when there is one. */
  next?: Cursor | null;
  /** Where the page is. */
  position: Position;
  /** What the CLI prints: the head, the lines, the appendix and the position line. */
  text: string;
  /** The size of `text` by the estimate of design 4.4. */
  tokens: number;
  /** The warnings of the page, for programs. */
  warnings: Warning[];
};

/** What a caller asks `view` for. */
export type ViewSpec = {
  /** Where to continue, from a previous page. */
  after?: Cursor | null;
  /** The size of a page. */
  budget?: Budget;
  /** The format that writes the page. */
  format?: FormatId;
  /** How tracked changes appear. */
  mode?: Mode;
  /** Margin notes and the appendix of the notes and comments a page cites. */
  notes?: boolean;
  /** What to select. */
  scope?: Scope;
};

/** Something the reader must know that is not in the text. */
export type Warning = {
  /** The ref of the block it is about, when there is one. */
  block?: string | null;
  /** What kind of warning it is. */
  code: WarningCode;
  /** What to know and what to do. */
  message: string;
  /** What to run next, when there is something. */
  next?: string | null;
  /** The story it is in: `body`, `notes`, `headers` or `comments`. A warning about an operation and not about a block has none. */
  story?: string | null;
};

export type WarningCode = "looks_hidden" | "hidden_text" | "numbering_approximate" | "read_anomaly" | "opaque_content" | "unreferenced_note" | "duplicate_number" | "looks_like_rewrite" | "split_at_wrap" | "no_change" | "own_text_removed" | "property_change_folded" | "revision_removed_with_text" | "quotes_curled" | "quotes_straightened" | "find_matched_loosely" | "already_resolved" | "reply_to_resolved" | "anchor_emptied" | "comment_removed" | "compare_revisions_accepted" | "compare_paragraph_replaced" | "compare_not_redlined" | "compare_not_restored" | "compare_keys_merged" | "compare_table_replaced" | "compare_story_unmatched" | "compare_comments_dropped" | "compare_bookmarks_dropped" | "compare_hyperlinks_unlinked" | "compare_objects_dropped" | "compare_notes_dropped" | "compare_lists_dropped" | "outline_shown" | "repaired";
