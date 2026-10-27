// The optional install step, as esbuild does it: it replaces the launcher bin/stetdoc with the native binary of this
// platform, so each `stetdoc` call skips starting Node. It only copies a file that npm already installed from the
// @clauseo/stetdoc-cli-<target> package: it downloads nothing, writes nothing outside this package, and never fails the install.
// It leaves the launcher in place, which works the same, more slowly, when:
//   - npm runs without install scripts (--ignore-scripts, or a package manager that blocks them);
//   - the platform is Windows, where npm's .cmd shim starts bin/stetdoc with Node, so it must stay JavaScript;
//   - the binary is missing, or does not print its version when run;
//   - the package folder is read-only, or anything else goes wrong.
// Set STETDOC_KEEP_LAUNCHER=1 to keep the launcher.
"use strict";

const { execFileSync } = require("node:child_process");
const fs = require("node:fs");
const { join } = require("node:path");
const { binaryPath } = require("./platform.js");

function main() {
  if (process.platform === "win32" || process.env.STETDOC_KEEP_LAUNCHER) return "kept the launcher";
  const binary = binaryPath();
  if (binary instanceof Error) return "kept the launcher: no binary for this platform";
  const { version } = JSON.parse(fs.readFileSync(join(__dirname, "..", "package.json"), "utf8"));
  const printed = execFileSync(binary, ["--version"], { encoding: "utf8", timeout: 10000 }).trim();
  if (printed !== `stetdoc ${version}`) return `kept the launcher: the binary printed ${JSON.stringify(printed)}`;
  const launcher = join(__dirname, "..", "bin", "stetdoc");
  const temporary = `${launcher}.${process.pid}.tmp`;
  try {
    fs.copyFileSync(binary, temporary);
    fs.chmodSync(temporary, 0o755);
    fs.renameSync(temporary, launcher);
  } finally {
    fs.rmSync(temporary, { force: true });
  }
  return "bin/stetdoc is the native binary";
}

try {
  main();
} catch {
  // The launcher still works, so the install goes on.
}
