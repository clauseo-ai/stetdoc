// Fills the placeholders of the Homebrew formula template from the release's checksum files, and writes the result.
// Usage: node packaging/scripts/fill-formula.mjs <template> <release assets dir> <version> <out file>
// It reads stetdoc-<triple>.tar.gz.sha256 for the four macOS and Linux triples and replaces @VERSION@ and
// @SHA256_<TRIPLE>@ (the triple in capitals, with underscores). It fails if a checksum is missing or a placeholder is left.
// It writes the file only. Nothing is pushed to a tap.
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [template, assets, version, out] = process.argv.slice(2);
if (!template || !assets || !version || !out) {
  console.error("usage: node packaging/scripts/fill-formula.mjs <template> <assets dir> <version> <out file>");
  process.exit(2);
}

const triples = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "aarch64-unknown-linux-musl",
  "x86_64-unknown-linux-musl",
];
const sums = {};
for (const triple of triples) {
  const file = join(assets, `stetdoc-${triple}.tar.gz.sha256`);
  const [hash, name] = readFileSync(file, "utf8").trim().split(/\s+/);
  if (!/^[0-9a-f]{64}$/.test(hash ?? "")) {
    console.error(`${file}: not a SHA-256 hash`);
    process.exit(1);
  }
  if (name !== `stetdoc-${triple}.tar.gz`) {
    console.error(`${file}: names ${name}, not stetdoc-${triple}.tar.gz`);
    process.exit(1);
  }
  sums[`@SHA256_${triple.toUpperCase().replaceAll("-", "_")}@`] = hash;
}

let text = readFileSync(template, "utf8").replaceAll("@VERSION@", version);
for (const [placeholder, hash] of Object.entries(sums)) text = text.replaceAll(placeholder, hash);
const left = text.match(/@[A-Z0-9_]+@/g);
if (left) {
  console.error(`the formula still has placeholders: ${[...new Set(left)].join(", ")}`);
  process.exit(1);
}
writeFileSync(out, text);
console.log(`wrote ${out} for version ${version}`);
