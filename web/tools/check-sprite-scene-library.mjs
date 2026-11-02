// The gate on the shipped sprite and scene library (Gitea #785) —
// `tools/check-library.sh`'s peer for `library/sprites/` and `library/scenes/`,
// and a step in `tools/ci.sh`.
//
// What it proves, and WHERE it proves it:
//
//   * every sprite source parses, and the record it encodes to is accepted by
//     `luxel_core::sprite::check` — the RUST codec, reached through the wasm
//     `lx_comp_sprite` ABI, not the TypeScript one that wrote the bytes. Both
//     sides of #740's "one byte layout" have to agree about the shipped art or
//     a tile draws in the console and nothing draws on the device;
//   * every record is within the 16 KiB cap the device's request buffer
//     imposes (`SPRITE_MAX_BYTES`), so a shipped sprite can always be POSTed;
//   * every scene source parses as a real scene record through
//     `luxel_core::scene::parse` (`lx_comp_set`, the same code path
//     `/api/scenes` runs);
//   * every REFERENCE resolves: `@pat/<slug>` names a `library/<slug>.js` that
//     exists and still carries the display name the shipped scene will look it
//     up by, and that pattern COMPILES; `@spr/<slug>` names a sprite source in
//     this same sweep. A reference that stops resolving is the one way this
//     feature breaks silently — the tile just draws fewer layers — so it is
//     checked here rather than discovered on a panel;
//   * every `I` line in a shipped scene is one of that scene's own references,
//     and no two references collapse onto the same derived id;
//   * the canonical wire that ships (the console's `serializeScene`) keeps
//     every keyframe (`M` line) and is still a record `luxel_core` takes;
//   * the scene's `kind` (`scenes.json`) is well declared — at most one
//     `# layout:` comment, `strip` or `grid` — and a STRIP scene is written
//     in segments: every box has `y` 0 and `h` 0 or 1, and no keyframe moves
//     `y` off 0 (a box below row 0 draws nothing on a strip), and it has no
//     text layer (text draws nothing on one row). Library/README.md "Strips".
//
// Usage (from web/):
//   node --experimental-strip-types tools/check-sprite-scene-library.mjs
//
// It needs `public/luxel.wasm` (`npm run wasm`), because the Rust half of every
// check above lives in there.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { libRefId, parseSceneSource, parseSpriteSource, libSlug } from "../src/lib/librarySource.ts";
import { parseScene, serializeScene } from "../src/lib/scene.ts";
import { checkSprite, encodeSprite, SPRITE_MAX_BYTES } from "../src/lib/sprite.ts";

const webDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const repo = path.dirname(webDir);
const WASM = path.join(webDir, "public/luxel.wasm");

if (!fs.existsSync(WASM)) {
  console.error(`check-sprite-scene-library: no ${path.relative(repo, WASM)} — run \`npm run wasm\` first`);
  process.exit(1);
}

const { instance } = await WebAssembly.instantiate(fs.readFileSync(WASM), {});
const e = instance.exports;
const dec = new TextDecoder();
const response = () => dec.decode(new Uint8Array(e.memory.buffer, e.lx_response_ptr(), e.lx_response_len()));

function put(bytes) {
  const ptr = e.lx_alloc(bytes.length);
  new Uint8Array(e.memory.buffer).set(bytes, ptr);
  return { ptr, len: bytes.length, free: () => e.lx_dealloc(ptr, bytes.length) };
}
const putText = (s) => put(new TextEncoder().encode(s));

/** `luxel_core::scene::parse` over a wire block: null, or its error. */
function rustParseScene(ch, wire) {
  const s = putText(wire);
  const rc = e.lx_comp_set(ch, s.ptr, s.len);
  s.free();
  return rc === 0 ? null : response();
}

/** `luxel_core::sprite::check` over a record: null, or its `sprite: …` reason. */
function rustCheckSprite(ch, record) {
  const s = put(record);
  const rc = e.lx_comp_sprite(ch, 0, s.ptr, s.len);
  s.free();
  return rc === 0 ? null : response();
}

/** `luxel_core`'s compiler over a pattern source: null, or its error. */
function rustCompile(source) {
  const s = putText(source);
  const h = e.lx_new(s.ptr, s.len, 256, 1);
  s.free();
  if (h < 0) return response();
  e.lx_free(h);
  return null;
}

const problems = [];
const fail = (what, why) => problems.push(`${what}: ${why}`);

