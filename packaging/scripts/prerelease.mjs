// Checks a release tag against the workspace version and, for a release candidate, stamps the candidate's version into the
// checkout, so its files can never be mistaken for the release. The release workflow runs it first in every job that
// builds or packs, with the ref it runs on. Usage: node packaging/scripts/prerelease.mjs <ref>
//
// - refs/tags/v<version>, with <version> the one in [workspace.package] of the root Cargo.toml: nothing to stamp.
// - refs/tags/v<version>-<pre>, such as v0.9.0-rc.1: the candidate of <version>. It writes <version>-<pre> into the
//   root Cargo.toml, the workspace's own packages in Cargo.lock (the ones without a `source`, so `--locked` still holds)
//   and the napi library's package.json files, then runs stamp.mjs --write for the other packaging files. maturin turns
//   0.9.0-rc.1 into the PEP 440 0.9.0rc1, and npm sees a prerelease that only `npm publish --tag next` takes.
// - any other tag starting with v fails, so a tag that disagrees with Cargo.toml never builds a release.
// - anything else (a branch, in a manual run): nothing to stamp.
// Nothing is committed: the stamp lives only in the job's checkout.
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const ref = process.argv[2];
if (!ref) {
  console.error("usage: node packaging/scripts/prerelease.mjs <ref>");
  process.exit(2);
}

const stamp = join(root, "packaging", "scripts", "stamp.mjs");
const workspace = execFileSync(process.execPath, [stamp, "--version"], { encoding: "utf8" }).trim();
const tag = ref.startsWith("refs/tags/") ? ref.slice("refs/tags/".length) : null;
if (!tag || !tag.startsWith("v")) {
  console.log(`${ref} is not a release tag: building version ${workspace}`);
  process.exit(0);
}
if (tag === `v${workspace}`) {
  console.log(`${tag} matches the workspace: building version ${workspace}`);
  process.exit(0);
}
const pre = tag.startsWith(`v${workspace}-`) ? tag.slice(`v${workspace}-`.length) : null;
if (!pre || !/^rc\.[1-9][0-9]*$/.test(pre)) {
  console.error(
    `${tag} is neither v${workspace} nor a candidate v${workspace}-rc.N. Set the version in Cargo.toml (and run ` +
      "node packaging/scripts/stamp.mjs --write), commit, then tag.",
  );
  process.exit(1);
}
const version = `${workspace}-${pre}`;

/** Replaces exactly `count` matches of `pattern` in a file, or fails. */
function replace(path, pattern, replacement, count) {
  const absolute = join(root, path);
  const text = readFileSync(absolute, "utf8");
  const found = text.match(pattern)?.length ?? 0;
  if (found !== count) {
    console.error(`${path}: expected ${count} version line(s) to stamp, found ${found}`);
    process.exit(1);
  }
  writeFileSync(absolute, text.replace(pattern, replacement));
}

const quoted = workspace.replaceAll(".", "\\.");
// [workspace.package] version and the stetdoc-core dependency.
replace("Cargo.toml", new RegExp(`^(version = |stetdoc-core = \\{ path = "crates/stetdoc-core", version = )"${quoted}"`, "gm"), `$1"${version}"`, 2);
// Lock entries of the workspace's packages: a [[package]] with the version and no source line. A Windows checkout may
// end its lines with CRLF, so lines are compared without a trailing \r and written back as they were.
{
  const absolute = join(root, "Cargo.lock");
  const lines = readFileSync(absolute, "utf8").split("\n");
  const bare = (line) => line.replace(/\r$/, "");
  let stamped = 0;
  for (let start = 0; start < lines.length; start += 1) {
    if (bare(lines[start]) !== "[[package]]") continue;
    let end = start + 1;
    while (end < lines.length && bare(lines[end]) !== "") end += 1;
    const block = lines.slice(start + 1, end).map(bare);
    const at = block.indexOf(`version = "${workspace}"`);
    if (at >= 0 && !block.some((line) => line.startsWith("source = "))) {
      const index = start + 1 + at;
      lines[index] = lines[index].replace(workspace, version);
      stamped += 1;
    }
  }
  if (stamped === 0) {
    console.error("Cargo.lock: no workspace package carries the workspace version");
    process.exit(1);
  }
  writeFileSync(absolute, lines.join("\n"));
}
// The napi library's packages, which stamp.mjs does not cover (cargo xtask release check does).
const napi = "crates/stetdoc-napi/npm";
for (const path of [`${napi}/package.json`, ...readdirSync(join(root, napi, "targets")).map((t) => `${napi}/targets/${t}/package.json`)]) {
  const absolute = join(root, path);
  const json = JSON.parse(readFileSync(absolute, "utf8"));
  json.version = version;
  for (const name of Object.keys(json.optionalDependencies ?? {})) json.optionalDependencies[name] = version;
  writeFileSync(absolute, JSON.stringify(json, null, 2) + "\n");
}
execFileSync(process.execPath, [stamp, "--write"], { stdio: "inherit" });
console.log(`${tag} is a release candidate: building version ${version}`);
