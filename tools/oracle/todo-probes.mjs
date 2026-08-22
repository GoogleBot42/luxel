// One-shot probe battery for the remaining TODO(oracle) markers (2026-08-22):
//   - method-form a.replace(...): does it bind to arrayReplace (write from 0)
//     or arrayReplaceAt (offset form)? Does global arrayReplaceAt exist?
//   - transform stack cap (we cap at 31 — verify the number and the failure
//     mode: silent cap vs abort)
//   - rotateX/rotateY/rotateZ sign conventions on 3D map coords
//   - null / undefined as runtime values (compiler accepts them; both 0?)
//   - clock builtins vs the device's configured timezone (sanity only — the
//     "no time source" case is untestable on a configured device)
//   - paint() with no palette installed, palette persistence across
//     live-code reloads, and lookup outside the first/last stop positions
//   - arity recon for the perlin family + friends via PB's own compiler
//     (extracted locally — no device round-trip needed)
//
// Self-judging: each probe prints a verdict. Luxel-side equivalents are
// pinned by unit tests, not diffed here (the oracle's installed map differs
// from a mapless CLI run, so raw diffs would be spurious for map-dependent
// probes).
//
// Usage (repo root, `nix develop`):  node tools/oracle/todo-probes.mjs <ip>

import { buildCompiler, packBytecode } from "./compiler.mjs";
import { PB, sleep } from "./pb.mjs";

const ip = process.argv[2] ?? "192.168.0.140";
const SENTINEL = 42;

const near = (a, b, tol = 0.02) => Math.abs(a - b) <= tol;

async function vars(pb, compile, source, keys) {
  const compiled = compile(source);
  if (!compiled.ok) return { ok: false, error: compiled.error };
  await pb.setCode(packBytecode(compiled));
  for (let attempt = 0; attempt < 10; attempt++) {
    await sleep(300);
    const v = await pb.getVars();
    if (keys.some((k) => v[k] !== undefined) && v.sent === SENTINEL) {
      return { ok: true, vars: v };
    }
    if (attempt >= 5 && keys.some((k) => v[k] !== undefined)) {
      // vars arrived but sentinel never landed → init/callback aborted
      return { ok: true, aborted: true, vars: v };
    }
  }
  return { ok: false, error: "vars never appeared" };
}

async function frame(pb, compile, source, settle = 700) {
  const compiled = compile(source);
  if (!compiled.ok) return { ok: false, error: compiled.error };
  await pb.setCode(packBytecode(compiled));
  return { ok: true, px: await pb.getPreviewFrame(settle) };
}

const rgbAt = (px, i) => [px[i * 3], px[i * 3 + 1], px[i * 3 + 2]];

