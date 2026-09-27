// `src/lib/imageImport.ts` — image → `LXSP` sprite (Gitea #784).
//
// Two halves, and the second is the interesting one:
//
//   * the algebra, on frames built right here: what fit/fill/crop place where,
//     what the two resamplers do, where the alpha threshold cuts, what the
//     median cut picks, how the frame rate is derived, and the two cap knobs.
//   * the GOLDENS in tests/fixtures/, which `crates/luxel-cli/tests/
//     spriteimport.rs` asserts against the same files. There are two
//     implementations of one wire-visible result (#748's rule), so the fixture
//     is what pins them: this suite feeds the `.rgba` sidecar to the TS
//     pipeline, the Rust suite decodes the `.png`/`.gif` with the `image`
//     crate, and both must produce the checked-in `.lxsp` byte for byte. Run
//     `node --experimental-strip-types web/tools/gen-sprite-fixtures.mjs` to
//     rewrite them, and expect the Rust suite to go red if only one side moved.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  DEFAULT_ALPHA_THRESHOLD,
  defaultImportOptions,
  defaultTargetSize,
  derivedFps,
  fitUnderCap,
  importSprite,
  keptIndices,
  nearestIndex,
  normalizedDelay,
  plannedBytes,
  resolveOptions,
  spriteName,
} from "../src/lib/imageImport.ts";
import { sheetFileName, sheetPixels } from "../src/lib/spriteExport.ts";
import {
  checkSprite,
  encodeSprite,
  SPRITE_MAX_BYTES,
  SPRITE_MAX_COLORS,
  texelIndex,
} from "../src/lib/sprite.ts";

const FIX = join(dirname(fileURLToPath(import.meta.url)), "fixtures");

/** Every option spelled out, so a test states exactly what it varies. */
function opts(over = {}) {
  return resolveOptions({
    name: "T",
    w: 4,
    h: 4,
    fit: "fit",
    resample: "nearest",
    colors: SPRITE_MAX_COLORS,
    alphaThreshold: DEFAULT_ALPHA_THRESHOLD,
    dither: false,
    keepEvery: 1,
    fpsOverride: null,
    ...over,
  });
}

/** A source image from a `w`×`h` grid of `[r,g,b,a]`. */
function image(w, h, pixels, delayMs = 0) {
  const rgba = new Uint8Array(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    const p = pixels[i] ?? [0, 0, 0, 0];
    rgba[i * 4] = p[0];
    rgba[i * 4 + 1] = p[1];
    rgba[i * 4 + 2] = p[2];
    rgba[i * 4 + 3] = p[3];
  }
  return { w, h, frames: [{ rgba, delayMs }] };
}

/** A flat block of one colour. */
function flat(w, h, rgba, delayMs = 0) {
  return image(w, h, Array.from({ length: w * h }, () => rgba), delayMs);
}

/** The sprite's texel as `[r,g,b]`, or null when transparent. */
function texel(s, frame, col, row) {
  const k = s.index[texelIndex(s, frame, col, row)] ?? 0;
  if (k === 0) return null;
  return s.palette[k - 1] ?? null;
}

// ---- the record's shape ----

test("an import is always a valid record", () => {
  const r = importSprite(flat(3, 3, [10, 20, 30, 255]), opts({ w: 6, h: 6 }));
  assert.equal(checkSprite(encodeSprite(r.sprite)), null);
  assert.equal(r.sprite.w, 6);
  assert.equal(r.sprite.h, 6);
  assert.equal(r.sprite.frames, 1);
  assert.equal(r.sprite.fps, 0, "a single frame is still");
  assert.equal(r.bytes, encodeSprite(r.sprite).length);
  assert.equal(r.overCap, false);
});

test("the name comes from the file name, extension and path stripped", () => {
  assert.equal(spriteName("heart.png"), "heart");
  assert.equal(spriteName("/tmp/a b/My Sprite.GIF"), "My Sprite");
  assert.equal(spriteName("C:\\art\\x.webp"), "x");
  assert.equal(spriteName(".png"), "Sprite", "nothing left is still a name");
  assert.equal(spriteName(""), "Sprite");
  // 64 bytes is the record's ceiling, and a name is clipped to it
  assert.equal(spriteName(`${"x".repeat(80)}.png`), "x".repeat(64));
});

