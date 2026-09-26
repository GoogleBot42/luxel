// `src/lib/sprite.ts` — the browser's codec for the `LXSP` sprite record
// (Gitea #740, contract §1, `crates/luxel-core/src/sprite.rs`).
//
// The interesting assertion is not that the codec round-trips itself: it is
// that the bytes it writes are the bytes the RUST codec writes. So the record
// in `heart()` below is assembled BY HAND here, exactly as
// `luxel_core::sprite::tests::build` assembles it there, and both sides are
// pinned to the same 35 bytes. If either codec drifts, one of the two test
// suites goes red on the same fixture.
//
// The rest is the editing algebra the sprite editor is built on (#741): paint,
// fill, resize, the frame ops, the frame clock, the ONE palette-cap
// enforcement point, and the one-release migration reader for the old
// `// @sprite` pattern format.
import test from "node:test";
import assert from "node:assert/strict";
import {
  SPRITE_COLORS_FULL,
  SPRITE_HDR,
  SPRITE_MAX_BYTES,
  SPRITE_MAX_COLORS,
  addFrame,
  checkSprite,
  colorIndex,
  compactPalette,
  decodeSprite,
  deleteFrame,
  duplicateFrame,
  encodeSprite,
  fill,
  frameAt,
  isTaggedSpriteSource,
  moveFrame,
  newSprite,
  paint,
  recordLen,
  renderFrame,
  resize,
  rgb8ToHex,
  hexToRgb8,
  spriteBytes,
  spriteFromTaggedPattern,
  spriteMetaLine,
  texelIndex,
  usedColors,
} from "../src/lib/sprite.ts";

/** The same assembler the Rust test has, so neither side can hide a bug in a
 *  shared helper: this builds the bytes from the LAYOUT, not from the codec. */
function build(name, w, h, frames, fps, palette, index) {
  const nm = new TextEncoder().encode(name);
  const out = [];
  out.push(0x4c, 0x58, 0x53, 0x50); // "LXSP"
  out.push(1); // version
  out.push(w, h, frames, fps, palette.length, nm.length, 0);
  out.push(...nm);
  for (const c of palette) out.push(...c);
  out.push(...index);
  return new Uint8Array(out);
}

/** 3×2, two frames, two colours — `luxel_core::sprite::tests::heart`. */
const HEART_INDEX = [1, 0, 1, 2, 1, 2, 1, 0, 1, 2, 1, 0];
const heart = () => build("Heart", 3, 2, 2, 10, [[255, 0, 0], [0, 0, 0]], HEART_INDEX);

test("the record is 12 + name + 3·colors + w·h·frames bytes", () => {
  const rec = heart();
  assert.equal(rec.length, 35, "12 + 5 + 6 + 12");
  assert.equal(rec.length, recordLen(5, 2, 3, 2, 2));
  assert.equal(SPRITE_HDR, 12);
});

test("encode → decode is byte-exact against a hand-built record", () => {
  const rec = heart();
  const sp = decodeSprite(rec);
  assert.ok(sp, "the hand-built record decodes");
  assert.equal(sp.name, "Heart");
  assert.deepEqual([sp.w, sp.h, sp.frames, sp.fps], [3, 2, 2, 10]);
  assert.deepEqual(sp.palette, [
    [255, 0, 0],
    [0, 0, 0],
  ]);
  assert.deepEqual([...sp.index], HEART_INDEX);
  // …and back out again, byte for byte
  assert.deepEqual([...encodeSprite(sp)], [...rec]);
  assert.equal(spriteBytes(sp), rec.length);
});

test("index 0 is TRANSPARENT and an opaque black texel is a colour", () => {
  const sp = decodeSprite(heart());
  const px = renderFrame(sp, 0);
  // texel 0 = index 1 = red, opaque
  assert.deepEqual([...px.slice(0, 4)], [255, 0, 0, 255]);
  // texel 1 = index 0 = transparent (alpha 0, not black)
  assert.deepEqual([...px.slice(4, 8)], [0, 0, 0, 0]);
  // texel 3 = index 2 = palette black, OPAQUE — the change from the old
  // tagged-pattern format, where v == 0 was the key
  assert.deepEqual([...px.slice(12, 16)], [0, 0, 0, 255]);
});