async function main() {
  console.error(`fetching web UI from ${ip}…`);
  const webUI = await (await fetch(`http://${ip}/`)).text();
  const compile = buildCompiler(webUI);
  console.error("compiler extracted OK");

  // ---------- arity recon (local compiler only) ----------
  console.log("=== arity recon (PB compiler, local — 'ok' = compiles) ===");
  const FNS = [
    "perlin", "perlinFbm", "perlinRidge", "perlinTurbulence",
    "setPerlinWrap", "arrayReplaceAt", "rotateX", "translate3D",
  ];
  for (const fn of FNS) {
    const oks = [];
    for (let n = 0; n <= 8; n++) {
      const call = `${fn}(${Array.from({ length: n }, (_, i) => i + 1).join(", ")})`;
      const src = `export function render(i) { hsv(0,0,0) }\nx = ${call}\n`;
      const c = compile(src);
      if (c.ok) oks.push(n);
      else if (n === 0 && /ndefined symbol/i.test(c.error ?? "")) {
        oks.push(`ABSENT: ${c.error.trim()}`);
        break;
      }
    }
    console.log(`  ${fn.padEnd(18)} arities: ${JSON.stringify(oks)}`);
  }

  const pb = await PB.connect(ip);
  const { settings, seq } = await pb.getConfig();
  const restoreId = seq.activeProgram?.activeProgramId;
  console.error(
    `device: ${settings.name} fw ${settings.ver}, ${settings.pixelCount} px, tz=${settings.timezone}, active="${seq.activeProgram?.name}"`,
  );

  try {
    // ---------- palette A: no setPalette, straight paint ramp ----------
    // FIRST device probe on purpose: nothing in this session has set a
    // palette yet, so this reads whatever state live-coding starts with.
    const PAINT_N = 8;
    const paintRamp = `export function render(index) {
  if (index < ${PAINT_N}) paint(index / ${PAINT_N})
  else rgb(0, 0, 0)
}
`;
    console.log("=== paint() with no setPalette (fresh live-code) ===");
    const palA = await frame(pb, compile, paintRamp);
    if (!palA.ok) console.log(`  compile error: ${palA.error}`);
    else {
      for (let i = 0; i < PAINT_N; i++) {
        console.log(`  paint(${(i / PAINT_N).toFixed(3)}) -> ${rgbAt(palA.px, i)}`);
      }
      const gray = Array.from({ length: PAINT_N }, (_, i) => rgbAt(palA.px, i))
        .every(([r, g, b]) => r === g && g === b);
      console.log(`  verdict: ${gray ? "grayscale ramp (matches Luxel)" : "NOT grayscale — see bytes"}`);
    }

    // ---------- palette B: set an all-red palette (leak marker) ----------
    const redSrc = `var pal = [0, 1,0,0,  1, 1,0,0]
export function beforeRender(delta) { setPalette(pal) }
${paintRamp}`;
    const palB = await frame(pb, compile, redSrc);
    console.log("=== all-red palette set (marker for persistence) ===");
    if (palB.ok) console.log(`  paint(0.5) -> ${rgbAt(palB.px, 4)} (expect red)`);
    else console.log(`  compile error: ${palB.error}`);

    // ---------- palette C: repeat A — does B's palette leak? ----------
    console.log("=== paint() with no setPalette AGAIN (after red palette) ===");
    const palC = await frame(pb, compile, paintRamp);
    if (palC.ok) {
      for (let i = 0; i < PAINT_N; i++) {
        console.log(`  paint(${(i / PAINT_N).toFixed(3)}) -> ${rgbAt(palC.px, i)}`);
      }
      const same = palA.ok &&
        Array.from({ length: PAINT_N * 3 }, (_, i) => i).every((i) => palA.px[i] === palC.px[i]);
      console.log(`  verdict: ${same ? "identical to first run — palette does NOT persist across loads"
        : "DIFFERS from first run — palette state leaks across live-code reloads"}`);
    }

    // ---------- palette D: stops not spanning 0..1 (clamp vs wrap) ----------
    const partialSrc = `var pal = [0.25, 0,0,1,  0.75, 0,1,0]
var vals = [0, 0.1, 0.2, 0.25, 0.5, 0.75, 0.8, 0.9, 0.999]
export function beforeRender(delta) { setPalette(pal) }
export function render(index) {
  if (index < 9) paint(vals[index])
  else rgb(0, 0, 0)
}
`;
    console.log("=== palette stops at 0.25(blue)/0.75(green): outside-lookup ===");
    const palD = await frame(pb, compile, partialSrc);
    if (!palD.ok) console.log(`  compile error: ${palD.error}`);
    else {
      const vals = [0, 0.1, 0.2, 0.25, 0.5, 0.75, 0.8, 0.9, 0.999];
      vals.forEach((v, i) => console.log(`  paint(${v}) -> ${rgbAt(palD.px, i)}`));
      const below = rgbAt(palD.px, 1); // 0.1
      const above = rgbAt(palD.px, 7); // 0.9
      const clampB = below[2] > 200 && below[1] < 30;
      const clampA = above[1] > 200 && above[2] < 30;
      console.log(`  verdict: below-first ${clampB ? "clamps to first stop" : "does NOT clamp (blend?)"}, ` +
        `above-last ${clampA ? "clamps to last stop" : "does NOT clamp (blend?)"}`);
    }

    // ---------- palette E: fine structure just past the last stop ----------
    // First run showed above-last renders BLACK (not a clamp, not a wrap
    // blend). Pin down the edge and whether a single-stop palette behaves
    // the same on both sides.
    const fineSrc = `var pal = [0.25, 0,0,1,  0.75, 0,1,0]
var vals = [0.75, 0.7500152587890625, 0.7501, 0.76, 0.85, 0.98, 0.9999847412109375]
export function beforeRender(delta) { setPalette(pal) }
export function render(index) {
  if (index < 7) paint(vals[index])
  else rgb(0, 0, 0)
}
`;
    console.log("=== fine probe just past last stop (0.75) ===");
    const palE = await frame(pb, compile, fineSrc);
    if (!palE.ok) console.log(`  compile error: ${palE.error}`);
    else {
      const vals = ["0.75", "0.75+1raw", "0.7501", "0.76", "0.85", "0.98", "1-1raw"];
      vals.forEach((v, i) => console.log(`  paint(${v}) -> ${rgbAt(palE.px, i)}`));
    }

    const singleSrc = `var pal = [0.5, 1,0,0]
var vals = [0, 0.25, 0.5, 0.51, 0.75, 0.999]
export function beforeRender(delta) { setPalette(pal) }
export function render(index) {
  if (index < 6) paint(vals[index])
  else rgb(0, 0, 0)
}
`;
    console.log("=== single-stop palette [0.5 -> red] ===");
    const palF = await frame(pb, compile, singleSrc);
    if (!palF.ok) console.log(`  compile error: ${palF.error}`);
    else {
      const vals = [0, 0.25, 0.5, 0.51, 0.75, 0.999];
      vals.forEach((v, i) => console.log(`  paint(${v}) -> ${rgbAt(palF.px, i)}`));
    }

    // ---------- method-form replace: from-0 or offset? ----------
    const repSrc = `a = array(4)
a.replace(2, 9)
export var r0 = a[0]
export var r1 = a[1]
export var r2 = a[2]
export var r3 = a[3]
export var sent = ${SENTINEL}
export function render(index) { hsv(0, 0, 0) }
`;
    console.log("=== a.replace(2, 9) on [0,0,0,0] ===");
    const rep = await vars(pb, compile, repSrc, ["r0"]);
    if (!rep.ok) console.log(`  error: ${rep.error}`);
    else if (rep.aborted) console.log("  ABORTED init (sentinel missing) — method rejected at runtime?");
    else {
      const got = [rep.vars.r0, rep.vars.r1, rep.vars.r2, rep.vars.r3];
      console.log(`  result: [${got}]`);
      if (got[0] === 2 && got[1] === 9) console.log("  verdict: writes from index 0 (= arrayReplace, matches Luxel)");
      else if (got[2] === 9 && got[0] === 0) console.log("  verdict: OFFSET form (= arrayReplaceAt) — Luxel method binding is WRONG");
      else console.log("  verdict: neither expected shape — investigate");
    }

    // ---------- global arrayReplaceAt on-device (if it compiled above) ----------
    const repAtSrc = `b = array(4)
arrayReplaceAt(b, 1, 7, 8)
export var s0 = b[0]
export var s1 = b[1]
export var s2 = b[2]
export var s3 = b[3]
export var sent = ${SENTINEL}
export function render(index) { hsv(0, 0, 0) }
`;
    console.log("=== arrayReplaceAt(b, 1, 7, 8) on [0,0,0,0] ===");
    if (!compile(repAtSrc).ok) {
      console.log(`  PB compile error: ${compile(repAtSrc).error} (builtin absent on PB)`);
    } else {
      const repAt = await vars(pb, compile, repAtSrc, ["s0"]);
      if (!repAt.ok) console.log(`  error: ${repAt.error}`);
      else if (repAt.aborted) console.log("  ABORTED init — runtime rejection");
      else console.log(`  result: [${[repAt.vars.s0, repAt.vars.s1, repAt.vars.s2, repAt.vars.s3]}] ` +
        `(Luxel gives [0,7,8,0])`);
    }

    // ---------- transform stack cap ----------
    const capSrc = `export var xs = array(40)
export var xbase = -99
export var done = -99
export var sent = -99
var ran = 0
export function beforeRender(delta) {
  if (ran == 1) { return 0 }
  ran = 1
  resetTransform()
  mapPixels((i, x, y, z) => { if (i == 0) xbase = x })
  for (k = 0; k < 40; k++) {
    translate(0.01, 0)
    mapPixels((i, x, y, z) => { if (i == 0) xs[k] = x })
  }
  done = 1
  sent = ${SENTINEL}
}
export function render(index) { hsv(0, 0, 0) }
`;
    console.log("=== transform stack cap (40 stacked translates) ===");
    const cap = await vars(pb, compile, capSrc, ["xbase"]);
    if (!cap.ok) console.log(`  error: ${cap.error}`);
    else {
      const xs = cap.vars.xs ?? [];
      const xbase = cap.vars.xbase;
      const steps = xs.map((x, k) => ({ k: k + 1, dx: +(x - xbase).toFixed(4) }));
      const stalled = steps.findIndex((s, i) => i > 0 && s.dx === steps[i - 1].dx);
      console.log(`  aborted=${cap.aborted === true}, done=${cap.vars.done}, xbase=${xbase}`);
      console.log(`  dx by depth: ${steps.map((s) => s.dx).join(" ")}`);
      if (cap.aborted || cap.vars.done !== 1) {
        const filled = xs.filter((x) => x !== 0).length;
        console.log(`  verdict: ABORTS at depth ~${filled + 1} (Luxel errors past 31)`);
      } else if (stalled >= 0) {
        console.log(`  verdict: silently CAPS at depth ${stalled} — Luxel aborts instead`);
      } else {
        console.log("  verdict: no cap observed up to 40 — Luxel's 31 cap is too strict");
      }
    }

    // ---------- rotateX/Y/Z direction on 3D map coords ----------
    const rotSrc = `export var bx=-99, by=-99, bz=-99
export var x1=-99, y1=-99, z1=-99
export var x2=-99, y2=-99, z2=-99
export var x3=-99, y3=-99, z3=-99
export var sent = -99
var h = PI / 2
var ran = 0
export function beforeRender(delta) {
  if (ran == 1) { return 0 }
  ran = 1
  resetTransform()
  mapPixels((i, x, y, z) => { if (i == 0) {
    bx = x
    by = y
    bz = z
  } })
  resetTransform()
  rotateX(h)
  mapPixels((i, x, y, z) => { if (i == 0) {
    x1 = x
    y1 = y
    z1 = z
  } })
  resetTransform()
  rotateY(h)
  mapPixels((i, x, y, z) => { if (i == 0) {
    x2 = x
    y2 = y
    z2 = z
  } })
  resetTransform()
  rotateZ(h)
  mapPixels((i, x, y, z) => { if (i == 0) {
    x3 = x
    y3 = y
    z3 = z
  } })
  resetTransform()
  sent = ${SENTINEL}
}
export function render(index) { hsv(0, 0, 0) }
`;
    console.log("=== rotateX/Y/Z(PI/2) on pixel 0's 3D map coords ===");
    const rot = await vars(pb, compile, rotSrc, ["bx"]);
    if (!rot.ok) console.log(`  error: ${rot.error}`);
    else if (rot.aborted) console.log("  ABORTED (3D transform unsupported?)");
    else {
      const v = rot.vars;
      const b = [v.bx, v.by, v.bz];
      console.log(`  base=(${b.map((n) => n.toFixed(4))})`);
      const conventions = {
        // CCW looking from +axis toward origin (right-handed, Luxel's):
        "X ccw": [b[0], -b[2], b[1]], "X cw": [b[0], b[2], -b[1]],
        "Y ccw": [b[2], b[1], -b[0]], "Y cw": [-b[2], b[1], b[0]],
        "Z ccw": [-b[1], b[0], b[2]], "Z cw": [b[1], -b[0], b[2]],
      };
      const got = { X: [v.x1, v.y1, v.z1], Y: [v.x2, v.y2, v.z2], Z: [v.x3, v.y3, v.z3] };
      for (const axis of ["X", "Y", "Z"]) {
        const g = got[axis];
        console.log(`  rotate${axis}(PI/2) -> (${g.map((n) => n.toFixed(4))})`);
        for (const dir of ["ccw", "cw"]) {
          const exp = conventions[`${axis} ${dir}`];
          if (exp.every((e, i) => near(e, g[i]))) {
            console.log(`    verdict: ${dir.toUpperCase()} for +angle (right-handed ${dir === "ccw" ? "— matches Luxel" : "— Luxel sign is WRONG"})`);
          }
        }
      }
    }

    // ---------- null / undefined runtime values ----------
    // First run: `undefined` is NOT a PB symbol ("Undefined symbol
    // undefined") — Luxel's predefined is a documented superset. Probe null
    // alone for its runtime value.
    const nullSrc = `export var o_null = null + 1
export var o_nullmul = null * 5 + 3
export var sent = ${SENTINEL}
export function render(index) { hsv(0, 0, 0) }
`;
    console.log("=== null at runtime (undefined: compile-rejected, see recon) ===");
    const undefC = compile("export function render(i) { hsv(0,0,0) }\nx = undefined\n");
    console.log(`  undefined compile: ${undefC.ok ? "ACCEPTED?!" : undefC.error.trim()}`);
    const nu = await vars(pb, compile, nullSrc, ["o_null"]);
    if (!nu.ok) console.log(`  error: ${nu.error}`);
    else console.log(`  null+1=${nu.vars.o_null} null*5+3=${nu.vars.o_nullmul} ` +
      `(Luxel: 1, 3) ${nu.vars.o_null === 1 && nu.vars.o_nullmul === 3 ? "MATCH" : "DIFF"}`);

    // ---------- clock sanity vs configured tz ----------
    const clkSrc = `export var cy = clockYear()
export var cmo = clockMonth()
export var cd = clockDay()
export var ch = clockHour()
export var cmi = clockMinute()
export var cw = clockWeekday()
export var sent = ${SENTINEL}
export function render(index) { hsv(0, 0, 0) }
`;
    console.log(`=== clock builtins (device tz=${settings.timezone}) ===`);
    const clk = await vars(pb, compile, clkSrc, ["cy"]);
    if (!clk.ok) console.log(`  error: ${clk.error}`);
    else {
      const v = clk.vars;
      console.log(`  device: ${v.cy}-${v.cmo}-${v.cd} ${v.ch}:${String(v.cmi).padStart(2, "0")} weekday=${v.cw}`);
      const now = new Date().toLocaleString("en-US", { timeZone: settings.timezone || "UTC" });
      console.log(`  host in that tz: ${now}`);
    }
  } finally {
    if (restoreId) await pb.setActivePattern(restoreId).catch(() => {});
    await pb.close();
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