// ---- placement ----

test("fit letterboxes, keeping proportions and leaving the rest transparent", () => {
  // a 4×2 opaque block into an 4×4 box: two rows drawn, one blank above and
  // one below
  const r = importSprite(flat(4, 2, [200, 100, 50, 255]), opts({ w: 4, h: 4, fit: "fit" }));
  const s = r.sprite;
  assert.deepEqual(texel(s, 0, 0, 0), null, "row 0 is letterbox");
  assert.deepEqual(texel(s, 0, 0, 1), [200, 100, 50]);
  assert.deepEqual(texel(s, 0, 3, 2), [200, 100, 50]);
  assert.deepEqual(texel(s, 0, 2, 3), null, "row 3 is letterbox");
});

test("fill stretches into the whole box, proportions be damned", () => {
  const r = importSprite(flat(4, 2, [200, 100, 50, 255]), opts({ w: 4, h: 4, fit: "fill" }));
  const s = r.sprite;
  for (let y = 0; y < 4; y++) {
    for (let x = 0; x < 4; x++) {
      assert.deepEqual(texel(s, 0, x, y), [200, 100, 50], `${x},${y}`);
    }
  }
});

test("crop covers the box and cuts the overflow off the long axis", () => {
  // a 4×1 strip: left half red, right half blue. Cropped into 2×2 it scales up
  // to cover (×2 vertically) and loses the outer quarters, so the two texel
  // columns are the two MIDDLE source pixels.
  const src = image(4, 1, [
    [255, 0, 0, 255],
    [0, 255, 0, 255],
    [0, 0, 255, 255],
    [255, 255, 0, 255],
  ]);
  const r = importSprite(src, opts({ w: 2, h: 2, fit: "crop", resample: "nearest" }));
  const s = r.sprite;
  assert.deepEqual(texel(s, 0, 0, 0), [0, 255, 0]);
  assert.deepEqual(texel(s, 0, 1, 0), [0, 0, 255]);
  assert.deepEqual(texel(s, 0, 0, 1), [0, 255, 0], "both rows are the same source row");
});

// ---- resampling ----

test("nearest takes one source pixel; area averages every pixel it covers", () => {
  // 2×2: three black, one white. Downscaled to 1×1 the target pixel's centre
  // maps to source (1, 1) — so nearest is that ONE pixel (the white one) and
  // area is the mean of all four. Same input, deliberately different answers.
  const quad = image(2, 2, [
    [0, 0, 0, 255],
    [0, 0, 0, 255],
    [0, 0, 0, 255],
    [255, 255, 255, 255],
  ]);
  const near = importSprite(quad, opts({ w: 1, h: 1, fit: "fill", resample: "nearest" }));
  assert.deepEqual(texel(near.sprite, 0, 0, 0), [255, 255, 255]);
  const area = importSprite(quad, opts({ w: 1, h: 1, fit: "fill", resample: "area" }));
  // (0+0+0+255)/4 = 63.75 → 64
  assert.deepEqual(texel(area.sprite, 0, 0, 0), [64, 64, 64]);
});

test("area weighting is by OVERLAP, not by which sample lands nearest", () => {
  // 3×1 → 2×1: the left texel covers source x in [0, 1.5) — all of pixel 0 and
  // half of pixel 1 — so it is (2·0 + 1·240)/3 = 80 on the green channel.
  const strip = image(3, 1, [
    [0, 0, 0, 255],
    [0, 240, 0, 255],
    [0, 0, 0, 255],
  ]);
  const r = importSprite(strip, opts({ w: 2, h: 1, fit: "fill", resample: "area" }));
  assert.deepEqual(texel(r.sprite, 0, 0, 0), [0, 80, 0]);
  assert.deepEqual(texel(r.sprite, 0, 1, 0), [0, 80, 0]);
});

