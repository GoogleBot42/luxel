// Render a corpus original and its clean-room port side by side, headlessly,
// under identical conditions — the command the port-fidelity judges call.
//
// Both sides run on the same engine build, same rig, same seed, same pinned
// wall clock and the same fixed frame delta, so any visual difference is a
// difference between the patterns. Outputs are images + numeric stats only:
// no pattern source ever reaches meta.json, because the judges must decide
// from pixels alone.
//
// Usage:
//   node tools/verify/snap.mjs <slug> [options]
//
//   --seconds N          capture window in seconds (default 6)
//   --fps N              frames per second (default 20)
//   --skip N             seconds of warmup discarded before capture (default 0)
//   --seed N             RNG seed handed to both sides (default 1)
//   --wall-clock N       epoch-seconds wall clock handed to both sides (default
//                        1756000000; fixed for the whole run — never advances)
//   --rig strip|grid|cloud   override the rig from pairs.json
//   --pixels N           override pixel count for the rig (snapped to a whole
//                        grid / cubic lattice where the rig needs it, with a
//                        warning)
//   --grid WxH           override grid dimensions (grid rig; implies pixels)
//   --controls-orig "name=v[,v,v];name2=v"   controls for the original
//   --controls-port "..."                    controls for the port
//   --sensors auto|synth|off   synthetic sensor-board feed (default auto —
//                        fed only to a side whose engine reports wants_sensors)
//   --label NAME         output subdirectory name (default "default")
//   --sheet-frames N     frames in the grid contact sheet (default 12)
//   --strip-frames N     consecutive frames in the grid filmstrip (default 12)
//   --strip-at S         seconds into the captured window where the filmstrip
//                        starts (default: window midpoint, clamped to fit)
//   --probe-controls     additionally sweep every settable control, one at a
//                        time, and write probe.json (see below)
//   --probe-seconds N    probe window length (default 4)
//   --dump "t1,t2,..."   write frames.json with the exact per-pixel values of
//                        the captured frames nearest those times (see below;
//                        the list is parsed strictly — a malformed entry exits
//                        2 rather than dumping some other set of times)
//   --out-root DIR       output root (default tools/verify/out)
//
// Writes <out-root>/<slug>/<label>/{orig.png,port.png,meta.json,stats.json}.
//
// meta.json is written for a reader on a context budget: it holds settings,
// provenance, run-level `warnings`, and per side `image`, `controls`,
// `controlsApplied`, `warnings`, `compileError`, `runtimeError`,
// `wantsSensors`, `sensors` (+ `sensorModel` when synth) and
// `statsSummary` ({avg,min,max,first,last} per series, plus `zeroMotionFrames`
// and `brightnessTrend`) — it is short enough to read whole. The FULL per-frame
// series live in the sibling stats.json (one line per series, per side) and
// only need reading when a summary flags something.
// Two motion series are kept: `motion` (mean abs frame-to-frame channel diff
// over the WHOLE rig — the historical, like-for-like number) and `motionLit`
// (the same diff averaged only over pixels lit in either frame). A pattern
// lighting a few percent of its pixels divides `motion` by the dark fraction
// and rounds to 0, reading as frozen while it animates; `motionLit` is the
// stat to use on sparse output. `zeroMotionFrames` still counts `motion`.
// `sheetTimesSeconds`/`stripTimesSeconds` are top-level in meta.json (both
// sides share them by construction). A top-level `provenance` records the
// worktree git sha and sha256 prefixes of the port source, the .epe, and this
// harness — a cached run whose provenance differs from a fresh one is stale
// and must be discarded rather than reused.
// Strip/cloud rigs render a waterfall (one row per frame, x = pixel index);
// the grid rig renders a contact sheet of evenly spaced frames (each cell
// stamped with its absolute timestamp), plus two extra pairs that make
// temporal behaviour legible:
//   *-motion.png   filmstrip of CONSECUTIVE frames (true frame-to-frame motion,
//                  unaliased by the sheet's even sampling)
//   *-rhythm.png   waterfall of EVERY captured frame collapsed to one row of
//                  per-column mean RGB (full-window rhythm: beats, cycles, drift)
//
// --sensors feeds the PB sensor-board surface (frequencyData, energyAverage,
// maxFrequency(Magnitude), accelerometer, light) so sound- and motion-reactive
// patterns render something judgeable instead of idling black. The signal is
// SYNTHETIC and fully deterministic — derived from the frame index and fps
// alone, never from wall time or randomness — so both sides hear exactly the
// same input and reruns are bit-identical. Model "beat120": a 120 BPM (2 Hz)
// beat envelope, a bass peak on the beat, a mid melody peak stepping an 8-step
// scale (one step per beat), a small HF shimmer, an 8 s accelerometer tilt
// circle and a constant light level. `auto` (default) feeds a side only when
// its engine reports wants_sensors, so non-sensor patterns render exactly as
// they did before this option existed; `synth` always feeds; `off` never does.
// meta.json records, per side, `wantsSensors` and the `sensors` mode ACTUALLY
// applied (plus `sensorModel` when synth) — not the flag you typed.
//
// --probe-controls answers "which dials are actually live?" in one command.
// For each side and each SETTABLE control (slider, toggle, inputNumber,
// hsvPicker, rgbPicker — display-only kinds and triggers are skipped) it
// renders a --probe-seconds window at the run's fps/skip/seed with the control
// left untouched, then re-renders it at 0, 0.5 and 1 (pickers: v,v,v) with
// every other control untouched, and records the mean absolute pixel difference
// from the untouched render across all frames. probe.json holds, per side per
// control, `{kind, deltas: {"0":d,"0.5":d,"1":d}, deltasLit: {…}, responsive}`;
// `deltas` averages over the whole rig, `deltasLit` over the pixels either
// render lights (the sparse-pattern rescue — same reasoning as `motionLit`),
// and `responsive` is true when EITHER clears its threshold
// (PROBE_THRESHOLD / PROBE_LIT_THRESHOLD). A compact table also prints
// to stdout. A dial the probe calls inert may simply act slower than the probe
// window — raise --probe-seconds (or probe at a later --skip) before believing
// it. Any --controls-orig/--controls-port on the run form the untouched
// baseline; the probed dial is overridden on top of them.
//
// --dump "t1,t2,..." answers "what are the ACTUAL numbers?" — the PNGs are
// nearest-neighbour upscaled and alias fine structure away, so stripe widths,
// feature pixel indices and exact RGB values are guesswork when read off an
// image. Each listed time is seconds INTO THE CAPTURED WINDOW (t=0 is the first
// captured frame, i.e. --skip seconds on the run's timeline); the nearest
// captured frame is taken and clamped into the window (clamping warns). It
// writes frames.json: `{times, frameIndices, orig, port}` where `times` are the
// ABSOLUTE times of the frames actually dumped (skip + index/fps, the same
// convention as meta.json's sheetTimesSeconds) and each side is one entry per
// time. A strip/cloud frame is a flat array of `pixels` [r,g,b] triples; a grid
// frame is nested as gridH rows of gridW triples, so the layout reads directly.
// A side that failed to compile is omitted.
// The list is parsed STRICTLY: entries are trimmed (so a list piped in with
// newlines after the commas is fine) and one trailing comma is allowed, but any
// entry that is not a plain non-negative decimal — empty, "1 2", "0x10", "1e3",
// negative — exits 2 naming the entry. Nothing is skipped and no default list
// exists, so a dump can never quietly cover moments other than the ones asked
// for. (frames.json also echoes `requestedTimes`; compare it against what you
// meant to pass when a list came from a file or a shell variable.)
//
// Exit status is 0 whenever the harness ran — a pattern that fails to compile
// or throws at runtime is a valid verification result, recorded in meta.json.
// Nonzero only for harness-level failures (unknown slug, bad arguments).