// A one-sprite-layer compositor is what `lx_comp_sprite` needs a slot on; a
// second handle carries the scene parses so the two never share a record.
const spriteCh = e.lx_comp_new(16, 16);
if (rustParseScene(spriteCh, "S - probe\nL sprite 0 0 0 0 normal 100 none fill 1\nI 00000000\n") !== null) {
  console.error("check-sprite-scene-library: the wasm build would not take a sprite-layer scene");
  process.exit(1);
}
const sceneCh = e.lx_comp_new(64, 64);

// ---- library/sprites/ ----

const spriteDir = path.join(repo, "library/sprites");
const spriteFiles = fs.existsSync(spriteDir)
  ? fs.readdirSync(spriteDir).filter((f) => f.endsWith(".sprite")).sort()
  : [];
/** slug → display name, for the scene pass. */
const spriteNames = new Map();
let spriteBytesTotal = 0;
for (const f of spriteFiles) {
  const what = `library/sprites/${f}`;
  const r = parseSpriteSource(fs.readFileSync(path.join(spriteDir, f), "utf8"));
  if (!r.ok) {
    fail(what, r.error);
    continue;
  }
  const record = encodeSprite(r.sprite);
  spriteBytesTotal += record.length;
  if (record.length > SPRITE_MAX_BYTES) {
    fail(what, `${record.length} B is over the ${SPRITE_MAX_BYTES / 1024} KiB cap`);
  }
  const ts = checkSprite(record);
  if (ts !== null) fail(what, `the console's codec refuses its own bytes: ${ts}`);
  const rust = rustCheckSprite(spriteCh, record);
  if (rust !== null) fail(what, `luxel_core::sprite refuses it: ${rust}`);
  spriteNames.set(libSlug(f), r.sprite.name);
  console.log(
    `  sprite ${libSlug(f).padEnd(14)} ${String(r.sprite.w)}×${r.sprite.h}` +
      ` · ${r.sprite.frames} frame${r.sprite.frames === 1 ? "" : "s"}` +
      ` · ${r.sprite.palette.length} colour${r.sprite.palette.length === 1 ? "" : "s"} · ${record.length} B`,
  );
}

// ---- library/*.js, as a @pat/<slug> reference target ----

const patternCache = new Map();
function patternEntry(slug) {
  if (patternCache.has(slug)) return patternCache.get(slug);
  const file = path.join(repo, "library", `${slug}.js`);
  let entry = null;
  if (fs.existsSync(file)) {
    const source = fs.readFileSync(file, "utf8");
    const m = source.match(/^\/\/ name:\s*(.+)$/m);
    entry = { name: (m ? m[1] : slug).trim(), source };
  }
  patternCache.set(slug, entry);
  return entry;
}

// ---- library/scenes/ ----

/** A scene's `kind` (library/README.md "Strips"): a `# layout: strip`
 *  comment line in the SOURCE makes it `"strip"`, a `# layout: grid` line or
 *  none at all `"grid"`. Any other `# layout:` word is an error (null), and so
 *  is a second `# layout:` line. A comment because the wire has no room for
 *  it, and declared rather than inferred because a scene of whole-layout boxes
 *  (`0 0 0 0`) is valid on both shapes — only its author knows which one it
 *  was drawn for. The generator's copy (gen-sprite-scene-gallery.mjs) is the
 *  one that writes `kind`; keep the two identical. */