test("upscaling with area behaves as nearest rather than smearing", () => {
  const two = image(2, 1, [
    [255, 0, 0, 255],
    [0, 0, 255, 255],
  ]);
  const a = importSprite(two, opts({ w: 4, h: 1, fit: "fill", resample: "area" }));
  const n = importSprite(two, opts({ w: 4, h: 1, fit: "fill", resample: "nearest" }));
  assert.deepEqual(encodeSprite(a.sprite), encodeSprite(n.sprite));
});

// ---- transparency ----

test("alpha below the threshold is index 0, and index 0 only", () => {
  const src = image(4, 1, [
    [9, 9, 9, 0],
    [9, 9, 9, 127],
    [9, 9, 9, 128],
    [9, 9, 9, 255],
  ]);
  const r = importSprite(src, opts({ w: 4, h: 1, fit: "fill", resample: "nearest" }));
  assert.equal(texel(r.sprite, 0, 0, 0), null);
  assert.equal(texel(r.sprite, 0, 1, 0), null, "127 is under the default 128");
  assert.deepEqual(texel(r.sprite, 0, 2, 0), [9, 9, 9]);
  assert.deepEqual(texel(r.sprite, 0, 3, 0), [9, 9, 9]);
  // …and the threshold is a knob
  const loose = importSprite(src, opts({ w: 4, h: 1, fit: "fill", alphaThreshold: 1 }));
  assert.equal(texel(loose.sprite, 0, 0, 0), null, "alpha 0 is never opaque");
  assert.deepEqual(texel(loose.sprite, 0, 1, 0), [9, 9, 9]);
});

test("an opaque BLACK texel is a palette colour, not transparency", () => {
  const r = importSprite(flat(2, 2, [0, 0, 0, 255]), opts({ w: 2, h: 2, fit: "fill" }));
  assert.deepEqual(r.sprite.palette, [[0, 0, 0]]);
  assert.deepEqual([...r.sprite.index], [1, 1, 1, 1]);
});

// ---- quantizing ----

test("at or under the ceiling every colour survives exactly", () => {
  const src = image(4, 1, [
    [1, 2, 3, 255],
    [4, 5, 6, 255],
    [7, 8, 9, 255],
    [1, 2, 3, 255],
  ]);
  const r = importSprite(src, opts({ w: 4, h: 1, fit: "fill", colors: 8 }));
  assert.equal(r.colorsSeen, 3);
  assert.equal(r.colorsUsed, 3, "the duplicate is one palette entry, not two");
  assert.deepEqual(r.sprite.palette, [
    [1, 2, 3],
    [4, 5, 6],
    [7, 8, 9],
  ]);
  assert.deepEqual([...r.sprite.index], [1, 2, 3, 1]);
});

test("median cut reduces to the ceiling and is deterministic", () => {
  // eight distinct greys down to two: the cut is on the widest axis, and each
  // half's representative is its texel-weighted mean
  const greys = Array.from({ length: 8 }, (_, i) => [i * 16, i * 16, i * 16, 255]);
  const src = image(8, 1, greys);
  const r = importSprite(src, opts({ w: 8, h: 1, fit: "fill", colors: 2 }));
  assert.equal(r.colorsSeen, 8);
  assert.equal(r.colorsUsed, 2);
  // 0,16,32,48 → mean 24; 64,80,96,112 → mean 88
  assert.deepEqual(r.sprite.palette, [
    [24, 24, 24],
    [88, 88, 88],
  ]);
  assert.deepEqual([...r.sprite.index], [1, 1, 1, 1, 2, 2, 2, 2]);
  // …and running it again gives the same bytes
  const again = importSprite(src, opts({ w: 8, h: 1, fit: "fill", colors: 2 }));
  assert.deepEqual(encodeSprite(again.sprite), encodeSprite(r.sprite));
});

test("the palette can never exceed the format's 255", () => {
  const many = Array.from({ length: 64 }, (_, i) => [i * 4, 255 - i * 4, (i * 7) % 256, 255]);
  const r = importSprite(image(64, 1, many), opts({ w: 64, h: 1, fit: "fill", colors: 999 }));
  assert.ok(r.sprite.palette.length <= SPRITE_MAX_COLORS);
  assert.equal(checkSprite(encodeSprite(r.sprite)), null);
});