import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { load, cubeLattice, sensorSlots, WASM_PATH } from "./enginehost.mjs";
import { synthSensorFrame, SENSOR_MODEL } from "./sensormodel.mjs";
import { encodePNG, upscale, drawText, textSize } from "./png.mjs";

const SELF = fileURLToPath(import.meta.url);
const HERE = path.dirname(SELF);
const ROOT = path.resolve(HERE, "../..");
const PAIRS = path.join(HERE, "pairs.json");

/** Pinned so time-of-day patterns are reproducible across runs and machines.
 *  Overridable per run with --wall-clock <epoch-seconds> to sample a clock-driven
 *  pattern (sunrise ramp, time-of-day palette) at other instants — the clock is
 *  still FIXED for the whole run; it never advances with simulated time. */
const WALL_CLOCK = 1756000000;
const SEP = [32, 32, 36]; // contact-sheet separator colour
const SHEET_COLS = 6;
const CLOUD_SIDE = 5; // default cloud rig: 5x5x5 = 125 pixels
const MAX_DIM = 1200;
const STAMP_FG = [128, 128, 128]; // timestamp ink
const STAMP_BG = [0, 0, 0]; // 1px backing box, so the ink survives any content

/** --probe-controls: default window length, settings swept, and the "it did
 *  something" bar (mean abs 0-255 pixel diff over every channel of every frame).
 *  4 s, not 2: a dial that switches modes/regimes on a multi-second timer reads
 *  as inert in a 2 s window because the first regime is the same either way.
 *  Raise it further with --probe-seconds for dials on an even slower clock. */
const PROBE_SECONDS = 4;
const PROBE_VALUES = [0, 0.5, 1];
const PROBE_THRESHOLD = 1.0;
/** The same bar for the LIT-restricted delta (mean abs channel diff over the
 *  pixels either render lights). It has to be higher than PROBE_THRESHOLD
 *  because the denominator is smaller — a whole-rig delta of 1.0 on a pattern
 *  lighting 3% of its pixels is ~33 lit. 4.0 ≈ a lit pixel shifting by 1.5% of
 *  full scale on average: comfortably above rounding/dither churn, far below a
 *  real dial response (measured: sparse false-inert dials land in the 20-70
 *  range, genuinely inert dials at 0.00). */
const PROBE_LIT_THRESHOLD = 4.0;
const PROBE_KINDS = new Set(["slider", "toggle", "inputNumber", "hsvPicker", "rgbPicker"]);
const PICKER_KINDS = new Set(["hsvPicker", "rgbPicker"]);

/** --sensors: the modes, and the name stamped into meta.json when synth data
 *  was actually fed. Bump the name if the signal below ever changes, so runs
 *  made against different signals can never be compared as if they matched. */
const SENSOR_MODES = ["auto", "synth", "off"];

/** Run-level warnings (clamped arguments &c). Also printed to stderr. */
const runWarnings = [];
function warn(msg) {
  runWarnings.push(msg);
  console.error(`snap: warning: ${msg}`);
}

function die(msg) {
  console.error(`snap: ${msg}`);
  process.exit(2);
}

// ---- args ------------------------------------------------------------------

function parseArgs(argv) {
  const opts = {
    seconds: 6,
    fps: 20,
    skip: 0,
    seed: 1,
    rig: null,
    pixels: null,
    grid: null,
    controlsOrig: "",
    controlsPort: "",
    sensors: "auto",
    label: "default",
    sheetFrames: 12,
    stripFrames: 12,
    stripAt: null, // null → window midpoint
    probeControls: false,
    probeSeconds: PROBE_SECONDS,
    dump: null, // null → no frames.json
    wallClock: WALL_CLOCK,
    outRoot: path.join(HERE, "out"),
  };
  const FLAGS = { "--probe-controls": "probeControls" };
  const KEYS = {
    "--seconds": ["seconds", Number],
    "--fps": ["fps", Number],
    "--skip": ["skip", Number],
    "--seed": ["seed", Number],
    "--rig": ["rig", String],
    "--pixels": ["pixels", Number],
    "--grid": ["grid", String],
    "--controls-orig": ["controlsOrig", String],
    "--controls-port": ["controlsPort", String],
    "--sensors": ["sensors", String],
    "--label": ["label", String],
    "--sheet-frames": ["sheetFrames", Number],
    "--strip-frames": ["stripFrames", Number],
    "--strip-at": ["stripAt", Number],
    "--probe-seconds": ["probeSeconds", Number],
    "--dump": ["dump", parseTimes],
    "--wall-clock": ["wallClock", Number],
    "--out-root": ["outRoot", String],
  };
  let slug = null;
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a.startsWith("--")) {
      if (FLAGS[a]) {
        opts[FLAGS[a]] = true;
        continue;
      }
      const spec = KEYS[a];
      if (!spec) die(`unknown option ${a}`);
      const v = argv[++i];
      if (v === undefined) die(`${a} needs a value`);
      opts[spec[0]] = spec[1](v);
    } else if (slug === null) {
      slug = a;
    } else {
      die(`unexpected argument ${a}`);
    }
  }
  if (!slug) die("usage: node tools/verify/snap.mjs <slug> [options]  (see header)");
  if (!(opts.fps > 0) || !(opts.seconds > 0)) die("--fps and --seconds must be > 0");
  if (!(opts.stripFrames >= 1)) die("--strip-frames must be >= 1");
  if (opts.stripAt !== null && !(opts.stripAt >= 0)) die("--strip-at must be >= 0");
  if (!(opts.probeSeconds > 0)) die("--probe-seconds must be > 0");
  if (opts.rig && !["strip", "grid", "cloud"].includes(opts.rig)) die(`bad --rig ${opts.rig}`);
  if (!SENSOR_MODES.includes(opts.sensors)) {
    die(`bad --sensors ${opts.sensors} (want ${SENSOR_MODES.join("|")})`);
  }
  return { slug, opts };
}

