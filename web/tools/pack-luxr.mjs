// Pack an app image + a LUXA asset archive into one `.luxr` release package
// (Gitea #643 — see src/lib/luxr.ts for the container and why it exists).
//
// Usage (from web/):
//   node --experimental-strip-types tools/pack-luxr.mjs \
//        --board "<board::NAME>" --version <X.Y.Z> \
//        --app <ota.bin> [--assets <dist.luxa>] <out.luxr>
//
// `--board` is the board's `board::NAME` string — `firmware/board-target.sh`'s
// `board_name` prints exactly it, and it is what `/api/status.board` reports,
// so the console can refuse a package built for another board. Omit
// `--assets` for a hosted-UI board, which serves no on-device console.
//
// The codec is src/lib/luxr.ts, shared with the browser and the unit test
// (tests/luxr.test.mjs) — hence the --experimental-strip-types flag; node 22
// strips the types but will not parse them without it.

import fs from "node:fs";
import { packLuxr } from "../src/lib/luxr.ts";

const args = process.argv.slice(2);
let board = "";
let version = "";
let app = "";
let assets = "";
let out = "";
for (let i = 0; i < args.length; i++) {
  const a = args[i];
  if (a === "--board") board = args[++i] ?? "";
  else if (a === "--version") version = args[++i] ?? "";
  else if (a === "--app") app = args[++i] ?? "";
  else if (a === "--assets") assets = args[++i] ?? "";
  else out = a;
}
if (!board || !version || !app || !out) {
  console.error(
    "usage: pack-luxr.mjs --board <name> --version <X.Y.Z> --app <ota.bin> [--assets <dist.luxa>] <out.luxr>",
  );
  process.exit(2);
}

const buf = await packLuxr({
  board,
  version,
  app: new Uint8Array(fs.readFileSync(app)),
  assets: assets ? new Uint8Array(fs.readFileSync(assets)) : new Uint8Array(0),
});
fs.writeFileSync(out, buf);
console.log(
  `${out}: ${buf.length} bytes — v${version} for "${board}" ` +
    `(app ${fs.statSync(app).size} B, assets ${assets ? fs.statSync(assets).size : 0} B)`,
);
