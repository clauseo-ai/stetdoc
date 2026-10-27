// The public surface of the stetdoc library, built on the native addon. See docs/design/napi-and-editor.md.
//
// Every data type crosses the addon as JSON made by the core's serde, so this file parses results and stringifies
// requests, maps a thrown StetError onto StetdocError, and turns the camelCase request objects into the core's names.
// It is CommonJS with static named exports, so both `require("@clauseo/stetdoc")` and `import { Document } from "@clauseo/stetdoc"` work
// in Node and in Bun.
"use strict";

const PREFIX = "stetdoc:";

/** The targets the addon is built for, as `${platform}-${arch}[-${abi}]`, in the order to try them. */
function targets() {
  const { platform, arch } = process;
  if (platform === "linux") {
    let glibc;
    try {
      glibc = process.report?.getReport?.()?.header?.glibcVersionRuntime;
    } catch {
      glibc = undefined;
    }
    const abis = glibc === undefined ? ["musl", "gnu"] : ["gnu", "musl"];
    return abis.map((abi) => `linux-${arch}-${abi}`);
  }
  if (platform === "win32") return [`win32-${arch}-msvc`];
  return [`${platform}-${arch}`];
}

function load() {
  const tried = [];
  for (const target of targets()) {
    for (const path of [`./stetdoc.${target}.node`, `@clauseo/stetdoc-${target}`]) {
      try {
        return require(path);
      } catch (error) {
        tried.push(`${path}: ${error && error.message ? error.message.split("\n")[0] : error}`);
      }
    }
  }
  throw new Error(
    `stetdoc has no native addon for ${process.platform}-${process.arch}. Install the package for your platform, ` +
      `or build one with \`npm run build\` in a checkout. Tried:\n  ${tried.join("\n  ")}`,
  );
}

const native = load();

/** A failure of stetdoc: a stable code, a one-line message, the next step, and where it came from. */
class StetdocError extends Error {
  constructor(data) {
    super(data.message);
    this.name = "StetdocError";
    this.code = data.code;
    this.next = data.next;
    if (data.op_index !== undefined && data.op_index !== null) this.opIndex = data.op_index;
    if (data.file !== undefined && data.file !== null) this.file = data.file;
    if (data.line !== undefined && data.line !== null) this.line = data.line;
  }

  /** The wire shape: what the CLI's --json prints and the JSON Schema calls StetError. */
  toJSON() {
    const out = { code: this.code, message: this.message, next: this.next };
    if (this.opIndex !== undefined) out.op_index = this.opIndex;
    if (this.file !== undefined) out.file = this.file;
    if (this.line !== undefined) out.line = this.line;
    return out;
  }
}

function mapError(error) {
  if (error && typeof error.message === "string" && error.message.startsWith(PREFIX)) {
    try {
      return new StetdocError(JSON.parse(error.message.slice(PREFIX.length)));
    } catch {
      return error;
    }
  }
  return error;
}

function call(fn) {
  try {
    return fn();
  } catch (error) {
    throw mapError(error);
  }
}

function invalid(message) {
  return new StetdocError({
    code: "invalid_payload",
    message,
    next: "Fix the payload and retry. The capabilities list names the valid operations and their fields.",
  });
}

function toBuffer(bytes, what) {
  if (Buffer.isBuffer(bytes)) return bytes;
  if (bytes instanceof Uint8Array) return Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes instanceof ArrayBuffer) return Buffer.from(bytes);
  throw invalid(`The ${what} must be a Uint8Array, a Buffer or an ArrayBuffer.`);
}

/** Refuses keys a request object does not have, so that a misspelt option is never silently ignored. */
function only(what, object, keys) {
  if (object === null || typeof object !== "object") throw invalid(`The ${what} must be an object.`);
  for (const key of Object.keys(object)) {
    if (!keys.includes(key)) throw invalid(`The ${what} has no field ${key}. Its fields are ${keys.join(", ")}.`);
  }
}

const EDIT_KEYS = ["base", "ops", "batch", "files", "author", "date", "untracked", "scope", "dryRun", "quotes", "offsets"];
const CHECK_KEYS = ["pending", "author", "date", "strict"];
const TOKEN = Symbol("stetdoc.native");

/** One open .docx package or resumed working copy. Keep one per file and serialize edits to it. */
class Document {
  #native;

  constructor(token, handle) {
    if (token !== TOKEN) throw new TypeError("Use Document.open, Document.resume or compare to make a Document.");
    this.#native = handle;
  }

  /** Opens the bytes of a .docx package. */
  static open(bytes, options = {}) {
    const buffer = toBuffer(bytes, "package");
    return new Document(TOKEN, call(() => native.NativeDocument.open(buffer, JSON.stringify(options))));
  }

  /** Resumes a working copy that `export` returned. Refs stay valid, which reopening saved bytes would not do. */
  static resume(workingCopy, options = {}) {
    only("working copy", workingCopy, ["files", "complement"]);
    const pairs = (list) => Array.isArray(list) && list.every((entry) => Array.isArray(entry) && entry.length === 2);
    if (!pairs(workingCopy.files) || !workingCopy.files.every(([name, text]) => typeof name === "string" && typeof text === "string")) {
      throw invalid("The working copy's files must be [name, text] pairs of strings, as export returned them.");
    }
    if (
      !pairs(workingCopy.complement) ||
      !workingCopy.complement.every(([name, bytes]) => typeof name === "string" && (bytes instanceof Uint8Array || bytes instanceof ArrayBuffer))
    ) {
      throw invalid("The working copy's complement must be [name, Uint8Array] pairs, as export returned them.");
    }
    const copy = {
      files: JSON.stringify(workingCopy.files),
      complement: workingCopy.complement.map(([name, bytes]) => ({ name, bytes: toBuffer(bytes, "complement entry") })),
    };
    return new Document(TOKEN, call(() => native.NativeDocument.resume(copy, JSON.stringify(options))));
  }