/** --dump "0,3,7.5" → [0, 3, 7.5]: times in seconds into the captured window.
 *
 *  Strict on purpose. A dump list is usually machine-generated and piped in
 *  through the shell, so a mangled list looks exactly like a good one — and a
 *  run that silently dumps the WRONG moments produces numbers a judge then
 *  reasons from. Every entry must be a plain non-negative decimal; whitespace
 *  around an entry (newlines from a `printf`/`seq` pipeline included) is
 *  trimmed, and a single trailing comma is tolerated because `tr '\n' ','`
 *  leaves one. Anything else — an empty entry, "1 2", "NaN", "1e", "0x10",
 *  "Infinity", a negative — is a hard error. There is NO default list and no
 *  fallback: this either parses what you wrote or exits 2. */
const TIME_RE = /^\d+(\.\d*)?$|^\.\d+$/;
function parseTimes(spec) {
  const raw = String(spec).trim().replace(/,$/, ""); // one trailing "," is fine
  if (raw === "") die(`--dump needs at least one time (e.g. --dump "0,3")`);
  const times = [];
  const entries = raw.split(",");
  entries.forEach((entry, i) => {
    const t = entry.trim();
    if (t === "") {
      die(
        `bad --dump: entry ${i + 1} of ${entries.length} is empty — ` +
          `the list has a stray or doubled comma (got "${clip(spec)}")`,
      );
    }
    if (!TIME_RE.test(t) || !Number.isFinite(Number(t))) {
      die(
        `bad --dump: entry ${i + 1} of ${entries.length} is "${t}", not a plain ` +
          `non-negative number — want e.g. "0,3,7.5" (got "${clip(spec)}")`,
      );
    }
    times.push(Number(t));
  });
  return times;
}

/** Shorten a long argument for an error message, keeping both ends. */
function clip(s, max = 80) {
  const one = String(s).replace(/\s+/g, " ");
  return one.length <= max ? one : `${one.slice(0, max / 2)}…${one.slice(-max / 2)}`;
}

/** "name=1;other=0.2,0.3,0.4" → { name: [1], other: [0.2,0.3,0.4] } */
function parseControls(spec) {
  const out = {};
  for (const part of String(spec).split(";")) {
    const s = part.trim();
    if (!s) continue;
    const eq = s.indexOf("=");
    if (eq < 0) die(`bad control spec "${s}" (want name=value)`);
    const name = s.slice(0, eq).trim();
    const values = s
      .slice(eq + 1)
      .split(",")
      .map((v) => Number(v.trim()));
    if (!name || values.some((v) => !Number.isFinite(v))) die(`bad control spec "${s}"`);
    out[name] = values;
  }
  return out;
}

// ---- synthetic sensor feed ("beat120") -------------------------------------
//
// The feed itself lives in sensormodel.mjs (shared with report.mjs).

/** Slot arrays are rebuilt for every frame of every render (a --probe-controls
 *  run redoes the same window a dozen times); memoize per fps so the feed costs
 *  nothing after the first pass — and so every render provably reuses the SAME
 *  numbers rather than recomputing them. */
const sensorCache = new Map();
function sensorSlotsAt(i, fps) {
  let byFrame = sensorCache.get(fps);
  if (!byFrame) sensorCache.set(fps, (byFrame = new Map()));
  let slots = byFrame.get(i);
  if (!slots) byFrame.set(i, (slots = sensorSlots(synthSensorFrame(i, fps))));
  return slots;
}

// ---- rendering -------------------------------------------------------------

function applyRig(eng, rig) {
  if (rig.kind === "grid") eng.setMapGrid(rig.gridW, rig.gridH);
  else if (rig.kind === "cloud") eng.setMap3D(rig.points);
  // strip: no map — the engine's 1D fallback
}

function frameStats(cur, prev) {
  const n = cur.length / 3;
  let r = 0,
    g = 0,
    b = 0,
    motion = 0,
    motionLit = 0;
  for (let i = 0; i < cur.length; i += 3) {
    r += cur[i];
    g += cur[i + 1];
    b += cur[i + 2];
  }
  if (prev) {
    // `motion` averages over the WHOLE rig, so a pattern lighting 2% of the
    // pixels has its motion divided by 50 and rounds to 0 — a false freeze.
    // `motionLit` averages over the lit set only (any channel non-zero in
    // either frame), so it measures how hard the lit pixels are working.
    let litSum = 0;
    let litPixels = 0;
    for (let i = 0; i < cur.length; i += 3) {
      const d =
        Math.abs(cur[i] - prev[i]) +
        Math.abs(cur[i + 1] - prev[i + 1]) +
        Math.abs(cur[i + 2] - prev[i + 2]);
      motion += d;
      if (cur[i] || cur[i + 1] || cur[i + 2] || prev[i] || prev[i + 1] || prev[i + 2]) {
        litSum += d;
        litPixels++;
      }
    }
    motion /= cur.length;
    motionLit = litPixels ? litSum / (litPixels * 3) : 0;
  }
  return {
    meanR: Math.round(r / n),
    meanG: Math.round(g / n),
    meanB: Math.round(b / n),
    meanBrightness: Math.round((r + g + b) / cur.length),
    motion: Math.round(motion),
    // One decimal, not rounded to int like `motion`: re-quantizing a stat whose
    // whole point is to survive sparseness would put the false freeze back.
    motionLit: round1(motionLit),
  };
}

const mean = (a) => a.reduce((x, y) => x + y, 0) / a.length;
const round1 = (v) => Math.round(v * 10) / 10;

/** {avg,min,max,first,last} for one numeric series. */
function seriesSummary(a) {
  let min = a[0];
  let max = a[0];
  for (const v of a) {
    if (v < min) min = v;
    if (v > max) max = v;
  }
  return { avg: round1(mean(a)), min, max, first: a[0], last: a[a.length - 1] };
}

