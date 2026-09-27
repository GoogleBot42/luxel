// Generate the shipped SPRITE and SCENE libraries (Gitea #785) — the peers of
// `gen-gallery.mjs`, run by the same npm scripts:
//
//   public/sprites.json  ← library/sprites/*.sprite
//   public/scenes.json   ← library/scenes/*.scene
//
// A fresh playground used to have zero sprites and zero scenes, and the only
// way to get one was to draw it. These two files are what the `Library` source
// on the Sprites and Scenes pages reads, and they are byte-for-byte the same
// records a device stores: `sprites.json` carries the `LXSP` record base64'd
// (`crates/luxel-core/src/sprite.rs`, docs/spec/scenes.md §4) and
// `scenes.json` carries the scene wire block (§1).
//
// SIZE IS THE CONSTRAINT, because these ship inside the device's asset bundle
// against a 983,040 B partition (docs/boards.md "the assets margin"). So the
// wire is CANONICALISED through `serializeScene` — only the lines that differ
// from the defaults survive — and nothing per-sprite is stored that the record
// itself already says. The whole addition is a few kilobytes; the numbers are
// printed on every run so a growing library is visible in the build log.
//
// THE PARSERS ARE NOT HERE. They are `web/src/lib/librarySource.ts`, typed and
// unit-tested (`web/tests/librarySource.test.mjs`), so this script is I/O plus
// the two cross-file checks a single file cannot make: that every `@pat/<slug>`
// names a pattern `library/` actually has, and that no two references in one
// scene collapse onto the same derived id.
//
// Usage (from web/):
//   node --experimental-strip-types tools/gen-sprite-scene-gallery.mjs
//
// It imports TypeScript, which is why the flag is there — the same one
// `npm test` already runs node with.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { libSlug, parseSceneSource, parseSpriteSource } from "../src/lib/librarySource.ts";
import { parseScene, serializeScene } from "../src/lib/scene.ts";
import { encodeSprite } from "../src/lib/sprite.ts";

const webDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repo = path.dirname(webDir);
const spriteDir = path.join(repo, "library/sprites");
const sceneDir = path.join(repo, "library/scenes");
const outDir = path.join(webDir, "public");

/** Collected refusals — every file is checked, so one bad sprite does not hide
 *  the next one's error. The exit is non-zero at the end. */
const problems = [];
const fail = (file, why) => problems.push(`${path.relative(repo, file)}: ${why}`);

const toBase64 = (bytes) => Buffer.from(bytes).toString("base64");

// ── library/sprites/*.sprite → sprites.json ─────────────────────────────
//
// `name` is the DISPLAY name out of the record and `slug` is the file stem —
// the stable key a scene reference and a clone both key on, because a display
// name is the one thing a library entry is allowed to change.

const spriteFiles = fs.existsSync(spriteDir)
  ? fs.readdirSync(spriteDir).filter((f) => f.endsWith(".sprite")).sort()
  : [];
const sprites = [];
/** slug → record, for the scene pass's reference check. */
const spriteBySlug = new Map();
for (const f of spriteFiles) {
  const file = path.join(spriteDir, f);
  const r = parseSpriteSource(fs.readFileSync(file, "utf8"));
  if (!r.ok) {
    fail(file, r.error);
    continue;
  }
  const record = encodeSprite(r.sprite);
  const slug = libSlug(f);
  spriteBySlug.set(slug, r.sprite);
  sprites.push({
    slug,
    name: r.sprite.name,
    w: r.sprite.w,
    h: r.sprite.h,
    frames: r.sprite.frames,
    fps: r.sprite.fps,
    colors: r.sprite.palette.length,
    bytes: record.length,
    b64: toBase64(record),
  });
}

// ── library/*.js → the names a @pat/<slug> reference resolves to ─────────
//
// A scene reference is resolved at CLONE time out of `gallery.json`, which is
// keyed by display name — so what the shipped scene has to carry is the name,
// and what this build has to prove is that `library/<slug>.js` exists and says
// that name today. A renamed pattern therefore fails the build here rather
// than shipping a scene with an unresolvable layer.
function patternName(slug) {
  const file = path.join(repo, "library", `${slug}.js`);
  if (!fs.existsSync(file)) return null;
  const head = fs.readFileSync(file, "utf8").match(/^\/\/ name:\s*(.+)$/m);
  return (head ? head[1] : slug).trim();
}

