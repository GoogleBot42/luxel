// `src/lib/librarySource.ts` — the two source formats the shipped sprite and
// scene library is authored in (Gitea #785).
//
// The generator and the gate both read `library/sprites/` and `library/scenes/`
// through these parsers, so an error they do not catch is a broken tile in the
// bundle. The last two tests therefore run the real shipped files: `npm test`
// alone fails on a malformed library entry, without waiting for the build.
//
// Run: `npm test` from web/.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  libRefId,
  libSceneId,
  libSlug,
  parseSceneSource,
  parseSpriteSource,
} from "../src/lib/librarySource.ts";
import { parseScene } from "../src/lib/scene.ts";
import { encodeSprite, SPRITE_MAX_BYTES } from "../src/lib/sprite.ts";

const repo = fileURLToPath(new URL("../..", import.meta.url));

const HEART = `# a comment, anywhere
name: Heart
fps: 4
palette:
  r ff3355
  h ff8fa3

frame
.r.
rhr
.r.

frame
.h.
hrh
.h.
`;

test("a sprite source becomes the record it draws", () => {
  const r = parseSpriteSource(HEART);
  assert.ok(r.ok, r.ok ? "" : r.error);
  const s = r.sprite;
  assert.equal(s.name, "Heart");
  assert.equal(s.fps, 4);
  assert.equal(s.w, 3);
  assert.equal(s.h, 3);
  assert.equal(s.frames, 2);
  assert.deepEqual(s.palette, [
    [255, 0x33, 0x55],
    [255, 0x8f, 0xa3],
  ]);
  // `.` is index 0 (transparent), and a palette key is its 1-based slot —
  // frame-major then row-major (docs/spec/scenes.md §4).
  assert.deepEqual([...s.index], [0, 1, 0, 1, 2, 1, 0, 1, 0, 0, 2, 0, 2, 1, 2, 0, 2, 0]);
  // and the record it encodes to is the format's own length
  assert.equal(encodeSprite(s).length, 12 + 5 + 3 * 2 + 3 * 3 * 2);
});

test("a still sprite needs no fps header", () => {
  const r = parseSpriteSource("name: Dot\npalette:\n  w ffffff\nframe\nw\n");
  assert.ok(r.ok);
  assert.equal(r.sprite.fps, 0);
  assert.equal(r.sprite.frames, 1);
});

for (const [why, src] of [
  ["no name", "fps: 2\npalette:\n  w ffffff\nframe\nw\n"],
  ["no frames", "name: X\npalette:\n  w ffffff\n"],
  ["fps over 30", "name: X\nfps: 31\npalette:\n  w ffffff\nframe\nw\n"],
  ["a palette key that is not one character", "name: X\npalette:\n  ww ffffff\nframe\nw\n"],
  ["`.` as a palette key", "name: X\npalette:\n  . ffffff\nframe\n.\n"],
  ["a repeated palette key", "name: X\npalette:\n  w ffffff\n  w 000000\nframe\nw\n"],
  ["a colour that is not rrggbb", "name: X\npalette:\n  w fff\nframe\nw\n"],
  ["a texel that is not in the palette", "name: X\npalette:\n  w ffffff\nframe\nq\n"],
  ["a ragged row", "name: X\npalette:\n  w ffffff\nframe\nww\nw\n"],
  ["a second frame of a different height", "name: X\npalette:\n  w ffffff\nframe\nw\nframe\nw\nw\n"],
  ["a palette colour nothing paints with", "name: X\npalette:\n  w ffffff\n  b 000000\nframe\nw\n"],
  ["an unknown header", "name: X\nspeed: 3\npalette:\n  w ffffff\nframe\nw\n"],
  ["an edge over 64", `name: X\npalette:\n  w ffffff\nframe\n${"w".repeat(65)}\n`],
]) {
  test(`a sprite source is refused for ${why}`, () => {
    const r = parseSpriteSource(src);
    assert.equal(r.ok, false, `expected a refusal for ${why}`);
    if (!r.ok) assert.match(r.error, /^sprite: /);
  });
}

test("a sprite source over the 16 KiB cap is refused by the parser, not truncated", () => {
  // 64x64x5 = 20,480 texels, comfortably past the cap
  const row = "w".repeat(64);
  const frame = `frame\n${Array(64).fill(row).join("\n")}\n`;
  const r = parseSpriteSource(`name: Big\npalette:\n  w ffffff\n${frame.repeat(5)}`);
  assert.equal(r.ok, false);
  if (!r.ok) assert.match(r.error, /over the 16 KiB cap/);
});

test("a reference id is derived, stable and namespaced", () => {
  assert.match(libRefId("pat", "aurora-2d"), /^[0-9a-f]{8}$/);
  assert.equal(libRefId("pat", "aurora-2d"), libRefId("pat", "aurora-2d"));
  assert.notEqual(libRefId("pat", "heart"), libRefId("spr", "heart"));
  assert.notEqual(libSceneId("heart"), libRefId("spr", "heart"));
  assert.match(libSceneId("sunrise"), /^[0-9a-f]{8}$/);
});

