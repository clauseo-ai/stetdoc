#!/usr/bin/env node
// Runs the stetdoc command of this platform. npm installs exactly one @stetdoc/cli-<target> package (the others are
// optional dependencies that do not match the platform), and that package holds the binary. This launcher runs it with
// the same arguments, the same standard input and output, and the same exit status, so `npx stetdoc mcp` is the MCP server
// over stdio. It runs no install script and makes no network calls.
"use strict";

const { spawnSync } = require("node:child_process");
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

function binary() {
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
  process.stderr.write(
    `stetdoc has no command for ${process.platform}-${process.arch}. Reinstall without --omit=optional, ` +
      `or build one with \`cargo install --path crates/stetdoc-cli\`. Tried:\n  ${tried.join("\n  ")}\n`,
  );
  process.exit(1);
}

const result = spawnSync(binary(), process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  process.stderr.write(`stetdoc could not start: ${result.error.message}\n`);
  process.exit(1);
}
if (result.signal) process.kill(process.pid, result.signal);
process.exit(result.status ?? 1);