test("nearest palette entry breaks ties towards the lower index", () => {
  const pal = [
    [0, 0, 0],
    [10, 0, 0],
    [0, 0, 0],
  ];
  assert.equal(nearestIndex(pal, 5, 0, 0), 1, "equidistant from 1 and 2 → 1");
  assert.equal(nearestIndex(pal, 10, 0, 0), 2);
  assert.equal(nearestIndex([], 1, 2, 3), 0, "no palette → transparent");
});

test("dithering is off by default and changes the drawing when on", () => {
  // a 16×16 black-to-white gradient forced through TWO colours. Undithered it
  // is two flat bands and every row identical; dithered, the error carried
  // down from the row above makes the rows differ from each other. (A
  // one-row-high image is the wrong test: only 7/16 of the error travels
  // sideways and it damps out, which is exactly what this used to assert.)
  const n = 16;
  const grad = [];
  for (let y = 0; y < n; y++) {
    for (let x = 0; x < n; x++) {
      const v = x * 17;
      grad.push([v, v, v, 255]);
    }
  }
  const src = image(n, n, grad);
  const o = { w: n, h: n, fit: "fill", colors: 2 };
  const plain = importSprite(src, opts(o));
  const dithered = importSprite(src, opts({ ...o, dither: true }));
  assert.equal(plain.sprite.palette.length, 2);
  const row = (s, y) => [...s.index.subarray(y * n, (y + 1) * n)];
  assert.deepEqual(row(plain.sprite, 0), row(plain.sprite, 7), "undithered: every row the same");
  assert.notDeepEqual([...dithered.sprite.index], [...plain.sprite.index]);
  assert.notDeepEqual(row(dithered.sprite, 0), row(dithered.sprite, 7), "dithered: rows differ");
  assert.equal(checkSprite(encodeSprite(dithered.sprite)), null);
  // …and it is reproducible, which is what pins it to the Rust twin
  const twice = importSprite(src, opts({ ...o, dither: true }));
  assert.deepEqual(encodeSprite(twice.sprite), encodeSprite(dithered.sprite));
});

// ---- frames and the frame rate ----

test("a frame delay becomes fps, capped at 30", () => {
  const frames = (n, delayMs) => ({
    w: 1,
    h: 1,
    frames: Array.from({ length: n }, () => ({ rgba: new Uint8Array([1, 2, 3, 255]), delayMs })),
  });
  assert.equal(derivedFps(frames(4, 100), 1, 4), 10);
  assert.equal(derivedFps(frames(4, 40), 1, 4), 25);
  assert.equal(derivedFps(frames(4, 10), 1, 4), 10, "0..10 ms is shown at 100 ms");
  assert.equal(derivedFps(frames(4, 0), 1, 4), 10, "and so is an unstated delay");
  assert.equal(derivedFps(frames(4, 20), 1, 4), 30, "50 fps is capped");
  assert.equal(derivedFps(frames(1, 100), 1, 1), 0, "one frame is still");
  // keeping one frame in two halves the rate
  assert.equal(derivedFps(frames(4, 100), 2, 2), 5);
  assert.equal(normalizedDelay(0), 100);
  assert.equal(normalizedDelay(250), 250);
});

test("keep-every-Nth drops frames and says how many", () => {
  const src = {
    w: 1,
    h: 1,
    frames: Array.from({ length: 9 }, (_, i) => ({
      rgba: new Uint8Array([i * 20, 0, 0, 255]),
      delayMs: 100,
    })),
  };
  assert.deepEqual(keptIndices(9, 1).length, 9);
  assert.deepEqual(keptIndices(9, 3), [0, 3, 6]);
  const r = importSprite(src, opts({ w: 1, h: 1, fit: "fill", keepEvery: 3 }));
  assert.equal(r.sprite.frames, 3);
  assert.equal(r.skippedFrames, 6);
  assert.equal(r.fps, 3, "100 ms × 3 → 3.33 fps → 3");
  // the frames kept are 0, 3 and 6, in order
  assert.deepEqual([...r.sprite.index].map((k) => r.sprite.palette[k - 1][0]), [0, 60, 120]);
});

