// Generate firmware/manifest.json for the installer page (web/flash.html).
//
// Scans a directory of release artifacts for per-board OTA images and the
// LUXA web-asset bundle, named as the release workflow names them:
//   luxel-<board>-<version>-ota.bin
//   luxel-web-assets-<version>.luxa
//
// Usage: node tools/gen-flash-manifest.mjs <version> <artifact-dir> [outfile]
// (used by .github/workflows/release.yml to compose the Pages bundle, and
// by tools/flash-e2e.mjs to build a local fixture)

import fs from "node:fs";
import path from "node:path";

const [version, dir, out = path.join(dir, "manifest.json")] = process.argv.slice(2);
if (!version || !dir) {
  console.error("usage: gen-flash-manifest.mjs <version> <artifact-dir> [outfile]");
  process.exit(2);
}

const boards = [];
let luxa = null;
for (const name of fs.readdirSync(dir).sort()) {
  const size = fs.statSync(path.join(dir, name)).size;
  const ota = name.match(new RegExp(`^luxel-(.+)-${version.replaceAll(".", "\\.")}-ota\\.bin$`));
  if (ota) boards.push({ id: ota[1], file: name, size });
  else if (name === `luxel-web-assets-${version}.luxa`) luxa = { file: name, size };
}
if (boards.length === 0 || !luxa) {
  console.error(`no v${version} OTA images or LUXA bundle found in ${dir}`);
  process.exit(1);
}

fs.writeFileSync(out, JSON.stringify({ version, boards, luxa }, null, 2) + "\n");
console.log(`${out}: v${version}, ${boards.length} boards, luxa ${luxa.size} B`);