test("every malformation is rejected, with the reason the routes answer with", () => {
  const bad = (mutate, reason) => {
    const rec = heart();
    const out = mutate(rec) ?? rec;
    assert.equal(checkSprite(out), reason, reason);
    assert.equal(decodeSprite(out), null, `${reason} → null`);
  };

  bad(() => new Uint8Array(8), "sprite: record is too short");
  bad((r) => void (r[0] = 0x4d), "sprite: bad magic");
  bad((r) => void (r[4] = 2), "sprite: unknown record version");
  bad((r) => void (r[5] = 0), "sprite: 1..64 texels on each edge");
  bad((r) => void (r[6] = 65), "sprite: 1..64 texels on each edge");
  bad((r) => void (r[7] = 0), "sprite: at least one frame");
  bad((r) => void (r[8] = 31), "sprite: fps 0..30");
  bad((r) => void (r[10] = 0), "sprite: name must be 1..=64 bytes");
  bad((r) => void (r[10] = 65), "sprite: name must be 1..=64 bytes");
  // a byte too many, with the header untouched
  bad(
    () => new Uint8Array([...heart(), 0]),
    "sprite: length does not match its header",
  );
  // a header that claims one more colour than the bytes hold
  bad((r) => void (r[9] = 3), "sprite: length does not match its header");
  // an index past the palette
  bad((r) => void (r[r.length - 1] = 9), "sprite: index out of palette");
  // a name that is not utf-8 (a lone continuation byte)
  bad((r) => void (r[SPRITE_HDR] = 0x80), "sprite: name is not utf-8");
  // over the 16 KiB cap: a header that is otherwise plausible
  bad(() => {
    const big = build("Big", 64, 64, 5, 0, [[1, 2, 3]], new Array(64 * 64 * 5).fill(0));
    assert.ok(big.length > SPRITE_MAX_BYTES);
    return big;
  }, "sprite: over the 16 KiB cap");

  assert.equal(checkSprite(heart()), null, "the good record has no complaint");
});

test("a new sprite is transparent, named, and has an empty palette", () => {
  const sp = newSprite("Heart", 4, 3);
  assert.equal(sp.name, "Heart");
  assert.deepEqual([sp.w, sp.h, sp.frames, sp.fps], [4, 3, 1, 0]);
  assert.deepEqual(sp.palette, []);
  assert.equal(sp.index.length, 12);
  assert.ok([...sp.index].every((k) => k === 0));
  assert.equal(spriteBytes(sp), 12 + 5 + 0 + 12);
  assert.equal(checkSprite(encodeSprite(sp)), null);
});

test("an empty or over-long name is clamped rather than refused", () => {
  assert.equal(decodeSprite(encodeSprite(newSprite("", 2, 2))).name, "Sprite");
  const long = "x".repeat(200);
  const back = decodeSprite(encodeSprite({ ...newSprite(long, 2, 2), name: long }));
  assert.equal(back.name.length, 64);
});

test("colorIndex is the ONE palette gate: dedupe, append, refuse at 255", () => {
  let sp = newSprite("Pal", 32, 32);
  const red = colorIndex(sp, [255, 0, 0]);
  assert.equal(typeof red, "object");
  assert.equal(red.k, 1);
  assert.deepEqual(red.sprite.palette, [[255, 0, 0]]);
  // the SAME colour again is the same slot and the same record
  const again = colorIndex(red.sprite, [255, 0, 0]);
  assert.equal(again.k, 1);
  assert.equal(again.sprite, red.sprite, "no new entry, no new object");

  // fill the palette to the cap
  for (let i = 0; i < SPRITE_MAX_COLORS; i++) {
    const got = colorIndex(sp, [i, 0, 0]);
    assert.equal(typeof got, "object", `colour ${i}`);
    sp = got.sprite;
  }
  assert.equal(sp.palette.length, SPRITE_MAX_COLORS);
  // a colour already in it is still fine…
  assert.equal(colorIndex(sp, [7, 0, 0]).k, 8);
  // …a new one is the one refusal, as a sentence
  assert.equal(colorIndex(sp, [1, 2, 3]), SPRITE_COLORS_FULL);
  assert.equal(SPRITE_COLORS_FULL, "sprite: 255 colours max");
});

test("paint replaces the record rather than mutating it; 0 erases", () => {
  const base = newSprite("P", 3, 2);
  const slot = colorIndex(base, [10, 20, 30]);
  const painted = paint(slot.sprite, 0, 1, 0, slot.k);
  assert.equal(painted.index[texelIndex(painted, 0, 1, 0)], slot.k);
  assert.equal(slot.sprite.index[texelIndex(base, 0, 1, 0)], 0, "the original is untouched");
  const erased = paint(painted, 0, 1, 0, 0);
  assert.equal(erased.index[texelIndex(erased, 0, 1, 0)], 0);
  // out of range is a no-op, and the SAME object (so nothing re-renders)
  assert.equal(paint(painted, 0, 9, 9, 1), painted);
  assert.equal(paint(painted, 5, 0, 0, 1), painted);
});

