// Fails when a Windows binary of a release imports the VC++ runtime, which a clean Windows does not have.
// Usage: node .github/scripts/pe-crt-imports.mjs <file or directory>...
//
// A directory is searched, with its subdirectories, for *.exe, *.dll, *.node and *.pyd files; a file named on the command
// line is checked whatever its name. For each file the script reads the import table and the delay-load import table of
// the PE image (PE32 or PE32+, any machine), prints the DLLs it imports, and fails if one of them is part of the VC++
// runtime: vcruntime*, msvcp*, msvcr<NN>, vccorlib*, concrt* or vcomp*, each followed by a digit (so the debug forms such
// as vcruntime140d.dll count too). A file that is not a PE image, or whose tables cannot be read, fails, and so does a run
// that finds no file at all.
//
// Why: the release links the C runtime statically on Windows (`+crt-static` in .cargo/config.toml), so `stetdoc.exe` and
// the napi addon start on a Windows without the VC++ Redistributable. A runner has the C++ build tools, so a binary that
// still imported VCRUNTIME140.dll would pass every smoke test there and fail on a user's machine. The Universal CRT
// (api-ms-win-crt-*.dll and ucrtbase.dll) is part of Windows 10 and later, and msvcrt.dll and msvcp_win.dll are Windows'
// own, so those imports are allowed.
//
// Node built-ins only. The tests are in pe-crt-imports.test.mjs next to it: node --test .github/scripts/pe-crt-imports.test.mjs
import { readFileSync, readdirSync, statSync } from "node:fs";
import { basename, join, resolve } from "node:path";

/** The VC++ runtime DLLs, matched against the lowercased name. Windows' msvcrt.dll and msvcp_win.dll do not match. */
const FORBIDDEN = /^(vcruntime|msvcp|msvcr|vccorlib|concrt|vcomp)\d/;

/** The extensions a directory search picks up. */
const EXTENSIONS = [".exe", ".dll", ".node", ".pyd"];

/** Whether an imported DLL is part of the VC++ runtime. */
export function isForbidden(dll) {
  return FORBIDDEN.test(dll.toLowerCase());
}

/**
 * The names of the DLLs a PE image imports, from its import table and its delay-load import table, in file order.
 * Throws an Error that says what is wrong when the buffer is not a PE image or a table points outside it.
 */