/** Coarse shape of the brightness envelope over the captured window. */
function brightnessTrend(a) {
  if (a.length < 2) return "steady";
  const avg = mean(a);
  const sd = Math.sqrt(mean(a.map((v) => (v - avg) * (v - avg))));
  if (avg > 0 && sd / avg > 0.6) return "volatile";
  const q = Math.max(1, Math.floor(a.length / 4));
  const head = mean(a.slice(0, q));
  const tail = mean(a.slice(-q));
  if (head <= 0 && tail <= 0) return "steady";
  if (tail > head * 1.2) return "rising";
  if (tail < head * 0.8) return "decaying";
  return "steady";
}

/** Cheap digest of the per-frame series — read this before the full arrays. */
function summarize(stats) {
  const out = {};
  for (const key of ["meanBrightness", "meanR", "meanG", "meanB", "motion", "motionLit"]) {
    out[key] = seriesSummary(stats[key]);
  }
  out.zeroMotionFrames = stats.motion.filter((v) => v === 0).length;
  out.brightnessTrend = brightnessTrend(stats.meanBrightness);
  return out;
}

/** Compile + drive one side. Never throws for pattern-level failures. */
function renderSide(host, source, rig, o) {
  const side = {
    controls: [],
    controlsApplied: {},
    warnings: [],
    compileError: null,
    runtimeError: null,
    wantsSensors: false,
    sensors: "off",
    stats: null,
    statsSummary: null,
    frames: null,
  };

  // Before compile: top-level init runs inside compile(), and clock-driven
  // patterns may read time-of-day there (Gitea #104).
  host.setDefaultWallClock(o.wallClock);
  const res = host.compile(source, rig.pixels, o.seed);
  if (res.compileError) {
    side.compileError = res.compileError;
    return side;
  }
  const eng = res;
  try {
    applyRig(eng, rig);
    eng.setWallClock(o.wallClock);

    // auto: feed only patterns that actually bind sensor globals, so every
    // non-sensor pattern renders exactly as it did before --sensors existed.
    side.wantsSensors = eng.wantsSensors();
    const feedSensors = o.sensors === "synth" || (o.sensors === "auto" && side.wantsSensors);
    side.sensors = feedSensors ? "synth" : "off";

    side.controls = eng.controls().map((c) => ({ name: c.name, kind: c.kind, label: c.label }));
    for (const [name, values] of Object.entries(o.controls)) {
      const shown = eng.setControl(name, values);
      if (shown === null) side.warnings.push(`unknown control "${name}" — not applied`);
      else side.controlsApplied[name] = values;
    }
    // A control handler can itself throw; don't blame the first render frame.
    const ctlErr = eng.takeError();
    if (ctlErr && !side.runtimeError) {
      side.runtimeError = { message: String(ctlErr.message ?? ctlErr), frame: -1, phase: "control" };
    }

    const delta = 1000 / o.fps;
    const warmup = Math.round(o.skip * o.fps);
    const capture = Math.round(o.seconds * o.fps);
    const frames = [];
    const stats = [];
    let prev = null;

    for (let i = 0; i < warmup + capture; i++) {
      // Fresh sensor frame BEFORE every render — warmup included, so a
      // pattern's beat detectors and filters see the same history a capture
      // starting at t=0 would have given them.
      if (feedSensors) eng.setSensors(sensorSlotsAt(i, o.fps));
      const px = eng.frame(delta);
      const err = eng.takeError();
      if (err && !side.runtimeError) {
        side.runtimeError = {
          message: String(err.message ?? err),
          frame: i - warmup, // negative = during warmup
          phase: i < warmup ? "warmup" : "capture",
          ...(err.line !== undefined ? { line: err.line, col: err.col, fn: err.fn } : {}),
        };
      }
      if (i >= warmup) {
        frames.push(px);
        stats.push(frameStats(px, prev));
      }
      prev = px;
    }

    side.frames = frames;
    side.stats = {
      meanBrightness: stats.map((s) => s.meanBrightness),
      meanR: stats.map((s) => s.meanR),
      meanG: stats.map((s) => s.meanG),
      meanB: stats.map((s) => s.meanB),
      motion: stats.map((s) => s.motion),
      motionLit: stats.map((s) => s.motionLit),
    };
    if (stats.length) side.statsSummary = summarize(side.stats);
  } finally {
    eng.free();
  }
  return side;
}

// ---- control probing -------------------------------------------------------

/** Mean absolute difference, per channel per frame, between two renders. */
function meanAbsDiff(a, b) {
  const n = Math.min(a.length, b.length);
  let sum = 0;
  let count = 0;
  for (let f = 0; f < n; f++) {
    const x = a[f];
    const y = b[f];
    const len = Math.min(x.length, y.length);
    for (let i = 0; i < len; i++) sum += Math.abs(x[i] - y[i]);
    count += len;
  }
  return count ? sum / count : 0;
}

/** Same, but averaged over the LIT pixels only — a pixel counts when any of
 *  its channels is non-zero in either render. The whole-rig mean divides a
 *  sparse pattern's real response by the dark fraction and reads as inert;
 *  this is what that response looks like where the pattern actually draws. */
function meanAbsDiffLit(a, b) {
  const n = Math.min(a.length, b.length);
  let sum = 0;
  let count = 0;
  for (let f = 0; f < n; f++) {
    const x = a[f];
    const y = b[f];
    const len = Math.min(x.length, y.length) - (Math.min(x.length, y.length) % 3);
    for (let i = 0; i < len; i += 3) {
      if (!(x[i] || x[i + 1] || x[i + 2] || y[i] || y[i + 1] || y[i + 2])) continue;
      sum +=
        Math.abs(x[i] - y[i]) + Math.abs(x[i + 1] - y[i + 1]) + Math.abs(x[i + 2] - y[i + 2]);
      count += 3;
    }
  }
  return count ? sum / count : 0;
}

const round2 = (v) => Math.round(v * 100) / 100;

/** Sweep each settable control of one side, one at a time, against a render
 *  with that control left untouched. Same seed/clock/skip as the run. */