test("an fps override wins over the delays, and 0 means still", () => {
  const src = {
    w: 1,
    h: 1,
    frames: Array.from({ length: 3 }, () => ({ rgba: new Uint8Array([1, 1, 1, 255]), delayMs: 100 })),
  };
  assert.equal(importSprite(src, opts({ w: 1, h: 1, fpsOverride: 24 })).fps, 24);
  assert.equal(importSprite(src, opts({ w: 1, h: 1, fpsOverride: 0 })).fps, 0);
  assert.equal(importSprite(src, opts({ w: 1, h: 1, fpsOverride: 99 })).fps, 30, "clamped");
});

test("frames past the format's 255 are REPORTED, never silently dropped", () => {
  const src = {
    w: 1,
    h: 1,
    frames: Array.from({ length: 300 }, () => ({ rgba: new Uint8Array([9, 9, 9, 255]), delayMs: 100 })),
  };
  const r = importSprite(src, opts({ w: 1, h: 1, fit: "fill" }));
  assert.equal(r.sprite.frames, 255);
  assert.equal(r.droppedFrames, 45);
  // …and the knob that fixes it really does
  const fixed = importSprite(src, opts({ w: 1, h: 1, fit: "fill", keepEvery: 2 }));
  assert.equal(fixed.droppedFrames, 0);
  assert.equal(fixed.sprite.frames, 150);
});

// ---- the 16 KiB cap ----

test("an over-cap import comes back over the cap, not truncated", () => {
  const src = {
    w: 64,
    h: 64,
    frames: Array.from({ length: 40 }, (_, i) => ({
      rgba: new Uint8Array(64 * 64 * 4).fill(200 + (i % 40)),
      delayMs: 100,
    })),
  };
  const o = opts({ name: "big", w: 64, h: 64, fit: "fill" });
  const r = importSprite(src, o);
  assert.equal(r.overCap, true);
  assert.ok(r.bytes > SPRITE_MAX_BYTES, String(r.bytes));
  // the record is intact and describes itself honestly — nothing was quietly
  // dropped to make it fit
  assert.equal(r.sprite.frames, 40);
  assert.equal(r.sprite.index.length, 64 * 64 * 40);
});

test("fitUnderCap turns the two knobs until the record fits", () => {
  const src = {
    w: 64,
    h: 64,
    frames: Array.from({ length: 40 }, (_, i) => ({
      rgba: new Uint8Array(64 * 64 * 4).fill(200 + (i % 40)),
      delayMs: 100,
    })),
  };
  const o = opts({ name: "big", w: 64, h: 64, fit: "fill" });
  const fix = fitUnderCap(src, o);
  assert.notEqual(fix.said, "", "it says what it changed");
  const r = importSprite(src, fix.options);
  assert.equal(r.overCap, false, `${r.bytes} B: ${fix.said}`);
  assert.ok(r.bytes <= SPRITE_MAX_BYTES);
  // an import that already fits is left completely alone
  const small = fitUnderCap(flat(4, 4, [1, 2, 3, 255]), opts({ w: 4, h: 4 }));
  assert.equal(small.said, "");
});

test("fitUnderCap shrinks the size when one frame alone will not fit", () => {
  const src = {
    w: 200,
    h: 200,
    frames: Array.from({ length: 6 }, () => ({
      rgba: new Uint8Array(200 * 200 * 4).fill(180),
      delayMs: 100,
    })),
  };
  const o = opts({ name: "huge", w: 64, h: 64, fit: "fill", colors: 255 });
  const fix = fitUnderCap(src, o);
  const r = importSprite(src, fix.options);
  assert.equal(r.overCap, false, `${r.bytes} B after: ${fix.said}`);
});