function sceneKind(text) {
  const words = [...text.matchAll(/^\s*#\s*layout:\s*(\S*)\s*$/gm)].map((m) => m[1]);
  if (words.length === 0) return "grid";
  if (words.length > 1 || (words[0] !== "strip" && words[0] !== "grid")) return null;
  return words[0];
}

/** Why a strip scene's layer is not a segment, or null. */
function notASegment(l) {
  const r = l.style.rect;
  if (r.y !== 0 || r.h > 1) return `box ${r.x} ${r.y} ${r.w} ${r.h} is not a segment (y 0, h 0 or 1)`;
  if (l.body.kind === "text") return "a text layer draws nothing on a strip";
  if ((l.keys ?? []).some((k) => k.y !== null && k.y !== 0)) return "a keyframe moves y off 0";
  return null;
}
const kinds = { grid: 0, strip: 0 };

const sceneDir = path.join(repo, "library/scenes");
const sceneFiles = fs.existsSync(sceneDir)
  ? fs.readdirSync(sceneDir).filter((f) => f.endsWith(".scene")).sort()
  : [];
/** derived id → `<kind>/<slug>`, to catch a hash collision across the set. */
const seenIds = new Map();
let refsChecked = 0;
for (const f of sceneFiles) {
  const what = `library/scenes/${f}`;
  const text = fs.readFileSync(path.join(sceneDir, f), "utf8");
  const kind = sceneKind(text);
  if (kind === null) fail(what, "`# layout:` must appear at most once, as `strip` or `grid`");
  else kinds[kind]++;
  const src = parseSceneSource(text);
  if (!src.ok) {
    fail(what, src.error);
    continue;
  }
  const rust = rustParseScene(sceneCh, src.wire);
  if (rust !== null) {
    fail(what, `luxel_core::scene refuses it: ${rust}`);
    continue;
  }
  const parsed = parseScene(src.wire);
  if (!parsed.ok) {
    fail(what, `the console's parser refuses what luxel_core took: ${parsed.error}`);
    continue;
  }
  if (parsed.scene.name === "") fail(what, "the S line carries no name");
  if (kind === "strip") {
    parsed.scene.layers.forEach((l, i) => {
      const why = notASegment(l);
      if (why !== null) fail(what, `layer ${i + 1} “${l.name}”: ${why}`);
    });
  }
  // What SHIPS is the console's canonical serialization
  // (gen-sprite-scene-gallery.mjs), not the source: it has to keep every
  // keyframe (`M` line) and still be a record luxel_core takes.
  const canon = serializeScene({ ...parsed.scene, id: "" });
  const mLines = (w) => w.split("\n").filter((l) => /^\s*M\s/.test(l)).length;
  if (mLines(canon) !== mLines(src.wire)) {
    fail(what, `the console's serializer kept ${mLines(canon)} of ${mLines(src.wire)} keyframes`);
  }
  const rustCanon = rustParseScene(sceneCh, canon);
  if (rustCanon !== null) fail(what, `luxel_core::scene refuses the canonical wire: ${rustCanon}`);

  for (const ref of src.refs) {
    refsChecked++;
    const key = `${ref.kind}/${ref.slug}`;
    if (libRefId(ref.kind, ref.slug) !== ref.id) fail(what, `${key}: derived id disagrees with itself`);
    const was = seenIds.get(ref.id);
    if (was !== undefined && was !== key) fail(what, `id ${ref.id} is shared by ${was} and ${key}`);
    seenIds.set(ref.id, key);
    if (ref.kind === "spr") {
      if (!spriteNames.has(ref.slug)) fail(what, `@spr/${ref.slug} — no library/sprites/${ref.slug}.sprite`);
      continue;
    }
    const entry = patternEntry(ref.slug);
    if (entry === null) {
      fail(what, `@pat/${ref.slug} — no library/${ref.slug}.js`);
      continue;
    }
    // The clone resolves a `pat` reference out of `gallery.json`, which is keyed
    // by DISPLAY NAME — so the name has to be there to be found, and the source
    // has to compile or the layer draws nothing.
    if (entry.name === "") fail(what, `@pat/${ref.slug} — the pattern has no \`// name:\` header`);
    const err = rustCompile(entry.source);
    if (err !== null) fail(what, `@pat/${ref.slug} does not compile: ${err}`);
  }

  const known = new Set(src.refs.map((r) => r.id));
  for (const l of parsed.scene.layers) {
    const id = l.body.kind === "pat" ? l.body.pat.id : l.body.kind === "sprite" ? l.body.id : "";
    if (id !== "" && !known.has(id)) {
      fail(what, `layer “${l.name}” binds ${id}, which is not one of this scene's references`);
    }
  }
  console.log(
    `  scene  ${libSlug(f).padEnd(20)} ${kind ?? "?"} · ${parsed.scene.layers.length} layers · ` +
      `${src.refs.length} ref${src.refs.length === 1 ? "" : "s"} · ${src.wire.length} B` +
      (mLines(src.wire) > 0 ? ` · ${mLines(src.wire)} keyframes` : ""),
  );
}

console.log(
  `\nsprite/scene library: ${spriteFiles.length} sprites (${spriteBytesTotal} B of records), ` +
    `${sceneFiles.length} scenes (${kinds.grid} grid, ${kinds.strip} strip), ${refsChecked} references`,
);

if (problems.length > 0) {
  console.error("\ncheck-sprite-scene-library FAILED\n");
  for (const p of problems) console.error(`  ${p}`);
  process.exit(1);
}
console.log("check-sprite-scene-library OK");
