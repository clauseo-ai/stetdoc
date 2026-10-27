// Finds the stetdoc binary of this platform. npm installs exactly one @stetdoc/cli-<target> package (the others are
// optional dependencies that do not match the platform), and that package holds the binary. The launcher
// (bin/stetdoc) and the install step (lib/install.js) both use this. It makes no network calls.
"use strict";

const { dirname, join } = require("node:path");

/** The target packages to try for this machine, in order. Linux tries glibc first when the runtime has it. */
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

/** The path of the binary, or an Error that lists the packages tried. */
function binaryPath() {
  const exe = process.platform === "win32" ? "stetdoc.exe" : "stetdoc";
  const tried = [];
  for (const target of targets()) {
    const name = `@stetdoc/cli-${target}`;
    try {
      const manifest = require.resolve(`${name}/package.json`);
      return join(dirname(manifest), "bin", exe);
    } catch (error) {
      tried.push(`${name}: ${error && error.code ? error.code : error}`);
    }
  }
  return new Error(
    `stetdoc has no command for ${process.platform}-${process.arch}. Reinstall without --omit=optional, ` +
      `or build one with \`cargo install --path crates/stetdoc-cli\`. Tried:\n  ${tried.join("\n  ")}`,
  );
}

module.exports = { binaryPath };
