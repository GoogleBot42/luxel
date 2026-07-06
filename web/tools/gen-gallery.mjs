// Generate public/gallery.json for the pattern browser from the
// clean-room pattern library (library/*.js), as [{ name, kind, source }].
// kind picks the thumbnail shape: "grid" for render2D patterns (previewed
// as a mapped rectangle), "strip" for 1D (previewed as a horizontal bar) —
// Jeremy's 1D-bar-vs-2D-rectangle distinction. render3D-only patterns are
// skipped until the preview grows a projection.
//
// The scraped corpus is no longer read here (unknown licensing); it stays
// an untracked, local-only compile-compatibility test battery.
//
// Usage (from web/): node tools/gen-gallery.mjs

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const webDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repo = path.dirname(webDir);
const libDir = path.join(repo, "library");
const outPath = path.join(webDir, "public/gallery.json");

if (!fs.existsSync(libDir)) {
  if (!fs.existsSync(outPath)) fs.writeFileSync(outPath, "[]");
  console.log("gallery: library/ not present; leaving gallery.json as-is");
  process.exit(0);
}

const seen = new Set();
const entries = [];
for (const f of fs.readdirSync(libDir).sort()) {
  if (!f.endsWith(".js")) continue;
  const source = fs.readFileSync(path.join(libDir, f), "utf8");
  const m = source.match(/^\/\/ name:\s*(.+)$/m);
  const name = (m ? m[1] : path.basename(f, ".js")).trim();
  const key = name.toLowerCase();
  if (seen.has(key)) continue;
  seen.add(key);
  const has2D = /render2D/.test(source);
  const has3D = /render3D/.test(source);
  if (has3D && !has2D) continue; // no 3D projection tiles yet
  entries.push({ name, kind: has2D ? "grid" : "strip", source });
}

entries.sort((a, b) => a.name.localeCompare(b.name));
fs.writeFileSync(outPath, JSON.stringify(entries));
const kb = (fs.statSync(outPath).size / 1024).toFixed(0);
console.log(`gallery: ${entries.length} library patterns → public/gallery.json (${kb} KB)`);
