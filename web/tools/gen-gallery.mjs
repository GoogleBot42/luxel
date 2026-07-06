// Generate public/gallery.json for the pattern browser: every corpus
// pattern that compiles clean on the current engine (per
// tools/corpus/last-report.json) with no missing builtins and no sensor
// bindings, as [{ name, kind, source }]. kind picks the thumbnail shape:
// "grid" for render2D patterns (previewed as a mapped rectangle), "strip"
// for 1D (previewed as a horizontal bar) — Jeremy's 1D-bar-vs-2D-rectangle
// distinction. render3D-only patterns are skipped until the preview grows
// a projection.
//
// Runs as part of `npm run build` when the corpus is present; silently
// keeps the existing (or empty) gallery.json otherwise, so builds without
// the corpus checkout still work.
//
// Usage (from web/): node tools/gen-gallery.mjs

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const webDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repo = path.dirname(webDir);
const reportPath = path.join(repo, "tools/corpus/last-report.json");
const corpusDir = path.join(repo, "corpus");
const outPath = path.join(webDir, "public/gallery.json");

if (!fs.existsSync(reportPath) || !fs.existsSync(corpusDir)) {
  if (!fs.existsSync(outPath)) fs.writeFileSync(outPath, "[]");
  console.log("gallery: corpus/report not present; leaving gallery.json as-is");
  process.exit(0);
}

const report = JSON.parse(fs.readFileSync(reportPath, "utf8"));
const seen = new Set();
const entries = [];
for (const p of report) {
  if (p.stage !== "ok") continue;
  if (p.uses.todo.length > 0 || p.uses.sensors.length > 0) continue;
  if (p.uses.render3D && !p.uses.render2D) continue;
  const name = (p.name ?? "").trim() || path.basename(p.file, ".epe");
  const key = name.toLowerCase();
  if (seen.has(key)) continue;
  seen.add(key);
  let source;
  try {
    source = JSON.parse(fs.readFileSync(path.join(repo, p.file), "utf8")).sources.main;
  } catch {
    continue;
  }
  entries.push({ name, kind: p.uses.render2D ? "grid" : "strip", source });
}
entries.sort((a, b) => a.name.localeCompare(b.name));
fs.writeFileSync(outPath, JSON.stringify(entries));
const kb = (fs.statSync(outPath).size / 1024).toFixed(0);
console.log(`gallery: ${entries.length} patterns → public/gallery.json (${kb} KB)`);
