// Build the port↔original pairing manifest the render harness works from.
//
// Each clean-room port in library/*.js carries a provenance line naming the
// community pattern it was described from; that name is matched against the
// `name` field of the scraped corpus/*.epe envelopes. Slug matching and a
// short hand-fixup table cover the files whose names drifted.
//
// The manifest holds only names, paths and a rig hint — no pattern source —
// because the fidelity judges read it and must never see code.
//
// Usage: node tools/verify/gen-pairs.mjs
//        → writes tools/verify/pairs.json, prints coverage to stdout

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "../..");
const LIB = path.join(ROOT, "library");
const CORPUS = path.join(ROOT, "corpus");
const OUT = path.join(HERE, "pairs.json");

/** Library basenames whose provenance name no longer matches any .epe name. */
const FIXUPS = {
  "Voronoi Mix 2D": "voronoi-2d",
  "Perlin Kaleidoscope 2D": "kaleidoscope-2d",
  "Doom Fire (v2.0) 2D": "doom-fire-2d",
  "Coronal Mass Ejection 2D sliders": "coronal-ejection-2d",
  "Unstable Orbits": "unstable-orbits-2d",
};

const slugify = (s) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");

// ---- load both sides -------------------------------------------------------

const libFiles = fs.readdirSync(LIB).filter((f) => f.endsWith(".js")).sort();
const epeFiles = fs.readdirSync(CORPUS).filter((f) => f.endsWith(".epe")).sort();

/** slug → {file, source, provenance} */
const lib = new Map();
for (const f of libFiles) {
  const source = fs.readFileSync(path.join(LIB, f), "utf8");
  const m = source.match(/community pattern "([^"]+)"/);
  lib.set(path.basename(f, ".js"), {
    file: `library/${f}`,
    source,
    provenance: m ? m[1] : null,
  });
}

/** epeFile → {name, id, source}; plus name→[epeFile] and slug→[epeFile] */
const epe = new Map();
const byName = new Map();
const bySlug = new Map();
for (const f of epeFiles) {
  const j = JSON.parse(fs.readFileSync(path.join(CORPUS, f), "utf8"));
  const rec = { file: `corpus/${f}`, name: j.name ?? "", id: j.id ?? null, source: j.sources?.main ?? "" };
  epe.set(f, rec);
  if (!byName.has(rec.name)) byName.set(rec.name, []);
  byName.get(rec.name).push(f);
  const s = slugify(rec.name);
  if (!bySlug.has(s)) bySlug.set(s, []);
  bySlug.get(s).push(f);
}

// ---- pair ------------------------------------------------------------------

const rigOf = (a, b) => {
  if (/render2D/.test(a) || /render2D/.test(b)) return "grid";
  if (/render3D/.test(a) || /render3D/.test(b)) return "cloud";
  return "strip";
};

const pairs = [];
const unpairedLibrary = [];
const usedEpe = new Set();

for (const [slug, L] of lib) {
  let candidates = null;

  // 1. provenance name → exact .epe name
  if (L.provenance && byName.has(L.provenance)) candidates = byName.get(L.provenance);

  // 2. hand fixups for drifted names
  if (!candidates) {
    for (const [name, target] of Object.entries(FIXUPS)) {
      if (target === slug && byName.has(name)) {
        candidates = byName.get(name);
        break;
      }
    }
  }

  // 3. slug of .epe name == library basename
  if (!candidates && bySlug.has(slug)) candidates = bySlug.get(slug);

  // 4. provenance name slugged == some .epe name slugged
  if (!candidates && L.provenance && bySlug.has(slugify(L.provenance))) {
    candidates = bySlug.get(slugify(L.provenance));
  }

  if (!candidates || candidates.length === 0) {
    unpairedLibrary.push(slug);
    continue;
  }

  const primary = epe.get(candidates[0]);
  for (const f of candidates) usedEpe.add(f);

  const entry = {
    slug,
    libFile: L.file,
    epeFile: primary.file,
    epeName: primary.name,
    rig: rigOf(L.source, primary.source),
  };
  if (candidates.length > 1) {
    entry.ambiguous = true;
    entry.epeIds = candidates.map((f) => epe.get(f).id);
    entry.epeFiles = candidates.map((f) => epe.get(f).file);
  }
  pairs.push(entry);
}

const unpairedCorpus = epeFiles
  .filter((f) => !usedEpe.has(f))
  .map((f) => ({ epeFile: epe.get(f).file, epeName: epe.get(f).name }));

pairs.sort((a, b) => a.slug.localeCompare(b.slug));

fs.writeFileSync(
  OUT,
  JSON.stringify({ pairs, unpairedLibrary, unpairedCorpus }, null, 2) + "\n",
);

const rigCount = pairs.reduce((m, p) => ((m[p.rig] = (m[p.rig] ?? 0) + 1), m), {});
console.log(`library files:      ${libFiles.length}`);
console.log(`corpus patterns:    ${epeFiles.length}`);
console.log(`paired:             ${pairs.length}  (ambiguous: ${pairs.filter((p) => p.ambiguous).length})`);
console.log(`corpus covered:     ${usedEpe.size}/${epeFiles.length}`);
console.log(`unpaired library:   ${unpairedLibrary.length}`);
console.log(`unpaired corpus:    ${unpairedCorpus.length}`);
console.log(`rigs:               ${JSON.stringify(rigCount)}`);
console.log(`wrote ${path.relative(ROOT, OUT)}`);