test("fill floods one frame, four-connected, over the seed index only", () => {
  let sp = newSprite("F", 3, 3);
  sp = { ...sp, frames: 2, index: new Uint8Array(3 * 3 * 2) };
  sp = { ...sp, palette: [[255, 255, 255], [0, 0, 255]] };
  // a wall down the middle of frame 0
  for (let row = 0; row < 3; row++) sp = paint(sp, 0, 1, row, 1);
  sp = fill(sp, 0, 0, 0, 2);
  for (let row = 0; row < 3; row++) {
    assert.equal(sp.index[texelIndex(sp, 0, 0, row)], 2, `left row ${row}`);
    assert.equal(sp.index[texelIndex(sp, 0, 1, row)], 1, `the wall stood, row ${row}`);
    assert.equal(sp.index[texelIndex(sp, 0, 2, row)], 0, `behind the wall, row ${row}`);
    assert.equal(sp.index[texelIndex(sp, 1, 0, row)], 0, `frame 1 never moved, row ${row}`);
  }
  // filling with the seed's own index changes nothing
  assert.equal(fill(sp, 0, 0, 0, 2), sp);
});

test("resize keeps what fits, in every frame", () => {
  let sp = newSprite("R", 3, 3);
  sp = { ...sp, palette: [[9, 9, 9]] };
  sp = paint(sp, 0, 0, 0, 1);
  sp = paint(sp, 0, 2, 2, 1);
  const small = resize(sp, 2, 2);
  assert.equal(small.index.length, 4);
  assert.equal(small.index[texelIndex(small, 0, 0, 0)], 1, "the near corner survived");
  const big = resize(small, 4, 4);
  assert.equal(big.index.length, 16);
  assert.equal(big.index[texelIndex(big, 0, 0, 0)], 1);
  assert.equal(big.index[texelIndex(big, 0, 3, 3)], 0, "the new room is transparent");
  // the clamp: 0 and 65 both land inside 1..64
  assert.equal(resize(sp, 0, 0).w, 1);
  assert.equal(resize(sp, 999, 999).w, 64);
  // a resize with the SAME size is a no-op object
  assert.equal(resize(sp, 3, 3), sp);
});

test("the frame ops add, duplicate, delete and reorder whole frames", () => {
  let sp = newSprite("A", 2, 1);
  sp = { ...sp, palette: [[1, 1, 1], [2, 2, 2]] };
  sp = paint(sp, 0, 0, 0, 1); // frame 0 = [1, 0]

  // `+` is a COPY of the frame you are on, placed after it
  sp = addFrame(sp, 0);
  assert.equal(sp.frames, 2);
  assert.deepEqual([...sp.index], [1, 0, 1, 0]);

  // edit the copy, then duplicate the FIRST frame again
  sp = paint(sp, 1, 1, 0, 2); // frame 1 = [1, 2]
  sp = duplicateFrame(sp, 0);
  assert.equal(sp.frames, 3);
  assert.deepEqual([...sp.index], [1, 0, 1, 0, 1, 2], "the copy went after frame 0");

  // move frame 0 to the end
  const moved = moveFrame(sp, 0, 2);
  assert.deepEqual([...moved.index], [1, 0, 1, 2, 1, 0]);
  assert.equal(moveFrame(sp, 1, 1), sp, "a move to where it already is is a no-op");

  // delete the middle one
  const cut = deleteFrame(sp, 1);
  assert.equal(cut.frames, 2);
  assert.deepEqual([...cut.index], [1, 0, 1, 2]);

  // a sprite always has at least one frame
  const one = deleteFrame(deleteFrame(cut, 0), 0);
  assert.equal(one.frames, 1);
  assert.equal(deleteFrame(one, 0), one);
});

test("the frame clock is (elapsed·fps/1000) mod frames, and 0 for a still", () => {
  const sp = decodeSprite(heart()); // 2 frames at 10 fps
  assert.equal(frameAt(sp, 0), 0);
  assert.equal(frameAt(sp, 99), 0);
  assert.equal(frameAt(sp, 100), 1);
  assert.equal(frameAt(sp, 200), 0);
  assert.equal(frameAt(sp, 2500), 1, "25 frames in: odd, so frame 1");
  assert.equal(frameAt({ ...sp, fps: 0 }, 5000), 0, "fps 0 is a still");
  assert.equal(frameAt({ ...sp, frames: 1 }, 5000), 0, "one frame is a still");
});

