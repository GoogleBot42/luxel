// The scene editor's design-size helpers (src/lib/scenePlace.ts, Gitea #997)
// — the Layout chooser, the cells-per-design-unit scale a marquee drag
// divides by, the `×2 on this panel` hint and the `Preview at` sizes.
//
// The scale and zoom are also checked against the REAL compositor's
// `lx_comp_layer_rect` (skipped without a built wasm), because the editor's
// drag and its hint must agree with where the engine draws.
//
// Run: `npm test` from web/.
import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  currentChoice,
  designScale,
  designZoom,
  layoutChoices,
  mapRect,
  previewSizes,
  zoomHint,
} from "../src/lib/scenePlace.ts";

test("layout choices: Pixels / Fit 32 / Fit 64 / This panel on a panel", () => {
  const c = layoutChoices({ w: 128, h: 128 });
  assert.deepEqual(
    c.map((x) => [x.id, x.label, x.space]),
    [
      ["px", "Pixels", null],
      ["32", "Fit 32", [32, 32]],
      ["64", "Fit 64", [64, 64]],
      ["panel", "This panel", [128, 128]],
    ],
  );
});

test("layout choices: a strip offers Fit 150 and This strip instead of 32/64", () => {
  const c = layoutChoices({ w: 300, h: 1 });
  assert.deepEqual(
    c.map((x) => [x.id, x.label, x.space]),
    [
      ["px", "Pixels", null],
      ["150", "Fit 150", [150, 1]],
      ["panel", "This strip", [300, 1]],
    ],
  );
  assert.deepEqual(layoutChoices(null).map((x) => x.id), ["px"]);
});

test("current choice: first match wins, an unknown U is custom", () => {
  const c = layoutChoices({ w: 64, h: 64 });
  assert.equal(currentChoice(undefined, c), "px");
  assert.equal(currentChoice([32, 32], c), "32");
  // `This panel` writes the same U 64 64 here; the named size lights up
  assert.equal(currentChoice([64, 64], c), "64");
  assert.equal(currentChoice([48, 40], c), "custom");
  assert.equal(currentChoice([128, 128], layoutChoices({ w: 128, h: 128 })), "panel");
});

test("design scale: identity without U and on X abs", () => {
  assert.deepEqual(designScale(undefined, { w: 64, h: 64 }), { x: 1, y: 1 });
  assert.deepEqual(designScale([32, 32], { w: 64, h: 64 }, true), { x: 1, y: 1 });
});

test("design scale: whole-number zoom on a panel, rational below it, x only on a strip", () => {
  assert.deepEqual(designScale([32, 32], { w: 64, h: 64 }), { x: 2, y: 2 });
  assert.deepEqual(designScale([32, 32], { w: 128, h: 128 }), { x: 4, y: 4 });
  // 64x32: z0 = min(2, 1) = 1 (design example B's letterbox)
  assert.deepEqual(designScale([32, 32], { w: 64, h: 32 }), { x: 1, y: 1 });
  // below the design size: f = min(16/32, 16/32) = 0.5
  assert.deepEqual(designScale([32, 32], { w: 16, h: 16 }), { x: 0.5, y: 0.5 });
  // strip regime: proportional along it
  assert.deepEqual(designScale([150, 1], { w: 300, h: 1 }), { x: 2, y: 1 });
  assert.deepEqual(designScale([150, 1], { w: 61, h: 1 }), { x: 61 / 150, y: 1 });
  assert.equal(designZoom([32, 32], { w: 128, h: 128 }), 4);
  assert.equal(designZoom([32, 32], { w: 16, h: 16 }), 1);
  assert.equal(designZoom([150, 1], { w: 300, h: 1 }), 2);
});

test("zoom hint", () => {
  const here = "on this panel";
  assert.equal(zoomHint(undefined, { w: 64, h: 64 }, 1, false, here), "");
  assert.equal(zoomHint([32, 32], { w: 64, h: 64 }, 2, false, here), "×2 on this panel");
  assert.equal(zoomHint([32, 32], { w: 128, h: 128 }, 4, false, "at 128×128"), "×4 at 128×128");
  assert.equal(zoomHint([64, 64], { w: 64, h: 64 }, 1, false, here), "1:1 on this panel");
  assert.equal(zoomHint([32, 32], { w: 16, h: 16 }, 1, false, here), "shrunk to fit on this panel");
  assert.equal(zoomHint([32, 32], { w: 64, h: 64 }, 1, true, here), "grid pixels, 1:1");
});

test("preview sizes: the device first, no duplicate, 150x1 only where it applies", () => {
  assert.deepEqual(
    previewSizes({ w: 64, h: 64 }, [32, 32]).map((p) => p.id),
    ["", "32x32", "128x128"],
  );
  assert.deepEqual(
    previewSizes({ w: 64, h: 64 }, [150, 1]).map((p) => p.id),
    ["", "32x32", "128x128", "150x1"],
  );
  const strip = previewSizes({ w: 300, h: 1 }, undefined);
  assert.equal(strip[0].label, "at strip");
  assert.deepEqual(strip.map((p) => p.id), ["", "32x32", "64x64", "128x128", "150x1"]);
  assert.equal(previewSizes({ w: 150, h: 1 }, undefined).some((p) => p.id === "150x1"), false);
});