export function peImports(buf) {
  const u16 = (at) => check(at, 2) && buf.readUInt16LE(at);
  const u32 = (at) => check(at, 4) && buf.readUInt32LE(at);
  function check(at, len) {
    if (at < 0 || at + len > buf.length) throw new Error(`truncated: needs bytes ${at}..${at + len}, has ${buf.length}`);
    return true;
  }
  if (buf.length < 64 || u16(0) !== 0x5a4d) throw new Error("not a PE image: no MZ header");
  const pe = u32(0x3c);
  if (u32(pe) !== 0x00004550) throw new Error("not a PE image: no PE signature");
  const coff = pe + 4;
  const sections = u16(coff + 2);
  const optionalSize = u16(coff + 16);
  const opt = coff + 20;
  const magic = u16(opt);
  let imageBase;
  let dirs;
  let dirCount;
  if (magic === 0x10b) {
    imageBase = BigInt(u32(opt + 28));
    dirCount = u32(opt + 92);
    dirs = opt + 96;
  } else if (magic === 0x20b) {
    check(opt + 24, 8);
    imageBase = buf.readBigUInt64LE(opt + 24);
    dirCount = u32(opt + 108);
    dirs = opt + 112;
  } else {
    throw new Error(`not a PE image: optional header magic 0x${magic.toString(16)}`);
  }
  const table = [];
  for (let i = 0, at = opt + optionalSize; i < sections; i += 1, at += 40) {
    table.push({ va: u32(at + 12), vsize: u32(at + 8), raw: u32(at + 20), rawSize: u32(at + 16) });
  }
  /** The file offset of a relative virtual address. */
  function offset(rva) {
    for (const s of table) {
      const size = Math.max(s.vsize, s.rawSize);
      if (rva >= s.va && rva < s.va + size) {
        const at = s.raw + (rva - s.va);
        check(at, 1);
        return at;
      }
    }
    throw new Error(`RVA 0x${rva.toString(16)} is in no section`);
  }
  function cString(rva) {
    const start = offset(rva);
    const end = buf.indexOf(0, start);
    if (end < 0) throw new Error(`the name at RVA 0x${rva.toString(16)} has no end`);
    return buf.toString("latin1", start, end);
  }
  const dir = (index) => (index < dirCount ? { rva: u32(dirs + index * 8), size: u32(dirs + index * 8 + 4) } : { rva: 0, size: 0 });
  const names = [];
  // The import table: 20-byte descriptors, the name's RVA at +12, ended by one of zeros.
  const imports = dir(1);
  if (imports.rva !== 0) {
    for (let at = offset(imports.rva); ; at += 20) {
      check(at, 20);
      if (buf.subarray(at, at + 20).every((b) => b === 0)) break;
      names.push(cString(u32(at + 12)));
    }
  }
  // The delay-load import table: 32-byte descriptors, the name at +4. Bit 0 of the attributes clear means the old form,
  // whose fields are virtual addresses rather than RVAs.
  const delayed = dir(13);
  if (delayed.rva !== 0) {
    for (let at = offset(delayed.rva); ; at += 32) {
      check(at, 32);
      if (buf.subarray(at, at + 32).every((b) => b === 0)) break;
      const attributes = u32(at);
      let name = u32(at + 4);
      if ((attributes & 1) === 0) name = Number(BigInt(name) - imageBase);
      names.push(cString(name));
    }
  }
  return names;
}

/** The files to check: each file named, and the matching files under each directory named. */
function collect(paths) {
  const files = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (EXTENSIONS.some((ext) => entry.name.toLowerCase().endsWith(ext))) files.push(path);
    }
  };
  for (const path of paths) {
    if (statSync(path).isDirectory()) walk(path);
    else files.push(path);
  }
  return files;
}

/** Checks the paths, prints a line per file, and returns the exit code. */
export function main(paths) {
  if (paths.length === 0) {
    console.error("usage: node .github/scripts/pe-crt-imports.mjs <file or directory>...");
    return 2;
  }
  let files;
  try {
    files = collect(paths.map((p) => resolve(p)));
  } catch (e) {
    console.error(e.message);
    return 1;
  }
  if (files.length === 0) {
    console.error(`no .exe, .dll, .node or .pyd file under ${paths.join(", ")}`);
    return 1;
  }
  let failed = 0;
  for (const file of files) {
    let names;
    try {
      names = peImports(readFileSync(file));
    } catch (e) {
      console.error(`FAIL ${file}: ${e.message}`);
      failed += 1;
      continue;
    }
    const bad = names.filter(isForbidden);
    if (bad.length > 0) {
      console.error(`FAIL ${file} imports the VC++ runtime: ${bad.join(", ")}. Link it with -C target-feature=+crt-static.`);
      failed += 1;
    } else {
      console.log(`ok   ${file}: ${names.join(", ") || "no imports"}`);
    }
  }
  if (failed > 0) {
    console.error(`${failed} of ${files.length} file(s) would not start on a Windows without the VC++ Redistributable`);
    return 1;
  }
  console.log(`${files.length} file(s) import no VC++ runtime DLL`);
  return 0;
}

// Run when started as a script, not when the tests import it. The name is compared rather than the URL, because on Windows
// the drive letter of import.meta.url and of argv[1] can differ in case, and a check that silently ran nothing would pass.
if (basename(process.argv[1] ?? "") === "pe-crt-imports.mjs") {
  process.exitCode = main(process.argv.slice(2));
}
