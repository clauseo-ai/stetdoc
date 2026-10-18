// Builds the addon with cargo and copies it next to index.js as stetdoc.<target>.node, which the loader tries first.
// Usage: node scripts/build.mjs [--release]
import { execFileSync } from "node:child_process";
import { copyFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const release = process.argv.includes("--release");
const args = ["build", "--locked", "-p", "stetdoc-napi", ...(release ? ["--release"] : [])];
execFileSync("cargo", args, { stdio: "inherit", cwd: here });

const metadata = JSON.parse(
  execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], { cwd: here, encoding: "utf8" }),
);
const library = { darwin: "libstetdoc_napi.dylib", win32: "stetdoc_napi.dll" }[process.platform] ?? "libstetdoc_napi.so";
const built = join(metadata.target_directory, release ? "release" : "debug", library);

function target() {
  const { platform, arch } = process;
  if (platform === "linux") {
    const glibc = process.report?.getReport?.()?.header?.glibcVersionRuntime;
    return `linux-${arch}-${glibc ? "gnu" : "musl"}`;
  }
  return platform === "win32" ? `win32-${arch}-msvc` : `${platform}-${arch}`;
}

const out = join(here, "..", `stetdoc.${target()}.node`);
copyFileSync(built, out);
console.log(`copied ${built} to ${out}`);
