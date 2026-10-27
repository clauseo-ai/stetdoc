// The public surface of the stetdoc library. See docs/design/napi-and-editor.md, section 2.
import type {
  Capabilities,
  CheckReport,
  CheckResult,
  CompareOptions,
  ErrorCode,
  Inspection,
  Lookup,
  Occurrences,
  Op,
  OpenOptions,
  ReadableFiles,
  Receipt,
  RevisionFilter,
  RevisionInfo,
  SaveReceipt,
  Scope,
  View,
  ViewSpec,
} from "./types";

export type * from "./types";

/** The readable files and the complement. Plain data: keep it between turns and resume it. */
export interface WorkingCopy {
  files: [string, string][];
  complement: [string, Uint8Array][];
}

/** One edit: exactly one of `ops` (with `base`), `batch` and `files`, signed by `author` and `date`. */
export interface EditRequest {
  base?: string;
  ops?: Op[];
  batch?: string;
  files?: ReadableFiles;
  author: string;
  date: string;
  untracked?: boolean;
  scope?: string[];
  dryRun?: boolean;
  quotes?: "match" | "as_typed";
  /** The unit of the offsets of `ops` and `batch`. The default is "utf16", as a JavaScript string counts. */
  offsets?: "utf16" | "scalar";
}

export interface CheckPendingRequest {
  pending?: ReadableFiles | null;
  author?: string;
  date?: string;
  strict?: boolean;
}

/** The input of a tool_use block of Claude's text editor tool. */
export type TextEditorInput =
  | { command: "view"; path: string; view_range?: [number, number] }
  | { command: "str_replace"; path: string; old_str: string; new_str?: string }
  | { command: "insert"; path: string; insert_line: number; insert_text?: string; new_str?: string }
  | { command: "create"; path: string; file_text?: string }
  | { command: "undo_edit"; path: string };

/** Who signs the edits of the text editor tool, and where its files appear. */
export interface TextEditorOptions {
  author: string;
  date: string;
  /** The virtual directory of the files, such as "/documents/acme/". The default puts them at the root. */
  pathPrefix?: string;
}

/** What goes into the tool_result block, and what the call did. */
export interface TextEditorResult {
  /** The text of the tool_result. */
  content: string;
  /** Whether the command failed. Nothing changed when it did. */
  isError: boolean;
  /** Whether a str_replace or insert committed an edit. */
  committed: boolean;
  /** The receipt of the committed edit. */
  receipt?: Receipt;
}

export declare class Document {
  private constructor();
  static open(bytes: Uint8Array | ArrayBuffer, options?: OpenOptions): Document;
  static resume(workingCopy: WorkingCopy, options?: OpenOptions): Document;
  base(): string;
  view(spec?: Partial<ViewSpec>): View;
  edit(request: EditRequest): Receipt;
  save(): { bytes: Uint8Array; receipt: SaveReceipt };
  inspect(target: string): Inspection;
  revisions(filter?: RevisionFilter): RevisionInfo[];
  /** Every place a text is, as an edit's `find` matches it. `scope` defaults to "auto". */
  occurrences(text: string, scope?: Scope): Occurrences;
  lookup(query: string): Lookup;
  check(): CheckReport;
  checkPending(request?: CheckPendingRequest): CheckResult;
  files(): ReadableFiles;
  export(): WorkingCopy;
  textEditor(input: TextEditorInput | string, options: TextEditorOptions): TextEditorResult;
}

export declare function compare(
  original: Uint8Array | ArrayBuffer,
  revised: Uint8Array | ArrayBuffer,
  options: CompareOptions,
): { document: Document; receipt: CompareReceipt };

/** The receipt of a comparison: a `Receipt` whose operations are empty and whose `moves` lists moved paragraphs. */
export type CompareReceipt = Receipt;

/** The release of stetdoc this addon was built from. Pin it against the CLI's `stetdoc --version`. */
export declare const version: string;

/** The version of the working copy's complement that this release writes and reads. */
export declare const formatVersion: string;

export declare function capabilities(): Capabilities;

export declare function jsonSchema(): { [key: string]: unknown };

/** A failure of stetdoc. `code` is stable from 1.0; `next` says what to do about it. */
export declare class StetdocError extends Error {
  readonly code: ErrorCode;
  readonly next: string;
  /** The zero-based index of the failing operation of a batch. */
  readonly opIndex?: number;
  /** The readable file of a failing edit of files. */
  readonly file?: string;
  /** The one-based line in `file`. */
  readonly line?: number;
  toJSON(): { code: ErrorCode; message: string; next: string; op_index?: number; file?: string; line?: number };
}
