// Per-chunk / per-module size report for the built console, with the number
// that actually matters: the GZIPPED bytes each chunk contributes to the
// `.luxa` asset archive (the device's 983,040 B assets partition is the gate
// — Gitea #683/#691, tools/ci.sh).
//
// It re-runs the real `vite build` through Vite's JS API with one extra
// plugin that records `chunk.modules`, so the numbers come from the shipped
// config (terser, inlineBoot, cssCodeSplit) and not from a parallel build.
// No new npm dependency: vite is already a devDependency.
//
// Usage (from web/, inside `nix develop`, after `npm run wasm` +
// `node tools/gen-gallery.mjs`):
//     node tools/bundle-report.mjs            # table to stdout
//     node tools/bundle-report.mjs out.json   # and the raw numbers as JSON
//
// Reading it: `gz` is what the chunk costs the partition. `modules` lists the
// biggest source modules inside the chunk by their *rendered* (post-treeshake,
// pre-minify) size, which is the right signal for "what should move behind a
// dynamic import()".
import { build } from "vite";
import { gzipSync } from "node:zlib";
import fs from "node:fs";

const OUT_JSON = process.argv[2] ?? null;
const report = { chunks: [], assets: [] };

/** Records chunk→module sizes; runs before `luxel-inline-boot` folds CSS in. */
const collect = {
  name: "luxel-bundle-report",
  enforce: "post",
  apply: "build",
  generateBundle(_o, bundle) {
    for (const c of Object.values(bundle)) {
      if (c.type === "chunk") {
        report.chunks.push({
          file: c.fileName,
          entry: c.isEntry,
          dynamicEntry: c.isDynamicEntry,
          raw: c.code.length,
          gz: gzipSync(Buffer.from(c.code), { level: 9 }).length,
          modules: Object.entries(c.modules)
            .map(([id, m]) => ({ id: id.replace(process.cwd() + "/", ""), raw: m.renderedLength }))
            .filter((m) => m.raw > 0)
            .sort((a, b) => b.raw - a.raw),
        });
      } else {
        const src = Buffer.from(c.source);
        report.assets.push({
          file: c.fileName,
          raw: src.length,
          gz: gzipSync(src, { level: 9 }).length,
        });
      }
    }
  },
};

await build({ configFile: "vite.config.ts", logLevel: "warn", plugins: [collect] });

const num = (n) => n.toLocaleString("en-US");
const rows = [...report.chunks, ...report.assets].sort((a, b) => b.gz - a.gz);
console.log("chunk / asset                        raw        gz");
console.log("-".repeat(58));
let tRaw = 0,
  tGz = 0;
for (const r of rows) {
  tRaw += r.raw;
  tGz += r.gz;
  const tag = r.dynamicEntry ? " (lazy)" : r.entry ? " (entry)" : "";
  console.log(`${(r.file + tag).padEnd(34)}${num(r.raw).padStart(10)}${num(r.gz).padStart(10)}`);
}
console.log("-".repeat(58));
console.log(`${"TOTAL".padEnd(34)}${num(tRaw).padStart(10)}${num(tGz).padStart(10)}`);

for (const c of report.chunks.sort((a, b) => b.gz - a.gz)) {
  console.log(`\n== ${c.file} — top modules (rendered, pre-minify)`);
  for (const m of c.modules.slice(0, 18)) {
    console.log(`   ${num(m.raw).padStart(8)}  ${m.id}`);
  }
  const rest = c.modules.slice(18).reduce((s, m) => s + m.raw, 0);
  if (rest) console.log(`   ${num(rest).padStart(8)}  (${c.modules.length - 18} more)`);
}

if (OUT_JSON) {
  fs.writeFileSync(OUT_JSON, JSON.stringify(report, null, 2));
  console.log(`\nwrote ${OUT_JSON}`);
}
