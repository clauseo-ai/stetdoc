// Keeps the version strings of the packaging files in step with the workspace version in the root Cargo.toml, which is the
// one place a version is bumped. `--check` lists every disagreement and exits 1. `--write` rewrites the files that disagree.
// The release workflow runs `--write` before it packs anything, so a release never ships a stale version. Before a tag,
// run `node packaging/scripts/stamp.mjs --write` and commit the result, so the channels that install from git (the Claude
// Code, Cursor and OpenAI plugins, which read the repository) carry the same version.
// The Python distribution (packaging/pypi/pyproject.toml) carries no version: maturin reads it from the stetdoc crate, which
// takes it from the workspace. `--check` fails if pyproject.toml declares a static version instead of `dynamic = ["version"]`.
// `--version` prints the workspace version, for the release workflow. Usage: node packaging/scripts/stamp.mjs [--check | --write | --version]
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const mode = process.argv[2] ?? "--check";
if (!["--check", "--write", "--version"].includes(mode)) {
  console.error("usage: node packaging/scripts/stamp.mjs [--check | --write | --version]");
  process.exit(2);
}

/** The version of [workspace.package] in the root Cargo.toml. */
function workspaceVersion() {
  let inPackage = false;
  for (const raw of readFileSync(join(root, "Cargo.toml"), "utf8").split("\n")) {
    const line = raw.trim();
    if (line.startsWith("[")) {
      inPackage = line === "[workspace.package]";
    } else if (inPackage) {
      const match = /^version\s*=\s*"([^"]+)"/.exec(line);
      if (match) return match[1];
    }
  }
  throw new Error("Cargo.toml: no version in [workspace.package]");
}

const version = workspaceVersion();
if (mode === "--version") {
  console.log(version);
  process.exit(0);
}
const cli = "packaging/npm/stetdoc";
const files = [
  "plugins/claude/.claude-plugin/plugin.json",
  "plugins/cursor/.cursor-plugin/plugin.json",
  "plugins/openai/plugin.json",
  `${cli}/package.json`,
  ...readdirSync(join(root, cli, "targets")).map((target) => `${cli}/targets/${target}/package.json`),
  "packaging/mcp-registry/server.json",
];

/** The places in a parsed file that carry the version, as [object, key, label] triples. */
function versionSlots(path, json) {
  const slots = [[json, "version", "version"]];
  if (path === `${cli}/package.json`) {
    for (const name of Object.keys(json.optionalDependencies ?? {})) {
      slots.push([json.optionalDependencies, name, `optionalDependencies ${name}`]);
    }
  }
  if (path === "packaging/mcp-registry/server.json") {
    for (const [index, pkg] of (json.packages ?? []).entries()) {
      slots.push([pkg, "version", `packages[${index}].version`]);
    }
  }
  return slots;
}

const problems = [];
let written = 0;
for (const path of files) {
  const absolute = join(root, path);
  const text = readFileSync(absolute, "utf8");
  const json = JSON.parse(text);
  let changed = false;
  for (const [object, key, label] of versionSlots(path, json)) {
    if (object[key] === version) continue;
    if (mode === "--write") {
      object[key] = version;
      changed = true;
    } else {
      problems.push(`${path}: ${label} is ${JSON.stringify(object[key])}, but Cargo.toml says ${version}. Run node packaging/scripts/stamp.mjs --write.`);
    }
  }
  if (changed) {
    writeFileSync(absolute, JSON.stringify(json, null, 2) + "\n");
    written += 1;
  }
}

// The pyproject's [project] table must leave the version to maturin, so a wheel always carries the workspace version.
const pyproject = "packaging/pypi/pyproject.toml";
{
  let table = "";
  let dynamicVersion = false;
  for (const raw of readFileSync(join(root, pyproject), "utf8").split("\n")) {
    const line = raw.trim();
    if (line.startsWith("[")) table = line;
    else if (table === "[project]" && /^version\s*=/.test(line)) {
      problems.push(`${pyproject}: [project] has a static version. Remove it and keep dynamic = ["version"], so maturin takes ${version} from Cargo.toml.`);
    } else if (table === "[project]" && /^dynamic\s*=.*"version"/.test(line)) dynamicVersion = true;
  }
  if (!dynamicVersion) problems.push(`${pyproject}: [project] lacks dynamic = ["version"]. Add it, so maturin takes ${version} from Cargo.toml.`);
}

if (mode === "--check") {
  if (problems.length > 0) {
    console.error(problems.join("\n"));
    process.exit(1);
  }
  console.log(`the packaging files carry version ${version}`);
} else {
  // --write cannot fix a pyproject.toml problem, so it reports one and fails.
  if (problems.length > 0) {
    console.error(problems.join("\n"));
    process.exitCode = 1;
  }
  console.log(written === 0 ? `the packaging files already carry version ${version}` : `wrote version ${version} into ${written} file(s)`);
}
