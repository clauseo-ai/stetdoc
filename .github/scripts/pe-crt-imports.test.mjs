// Tests of pe-crt-imports.mjs on PE images built here: node --test .github/scripts/pe-crt-imports.test.mjs
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { isForbidden, main, peImports } from "./pe-crt-imports.mjs";

/**
 * A minimal PE image with one section that holds an import table naming `imports` and a delay-load table naming
 * `delayed`. `pe32` makes a 32-bit image, and `oldDelay` writes the delay-load names as virtual addresses (attributes 0).
 */
function image({ imports = [], delayed = [], pe32 = false, oldDelay = false } = {}) {
  const imageBase = pe32 ? 0x400000 : 0x140000000;
  const optSize = pe32 ? 96 + 16 * 8 : 112 + 16 * 8;
  const opt = 0x58;
  const sectionHeader = opt + optSize;
  const raw = 0x200;
  const va = 0x1000;
  const size = 0x400;
  const buf = Buffer.alloc(raw + size);
  buf.writeUInt16LE(0x5a4d, 0);
  buf.writeUInt32LE(0x40, 0x3c);
  buf.writeUInt32LE(0x00004550, 0x40);
  buf.writeUInt16LE(pe32 ? 0x14c : 0x8664, 0x44);
  buf.writeUInt16LE(1, 0x46);
  buf.writeUInt16LE(optSize, 0x44 + 16);
  buf.writeUInt16LE(pe32 ? 0x10b : 0x20b, opt);
  if (pe32) buf.writeUInt32LE(imageBase, opt + 28);
  else buf.writeBigUInt64LE(BigInt(imageBase), opt + 24);
  const dirs = pe32 ? opt + 96 : opt + 112;
  buf.writeUInt32LE(16, dirs - 4);
  buf.writeUInt32LE(size, sectionHeader + 8);
  buf.writeUInt32LE(va, sectionHeader + 12);
  buf.writeUInt32LE(size, sectionHeader + 16);
  buf.writeUInt32LE(raw, sectionHeader + 20);
  // Descriptors first, each table ended by a zero one, then the names.
  const importAt = 0;
  const delayAt = importAt + (imports.length + 1) * 20;
  let nameAt = delayAt + (delayed.length + 1) * 32;
  const putName = (name) => {
    const rva = va + nameAt;
    buf.write(`${name}\0`, raw + nameAt, "latin1");
    nameAt += name.length + 1;
    return rva;
  };
  if (imports.length > 0) {
    buf.writeUInt32LE(va + importAt, dirs + 8);
    buf.writeUInt32LE((imports.length + 1) * 20, dirs + 12);
  }
  imports.forEach((name, i) => buf.writeUInt32LE(putName(name), raw + importAt + i * 20 + 12));
  if (delayed.length > 0) {
    buf.writeUInt32LE(va + delayAt, dirs + 13 * 8);
    buf.writeUInt32LE((delayed.length + 1) * 32, dirs + 13 * 8 + 4);
  }
  delayed.forEach((name, i) => {
    const at = raw + delayAt + i * 32;
    const rva = putName(name);
    buf.writeUInt32LE(oldDelay ? 0 : 1, at);
    buf.writeUInt32LE(oldDelay ? imageBase + rva : rva, at + 4);
  });
  return buf;
}

test("the VC++ runtime is forbidden and the Universal CRT and Windows' own CRT are not", () => {
  for (const dll of ["VCRUNTIME140.dll", "vcruntime140_1.dll", "vcruntime140d.dll", "MSVCP140.dll", "msvcp140_atomic_wait.dll",
    "msvcr120.dll", "vccorlib140.dll", "concrt140.dll", "vcomp140.dll"]) {
    assert.equal(isForbidden(dll), true, dll);
  }
  for (const dll of ["api-ms-win-crt-runtime-l1-1-0.dll", "ucrtbase.dll", "msvcrt.dll", "msvcp_win.dll", "KERNEL32.dll",
    "ntdll.dll", "node.exe"]) {
    assert.equal(isForbidden(dll), false, dll);
  }
});

test("the import and delay-load tables are read from PE32+ and PE32 images", () => {
  const imports = ["KERNEL32.dll", "VCRUNTIME140.dll", "api-ms-win-crt-runtime-l1-1-0.dll"];
  assert.deepEqual(peImports(image({ imports, delayed: ["node.exe"] })), [...imports, "node.exe"]);
  assert.deepEqual(peImports(image({ imports, pe32: true, delayed: ["user32.dll"], oldDelay: true })), [...imports, "user32.dll"]);
  assert.deepEqual(peImports(image()), []);
});

test("a file that is not a PE image is refused", () => {
  assert.throws(() => peImports(Buffer.from("#!/bin/sh\necho hi\n")), /not a PE image/);
  const truncated = image({ imports: ["KERNEL32.dll"] }).subarray(0, 0x100);
  assert.throws(() => peImports(truncated), /truncated|no section/);
});

test("the command fails on a VC++ import, a non-PE file or an empty directory, and passes a clean image", () => {
  const dir = mkdtempSync(join(tmpdir(), "pe-crt-imports-"));
  const quiet = (fn) => {
    const [log, error] = [console.log, console.error];
    console.log = console.error = () => {};
    try {
      return fn();
    } finally {
      [console.log, console.error] = [log, error];
    }
  };
  try {
    writeFileSync(join(dir, "clean.exe"), image({ imports: ["KERNEL32.dll", "api-ms-win-crt-heap-l1-1-0.dll"] }));
    writeFileSync(join(dir, "readme.txt"), "not checked: a directory search skips it");
    assert.equal(quiet(() => main([dir])), 0);
    writeFileSync(join(dir, "addon.node"), image({ imports: ["KERNEL32.dll"], delayed: ["VCRUNTIME140.dll"] }));
    assert.equal(quiet(() => main([dir])), 1);
    writeFileSync(join(dir, "fake.dll"), "MZ but nothing else");
    assert.equal(quiet(() => main([join(dir, "fake.dll")])), 1);
    rmSync(join(dir, "clean.exe"));
    rmSync(join(dir, "addon.node"));
    rmSync(join(dir, "fake.dll"));
    assert.equal(quiet(() => main([dir])), 1);
    assert.equal(quiet(() => main([])), 2);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
