// The smoke test of the library: Node runs it with its built-in type stripping (`node test/smoke.mts`) and Bun runs it as it
// is (`bun test/smoke.mts`). It builds a small .docx in memory, so no fixture is committed, and exits non-zero on the first
// failed check. It uses only erasable TypeScript.
import { Worker } from "node:worker_threads";
import { Document, StetdocError, capabilities, compare, formatVersion, jsonSchema, version } from "../index.js";
import type { Receipt, View, WorkingCopy } from "../index.js";

const runtime = typeof (globalThis as { Bun?: unknown }).Bun === "undefined" ? `node ${process.version}` : `bun ${(globalThis as { Bun?: { version: string } }).Bun?.version}`;
let passed = 0;

function check(what: string, ok: unknown, detail?: unknown): void {
  if (!ok) {
    console.error(`not ok: ${what}`);
    if (detail !== undefined) console.error(detail);
    process.exit(1);
  }
  passed += 1;
}

function throws(what: string, fn: () => unknown): StetdocError {
  try {
    fn();
  } catch (error) {
    check(`${what} throws a StetdocError`, error instanceof StetdocError, error);
    return error as StetdocError;
  }
  check(`${what} throws`, false);
  throw new Error("unreachable");
}

// A stored ZIP of the smallest package Word opens.
const CRC = new Uint32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});