test("the slug is the file stem", () => {
  assert.equal(libSlug("heart.sprite"), "heart");
  assert.equal(libSlug("clock-over-aurora.scene"), "clock-over-aurora");
});

const SCENE = `# what this scene is for
S - Demo
L pat 0 0 0 0 normal 100 none fill 1
I @pat/aurora-2d
L sprite 2 2 9 8 add 80 none fill 1
I @spr/heart
`;

test("a scene source becomes an ordinary wire record plus its references", () => {
  const r = parseSceneSource(SCENE);
  assert.ok(r.ok, r.ok ? "" : r.error);
  // the comment is gone and both `I` lines carry the derived ids
  assert.equal(r.wire.includes("#"), false);
  assert.match(r.wire, new RegExp(`^I ${libRefId("pat", "aurora-2d")}$`, "m"));
  assert.match(r.wire, new RegExp(`^I ${libRefId("spr", "heart")}$`, "m"));
  assert.deepEqual(
    r.refs.map((x) => `${x.kind}/${x.slug}`),
    ["pat/aurora-2d", "spr/heart"],
  );
  // and the result is what the REAL parser takes
  const p = parseScene(r.wire);
  assert.ok(p.ok, p.ok ? "" : p.error);
  assert.equal(p.scene.name, "Demo");
  assert.equal(p.scene.layers.length, 2);
});

test("one library entry named twice is one reference", () => {
  const r = parseSceneSource(
    "S - Two\nL sprite 0 0 9 8 normal 100 none fill 1\nI @spr/heart\n" +
      "L sprite 12 0 9 8 normal 100 none fill 1\nI @spr/heart\n",
  );
  assert.ok(r.ok);
  assert.equal(r.refs.length, 1);
});

for (const [why, src] of [
  ["a raw store id", "S - X\nL pat 0 0 0 0 normal 100 none fill 1\nI 5eed1c92\n"],
  ["a malformed reference", "S - X\nL pat 0 0 0 0 normal 100 none fill 1\nI @pat/Aurora 2D\n"],
  ["an unknown reference kind", "S - X\nL pat 0 0 0 0 normal 100 none fill 1\nI @ptn/aurora-2d\n"],
  ["a reference off an I line", "S - X\nL pat 0 0 0 0 normal 100 none fill 1\nN @spr/heart\n"],
  ["nothing at all", "# only comments\n"],
]) {
  test(`a scene source is refused for ${why}`, () => {
    const r = parseSceneSource(src);
    assert.equal(r.ok, false, `expected a refusal for ${why}`);
  });
}

// ---- the SHIPPED library ----

const spriteDir = `${repo}library/sprites`;
const sceneDir = `${repo}library/scenes`;

test("every shipped sprite parses and fits the cap", { skip: !existsSync(spriteDir) }, () => {
  const files = readdirSync(spriteDir).filter((f) => f.endsWith(".sprite"));
  assert.ok(files.length >= 8, `only ${files.length} shipped sprites`);
  const names = new Set();
  for (const f of files) {
    const r = parseSpriteSource(readFileSync(`${spriteDir}/${f}`, "utf8"));
    assert.ok(r.ok, `${f}: ${r.ok ? "" : r.error}`);
    assert.ok(encodeSprite(r.sprite).length <= SPRITE_MAX_BYTES, `${f} is over the cap`);
    // the picker and the tile grid both key rows on the display name
    assert.equal(names.has(r.sprite.name), false, `two shipped sprites are called “${r.sprite.name}”`);
    names.add(r.sprite.name);
  }
});

test("every shipped scene parses and resolves", { skip: !existsSync(sceneDir) }, () => {
  const slugs = new Set(
    existsSync(spriteDir)
      ? readdirSync(spriteDir).filter((f) => f.endsWith(".sprite")).map(libSlug)
      : [],
  );
  const files = readdirSync(sceneDir).filter((f) => f.endsWith(".scene"));
  assert.ok(files.length >= 4, `only ${files.length} shipped scenes`);
  for (const f of files) {
    const r = parseSceneSource(readFileSync(`${sceneDir}/${f}`, "utf8"));
    assert.ok(r.ok, `${f}: ${r.ok ? "" : r.error}`);
    const p = parseScene(r.wire);
    assert.ok(p.ok, `${f}: ${p.ok ? "" : p.error}`);
    assert.notEqual(p.scene.name, "", `${f} has no scene name`);
    for (const ref of r.refs) {
      if (ref.kind === "spr") assert.ok(slugs.has(ref.slug), `${f}: no sprite ${ref.slug}`);
      else assert.ok(existsSync(`${repo}library/${ref.slug}.js`), `${f}: no pattern ${ref.slug}`);
    }
    // the cap that actually bites: a device's whole scene blob is 3840 B
    assert.ok(r.wire.length < 1024, `${f} is ${r.wire.length} B — too big a share of the 3840 B blob`);
  }
});