test("usedColors is first-appearance order, and compactPalette drops the rest", () => {
  let sp = newSprite("U", 3, 1);
  // three palette entries, only two of them painted, and the LATER one first
  sp = { ...sp, palette: [[1, 1, 1], [2, 2, 2], [3, 3, 3]] };
  sp = paint(sp, 0, 0, 0, 3);
  sp = paint(sp, 0, 1, 0, 1);
  sp = paint(sp, 0, 2, 0, 3);
  assert.deepEqual(
    usedColors(sp).map((u) => u.rgb),
    [
      [3, 3, 3],
      [1, 1, 1],
    ],
    "the order the drawing shows them in",
  );

  const tidy = compactPalette(sp);
  assert.deepEqual(tidy.palette, [
    [3, 3, 3],
    [1, 1, 1],
  ]);
  assert.deepEqual([...tidy.index], [1, 2, 1], "the index was renumbered");
  // the PICTURE is unchanged, which is the whole requirement
  assert.deepEqual([...renderFrame(tidy, 0)], [...renderFrame(sp, 0)]);
  // and a sprite with nothing unused is returned as-is
  assert.equal(compactPalette(tidy), tidy);
});

test("the tagged-pattern migration reader converts the old HSV format", () => {
  // The old emitter's shape: the tag line, the name line, three HSV arrays.
  // 2×2, one frame; v == 0 was the transparency key there.
  const src = [
    "// @sprite w=2 h=2 frames=1 fps=0",
    "// Old heart",
    "",
    "var sprH = [0, 0, 0.3333, 0]",
    "var sprS = [1, 0, 1, 0]",
    "var sprV = [1, 0, 1, 0]",
    "",
    "export function renderFrame() {}",
  ].join("\n");

  assert.ok(isTaggedSpriteSource(src));
  assert.ok(!isTaggedSpriteSource("export function renderFrame() {}"));

  const sp = spriteFromTaggedPattern(src);
  assert.ok(sp);
  assert.equal(sp.name, "Old heart");
  assert.deepEqual([sp.w, sp.h, sp.frames, sp.fps], [2, 2, 1, 0]);
  // hue 0 sat 1 val 1 = pure red; hue 1/3 = pure green
  assert.deepEqual(sp.palette, [
    [255, 0, 0],
    [0, 255, 0],
  ]);
  // v == 0 became index 0, not a black palette entry
  assert.deepEqual([...sp.index], [1, 0, 2, 0]);
  assert.equal(checkSprite(encodeSprite(sp)), null, "and it encodes to a valid record");

  // the name falls back when the tag line has no follower
  const noName = spriteFromTaggedPattern(
    "// @sprite w=1 h=1 frames=1 fps=0\nvar sprH = [0]\nvar sprS = [0]\nvar sprV = [0]\n",
  );
  assert.equal(noName.name, "Sprite");

  // and it refuses anything it cannot read
  assert.equal(spriteFromTaggedPattern("export function renderFrame() {}"), null, "no tag");
  assert.equal(
    spriteFromTaggedPattern("// @sprite w=2 h=2 frames=1 fps=0\nvar sprH = [0,0,0,0]\n"),
    null,
    "a missing array",
  );
  assert.equal(
    spriteFromTaggedPattern(
      "// @sprite w=2 h=2 frames=1 fps=0\nvar sprH = [0]\nvar sprS = [0]\nvar sprV = [0]\n",
    ),
    null,
    "arrays that disagree with the tag",
  );
  assert.equal(
    spriteFromTaggedPattern("// @sprite w=99 h=2 frames=1 fps=0\n"),
    null,
    "a size the format cannot hold",
  );
});

test("the small shared helpers", () => {
  assert.equal(rgb8ToHex([232, 163, 61]), "e8a33d");
  assert.deepEqual(hexToRgb8("#e8a33d"), [232, 163, 61]);
  assert.deepEqual(hexToRgb8("e8a33d"), [232, 163, 61]);
  assert.equal(hexToRgb8("nope"), null);
  assert.equal(spriteMetaLine(9, 8, 1), "9×8 · 1 frame");
  assert.equal(spriteMetaLine(9, 8, 2), "9×8 · 2 frames");
});
