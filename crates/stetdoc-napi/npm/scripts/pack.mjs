// Packs the npm tarballs of a release: one package per target, each holding its prebuilt addon, and the main package.
// It only writes files: nothing is published. Usage: node scripts/pack.mjs <dir of stetdoc.<target>.node files> <out dir>
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const [addons, out] = process.argv.slice(2).map((path) => resolve(path));
if (!addons || !out) {
  console.error("usage: node scripts/pack.mjs <addons dir> <out dir>");
  process.exit(2);
}
mkdirSync(out, { recursive: true });

const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const pack = (dir) => execFileSync(npm, ["pack", "--pack-destination", out], { cwd: dir, stdio: "inherit" });

const targets = readdirSync(join(here, "targets"));
const missing = targets.filter((target) => !existsSync(join(addons, `stetdoc.${target}.node`)));
if (missing.length > 0) {
  console.error(`no addon for ${missing.join(", ")} in ${addons}`);
  process.exit(1);
}
for (const target of targets) {
  const file = `stetdoc.${target}.node`;
  copyFileSync(join(addons, file), join(here, "targets", target, file));
  pack(join(here, "targets", target));
}
pack(here);