function probeSide(host, source, rig, o) {
  const base = renderSide(host, source, rig, { ...o, seconds: o.probeSeconds });
  if (!base.frames) {
    return {
      compileError: base.compileError,
      runtimeError: base.runtimeError,
      wantsSensors: base.wantsSensors,
      sensors: base.sensors,
      controls: {},
    };
  }
  const out = {
    compileError: null,
    runtimeError: base.runtimeError,
    wantsSensors: base.wantsSensors,
    sensors: base.sensors,
    ...(base.sensors === "synth" ? { sensorModel: SENSOR_MODEL } : {}),
    controls: {},
  };
  for (const c of base.controls) {
    if (!PROBE_KINDS.has(c.kind)) continue;
    const picker = PICKER_KINDS.has(c.kind);
    const deltas = {};
    const deltasLit = {};
    for (const v of PROBE_VALUES) {
      const trial = renderSide(host, source, rig, {
        ...o,
        seconds: o.probeSeconds,
        controls: { ...o.controls, [c.name]: picker ? [v, v, v] : [v] },
      });
      deltas[String(v)] = trial.frames ? round2(meanAbsDiff(base.frames, trial.frames)) : null;
      deltasLit[String(v)] = trial.frames
        ? round2(meanAbsDiffLit(base.frames, trial.frames))
        : null;
    }
    const seen = Object.values(deltas).filter((d) => d !== null);
    const seenLit = Object.values(deltasLit).filter((d) => d !== null);
    out.controls[c.name] = {
      kind: c.kind,
      deltas,
      deltasLit,
      // EITHER bar counts: the whole-rig delta is the historical number, the
      // lit-restricted one is what rescues a sparse pattern from a false inert.
      responsive:
        seen.some((d) => d >= PROBE_THRESHOLD) || seenLit.some((d) => d >= PROBE_LIT_THRESHOLD),
    };
  }
  return out;
}

/** Compact stdout table: one row per side per probed control. */
function printProbeTable(probe) {
  const rows = [["side", "control", "kind", "responsive", "maxDelta", "maxDeltaLit"]];
  for (const [side, s] of Object.entries(probe.sides)) {
    if (s.compileError) {
      rows.push([side, "(compile failed)", "-", "-", "-", "-"]);
      continue;
    }
    const names = Object.keys(s.controls);
    if (!names.length) {
      rows.push([side, "(no settable controls)", "-", "-", "-", "-"]);
      continue;
    }
    for (const name of names) {
      const c = s.controls[name];
      const biggest = (m) => {
        const seen = Object.values(m).filter((d) => d !== null);
        return (seen.length ? Math.max(...seen) : 0).toFixed(2);
      };
      rows.push([
        side,
        name,
        c.kind,
        c.responsive ? "yes" : "NO",
        biggest(c.deltas),
        biggest(c.deltasLit),
      ]);
    }
  }
  const w = rows[0].map((_, i) => Math.max(...rows.map((r) => r[i].length)));
  const st = probe.settings;
  console.log(
    `  probe: ${st.seconds}s @ ${st.fps}fps, skip ${st.skip}, seed ${st.seed}, ` +
      `responsive if maxDelta >= ${PROBE_THRESHOLD} or maxDeltaLit >= ${PROBE_LIT_THRESHOLD}`,
  );
  for (const r of rows) {
    console.log(`  ${r.map((cell, i) => cell.padEnd(w[i])).join("  ")}`.trimEnd());
  }
}

// ---- image assembly --------------------------------------------------------

/** One row per frame, x = pixel index; nearest-neighbour upscaled. */
function waterfall(frames, pixels) {
  const h = frames.length;
  const raw = Buffer.alloc(pixels * h * 3);
  for (let y = 0; y < h; y++) raw.set(frames[y], y * pixels * 3);
  const sx = Math.max(1, Math.min(Math.ceil(240 / pixels), Math.floor(MAX_DIM / pixels) || 1));
  const sy = Math.max(1, Math.min(2, Math.floor(MAX_DIM / h) || 1));
  const up = upscale(pixels, h, raw, sx, sy);
  return encodePNG(up.width, up.height, up.rgb);
}

/** Evenly spaced frames as grid tiles, `SHEET_COLS` per row.
 *  `labels` (optional, parallel to `picks`) is stamped into each cell's
 *  bottom-left corner after upscale, so the glyphs are 1:1 output pixels. */
function contactSheet(frames, gw, gh, picks, labels = null) {
  const scale = Math.max(1, Math.round(96 / Math.max(gw, gh)));
  const cw = gw * scale;
  const ch = gh * scale;
  const cols = Math.min(SHEET_COLS, picks.length);
  const rows = Math.ceil(picks.length / cols);
  const gap = 2;
  const W = cols * cw + (cols - 1) * gap;
  const H = rows * ch + (rows - 1) * gap;
  const out = Buffer.alloc(W * H * 3);
  for (let i = 0; i < out.length; i += 3) {
    out[i] = SEP[0];
    out[i + 1] = SEP[1];
    out[i + 2] = SEP[2];
  }
  picks.forEach((fi, n) => {
    const cell = upscale(gw, gh, frames[fi], scale, scale);
    const ox = (n % cols) * (cw + gap);
    const oy = Math.floor(n / cols) * (ch + gap);
    for (let y = 0; y < ch; y++) {
      cell.rgb.copy(out, ((oy + y) * W + ox) * 3, y * cw * 3, (y + 1) * cw * 3);
    }
    const text = labels?.[n];
    if (text) {
      const { width: tw, height: th } = textSize(text);
      // 1px inset for the backing box, 1px inset from the cell edge.
      if (cw >= tw + 4 && ch >= th + 4) {
        drawText(out, W, H, ox + 2, oy + ch - 2 - th, text, STAMP_FG, STAMP_BG);
      }
    }
  });
  return encodePNG(W, H, out);
}

/** Cell timestamps for `picks` — absolute times on the run's timeline. A second
 *  decimal appears only when the cells are closer together than 0.1 s (a
 *  consecutive-frame filmstrip at a high fps), so labels stay distinct. */
function stampsFor(picks, skip, fps) {
  const step = picks.length > 1 ? (picks[1] - picks[0]) / fps : 1;
  const dp = step < 0.1 ? 2 : 1;
  return picks.map((i) => `${(skip + i / fps).toFixed(dp)}s`);
}

/** Indices evenly spanning [0, n) inclusive of both ends. */
function evenPicks(n, want) {
  const k = Math.max(1, Math.min(want, n));
  if (k === 1) return [0];
  return Array.from({ length: k }, (_, i) => Math.round((i * (n - 1)) / (k - 1)));
}

/** `want` CONSECUTIVE indices starting at `at`, clamped to fit inside [0, n). */
function runPicks(n, want, at) {
  const k = Math.max(1, Math.min(want, n));
  const start = Math.max(0, Math.min(Math.round(at), n - k));
  return Array.from({ length: k }, (_, i) => start + i);
}