// ---- against the real compositor ----

const WASM = fileURLToPath(new URL("../public/luxel.wasm", import.meta.url));

async function load() {
  if (!existsSync(WASM)) return null;
  const { instance } = await WebAssembly.instantiate(readFileSync(WASM), {});
  const e = instance.exports;
  if (typeof e.lx_comp_layer_rect !== "function") return null;
  const setScene = (ch, wire) => {
    const b = new TextEncoder().encode(wire);
    const ptr = e.lx_alloc(b.length);
    new Uint8Array(e.memory.buffer).set(b, ptr);
    const rc = e.lx_comp_set(ch, ptr, b.length);
    e.lx_dealloc(ptr, b.length);
    return rc;
  };
  const rect = (ch, i) => {
    const p = e.lx_comp_layer_rect(ch, i);
    if (p === 0) return null;
    const [x, y, w, h, zoom] = new Int32Array(e.memory.buffer.slice(p, p + 20));
    return { x, y, w, h, zoom };
  };
  return { e, setScene, rect };
}

test("the scale a drag divides by is the engine's: a 1-unit move is f cells", async () => {
  const w = await load();
  if (!w) return void console.log("skipped: web/public/luxel.wasm not built (or predates lx_comp_layer_rect)");
  for (const [gw, gh, space] of [
    [64, 64, [32, 32]],
    [128, 128, [32, 32]],
    [64, 32, [32, 32]],
    [300, 1, [150, 1]],
  ]) {
    const ch = w.e.lx_comp_new(gw, gh);
    const block = (x) =>
      `S - t\nU ${space[0]} ${space[1]}\nL color ${x} 0 4 ${gh === 1 ? 0 : 4} normal 100 none fill 1\nK ff0000\n` +
      `L color ${x} 0 4 ${gh === 1 ? 0 : 4} normal 100 none fill 1\nK 00ff00\nX abs\n`;
    assert.equal(w.setScene(ch, block(4)), 0);
    const a = w.rect(ch, 0);
    const pinned = w.rect(ch, 1);
    assert.equal(w.setScene(ch, block(5)), 0);
    const b = w.rect(ch, 0);
    const f = designScale(space, { w: gw, h: gh });
    assert.equal(b.x - a.x, f.x, `${gw}x${gh}: one design unit`);
    assert.equal(a.zoom, designZoom(space, { w: gw, h: gh }), `${gw}x${gh}: zoom`);
    // `X abs` keeps grid pixels and zoom 1 — what the Pin toggle writes
    assert.equal(pinned.x, 4);
    assert.equal(pinned.zoom, 1);
    w.e.lx_comp_free(ch);
  }
});

test("mapRect is the engine's layer_rect for every box shape (no track)", async () => {
  const w = await load();
  if (!w) return void console.log("skipped: web/public/luxel.wasm not built (or predates lx_comp_layer_rect)");
  const boxes = [
    [0, 0, 0, 0],
    [8, 0, 16, 28],
    [0, 30, 0, 0],
    [-1, 12, 0, 0],
    [5, 7, 0, 3],
    [3, -2, 9, 0],
    [31, 31, 4, 4],
  ];
  for (const [gw, gh, space] of [
    [64, 64, [32, 32]],
    [128, 128, [32, 32]],
    [64, 32, [32, 32]],
    [16, 16, [32, 32]],
    [48, 30, [64, 64]],
    [300, 1, [150, 1]],
    [61, 1, [150, 1]],
    [64, 64, undefined],
  ]) {
    const ch = w.e.lx_comp_new(gw, gh);
    let wire = `S - t\n${space ? `U ${space[0]} ${space[1]}\n` : ""}`;
    for (const [x, y, bw, bh] of boxes) wire += `L color ${x} ${y} ${bw} ${bh} normal 100 none fill 1\nK ff0000\n`;
    for (const [x, y, bw, bh] of boxes) wire += `L color ${x} ${y} ${bw} ${bh} normal 100 none fill 1\nK ff0000\nX abs\n`;
    assert.equal(w.setScene(ch, wire), 0, wire);
    [false, true].forEach((abs, half) =>
      boxes.forEach(([x, y, bw, bh], i) => {
        const r = w.rect(ch, half * boxes.length + i);
        const got = mapRect({ x, y, w: bw, h: bh }, space, { w: gw, h: gh }, abs, null);
        assert.deepEqual(got, { x: r.x, y: r.y, w: r.w, h: r.h }, `${gw}x${gh} U ${space} abs=${abs} box ${[x, y, bw, bh]}`);
      }),
    );
    w.e.lx_comp_free(ch);
  }
});
