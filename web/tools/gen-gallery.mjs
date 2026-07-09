// Generate the playground's pattern-browser JSON(s), as [{ name, kind, source }]:
//
//   public/gallery.json            ← library/*.js   (the clean-room library)
//   public/pixelblaze-library.json ← corpus/*.epe   (scraped Pixelblaze exports)
//
// `kind` picks the thumbnail shape: "cloud" for render3D-only patterns (a
// rotating projected point cloud on a cube-lattice map), "grid" for render2D
// (a mapped rectangle), "strip" for 1D (a horizontal bar) — Jeremy's
// 1D-bar-vs-2D-rectangle distinction.
//
// The corpus gallery is a LOCAL-ONLY convenience: the corpus is untracked and
// of unknown licensing (the clean-room policy keeps it out of library/ and
// out of git), so its output is git-ignored and the playground's "PixelBlaze
// Library" tab only appears where corpus/ is present. When corpus/ is absent
// or empty, any stale output is removed so the tab disappears.
//
// Usage (from web/): node tools/gen-gallery.mjs

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const webDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repo = path.dirname(webDir);

const kindOf = (source) => {
  const has2D = /render2D/.test(source);
  const has3D = /render3D/.test(source);
  return has3D && !has2D ? "cloud" : has2D ? "grid" : "strip";
};

// Collect entries, disambiguating duplicate names (append " (2)", " (3)", …)
// so every pattern stays visible — the Gallery component dedups by name, so
// collisions would otherwise silently vanish.
function collector() {
  const counts = new Map();
  const entries = [];
  return {
    add(name, source) {
      const key = name.toLowerCase();
      const seen = counts.get(key) ?? 0;
      counts.set(key, seen + 1);
      entries.push({ name: seen > 0 ? `${name} (${seen + 1})` : name, kind: kindOf(source), source });
    },
    write(outPath, label) {
      entries.sort((a, b) => a.name.localeCompare(b.name));
      fs.writeFileSync(outPath, JSON.stringify(entries));
      const kb = (fs.statSync(outPath).size / 1024).toFixed(0);
      console.log(`${label}: ${entries.length} patterns → public/${path.basename(outPath)} (${kb} KB)`);
    },
  };
}

// ── library/*.js → gallery.json ─────────────────────────────────────────
const libDir = path.join(repo, "library");
const galleryOut = path.join(webDir, "public/gallery.json");
if (fs.existsSync(libDir)) {
  const c = collector();
  for (const f of fs.readdirSync(libDir).sort()) {
    if (!f.endsWith(".js")) continue;
    const source = fs.readFileSync(path.join(libDir, f), "utf8");
    const m = source.match(/^\/\/ name:\s*(.+)$/m);
    c.add((m ? m[1] : path.basename(f, ".js")).trim(), source);
  }
  c.write(galleryOut, "gallery");
} else {
  if (!fs.existsSync(galleryOut)) fs.writeFileSync(galleryOut, "[]");
  console.log("gallery: library/ not present; leaving gallery.json as-is");
}

// ── corpus/*.epe → pixelblaze-library.json (local-only) ─────────────────
const corpusDir = path.join(repo, "corpus");
const corpusOut = path.join(webDir, "public/pixelblaze-library.json");
const epes = fs.existsSync(corpusDir)
  ? fs.readdirSync(corpusDir).filter((f) => f.endsWith(".epe")).sort()
  : [];
if (epes.length === 0) {
  if (fs.existsSync(corpusOut)) fs.rmSync(corpusOut);
  console.log("pixelblaze library: corpus/ absent or empty — no tab");
} else {
  const c = collector();
  for (const f of epes) {
    let epe;
    try {
      epe = JSON.parse(fs.readFileSync(path.join(corpusDir, f), "utf8"));
    } catch {
      continue; // skip unparseable exports
    }
    const source = epe?.sources?.main;
    if (typeof source !== "string" || source.trim() === "") continue;
    const name = (typeof epe.name === "string" && epe.name.trim()) || path.basename(f, ".epe");
    c.add(name, source);
  }
  c.write(corpusOut, "pixelblaze library");
}
