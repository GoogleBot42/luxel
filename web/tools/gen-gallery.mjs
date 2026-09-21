// Generate the playground's pattern-browser JSON(s), as [{ name, kind, source }]:
//
//   public/gallery.json            ← library/*.js   (the clean-room library)
//   public/pixelblaze-library.json ← corpus/*.epe   (scraped Pixelblaze exports)
//
// `kind` is an ADVISORY hint at the pattern's dimensionality — "cloud" for
// render3D-only, "grid" for render2D (or a renderFrame that calls a
// coordinate/grid-space bulk builtin), "strip" for a `render(index)` 1D
// pattern, and "any" for a DIMENSIONLESS one: `renderFrame` alone, painting
// in index space, which names no geometry and is native on every Layout
// (`library/fairies.js`). "any" must not read as "strip" — a strip pattern is
// projectable along an axis and a dimensionless one is not. Since Gitea #463 the
// playground does NOT take a tile's shape from it: the shape is the Layout's
// and the dimensionality comes from the COMPILED pattern
// (`Engine.preferredDims()`), which a regex over source text cannot know (a
// `render2D` inside a comment or a string counts here and must not). All this
// field buys is the pixel count of a tile's FIRST compile, so the common case
// does not pay for a second one — see `compileForLayout` in
// web/src/stores/geometry.ts. Nothing breaks when it is wrong.
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

// Coordinate- and grid-space bulk builtins: a `renderFrame` pattern that
// calls any of these draws in 2D even though it never mentions render2D.
// Same list the engine uses to decide whether a renderFrame-only pattern
// gets the default square grid map (`uses_coordinate_bulk_op`, engine.rs) —
// keep the two in sync. A pattern that declares its OWN function of one of
// these names shadows the builtin and does not count (several 1D patterns
// have a local `splat`/`drawLine` helper).
const BULK_2D_NAMES = [
  "fillRect",
  "fillCircle",
  "splat",
  "drawLine",
  "fillCanvas",
  "blit",
  "gridWidth",
  "gridHeight",
];

const usesBulk2D = (source) =>
  BULK_2D_NAMES.some(
    (n) =>
      new RegExp(`\\b${n}\\s*\\(`).test(source) &&
      !new RegExp(`function\\s+${n}\\s*\\(`).test(source),
  );

// Mirrors `guessPatternDims` in web/src/lib/geometry.ts and, through it, the
// engine's `pattern_dims()` — all three move together.
const kindOf = (source) => {
  if (/render2D/.test(source) || usesBulk2D(source)) return "grid";
  if (/render3D/.test(source)) return "cloud";
  return /\brender\s*\(/.test(source) ? "strip" : "any";
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
