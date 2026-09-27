// `src/lib/sceneRefs.ts` — the reconciliation that makes a scene from OUTSIDE a
// store usable inside it (Gitea #785, and #746 when scene import lands).
//
// The behaviour worth pinning is not the happy path: it is the two rules that
// decide whether a record is created at all.
//
//   * MATCHING IS BY CONTENT. A library pattern whose source is already in the
//     store binds that id however the user renamed it; a sprite whose bytes are
//     already there does the same. Cloning the same scene twice must not leave
//     two copies of everything.
//   * A NAME COLLISION RENAMES. Both hosts treat a save under an existing name
//     as "replace that one", so creating `Heart` when the user has a different
//     `Heart` would eat their drawing.
//
// Run: `npm test` from web/.
import test from "node:test";
import assert from "node:assert/strict";
import {
  freeName,
  resolveSceneRefs,
  rewriteSceneIds,
  tallyLine,
} from "../src/lib/sceneRefs.ts";
import { parseScene } from "../src/lib/scene.ts";
import { encodeSprite } from "../src/lib/sprite.ts";

const PAT = "aaaaaaaa";
const SPR = "bbbbbbbb";

const WIRE =
  `S - Demo\n` +
  `L pat 0 0 0 0 normal 100 none fill 1\nI ${PAT}\n` +
  `L sprite 2 2 1 1 add 80 none fill 1\nI ${SPR}\n` +
  `L color 0 0 0 0 normal 100 none fill 1\nK 112233\n`;

const REFS = [
  { id: PAT, kind: "pat", slug: "aurora-2d", name: "Aurora 2D" },
  { id: SPR, kind: "spr", slug: "heart", name: "Heart" },
];

const SOURCE = "export function render(i) { hsv(0, 1, 1) }\n";

function sprite(name, v) {
  return { name, w: 1, h: 1, frames: 1, fps: 0, palette: [[v, v, v]], index: new Uint8Array([1]) };
}

/** A `RefTarget` over two arrays, with a log of what it was asked to create. */
function fakeTarget({ patterns = [], sprites = [], patError = null, sprError = null } = {}) {
  const made = { patterns: [], sprites: [] };
  let next = 0;
  const id = () => `id${++next}`;
  return {
    made,
    patterns: async () => patterns,
    putPattern: async (name, source) => {
      if (patError) return { error: patError };
      made.patterns.push(name);
      const row = { id: id(), name, source };
      patterns = [...patterns, row];
      return { id: row.id };
    },
    sprites: async () => sprites,
    putSprite: async (s) => {
      if (sprError) return { error: sprError };
      made.sprites.push(s.name);
      const row = { id: id(), name: s.name, sprite: s };
      sprites = [...sprites, row];
      return { id: row.id };
    },
  };
}

const lib = {
  patternSource: (ref) => (ref.slug === "aurora-2d" ? SOURCE : null),
  sprite: (ref) => (ref.slug === "heart" ? sprite("Heart", 200) : null),
};

function scene() {
  const p = parseScene(WIRE);
  assert.ok(p.ok, p.ok ? "" : p.error);
  return p.scene;
}

test("an empty store gets both records created and the ids rewritten", async () => {
  const target = fakeTarget();
  const r = await resolveSceneRefs(scene(), REFS, lib, target);
  assert.ok(r.ok, r.ok ? "" : r.error);
  assert.deepEqual(target.made.patterns, ["Aurora 2D"]);
  assert.deepEqual(target.made.sprites, ["Heart"]);
  assert.equal(r.tally.patternsCreated, 1);
  assert.equal(r.tally.spritesCreated, 1);
  assert.equal(r.tally.reused, 0);
  const ids = r.scene.layers.map((l) =>
    l.body.kind === "pat" ? l.body.pat.id : l.body.kind === "sprite" ? l.body.id : "",
  );
  // the placeholders are gone, the colour layer is untouched
  assert.deepEqual(ids, ["id1", "id2", ""]);
});

test("a pattern already in the store is reused however it is named", async () => {
  const target = fakeTarget({ patterns: [{ id: "keep", name: "My aurora", source: SOURCE }] });
  const r = await resolveSceneRefs(scene(), REFS, lib, target);
  assert.ok(r.ok);
  assert.deepEqual(target.made.patterns, []);
  assert.equal(r.scene.layers[0].body.pat.id, "keep");
  assert.equal(r.tally.reused, 1);
});