// ── library/scenes/*.scene → scenes.json ────────────────────────────────

const sceneFiles = fs.existsSync(sceneDir)
  ? fs.readdirSync(sceneDir).filter((f) => f.endsWith(".scene")).sort()
  : [];
const scenes = [];
for (const f of sceneFiles) {
  const file = path.join(sceneDir, f);
  const src = parseSceneSource(fs.readFileSync(file, "utf8"));
  if (!src.ok) {
    fail(file, src.error);
    continue;
  }
  // The real parser, on the real wire — the shipped record has to be one a
  // device would accept, and this is the same code path `/api/scenes` runs.
  const parsed = parseScene(src.wire);
  if (!parsed.ok) {
    fail(file, parsed.error);
    continue;
  }
  if (parsed.scene.name === "") {
    fail(file, "the S line carries no name");
    continue;
  }
  const refs = [];
  let broken = false;
  for (const ref of src.refs) {
    if (ref.kind === "spr") {
      const sp = spriteBySlug.get(ref.slug);
      if (!sp) {
        fail(file, `@spr/${ref.slug} — no library/sprites/${ref.slug}.sprite`);
        broken = true;
        continue;
      }
      refs.push({ id: ref.id, kind: "spr", slug: ref.slug, name: sp.name });
    } else {
      const name = patternName(ref.slug);
      if (name === null) {
        fail(file, `@pat/${ref.slug} — no library/${ref.slug}.js`);
        broken = true;
        continue;
      }
      refs.push({ id: ref.id, kind: "pat", slug: ref.slug, name });
    }
  }
  if (broken) continue;
  // Every `I` line in the shipped wire must be one of these, or a clone would
  // leave a layer pointing at an id no store will ever hold.
  const known = new Set(refs.map((r) => r.id));
  for (const l of parsed.scene.layers) {
    const id = l.body.kind === "pat" ? l.body.pat.id : l.body.kind === "sprite" ? l.body.id : "";
    if (id !== "" && !known.has(id)) {
      fail(file, `layer “${l.name}” binds ${id}, which is not one of this scene's references`);
      broken = true;
    }
  }
  if (broken) continue;
  scenes.push({
    slug: libSlug(f),
    name: parsed.scene.name,
    // Scenes need a regular 2D matrix to exist at all (docs/spec/scenes.md
    // §2: a draw op is a silent no-op without one), so the field is a
    // constant — it is here so a tile filter reads a scene's shape the same
    // way it reads a pattern's `kind`.
    kind: "grid",
    layers: parsed.scene.layers.length,
    refs,
    // Canonical, not verbatim: `serializeScene` drops every line equal to a
    // default, which is both the smaller bundle and the guarantee that what
    // ships round-trips (§1 "Round trip").
    wire: serializeScene({ ...parsed.scene, id: "" }),
  });
}

// One derived id standing for two different library entries would make the two
// references indistinguishable at clone time. 32 bits over a few dozen entries
// will not collide, but "will not" is not a check.
const byId = new Map();
for (const s of scenes) {
  for (const r of s.refs) {
    const key = `${r.kind}/${r.slug}`;
    const was = byId.get(r.id);
    if (was !== undefined && was !== key) {
      problems.push(`reference id ${r.id} is shared by ${was} and ${key} — rename one`);
    }
    byId.set(r.id, key);
  }
}

if (problems.length > 0) {
  console.error("sprite/scene library: refusing to generate\n");
  for (const p of problems) console.error(`  ${p}`);
  process.exit(1);
}

fs.mkdirSync(outDir, { recursive: true });
const write = (name, value, label) => {
  const file = path.join(outDir, name);
  fs.writeFileSync(file, JSON.stringify(value));
  const kb = (fs.statSync(file).size / 1024).toFixed(1);
  console.log(`${label}: ${value.length} → public/${name} (${kb} KB)`);
};
write("sprites.json", sprites, "sprite library");
write("scenes.json", scenes, "scene library");