  /** The base token: what a batch names when it was written with no view. */
  base() {
    return this.#native.base();
  }

  /** A page of the document. The default is the body whole when it fits the budget, otherwise its outline. */
  view(spec = {}) {
    return JSON.parse(call(() => this.#native.view(JSON.stringify(spec))));
  }

  /** Applies operations or hand-edited readable files, tracked by default, and returns the receipt. */
  edit(request) {
    only("edit request", request, EDIT_KEYS);
    const wire = {
      base: request.base,
      ops: request.ops,
      batch: request.batch,
      files: request.files,
      author: request.author,
      date: request.date,
      untracked: request.untracked,
      scope: request.scope,
      dry_run: request.dryRun,
      quotes: request.quotes,
      offsets: request.offsets,
    };
    return JSON.parse(call(() => this.#native.edit(JSON.stringify(wire))));
  }

  /** The package. An unedited document returns exactly the bytes it was opened with. */
  save() {
    const saved = call(() => this.#native.save());
    return { bytes: saved.bytes, receipt: JSON.parse(saved.receipt) };
  }

  /** The details of one block, or of `styles`, `numbering` or `sections`. */
  inspect(target) {
    return JSON.parse(call(() => this.#native.inspect(String(target))));
  }

  /** The tracked changes, filtered by author or kind. */
  revisions(filter = {}) {
    return JSON.parse(call(() => this.#native.revisions(JSON.stringify(filter))));
  }

  /**
   * Every place a text is, matched as an edit's `find` matches it: in the Final text, exactly, or with curly and straight
   * quotes taken as the same when nothing matches exactly. `scope` is "auto" (the body, the notes and the headers and
   * footers), `{ range: { from, to } }`, `{ clause: "11.1" }` (or several, `{ clause: ["11.1", "12.3"] }`) or
   * `{ story: "body" }`. Each hit names its block and its `occurrence`, so an edit against `base()` can target it.
   */
  occurrences(text, scope = "auto") {
    return JSON.parse(call(() => this.#native.occurrences(String(text), JSON.stringify(scope))));
  }

  /** A defined term, a clause number or a cross-reference, resolved. */
  lookup(query) {
    return JSON.parse(call(() => this.#native.lookup(String(query))));
  }

  /** The schema and Word-rule problems of the document as it is. */
  check() {
    return JSON.parse(call(() => this.#native.check()));
  }

  /** A dry run of pending readable files, the revisions already there, and the check report. */
  checkPending(request = {}) {
    only("check request", request, CHECK_KEYS);
    return JSON.parse(call(() => this.#native.checkPending(JSON.stringify(request))));
  }

  /** The readable files, as `export` would write them. */
  files() {
    return JSON.parse(call(() => this.#native.files()));
  }

  /**
   * One call of Claude's text editor tool (text_editor_20250728, str_replace_based_edit_tool) over the readable files.
   * `input` is the tool_use block's input exactly as the model sent it, as an object or its JSON. A str_replace or insert
   * commits at once, tracked, signed by `author` and `date`. `pathPrefix`, such as "/documents/acme/", is the virtual
   * directory the files appear in. A failure the model should read comes back with `isError` set and is not thrown.
   */
  textEditor(input, options) {
    only("text editor options", options, ["author", "date", "pathPrefix"]);
    const text = typeof input === "string" ? input : JSON.stringify(input);
    const wire = { author: options.author, date: options.date, path_prefix: options.pathPrefix };
    const result = call(() => this.#native.textEditor(text, JSON.stringify(wire)));
    const receipt = result.receipt == null ? undefined : JSON.parse(result.receipt);
    return { content: result.content, isError: result.isError, committed: receipt !== undefined, receipt };
  }

  /** The working copy: the readable files and the complement. Keep it between turns and resume it. */
  export() {
    const copy = call(() => this.#native.export());
    return { files: JSON.parse(copy.files), complement: copy.complement.map((entry) => [entry.name, entry.bytes]) };
  }
}

/** Compares two packages into a redline that accepts to the revised one and rejects to the original. */
function compare(original, revised, options) {
  const a = toBuffer(original, "original package");
  const b = toBuffer(revised, "revised package");
  const handle = call(() => native.NativeDocument.compare(a, b, JSON.stringify(options)));
  return { document: new Document(TOKEN, handle), receipt: JSON.parse(handle.comparedReceipt()) };
}

/** What this build supports: operations, fields, formats, error codes and finding codes. */
function capabilities() {
  return JSON.parse(call(() => native.capabilities()));
}

/** One JSON Schema document for every public type. */
function jsonSchema() {
  return JSON.parse(call(() => native.jsonSchema()));
}

/** The release of stetdoc this addon was built from, such as "1.0.0". Pin it against the CLI's `stetdoc --version`. */
const version = capabilities().version;

/** The version of the working copy's complement that this release writes and reads, such as "1.0". */
const formatVersion = capabilities().complement_version;

module.exports = { Document, StetdocError, compare, capabilities, jsonSchema, version, formatVersion };