/** Collapse one grid frame to `gw` per-column mean RGB triples. */
function columnMeans(frame, gw, gh) {
  const row = Buffer.alloc(gw * 3);
  for (let x = 0; x < gw; x++) {
    let r = 0,
      g = 0,
      b = 0;
    for (let y = 0; y < gh; y++) {
      const i = (y * gw + x) * 3;
      r += frame[i];
      g += frame[i + 1];
      b += frame[i + 2];
    }
    row[x * 3] = Math.round(r / gh);
    row[x * 3 + 1] = Math.round(g / gh);
    row[x * 3 + 2] = Math.round(b / gh);
  }
  return row;
}

/** How many captured frames each output row of the rhythm waterfall covers. */
function rhythmRowsPerPixel(nFrames) {
  const maxRows = Math.max(1, Math.floor(MAX_DIM / 2));
  return Math.max(1, Math.ceil(nFrames / maxRows));
}

/** Every captured frame as one row of per-column mean RGB, stacked over time.
 *  Rows are averaged (never dropped) when the window is longer than fits. */
function rhythmWaterfall(frames, gw, gh, rowsPerPixel) {
  const rows = Math.ceil(frames.length / rowsPerPixel);
  const raw = Buffer.alloc(gw * rows * 3);
  for (let y = 0; y < rows; y++) {
    const from = y * rowsPerPixel;
    const to = Math.min(frames.length, from + rowsPerPixel);
    const acc = new Float64Array(gw * 3);
    for (let f = from; f < to; f++) {
      const row = columnMeans(frames[f], gw, gh);
      for (let i = 0; i < acc.length; i++) acc[i] += row[i];
    }
    const n = to - from;
    for (let i = 0; i < acc.length; i++) raw[y * gw * 3 + i] = Math.round(acc[i] / n);
  }
  const sx = Math.max(1, Math.min(Math.ceil(240 / gw), Math.floor(MAX_DIM / gw) || 1));
  const sy = Math.max(1, Math.min(2, Math.floor(MAX_DIM / rows) || 1));
  const up = upscale(gw, rows, raw, sx, sy);
  return encodePNG(up.width, up.height, up.rgb);
}

// ---- frame dumps -----------------------------------------------------------

/** Index of the captured frame nearest `t` seconds into the window, clamped. */
function nearestFrame(t, fps, captured) {
  return Math.max(0, Math.min(captured - 1, Math.round(t * fps)));
}

/** One captured frame as [r,g,b] triples: flat for 1D/3D rigs, nested as
 *  `gridH` rows of `gridW` triples for the grid rig so the layout reads. */
function dumpFrame(frame, rig, kind) {
  const px = [];
  for (let i = 0; i < rig.pixels; i++) {
    px.push([frame[i * 3], frame[i * 3 + 1], frame[i * 3 + 2]]);
  }
  if (kind !== "grid") return px;
  const rows = [];
  for (let y = 0; y < rig.gridH; y++) rows.push(px.slice(y * rig.gridW, (y + 1) * rig.gridW));
  return rows;
}

// ---- meta serialization ----------------------------------------------------

/** Width past which a one-line leaf object is broken up again. */
const META_LINE = 100;

/** JSON.stringify(v, null, 2), except all-numeric arrays stay on one line and
 *  short scalar-only objects (a stats summary, a control) collapse to one line.
 *  Per-frame series are the bulk of meta.json and a judge reads them by shape,
 *  not by element — one number per line just burns the reader's context. */
function stringifyMeta(value, indent = "") {
  const pad = indent + "  ";
  if (value === null || typeof value !== "object") return JSON.stringify(value ?? null);
  if (Array.isArray(value)) {
    if (value.length === 0) return "[]";
    if (value.every((v) => typeof v === "number")) return `[${value.join(", ")}]`;
    // Pixel data (a run of [r,g,b] triples: one dumped strip frame, or one grid
    // row) stays on ONE line — read by position, not element, same as a series.
    if (value.every((v) => Array.isArray(v) && v.every((n) => typeof n === "number"))) {
      return `[${value.map((v) => `[${v.join(", ")}]`).join(", ")}]`;
    }
    return `[\n${value.map((v) => pad + stringifyMeta(v, pad)).join(",\n")}\n${indent}]`;
  }
  const keys = Object.keys(value).filter((k) => value[k] !== undefined);
  if (keys.length === 0) return "{}";
  // JS puts integer-like keys first; a numeric-keyed map (probe deltas) should
  // read in value order instead: "0", "0.5", "1".
  if (keys.every((k) => /^-?\d+(\.\d+)?$/.test(k))) keys.sort((a, b) => Number(a) - Number(b));
  const pairs = keys.map((k) => `${JSON.stringify(k)}: ${stringifyMeta(value[k], pad)}`);
  if (keys.every((k) => value[k] === null || typeof value[k] !== "object")) {
    const flat = `{${pairs.join(", ")}}`;
    if (indent.length + flat.length <= META_LINE) return flat;
  }
  return `{\n${pairs.map((p) => pad + p).join(",\n")}\n${indent}}`;
}

const sha12 = (data) => crypto.createHash("sha256").update(data).digest("hex").slice(0, 12);

