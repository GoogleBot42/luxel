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
// Second battery, Gitea #107 (2026-08-29) — is the out-of-range access an
// ERROR at all on PB, or does PB tolerate it (clamp / wrap / silent no-op)?
// #84 settled the blast radius; #107 asks the prior question, separately for
// every irregular index shape, and whether a tolerated write mutates
// anything:
//   Q1  OOB write a[5]=1 on array(3): abort or continue? (before/after vars)
//   Q2  negative write a[-1]=1: abort or continue?
//   Q3  OOB read a[5] / negative read a[-1]: abort or continue?
//   Q4  fractional index: in-bounds read a[1.5], OOB read a[3.5], and an
//       in-bounds fractional *variable* write i=1.5; a[i]=9 (truncate?)
//   Q5  does an aborted OOB write MUTATE the array — clamp to the last slot,
//       wrap modulo length, or leave it untouched? (write in frame 1, read
//       every slot back in frame 2, +1-offset so unset reads as 1)
//   Q6  recovery: does the NEXT invocation run normally after an abort, i.e.
//       is rainbow-comet's one-shot frame-982 error a one-frame glitch?
//   Q7  tixy's shape: calling a never-assigned array slot, t=array(4);
//       t[0](1,2) — "call of a non-function value" — abort or no-op?
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
  const restoreId = (await pb.getConfig()).seq?.activeProgram?.activeProgramId;
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
    // ---- Gitea #107 battery: is it an error at all, or is PB tolerant? ----

    // Shared shape: `before`/`after` bracket the suspect statement, so
    // after==7 means execution continued past it and after==0 means the
    // invocation aborted there. `sent` proves the handler ran at all.
    const bracket = (stmt, extraVars = "") => `a = array(3)
export var sent, before, after${extraVars}
export function beforeRender(delta) {
  sent = ${SENTINEL}
  before = 7
  ${stmt}
  after = 7
}
export function render(index) { hsv(0, 0, 0) }`;

    const verdict = async (label, stmt, extraVars, extra) => {
      // one flaky getVars must not abandon the battery mid-way — the
      // `finally` restore only runs once, and a half-run battery costs
      // another full pass over the device
      const r = await vars(pb, compile, bracket(stmt, extraVars), ["sent"]).catch((e) => ({
        ok: false,
        error: String(e.message ?? e),
      }));
      if (!r.ok) {
        console.log(`${label}: PROBE FAILED (${r.error})`);
        return null;
      }
      const v = r.vars;
      const tolerated = v.after === 7;
      console.log(
        `${label}  sent=${v.sent} before=${v.before} after=${v.after}` +
          (extra ? ` ${extra.map((k) => `${k}=${v[k]}`).join(" ")}` : "") +
          ` → ${tolerated ? "TOLERATED (no abort)" : v.before === 7 ? "ABORTS at the statement" : "unexpected var state"}`,
      );
      return v;
    };

    await verdict("Q1 OOB write   a[5]=1  ", "a[5] = 1", "");
    await verdict("Q2 neg write   a[-1]=1 ", "a[-1] = 1", "");
    await verdict("Q3a OOB read   v=a[5]  ", "v = a[5] + 1", ", v", ["v"]);
    await verdict("Q3b neg read   v=a[-1] ", "v = a[-1] + 1", ", v", ["v"]);
    await verdict(
      "Q4a frac read  v=a[1.5]",
      "a[1] = 11\n  v = a[1.5] + 1",
      ", v",
      ["v"],
    );
    await verdict("Q4b OOBfrac    v=a[3.5]", "v = a[3.5] + 1", ", v", ["v"]);
    await verdict(
      "Q4c frac write a[i]=9  ",
      "i = 1.5\n  a[i] = 9\n  v = a[1] + 1",
      ", v, i",
      ["v"],
    );
    await verdict(
      "Q4d lit write  a[1.5]=9",
      "a[1.5] = 9\n  v = a[1] + 1",
      ", v",
      ["v"],
    );
    await verdict(
      "Q4e lit arr    b[1.5]=9",
      "b = [10, 20, 30]\n  b[1.5] = 9\n  v = b[1] + 1",
      ", v",
      ["v"],
    );
    await verdict("Q7 call slot   t[0](1,2)", "t = array(4)\n  v = t[0](1, 2) + 1", ", v", ["v"]);

    // Q8: the sibling write path. `arrayReplace`/`arrayReplaceAt` splat N
    // values from an offset; Luxel USED to silently drop the elements that
    // fell off the end and clamp a negative offset to 0, both the opposite of
    // what `a[i] = v` does — these probes settled it the other way and the
    // engine now matches (783978f, #107): an overrun is a whole-span error
    // that leaves the array untouched, and a negative offset shifts rather
    // than clamps. Re-run them as a regression check, not as an open question.
    //
    // DEVICE HAZARD — do not add these back. Two shapes reproducibly hang
    // the oracle's engine (websocket stops acking `setCode`; it recovers on
    // its own after a minute or so, and it also drops off WiFi meanwhile):
    //   arrayReplace(array(4), 1, 2, 3, 4, 5)      — 6-arg call, overruns
    //   arrayReplaceAt(array(4), -3, 1, 2, 3, 4)   — 6-arg call, 3 negative
    // Both are 6-argument calls; Q8e below overruns with 4 args and errors
    // cleanly, so the bounds rule is probe-able without the hang. Filed
    // separately — this battery must stay safe to re-run.
    await verdict(
      "Q8a overrun    At(b,3,7,8,9)",
      "b = array(4)\n  arrayReplaceAt(b, 3, 7, 8, 9)\n  v = b[3] + 1",
      ", v",
      ["v"],
    );
    await verdict(
      "Q8b past end   At(b,9,7)",
      "b = array(4)\n  arrayReplaceAt(b, 9, 7)\n  v = b[3] + 1",
      ", v",
      ["v"],
    );
    await verdict(
      "Q8c negative   At(b,-1,7)",
      "b = array(4)\n  arrayReplaceAt(b, -1, 7)\n  v = b[0] + 1",
      ", v",
      ["v"],
    );
    // does a negative offset SHIFT (dropping only the negative slots) or is
    // the whole call a no-op? b[0] == 8 means shift, 0 means no-op.
    await verdict(
      "Q8d neg shift  At(b,-1,7,8)",
      "b = array(4)\n  arrayReplaceAt(b, -1, 7, 8)\n  v = b[0] + 1",
      ", v",
      ["v"],
    );
    // the offset-0 form (`arrayReplace`) bounds-checks too — 4 args, so the
    // hang shape above is not in play
    await verdict(
      "Q8e overrun    Replace(b,1,2,3)",
      "b = array(2)\n  arrayReplace(b, 1, 2, 3)\n  v = b[1] + 1",
      ", v",
      ["v"],
    );
    // exact fit is the boundary: off + count == length must be accepted
    await verdict(
      "Q8g exact fit  At(b,2,7,8)",
      "b = array(4)\n  arrayReplaceAt(b, 2, 7, 8)\n  v = b[3] + 1",
      ", v",
      ["v"],
    );

    // Q8f: does the aborted overrun leave the in-bounds prefix written?
    {
      const r = await vars(
        pb,
        compile,
        `b = array(4)
export var sent, phase, s0, s1, s2, s3
export function beforeRender(delta) {
  sent = ${SENTINEL}
  if (phase == 0) {
    phase = 1
    arrayReplaceAt(b, 2, 7, 8, 9)
  } else {
    s0 = b[0] + 1
    s1 = b[1] + 1
    s2 = b[2] + 1
    s3 = b[3] + 1
  }
}
export function render(index) { hsv(0, 0, 0) }`,
        ["s0"],
      );
      if (!r.ok) {
        console.log(`Q8f partial: PROBE FAILED (${r.error})`);
      } else {
        const s = [r.vars.s0, r.vars.s1, r.vars.s2, r.vars.s3];
        console.log(
          `Q8f arrayReplaceAt(b,2,7,8,9) then read back → slots(+1)=[${s}] → ` +
            (s.every((x) => x === 1)
              ? "NOTHING written (all-or-nothing)"
              : s[2] === 8 && s[3] === 9
                ? "in-bounds PREFIX written, then aborts"
                : "unexpected"),
        );
      }
    }

    // Q4f: the same fractional literal write in TOP-LEVEL init scope, where
    // 04-oracle-findings.md (2026-07) recorded PB aborting. An aborted init
    // leaves every exported var at 0, so `sent` is the abort detector here.
    {
      const r = await vars(
        pb,
        compile,
        `c = [10, 20, 30]
c[1.5] = 9
export var sent = ${SENTINEL}, v = c[1] + 1
export function beforeRender(delta) { }
export function render(index) { hsv(0, 0, 0) }`,
        ["sent", "v"],
      );
      if (!r.ok) {
        console.log(`Q4f init-scope c[1.5]=9: PROBE FAILED (${r.error})`);
      } else {
        console.log(
          `Q4f init-scope c[1.5]=9  sent=${r.vars.sent} v=${r.vars.v} → ` +
            (r.vars.sent !== SENTINEL
              ? "ABORTS pattern init"
              : r.vars.v === 10
                ? "TOLERATED, truncates to c[1]"
                : "TOLERATED but did not write c[1]"),
        );
      }
    }

    // Q5: does the out-of-range write mutate anything? array(4) with index 6
    // separates the hypotheses: clamp → slot 3, wrap (6%4) → slot 2,
    // untouched → every slot reads back 0. Slots are read in a LATER
    // invocation (+1 offset, so an unset slot reads 1) because the write
    // itself may abort this one.
    {
      const r = await vars(
        pb,
        compile,
        `a = array(4)
export var sent, phase, wrote, s0, s1, s2, s3
export function beforeRender(delta) {
  sent = ${SENTINEL}
  if (phase == 0) {
    phase = 1
    a[6] = 1
    wrote = 1
  } else {
    s0 = a[0] + 1
    s1 = a[1] + 1
    s2 = a[2] + 1
    s3 = a[3] + 1
  }
}
export function render(index) { hsv(0, 0, 0) }`,
        ["s0"],
      );
      if (!r.ok) {
        console.log(`Q5 mutation: PROBE FAILED (${r.error})`);
      } else {
        const s = [r.vars.s0, r.vars.s1, r.vars.s2, r.vars.s3];
        const untouched = s.every((x) => x === 1);
        console.log(
          `Q5 a=array(4); a[6]=1 → slots(+1)=[${s}] wrote=${r.vars.wrote} → ` +
            (untouched
              ? "array UNTOUCHED (no clamp, no wrap)"
              : s[3] === 2
                ? "CLAMPED to the last slot"
                : s[2] === 2
                  ? "WRAPPED modulo length"
                  : "unexpected mutation"),
        );
      }
    }

    // Q6: recovery. Every 3rd invocation OOBs; if the abort were permanent
    // (or killed the program) `frames` would stop advancing. This is
    // rainbow-comet's frame-982 shape.
    {
      const src = `a = array(3)
export var sent, frames
export function beforeRender(delta) {
  sent = ${SENTINEL}
  frames = frames + 1
  if (frames % 3 == 0) { a[9] = 1 }
}
export function render(index) { hsv(0, 0, 0) }`;
      const r = await vars(pb, compile, src, ["frames"]);
      if (!r.ok) {
        console.log(`Q6 recovery: PROBE FAILED (${r.error})`);
      } else {
        const first = r.vars.frames;
        await sleep(1500);
        const second = (await pb.getVars()).frames;
        console.log(
          `Q6 every-3rd-frame OOB write: frames ${first} → ${second} after 1.5 s → ` +
            (second > first + 5
              ? "KEEPS RUNNING (abort is per-invocation, pattern survives)"
              : "STALLED (the error is fatal to the program)"),
        );
      }
    }
  } finally {
    // put the device back on its saved pattern — live-coded probe patterns
    // otherwise stay on the LEDs and read as a wedged/broken oracle
    // (run.mjs and todo-probes.mjs already follow this convention)
    if (restoreId) await pb.setActivePattern(restoreId).catch(() => {});
    await pb.close();
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