function crc32(bytes: Uint8Array): number {
  let c = 0xffffffff;
  for (const b of bytes) c = CRC[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function zip(entries: [string, string][]): Uint8Array {
  const enc = new TextEncoder();
  const locals: Uint8Array[] = [];
  const centrals: Uint8Array[] = [];
  let offset = 0;
  for (const [name, text] of entries) {
    const nameBytes = enc.encode(name);
    const data = enc.encode(text);
    const crc = crc32(data);
    const local = new DataView(new ArrayBuffer(30));
    local.setUint32(0, 0x04034b50, true);
    local.setUint16(4, 20, true);
    local.setUint16(12, 0x21, true);
    local.setUint32(14, crc, true);
    local.setUint32(18, data.length, true);
    local.setUint32(22, data.length, true);
    local.setUint16(26, nameBytes.length, true);
    const central = new DataView(new ArrayBuffer(46));
    central.setUint32(0, 0x02014b50, true);
    central.setUint16(4, 20, true);
    central.setUint16(6, 20, true);
    central.setUint16(14, 0x21, true);
    central.setUint32(16, crc, true);
    central.setUint32(20, data.length, true);
    central.setUint32(24, data.length, true);
    central.setUint16(28, nameBytes.length, true);
    central.setUint32(42, offset, true);
    locals.push(new Uint8Array(local.buffer), nameBytes, data);
    centrals.push(new Uint8Array(central.buffer), nameBytes);
    offset += 30 + nameBytes.length + data.length;
  }
  const size = centrals.reduce((n, b) => n + b.length, 0);
  const end = new DataView(new ArrayBuffer(22));
  end.setUint32(0, 0x06054b50, true);
  end.setUint16(8, entries.length, true);
  end.setUint16(10, entries.length, true);
  end.setUint32(12, size, true);
  end.setUint32(16, offset, true);
  const parts = [...locals, ...centrals, new Uint8Array(end.buffer)];
  const out = new Uint8Array(parts.reduce((n, b) => n + b.length, 0));
  let at = 0;
  for (const part of parts) {
    out.set(part, at);
    at += part.length;
  }
  return out;
}

const DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n';
const W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

function docx(paragraphs: string[]): Uint8Array {
  const body = paragraphs.map((p) => `<w:p><w:r><w:t xml:space="preserve">${p}</w:t></w:r></w:p>`).join("");
  return zip([
    [
      "[Content_Types].xml",
      `${DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>`,
    ],
    [
      "_rels/.rels",
      `${DECL}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>`,
    ],
    ["word/document.xml", `${DECL}<w:document xmlns:w="${W}"><w:body>${body}</w:body></w:document>`],
  ]);
}

const original = docx([
  "The Supplier shall indemnify the Customer.",
  "Emoji \u{1F600} then the end.",
  "Payment is due within thirty days.",
  "Payment is due on demand.",
]);
const stamp = { author: "Jane Doe", date: "2026-10-06T10:00:00Z" };

// Open, and an unedited save returns the input bytes.
const doc = Document.open(original);
const unedited = doc.save();
check("an unedited save returns the input bytes", Buffer.compare(Buffer.from(unedited.bytes), Buffer.from(original)) === 0);
check("an unedited save says so", unedited.receipt.edited === false, unedited.receipt);

// A view.
const view = doc.view();
check("a view carries the base token", view.base === doc.base(), view);
check("a view shows the text", view.text.includes("Payment is due within thirty days."), view.text);

// An edit with a UTF-16 offset after the emoji: "Emoji 😀 then" has "then" at 9..13 in UTF-16 units.
const receipt: Receipt = doc.edit({
  base: doc.base(),
  ops: [{ op: "replace", ref: "p2", start: 9, end: 13, with: "and" }],
  ...stamp,
});
check("an edit returns a receipt with a new base", receipt.base === doc.base() && receipt.base !== view.base, receipt);
const final = { mode: "final" as const };
check("the edit counted UTF-16 units", doc.view(final).text.includes("Emoji \u{1F600} and the end."), doc.view(final).text);
const revisions = doc.revisions({ author: "Jane Doe" });
check("the edit wrote tracked revisions by the author", revisions.length > 0, revisions);

// Export and resume keep refs.
const copy: WorkingCopy = doc.export();
check("a working copy has readable files", copy.files.some(([name]) => name === "document.md"), copy.files);
check("a working copy has a complement of bytes", copy.complement.length > 0 && copy.complement[0][1] instanceof Uint8Array);
const resumed = Document.resume(copy);
check("a resumed document has the same base", resumed.base() === doc.base());
resumed.edit({
  base: resumed.base(),
  ops: [{ op: "replace", ref: "p3", start: 0, end: 7, with: "The fee" }],
  ...stamp,
});
check("refs stay valid after resume", resumed.view(final).text.includes("The fee is due within thirty days."), resumed.view(final).text);

// A failing operation is a StetdocError with its code and operation index.
const missing = throws("an edit naming a missing ref", () =>
  resumed.edit({
    base: resumed.base(),
    ops: [
      { op: "replace", ref: "p1", start: 0, end: 3, with: "A" },
      { op: "replace", ref: "p99", start: 0, end: 1, with: "x" },
    ],
    ...stamp,
  }),
);
check("the error has a code and a next step", missing.code === "not_found" && missing.next.length > 0, missing);
check("the error names the operation", missing.opIndex === 1, missing);
check("the error serializes to the wire shape", JSON.stringify(missing).includes('"op_index":1'), JSON.stringify(missing));

// A failing edit of files names the file and the line: a scope of p3, and an edit that also changes p4.
const files = resumed.files();
const [name, text] = files.files[0];
const outside = text.replace("thirty days.", "sixty days.").replace("on demand.", "at once.");
const scoped = throws("an edit of files outside its scope", () =>
  resumed.edit({ files: { ...files, files: [[name, outside]] }, scope: ["p3"], ...stamp }),
);
check("the error is scope_violation", scoped.code === "scope_violation", scoped);
check("the error names the file and line", scoped.file === name && typeof scoped.line === "number" && scoped.line > 0, scoped);
check("the error names no operation", scoped.opIndex === undefined, scoped);

// A misspelt option is refused, not ignored.
const misspelt = throws("a misspelt edit option", () =>
  resumed.edit({ base: resumed.base(), ops: [], ...stamp, dry_run: true } as never),
);
check("a misspelt option is invalid_payload", misspelt.code === "invalid_payload", misspelt);

// A package that is not a .docx.
const malformed = throws("opening bytes that are not a package", () => Document.open(new Uint8Array([1, 2, 3])));
check("bad bytes are malformed_input", malformed.code === "malformed_input", malformed);

// Compare the original with the saved package.
const saved = resumed.save();
check("an edited save says so", saved.receipt.edited === true, saved.receipt);
const compared = compare(original, saved.bytes, { stamp });
check("compare returns a document", compared.document instanceof Document);
check("compare returns a receipt", typeof compared.receipt.text === "string" && compared.receipt.operations.length === 0, compared.receipt);
check("the redline has revisions by the stamp's author", compared.document.revisions({ author: "Jane Doe" }).length > 0);
check("the redline reads as the revised text", compared.document.view(final).text.includes("The fee is due within thirty days."), compared.document.view(final).text);
const rejected = compared.document.view({ mode: "original" }).text;
check("rejecting the redline gives the original text", rejected.includes("Payment is due within thirty days."), rejected);
const refused = throws("compare of bytes that are not a package", () => compare(new Uint8Array([1, 2, 3]), original, { stamp }));
check("a bad input to compare is malformed_input", refused.code === "malformed_input", refused);

// A working copy of the wrong shape is refused as a payload, not with a TypeError.
for (const bad of [null, { files: "x", complement: [] }, { files: [], complement: [["a", "not bytes"]] }]) {
  const error = throws(`resuming ${JSON.stringify(bad)}`, () => Document.resume(bad as never));
  check("a malformed working copy is invalid_payload", error.code === "invalid_payload", error);
}

// Half of a surrogate pair, as slicing a string through an emoji leaves, is named as such.
const half = "\u{1F600}".slice(0, 1);
const surrogate = throws("an edit with an unpaired surrogate", () =>
  resumed.edit({ base: resumed.base(), ops: [{ op: "replace", ref: "p1", start: 0, end: 3, with: `A${half}` }], ...stamp }),
);
check("an unpaired surrogate is invalid_payload, named", surrogate.code === "invalid_payload" && surrogate.message.includes("unpaired surrogate"), surrogate);

// Claude's text editor tool over the readable files, with its input exactly as the model sends it.
const editing = Document.open(original);
const tool = { ...stamp, pathPrefix: "/documents/acme/" };
const listed = editing.textEditor({ command: "view", path: "/documents/acme" }, tool);
check("the editor lists the files under the prefix", !listed.isError && listed.content.includes("/documents/acme/document.md"), listed);
const shown = editing.textEditor({ command: "view", path: "/documents/acme/document.md" }, tool);
check("the editor numbers lines", /^ {5}1\t\{stet 1 file=document\.md/.test(shown.content), shown.content);
check("a view commits nothing", !shown.committed && shown.receipt === undefined, shown);
const stray = editing.textEditor({ command: "view", path: "/document.md" }, tool);
check("a path outside the prefix is not_found", stray.isError && stray.content.startsWith("Error not_found"), stray.content);
const replaced = editing.textEditor(
  { command: "str_replace", path: "/documents/acme/document.md", old_str: "within thirty days", new_str: "within sixty days" },
  tool,
);
check("a unique str_replace commits", !replaced.isError && replaced.committed, replaced.content);
check("it returns the receipt", replaced.receipt?.base === editing.base(), replaced.receipt);
check("the edit is tracked", editing.revisions({ author: "Jane Doe" }).length > 0);
const inserted = editing.textEditor(
  JSON.stringify({ command: "insert", path: "/documents/acme/document.md", insert_line: 5, insert_text: "Interest accrues daily." }),
  tool,
);
check("insert takes the JSON input and commits", inserted.committed && inserted.receipt?.added_count === 1, inserted.content);
const ambiguous = editing.textEditor(
  { command: "str_replace", path: "/documents/acme/document.md", old_str: "Payment is due", new_str: "x" },
  tool,
);
check("an ambiguous str_replace is an error result", ambiguous.isError && ambiguous.content.startsWith("Error ambiguous_match"), ambiguous.content);
check("it lists each match", ambiguous.content.includes("Match 2 at line"), ambiguous.content);
check("a failed command commits nothing", !ambiguous.committed, ambiguous);
const halfTool = editing.textEditor({ command: "str_replace", path: "/documents/acme/document.md", old_str: "Payment", new_str: half }, tool);
check("the editor names an unpaired surrogate", halfTool.isError && halfTool.content.includes("unpaired surrogate"), halfTool.content);
const created = editing.textEditor({ command: "create", path: "/documents/acme/new.md", file_text: "x" }, tool);
check("create is unsupported", created.isError && created.content.startsWith("Error unsupported"), created.content);

// The JSON view, with no budget and the Final text.
const jsonView: View = editing.view({ budget: 0, mode: "final", format: "json/1" });
check("a JSON view parses", typeof JSON.parse(jsonView.text) === "object", jsonView.text.slice(0, 200));

// Worker threads: two workers load the addon and edit their own document at the same time as this thread.
function inWorker(replacement: string): Promise<{ version: string; base: string; text: string; saved: number }> {
  return new Promise((resolve, reject) => {
    const worker = new Worker(new URL("./worker.mjs", import.meta.url), { workerData: { bytes: original, with: replacement } });
    worker.once("message", (message) => {
      void worker.terminate();
      resolve(message);
    });
    worker.once("error", reject);
  });
}
const [first, second] = await Promise.all([inWorker("The fee"), inWorker("The price")]);
check("a worker loads the same addon", first.version === version && second.version === version, [first.version, second.version]);
check("each worker edits its own document", first.text.includes("The fee is due") && second.text.includes("The price is due"), [first.text, second.text]);
check("the main thread still works after the workers end", Document.open(original).base().length > 0);
const third = await inWorker("The sum");
check("a new worker after others ended loads it again", third.text.includes("The sum is due"), third.text);

// Capabilities and the schema.
const caps = capabilities();
check("capabilities list the operations", Array.isArray(caps.ops) && caps.ops.length > 0, Object.keys(caps));
const schema = jsonSchema();
check("the schema is a JSON Schema", typeof schema === "object" && schema !== null && "$defs" in schema, Object.keys(schema));
check("version is the release", typeof version === "string" && version === caps.version, version);
check("formatVersion is the complement's", typeof formatVersion === "string" && formatVersion === caps.complement_version, formatVersion);

console.log(`ok: ${passed} checks on ${runtime}`);