/** HEAD of the worktree this harness lives in; null outside a git checkout. */
function gitSha12() {
  try {
    const out = execFileSync("git", ["rev-parse", "HEAD"], {
      cwd: ROOT,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    return out.trim().slice(0, 12);
  } catch {
    return null;
  }
}

// ---- main ------------------------------------------------------------------

const { slug, opts } = parseArgs(process.argv.slice(2));

if (!fs.existsSync(PAIRS)) die(`${PAIRS} missing — run: node tools/verify/gen-pairs.mjs`);
const manifest = JSON.parse(fs.readFileSync(PAIRS, "utf8"));
const pair = manifest.pairs.find((p) => p.slug === slug);
if (!pair) die(`unknown slug "${slug}" — not in tools/verify/pairs.json`);

const epePath = path.join(ROOT, pair.epeFile);
const libPath = path.join(ROOT, pair.libFile);
if (!fs.existsSync(epePath)) die(`corpus file missing: ${pair.epeFile}`);
if (!fs.existsSync(libPath)) die(`library file missing: ${pair.libFile}`);
const origSource = JSON.parse(fs.readFileSync(epePath, "utf8"))?.sources?.main;
if (typeof origSource !== "string") die(`${pair.epeFile} has no sources.main`);
const portSource = fs.readFileSync(libPath, "utf8");

// rig geometry — identical for both sides, always
const kind = opts.rig ?? pair.rig;
const rig = { kind };
if (kind === "grid") {
  let gw = 16;
  let gh = 16;
  if (opts.grid) {
    const m = /^(\d+)x(\d+)$/i.exec(opts.grid);
    if (!m) die(`bad --grid "${opts.grid}" (want WxH)`);
    gw = Number(m[1]);
    gh = Number(m[2]);
  }
  rig.pixels = opts.pixels ?? gw * gh;
  // The engine derives rows from pixelCount/w — keep the sheet layout in step
  // so a --pixels override can't make the tiles read past the frame buffer.
  rig.gridW = gw;
  rig.gridH = Math.max(1, Math.ceil(rig.pixels / gw));
  if (rig.gridH !== gh) {
    const want = rig.pixels;
    rig.pixels = rig.gridW * rig.gridH;
    if (opts.pixels !== null && rig.pixels !== want) {
      warn(
        `--pixels ${want} snapped to ${rig.pixels} — the grid rig fills whole ` +
          `rows of ${rig.gridW} (${rig.gridW}x${rig.gridH})`,
      );
    }
  }
} else if (kind === "cloud") {
  // The cloud rig is an n×n×n lattice, so its pixel count is always a cube:
  // derive the side from the requested count and snap the count to side³.
  // Anything else leaves pixels off the map — they all render at the same
  // default coordinate and light up together as one bogus clump.
  const want = opts.pixels ?? CLOUD_SIDE ** 3;
  if (!(want > 0)) die("--pixels must be > 0");
  const side = Math.max(1, Math.round(Math.cbrt(want)));
  rig.points = cubeLattice(side);
  rig.pixels = rig.points.length;
  if (rig.pixels !== want) {
    warn(
      `--pixels ${want} snapped to ${rig.pixels} — the cloud rig is a ` +
        `${side}x${side}x${side} lattice and its pixel count must be a cube`,
    );
  }
} else {
  rig.pixels = opts.pixels ?? 60;
}
if (!(rig.pixels > 0)) die("--pixels must be > 0");

const host = await load();
const sides = {
  orig: renderSide(host, origSource, rig, {
    seed: opts.seed,
    fps: opts.fps,
    skip: opts.skip,
    seconds: opts.seconds,
    sensors: opts.sensors,
    wallClock: opts.wallClock,
    controls: parseControls(opts.controlsOrig),
  }),
  port: renderSide(host, portSource, rig, {
    seed: opts.seed,
    fps: opts.fps,
    skip: opts.skip,
    seconds: opts.seconds,
    sensors: opts.sensors,
    wallClock: opts.wallClock,
    controls: parseControls(opts.controlsPort),
  }),
};

const outDir = path.resolve(ROOT, opts.outRoot, slug, opts.label);
fs.mkdirSync(outDir, { recursive: true });

const captured = Math.round(opts.seconds * opts.fps);
const picks = kind === "grid" ? evenPicks(captured, opts.sheetFrames) : null;
// Filmstrip: consecutive frames, default centred on the window midpoint.
const stripAt = opts.stripAt ?? opts.seconds / 2;
const stripPicks =
  kind === "grid" ? runPicks(captured, opts.stripFrames, stripAt * opts.fps) : null;
const rowsPerPixel = kind === "grid" ? rhythmRowsPerPixel(captured) : null;

// Clamping silently moves the filmstrip; say so, in stderr and in meta.json.
if (stripPicks) {
  if (stripPicks.length !== opts.stripFrames) {
    warn(
      `--strip-frames ${opts.stripFrames} clamped to ${stripPicks.length} — ` +
        `only ${captured} frames captured (${opts.seconds}s @ ${opts.fps}fps)`,
    );
  }
  const wantStart = Math.round(stripAt * opts.fps);
  if (stripPicks[0] !== wantStart) {
    const eff = +(stripPicks[0] / opts.fps).toFixed(3);
    const asked =
      opts.stripAt !== null
        ? `--strip-at ${round1(stripAt)}s`
        : `filmstrip start (window midpoint ${round1(stripAt)}s)`;
    warn(
      `${asked} clamped to ${eff}s — a ${stripPicks.length}-frame filmstrip must fit ` +
        `inside the ${opts.seconds}s window`,
    );
  }
}

// --dump: pick the captured frame nearest each requested time. A time past the
// end of the window is clamped to the last captured frame — say so, since the
// numbers a judge then reads are not from the moment they asked for.
const dumpPicks = opts.dump ? opts.dump.map((t) => nearestFrame(t, opts.fps, captured)) : null;
if (dumpPicks) {
  opts.dump.forEach((t, i) => {
    const eff = +(dumpPicks[i] / opts.fps).toFixed(3);
    // Only a time genuinely past the window is a clamp worth warning about;
    // in-window times snapping to the frame grid are expected and silent
    // (the epsilon guards float noise from making exact half-interval times
    // read as out-of-window).
    if (t > opts.seconds + 1e-9 && Math.abs(eff - t) > 0.5 / opts.fps + 1e-9) {
      warn(
        `--dump time ${t}s clamped to ${eff}s — outside the ${opts.seconds}s ` +
          `captured window (${captured} frames @ ${opts.fps}fps)`,
      );
    }
  });
}

const sheetLabels = picks ? stampsFor(picks, opts.skip, opts.fps) : null;
const stripLabels = stripPicks ? stampsFor(stripPicks, opts.skip, opts.fps) : null;

for (const [name, side] of Object.entries(sides)) {
  const file = path.join(outDir, `${name}.png`);
  const extras =
    kind === "grid"
      ? [`${name}-motion.png`, `${name}-rhythm.png`].map((f) => path.join(outDir, f))
      : [];
  if (!side.frames) {
    // stale images from a previous run
    for (const f of [file, ...extras]) fs.rmSync(f, { force: true });
    continue;
  }
  if (kind === "grid") {
    fs.writeFileSync(file, contactSheet(side.frames, rig.gridW, rig.gridH, picks, sheetLabels));
    fs.writeFileSync(
      extras[0],
      contactSheet(side.frames, rig.gridW, rig.gridH, stripPicks, stripLabels),
    );
    fs.writeFileSync(extras[1], rhythmWaterfall(side.frames, rig.gridW, rig.gridH, rowsPerPixel));
  } else {
    fs.writeFileSync(file, waterfall(side.frames, rig.pixels));
  }
}

const meta = {
  slug,
  label: opts.label,
  epeName: pair.epeName,
  settings: {
    seconds: opts.seconds,
    fps: opts.fps,
    skip: opts.skip,
    seed: opts.seed,
    rig: kind,
    pixels: rig.pixels,
    grid: kind === "grid" ? `${rig.gridW}x${rig.gridH}` : null,
    wallClock: opts.wallClock,
    capturedFrames: captured,
  },
  warnings: runWarnings,
  // Identity of everything that can change a render. A reused run whose
  // provenance differs from a fresh one describes different inputs — discard it.
  provenance: {
    gitSha: gitSha12(),
    portSha256: sha12(fs.readFileSync(libPath)),
    epeSha256: sha12(fs.readFileSync(epePath)),
    // Covers every input that shapes a render: this file, its sibling
    // modules, and the engine wasm itself — not just snap.mjs.
    harnessSha256: sha12(
      Buffer.concat(
        [
          SELF,
          path.join(HERE, "enginehost.mjs"),
          path.join(HERE, "png.mjs"),
          path.join(HERE, "sensormodel.mjs"),
          WASM_PATH,
        ].map((p) => fs.readFileSync(p)),
      ),
    ),
  },
  ...(picks ? { sheetTimesSeconds: picks.map((i) => +(opts.skip + i / opts.fps).toFixed(3)) } : {}),
  ...(stripPicks
    ? {
        stripFrames: stripPicks.length,
        // effective (post-clamp) offset into the captured window
        stripAtSeconds: +(stripPicks[0] / opts.fps).toFixed(3),
        stripTimesSeconds: stripPicks.map((i) => +(opts.skip + i / opts.fps).toFixed(3)),
        rhythmRowsPerPixel: rowsPerPixel,
      }
    : {}),
  sides: Object.fromEntries(
    Object.entries(sides).map(([k, s]) => [
      k,
      {
        image: s.frames ? `${k}.png` : null,
        controls: s.controls,
        controlsApplied: s.controlsApplied,
        warnings: s.warnings,
        compileError: s.compileError,
        runtimeError: s.runtimeError,
        // What this side ACTUALLY got, not what was asked for: `wantsSensors`
        // is the pattern's own property, `sensors` is what was fed.
        wantsSensors: s.wantsSensors,
        sensors: s.sensors,
        ...(s.sensors === "synth" ? { sensorModel: SENSOR_MODEL } : {}),
        statsSummary: s.statsSummary,
      },
    ]),
  ),
};
fs.writeFileSync(path.join(outDir, "meta.json"), stringifyMeta(meta) + "\n");

// Full per-frame series live here, one line per series, so meta.json stays
// short enough to read whole.
const statsFile = {
  slug,
  label: opts.label,
  capturedFrames: captured,
  sides: Object.fromEntries(Object.entries(sides).map(([k, s]) => [k, s.stats])),
};
fs.writeFileSync(path.join(outDir, "stats.json"), stringifyMeta(statsFile) + "\n");

// ---- optional frame dump ---------------------------------------------------

// Exact per-pixel values at the requested moments — what the upscaled PNGs
// alias away. Both sides are dumped from the same frame indices.
const framesPath = path.join(outDir, "frames.json");
let dumpTimes = null;
if (dumpPicks) {
  const framesFile = {
    slug,
    label: opts.label,
    rig: kind,
    pixels: rig.pixels,
    grid: kind === "grid" ? `${rig.gridW}x${rig.gridH}` : null,
    requestedTimes: opts.dump,
    // absolute times on the run's timeline, like meta.json's sheetTimesSeconds
    times: dumpPicks.map((i) => +(opts.skip + i / opts.fps).toFixed(3)),
    frameIndices: dumpPicks,
    ...Object.fromEntries(
      Object.entries(sides)
        .filter(([, s]) => s.frames)
        .map(([k, s]) => [k, dumpPicks.map((i) => dumpFrame(s.frames[i], rig, kind))]),
    ),
  };
  fs.writeFileSync(framesPath, stringifyMeta(framesFile) + "\n");
  dumpTimes = framesFile.times;
} else {
  fs.rmSync(framesPath, { force: true }); // stale dump from a previous run
}

// ---- optional control probe ------------------------------------------------

const probeFile = path.join(outDir, "probe.json");
let probe = null;
if (opts.probeControls) {
  const probeOpts = {
    seed: opts.seed,
    fps: opts.fps,
    skip: opts.skip,
    probeSeconds: opts.probeSeconds,
    // Same feed as the run, so a dial on a sound pattern is probed against a
    // signal rather than against silence.
    sensors: opts.sensors,
    wallClock: opts.wallClock,
  };
  probe = {
    slug,
    label: opts.label,
    settings: {
      seconds: opts.probeSeconds,
      fps: opts.fps,
      skip: opts.skip,
      seed: opts.seed,
      rig: kind,
      pixels: rig.pixels,
      wallClock: opts.wallClock,
      values: PROBE_VALUES,
      threshold: PROBE_THRESHOLD,
      thresholdLit: PROBE_LIT_THRESHOLD,
      probedKinds: [...PROBE_KINDS],
    },
    sides: {
      orig: probeSide(host, origSource, rig, {
        ...probeOpts,
        controls: parseControls(opts.controlsOrig),
      }),
      port: probeSide(host, portSource, rig, {
        ...probeOpts,
        controls: parseControls(opts.controlsPort),
      }),
    },
  };
  fs.writeFileSync(probeFile, stringifyMeta(probe) + "\n");
} else {
  fs.rmSync(probeFile, { force: true }); // stale probe from a previous run
}

const brief = (k) => {
  const s = sides[k];
  if (s.compileError) return `${k}: COMPILE FAILED (${s.compileError})`;
  const b = s.stats.meanBrightness;
  const m = s.stats.motion;
  const avg = (a) => Math.round(a.reduce((x, y) => x + y, 0) / a.length);
  const sensors = s.sensors === "synth" ? ` sensors ${SENSOR_MODEL}` : "";
  return `${k}: mean ${avg(b)} motion ${avg(m)}${sensors}${s.runtimeError ? ` RUNTIME ERR @${s.runtimeError.frame}: ${s.runtimeError.message}` : ""}`;
};
console.log(`${slug} [${kind}, ${rig.pixels}px, ${captured} frames] → ${path.relative(ROOT, outDir)}`);
console.log(`  ${brief("orig")}`);
console.log(`  ${brief("port")}`);
if (dumpTimes)
  console.log(
    dumpTimes.length <= 12
      ? `  dump: frames.json at ${dumpTimes.join("s, ")}s`
      : `  dump: frames.json — ${dumpTimes.length} times, ${dumpTimes[0]}s .. ${dumpTimes[dumpTimes.length - 1]}s (full list in frames.json requestedTimes)`,
  );
if (probe) printProbeTable(probe);