test("plannedBytes matches the record the pipeline actually writes", () => {
  const o = opts({ name: "abc", w: 5, h: 7, fit: "fill", colors: 4 });
  const src = image(
    5 * 7,
    1,
    Array.from({ length: 35 }, (_, i) => [i * 7, 255 - i * 7, 128, 255]),
  );
  const r = importSprite({ ...src, w: 5, h: 7 }, o);
  assert.equal(plannedBytes(o, 1), r.bytes);
});

// ---- the defaults ----

test("the default target scales the long edge to 64, or to a smaller panel", () => {
  assert.deepEqual(defaultTargetSize(320, 240), { w: 64, h: 48 });
  assert.deepEqual(defaultTargetSize(240, 320), { w: 48, h: 64 });
  assert.deepEqual(defaultTargetSize(16, 16), { w: 16, h: 16 }, "already small: left alone");
  assert.deepEqual(defaultTargetSize(320, 240, { w: 32, h: 32 }), { w: 32, h: 24 });
  assert.deepEqual(defaultTargetSize(320, 240, { w: 128, h: 128 }), { w: 64, h: 48 }, "64 is the cap");
  assert.deepEqual(defaultTargetSize(1, 4000), { w: 1, h: 64 }, "never zero");
});

test("the default options read the source and nothing else", () => {
  const big = defaultImportOptions({ w: 400, h: 200, frames: [] }, "photo.jpg");
  assert.equal(big.resample, "area", "a big image is treated as a photo");
  assert.equal(big.name, "photo");
  assert.equal(big.dither, false, "dithering is off by default");
  assert.equal(big.fit, "fit");
  assert.equal(big.colors, SPRITE_MAX_COLORS);
  const small = defaultImportOptions({ w: 16, h: 16, frames: [] }, "heart.png");
  assert.equal(small.resample, "nearest", "texel-sized art keeps its pixels");
});

// ---- the export half ----

test("a sprite sheet is its frames in one row, transparency kept", () => {
  const r = importSprite(
    {
      w: 2,
      h: 1,
      frames: [
        { rgba: new Uint8Array([255, 0, 0, 255, 0, 0, 0, 0]), delayMs: 100 },
        { rgba: new Uint8Array([0, 0, 255, 255, 0, 0, 0, 0]), delayMs: 100 },
      ],
    },
    opts({ name: "Pair", w: 2, h: 1, fit: "fill" }),
  );
  const sheet = sheetPixels(r.sprite);
  assert.equal(sheet.w, 4, "2 texels × 2 frames");
  assert.equal(sheet.h, 1);
  assert.deepEqual([...sheet.rgba.slice(0, 4)], [255, 0, 0, 255]);
  assert.deepEqual([...sheet.rgba.slice(4, 8)], [0, 0, 0, 0], "transparent stays transparent");
  assert.deepEqual([...sheet.rgba.slice(8, 12)], [0, 0, 255, 255], "frame 1 starts at x = 2");
  assert.equal(sheetFileName(r.sprite), "Pair-2x1x2.png");
  assert.equal(sheetFileName({ ...r.sprite, name: "a/b c!" }), "ab-c-2x1x2.png");
});

// ---- the goldens the Rust pipeline is pinned to ----

for (const name of ["ramp", "blob", "spin", "spin-half"]) {
  test(`fixture ${name}: the TS pipeline writes the checked-in record`, () => {
    const meta = JSON.parse(readFileSync(join(FIX, `${name}.json`), "utf8"));
    const raw = new Uint8Array(readFileSync(join(FIX, `${name}.rgba`)));
    const per = meta.w * meta.h * 4;
    assert.equal(raw.length, per * meta.frames, "the sidecar is every frame's RGBA");
    const src = {
      w: meta.w,
      h: meta.h,
      frames: Array.from({ length: meta.frames }, (_, i) => ({
        rgba: raw.subarray(i * per, (i + 1) * per),
        delayMs: meta.delaysMs[i],
      })),
    };
    const got = encodeSprite(importSprite(src, meta.options).sprite);
    const want = new Uint8Array(readFileSync(join(FIX, `${name}.lxsp`)));
    assert.equal(checkSprite(want), null, "the golden is itself a valid record");
    assert.deepEqual([...got], [...want], `${name}.lxsp`);
  });
}