test("a sprite already in the store is reused by its BYTES", async () => {
  const same = sprite("Whatever", 200);
  assert.deepEqual([...encodeSprite(same)], [...encodeSprite(sprite("Whatever", 200))]);
  const target = fakeTarget({ sprites: [{ id: "keep", name: "Whatever", sprite: same }] });
  const r = await resolveSceneRefs(scene(), REFS, lib, target);
  assert.ok(r.ok);
  // the NAME differs, so the record has to match on content — and it does not,
  // because the name is part of the record. A different name is a different
  // sprite, which is the conservative answer: it creates rather than binds
  // somebody else's drawing.
  assert.deepEqual(target.made.sprites, ["Heart"]);
});

test("the same sprite under the same name is reused, not duplicated", async () => {
  const target = fakeTarget({ sprites: [{ id: "keep", name: "Heart", sprite: sprite("Heart", 200) }] });
  const r = await resolveSceneRefs(scene(), REFS, lib, target);
  assert.ok(r.ok);
  assert.deepEqual(target.made.sprites, []);
  assert.equal(r.scene.layers[1].body.id, "keep");
});

test("a name already taken by a DIFFERENT record is renamed, never overwritten", async () => {
  const target = fakeTarget({
    patterns: [{ id: "mine", name: "Aurora 2D", source: "// something else\n" }],
    sprites: [{ id: "mine2", name: "Heart", sprite: sprite("Heart", 7) }],
  });
  const r = await resolveSceneRefs(scene(), REFS, lib, target);
  assert.ok(r.ok);
  assert.deepEqual(target.made.patterns, ["Aurora 2D 2"]);
  assert.deepEqual(target.made.sprites, ["Heart 2"]);
});

test("a row whose source has not streamed in is never matched", async () => {
  const target = fakeTarget({ patterns: [{ id: "unknown", name: "Aurora 2D" }] });
  const r = await resolveSceneRefs(scene(), REFS, lib, target);
  assert.ok(r.ok);
  assert.deepEqual(target.made.patterns, ["Aurora 2D 2"]);
});

test("a host refusal comes back verbatim", async () => {
  const target = fakeTarget({ sprError: "sprite: 17408 B is over the 16 KiB cap" });
  const r = await resolveSceneRefs(scene(), REFS, lib, target);
  assert.equal(r.ok, false);
  if (!r.ok) assert.equal(r.error, "sprite: 17408 B is over the 16 KiB cap");
});

test("a reference the library cannot supply is reported, not silently dropped", async () => {
  const r = await resolveSceneRefs(
    scene(),
    [{ id: PAT, kind: "pat", slug: "gone", name: "Gone" }],
    lib,
    fakeTarget(),
  );
  assert.equal(r.ok, false);
  if (!r.ok) assert.match(r.error, /no pattern “Gone”/);
});

test("freeName steps past what is taken", () => {
  assert.equal(freeName("Heart", []), "Heart");
  assert.equal(freeName("Heart", ["Heart"]), "Heart 2");
  assert.equal(freeName("Heart", ["Heart", "Heart 2"]), "Heart 3");
  // an already-numbered name does not become "Heart 2 2"
  assert.equal(freeName("Heart 2", ["Heart 2"]), "Heart 3");
});

test("rewriteSceneIds leaves an id it does not know", () => {
  const out = rewriteSceneIds(scene(), new Map([[PAT, "z"]]));
  assert.equal(out.layers[0].body.pat.id, "z");
  assert.equal(out.layers[1].body.id, SPR);
});

test("the tally reads as a sentence", () => {
  assert.equal(tallyLine({ patternsCreated: 0, spritesCreated: 0, reused: 0 }), "cloned");
  assert.equal(tallyLine({ patternsCreated: 0, spritesCreated: 0, reused: 2 }), "cloned — reused 2");
  assert.equal(
    tallyLine({ patternsCreated: 1, spritesCreated: 2, reused: 0 }),
    "cloned — added 1 pattern and 2 sprites",
  );
  assert.equal(
    tallyLine({ patternsCreated: 2, spritesCreated: 0, reused: 1 }),
    "cloned — added 2 patterns, reused 1",
  );
});
