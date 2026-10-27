// Packs the npm tarballs of the CLI: one package per target, each holding its stetdoc binary, and the main package.
// It only writes files: nothing is published. The release workflow runs it on the binaries of its CLI build.
// Usage: node scripts/pack.mjs <binaries dir> <out dir>
//   <binaries dir> holds one directory per target, named like the target package, each with `stetdoc` (or `stetdoc.exe`).
// npm names the tarballs after the packages: stetdoc-<version>.tgz for the main one and clauseo-stetdoc-cli-<target>-<version>.tgz
// for each target, so they never collide with the napi addon's clauseo-stetdoc-*.tgz in the release.
import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, existsSync, mkdirSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const [binaries, out] = process.argv.slice(2).map((path) => resolve(path));
if (!binaries || !out) {
  console.error("usage: node scripts/pack.mjs <binaries dir> <out dir>");
  process.exit(2);
}
mkdirSync(out, { recursive: true });

const npm = process.platform === "win32" ? "npm.cmd" : "npm";
/** Packs one package directory and returns the tarball's file name. */
function pack(dir) {
  const [{ filename }] = JSON.parse(
    execFileSync(npm, ["pack", "--json", "--pack-destination", out], { cwd: dir, encoding: "utf8" }),
  );
  return filename;
}

const targets = readdirSync(join(here, "targets"));
const binaryOf = (target) => (target.startsWith("win32-") ? "stetdoc.exe" : "stetdoc");
const missing = targets.filter((target) => !existsSync(join(binaries, target, binaryOf(target))));
if (missing.length > 0) {
  console.error(`no binary for ${missing.join(", ")} in ${binaries}`);
  process.exit(1);
}
for (const target of targets) {
  const bin = join(here, "targets", target, "bin");
  mkdirSync(bin, { recursive: true });
  const file = join(bin, binaryOf(target));
  copyFileSync(join(binaries, target, binaryOf(target)), file);
  chmodSync(file, 0o755);
  pack(join(here, "targets", target));
}
pack(here);
console.log(`packed ${targets.length + 1} tarballs into ${out}`);
