// One-shot probe battery for runtime-error blast radius (Gitea #84, 2026-08-22):
//
// docs/research/04-oracle-findings.md already pins that OOB array access
// "aborts execution" — but corpus patterns that OOB every frame (Nano
// Orbital, Orv - Christmas Tree) visibly WORK on a real PB, so the abort
// cannot kill the whole frame pipeline the way Luxel's engine currently
// does (engine.rs drive(): any Err ends the frame; a beforeRender error
// means render never runs, so a frame-0 error renders black forever).
//
// Probes:
//   P1  array(3.2).length — 3 or 4? (decides whether Orv's star loop OOBs
//       on PB at fractional numStars, or is quietly in-bounds)
//   P2  OOB *read* aborts beforeRender mid-body: does the per-pixel render
//       pass still run that frame? Do writes made before the abort stick?
//   P2w same with an OOB *write* (Nano Orbital's case)
//   P3  error inside render(index) for ONE pixel: do later pixels still
//       render? Does an hsv() issued before the error stick for that pixel?
//
// Self-judging: each probe prints a verdict. Live-code only; nothing is
// saved to the device.
//
// Usage (repo root, `nix develop`):  node tools/oracle/oob-probes.mjs <ip>

import { buildCompiler, packBytecode } from "./compiler.mjs";
import { PB, sleep } from "./pb.mjs";

const ip = process.argv[2] ?? "192.168.0.140";
const SENTINEL = 42;

async function vars(pb, compile, source, keys) {
  const compiled = compile(source);
  if (!compiled.ok) return { ok: false, error: compiled.error };
  await pb.setCode(packBytecode(compiled));
  for (let attempt = 0; attempt < 10; attempt++) {
    await sleep(300);
    const v = await pb.getVars();
    if (keys.some((k) => v[k] !== undefined)) return { ok: true, vars: v };
  }
  return { ok: false, error: "vars never appeared" };
}

async function frame(pb, compile, source, settle = 900) {
  const compiled = compile(source);
  if (!compiled.ok) return { ok: false, error: compiled.error };
  await pb.setCode(packBytecode(compiled));
  return { ok: true, px: await pb.getPreviewFrame(settle) };
}

const rgbAt = (px, i) => [px[i * 3], px[i * 3 + 1], px[i * 3 + 2]];
const isGreenish = ([r, g, b]) => g > 100 && r < 80 && b < 80;
const isReddish = ([r, g, b]) => r > 100 && g < 80 && b < 80;
const isBlack = ([r, g, b]) => r < 10 && g < 10 && b < 10;

async function main() {
  console.error(`fetching web UI from ${ip}…`);
  const webUI = await (await fetch(`http://${ip}/`)).text();
  const compile = buildCompiler(webUI);
  const pb = await PB.connect(ip);
  try {
    // P1: fractional array() length
    {
      const r = await vars(
        pb,
        compile,
        `a = array(3.2)
export var n, sent
export function beforeRender(delta) { n = a.length; sent = ${SENTINEL} }
export function render(index) { hsv(0, 0, 0) }`,
        ["n"],
      );
      if (!r.ok) console.log(`P1 array(3.2).length: PROBE FAILED (${r.error})`);
      else
        console.log(
          `P1 array(3.2).length = ${r.vars.n} (sent=${r.vars.sent}) → ` +
            (r.vars.n === 3 ? "truncates like Luxel" : "DIVERGES from Luxel's 3"),
        );
    }

    // P2: OOB read aborts beforeRender — does render still run? do earlier
    // writes stick? `before` is written pre-abort, `after` post-abort.
    {
      const r = await vars(
        pb,
        compile,
        `a = array(3)
export var before, after, sent
export function beforeRender(delta) {
  sent = ${SENTINEL}
  before = 7
  x = a[5]
  after = 7
}
export function render(index) { hsv(0.33, 1, 1) }`,
        ["before"],
      );
      if (!r.ok) {
        console.log(`P2 vars: PROBE FAILED (${r.error})`);
      } else {
        const aborted = r.vars.before === 7 && r.vars.after !== 7;
        console.log(
          `P2 beforeRender OOB-read: before=${r.vars.before} after=${r.vars.after} → ` +
            (aborted
              ? "aborts mid-body, earlier writes stick"
              : r.vars.after === 7
                ? "NO abort?! (contradicts 04-oracle-findings)"
                : "unexpected var state"),
        );
        const px = await pb.getPreviewFrame(900);
        const p0 = rgbAt(px, 0);
        console.log(
          `P2 render pass after beforeRender abort: pixel0=[${p0}] → ` +
            (isGreenish(p0)
              ? "render STILL RUNS (Luxel currently skips it — the #84 gap)"
              : isBlack(p0)
                ? "render skipped (matches current Luxel; #84 needs another cause)"
                : "unexpected color"),
        );
      }
    }

    // P2w: same blast radius for an OOB *write* (Nano Orbital's shape)
    {
      const r = await frame(
        pb,
        compile,
        `a = array(3)
export var sent
export function beforeRender(delta) { sent = ${SENTINEL}; a[5] = 1 }
export function render(index) { hsv(0.33, 1, 1) }`,
      );
      if (!r.ok) {
        console.log(`P2w: PROBE FAILED (${r.error})`);
      } else {
        const p0 = rgbAt(r.px, 0);
        console.log(
          `P2w render pass after beforeRender OOB-write: pixel0=[${p0}] → ` +
            (isGreenish(p0) ? "render still runs" : isBlack(p0) ? "render skipped" : "unexpected"),
        );
      }
    }

    // P3: error inside render(index) for pixel 2 only. hsv(red) lands
    // before the error; green after. Later pixels tell whether one bad
    // pixel kills the rest of the frame.
    {
      const r = await frame(
        pb,
        compile,
        `a = array(3)
export var sent
export function beforeRender(delta) { sent = ${SENTINEL} }
export function render(index) {
  hsv(0, 1, 1)
  if (index == 2) { x = a[9] }
  hsv(0.33, 1, 1)
}`,
      );
      if (!r.ok) {
        console.log(`P3: PROBE FAILED (${r.error})`);
      } else {
        const [p0, p2, p3, p4] = [0, 2, 3, 4].map((i) => rgbAt(r.px, i));
        console.log(`P3 pixels: 0=[${p0}] 2=[${p2}] 3=[${p3}] 4=[${p4}]`);
        console.log(
          `P3 later pixels: ` +
            (isGreenish(p3) && isGreenish(p4)
              ? "frame CONTINUES past a bad pixel"
              : isBlack(p3) && isBlack(p4)
                ? "rest of frame blanked (matches current Luxel)"
                : "unexpected"),
        );
        console.log(
          `P3 errored pixel keeps pre-error hsv: ` +
            (isReddish(p2) ? "YES (red stuck)" : isBlack(p2) ? "no (black)" : `no ([${p2}])`),
        );
      }
    }
  } finally {
    await pb.close();
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
