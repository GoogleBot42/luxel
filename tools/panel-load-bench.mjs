#!/usr/bin/env node
// Before/after harness for Gitea #395 — does moving the HUB75 compose to an
// interrupt-priority executor stop web/net work from delaying it?
//
// The thesis under test: with vsync the compose must finish inside one panel
// rescan (8.7 ms at 30 MHz, of which it uses 7.4), and it shares a cooperative
// executor with the web server. A request handler that runs >1.3 ms without
// yielding pushes the swap past the wrap, and the panel rescans the previous
// frame once more — a REPEAT. So repeats should track web load, and this
// measures exactly that.
//
// Usage:
//   node tools/panel-load-bench.mjs <ip> [--idle SEC] [--tab SEC] [--busy SEC]
//                                        [--clients N] [--label NAME] [--out FILE]
// Defaults: --idle 300 --tab 180 --busy 300 --clients 3; 0 disables a phase.
//
// NOTE on --clients: 3 clients looping the bundle saturate the device's
// 3-socket web pool so completely that the sampler's own /api/status is
// refused and the busy phase yields NO data (seen on the Seengreat panel,
// 2026-09-07). Use --clients 1 for a busy phase you can actually read.
//
// Three phases: idle (nothing but the sampler), tab (ONE playground tab —
// 1 Hz status + pattern list, the realistic case), and busy (N clients
// hammering, deliberately harsher than real usage).
//
// Since #398 the repeat count is EXACT: `pass.repeats`/`pass.n` are ISR
// counters, so a start/end delta is the true number over the phase. The old
// (rescan_hz - out_fps) integral is still reported so a comparison against a
// pre-#398 build still has one common column.
//
// Run it twice — once on the current build, once after the change — with
// --label before / --label after, then diff the two reports.
//
// It only ever GETs. Nothing here writes to the device.

const args = process.argv.slice(2);
const ip = args[0];
if (!ip || ip.startsWith("--")) {
  console.error("usage: panel-load-bench.mjs <ip> [--idle SEC] [--tab SEC] [--busy SEC] [--clients N] [--label NAME] [--out FILE]");
  process.exit(2);
}
const flag = (name, dflt) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : dflt;
};
const IDLE_S = Number(flag("idle", 300));
const BUSY_S = Number(flag("busy", 300));
// One-open-tab phase (Gitea #395): the realistic middle case between idle and
// the deliberately-heavy busy phase. 0 disables it.
const TAB_S = Number(flag("tab", 180));
const CLIENTS = Number(flag("clients", 3));
const LABEL = flag("label", "run");
const OUT = flag("out", null);

const base = `http://${ip}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---------------------------------------------------------------- helpers

/** Timed GET. Returns {ok, ms, bytes, status} and never throws. */
async function timedGet(path, timeoutMs = 20000) {
  const t0 = performance.now();
  try {
    const r = await fetch(base + path, { signal: AbortSignal.timeout(timeoutMs) });
    const buf = await r.arrayBuffer();
    return { ok: r.ok, ms: performance.now() - t0, bytes: buf.byteLength, status: r.status };
  } catch (e) {
    return { ok: false, ms: performance.now() - t0, bytes: 0, status: 0, err: String(e.name || e) };
  }
}

async function status(timeoutMs = 20000) {
  const t0 = performance.now();
  try {
    const r = await fetch(`${base}/api/status`, { signal: AbortSignal.timeout(timeoutMs) });
    const j = await r.json();
    return { ok: true, ms: performance.now() - t0, j };
  } catch (e) {
    return { ok: false, ms: performance.now() - t0, err: String(e.name || e) };
  }
}

const pct = (xs, p) => {
  if (!xs.length) return NaN;
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.floor((p / 100) * s.length))];
};
const mean = (xs) => (xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : NaN);
const f1 = (x) => (Number.isFinite(x) ? x.toFixed(1) : "-");

// ------------------------------------------------------- bundle discovery

/** The playground's biggest asset, found from the served index rather than
 *  guessed, so a rebuilt bundle's hashed name does not break the harness. */
async function findBundle() {
  try {
    const r = await fetch(`${base}/`, { signal: AbortSignal.timeout(20000) });
    const html = await r.text();
    const hits = [...html.matchAll(/(?:src|href)="([^"]+\.(?:js|css))"/g)].map((m) => m[1]);
    const js = hits.find((h) => h.endsWith(".js")) ?? hits[0];
    if (!js) return null;
    return js.startsWith("http") ? new URL(js).pathname : js.startsWith("/") ? js : `/${js}`;
  } catch {
    return null;
  }
}

// ------------------------------------------------------------- sampling

/** Panel counters.
 *
 *  `pass.repeats` / `pass.n` are the EXACT figures: since #398 the driver's
 *  own ISR counts every rescan pass and every pass that re-showed the frame
 *  before it, so a start/end delta is the true repeat count over the phase,
 *  independent of how often we sample. Prefer those.
 *
 *  `rescan_hz - out_fps` integrated over the phase estimates the same thing
 *  from two published rates and is kept because it is the only figure a
 *  pre-#398 build can produce — so a before/after across that boundary still
 *  has one comparable column. `dropped` is the #394 ground truth for frames
 *  rendered and never shown at all — a different failure. */
function panel(j) {
  return {
    fps: j.fps ?? 0,
    out_fps: j.out_fps ?? 0,
    rescan_hz: j.rescan_hz ?? 0,
    passN: j.pass?.n ?? null,
    passRepeats: j.pass?.repeats ?? null,
    passSkips: j.pass?.skips ?? null,
    passPerFrameMax: j.pass?.per_frame_max ?? null,
    dropped: j.dropped ?? 0,
    eof_race: j.swap?.eof_race ?? 0,
    slow_path: j.swap?.slow_path ?? 0,
    fence_timeouts: j.core1?.fence_timeouts ?? 0,
    fence_wait_us: j.core1?.fence_wait_us ?? 0,
    heap_free: j.heap_free ?? 0,
    vmerr: j.vmerr ?? null,
  };
}

/** One phase: sample /api/status every `everyMs`, optionally with `load`
 *  running concurrently. Integrating (rescan_hz - out_fps) over the phase is
 *  the repeat count; both are per-second rates the firmware publishes. */
async function phase(name, seconds, everyMs, loadFn) {
  const samples = [];
  const statusMs = [];
  let statusFail = 0;
  let repeats = 0;
  let covered = 0; // seconds the integral actually spans
  let stop = false;
  const loadStats = { reqs: 0, fails: 0, ms: [], bytes: 0 };
  const loaders = [];
  if (loadFn) for (let i = 0; i < CLIENTS; i++) loaders.push(loadFn(() => stop, loadStats));

  const t0 = Date.now();
  let last = null;
  while ((Date.now() - t0) / 1000 < seconds) {
    const s = await status();
    if (!s.ok) {
      statusFail++;
    } else {
      statusMs.push(s.ms);
      const p = panel(s.j);
      const now = Date.now();
      if (last) {
        // Rectangle rule over the gap between samples. Normalise by the span
        // the integral COVERS, not by wall time: the two phases sample at
        // different rates, and dividing by wall time would make them
        // incomparable (it silently under-reports the sparsely sampled one).
        const dt = (now - last.t) / 1000;
        repeats += Math.max(0, p.rescan_hz - p.out_fps) * dt;
        covered += dt;
      }
      samples.push(p);
      last = { t: now, p };
    }
    await sleep(everyMs);
  }
  stop = true;
  await Promise.allSettled(loaders);

  const first = samples[0], lastS = samples[samples.length - 1];
  const elapsed = (Date.now() - t0) / 1000;
  /** Start-to-end delta of a cumulative counter, or null if the build lacks it. */
  const dPass = (k) =>
    first && lastS && first[k] !== null && lastS[k] !== null ? lastS[k] - first[k] : null;
  const reboot = first && lastS && (lastS.eof_race < first.eof_race || lastS.dropped < first.dropped);
  return {
    name, elapsed, samples: samples.length, statusFail, reboot,
    fps: mean(samples.map((s) => s.fps)),
    out_fps: mean(samples.map((s) => s.out_fps)),
    rescan_hz: mean(samples.map((s) => s.rescan_hz)),
    repeatsPerMin: covered > 0 ? (repeats / covered) * 60 : NaN,
    coveredS: covered,
    // Exact, from the driver's per-pass ISR counter (#398).
    passRepeats: dPass("passRepeats"),
    passN: dPass("passN"),
    repeatsPerMinExact:
      elapsed > 0 && dPass("passRepeats") !== null ? (dPass("passRepeats") / elapsed) * 60 : NaN,
    repeatPct:
      dPass("passN") ? (dPass("passRepeats") / dPass("passN")) * 100 : NaN,
    passSkips: dPass("passSkips"),
    passPerFrameMax: Math.max(...samples.map((s) => s.passPerFrameMax ?? 0), 0),
    dropped: first && lastS ? lastS.dropped - first.dropped : NaN,
    eof_race: first && lastS ? lastS.eof_race - first.eof_race : NaN,
    slow_path: first && lastS ? lastS.slow_path - first.slow_path : NaN,
    fence_timeouts: lastS ? lastS.fence_timeouts : NaN,
    fence_wait_us: Math.max(...samples.map((s) => s.fence_wait_us), 0),
    heapMin: Math.min(...samples.map((s) => s.heap_free)),
    vmerr: samples.map((s) => s.vmerr).find((v) => v) ?? null,
    statusP50: pct(statusMs, 50), statusP95: pct(statusMs, 95), statusMax: Math.max(...statusMs, 0),
    load: loadStats,
  };
}

// ----------------------------------------------------------------- load

/** What a playground tab actually does to a busy device: poll status, read
 *  the whole pattern list, and pull the bundle. The pattern read is the one
 *  that matters — it is a large synchronous copy out of the mapped store. */
/** One open playground tab, which is what Jeremy actually has running while
 *  he films: a 1 Hz status poll and a pattern-list read, nothing else. The
 *  `busy` phase above is deliberately heavier than real usage; this one is
 *  the realistic middle case, and repeats under it are the number that
 *  matches what he sees (Gitea #395). */
function tabLoad() {
  return async (stopped, stats) => {
    while (!stopped()) {
      for (const p of ["/api/status", "/api/patterns"]) {
        if (stopped()) break;
        const r = await timedGet(p);
        stats.reqs++;
        if (!r.ok) stats.fails++;
        else { stats.ms.push(r.ms); stats.bytes += r.bytes; }
      }
      await sleep(1000);
    }
  };
}

function playgroundLoad(bundlePath) {
  return async (stopped, stats) => {
    while (!stopped()) {
      for (const p of ["/api/patterns", "/api/status", bundlePath].filter(Boolean)) {
        if (stopped()) break;
        const r = await timedGet(p);
        stats.reqs++;
        if (!r.ok) stats.fails++;
        else { stats.ms.push(r.ms); stats.bytes += r.bytes; }
      }
    }
  };
}

// ------------------------------------------------------------------ main

const bundlePath = await findBundle();
console.log(`# panel-load-bench ${LABEL} — ${ip}`);
console.log(`bundle: ${bundlePath ?? "(not found; skipping bundle fetches)"}`);

// Cold bundle timing before any load, three tries, best-of.
const cold = [];
if (bundlePath) for (let i = 0; i < 3; i++) cold.push((await timedGet(bundlePath)).ms);

console.log(`\n## idle ${IDLE_S}s (status every 10s, no other load)`);
const idle = IDLE_S > 0 ? await phase("idle", IDLE_S, 10000, null) : null;
console.log(`\n## tab ${TAB_S}s (one playground tab: 1 Hz /api/status + /api/patterns, status every 5s)`);
const tab = TAB_S > 0 ? await phase("tab", TAB_S, 5000, tabLoad()) : null;
console.log(`\n## busy ${BUSY_S}s (${CLIENTS} clients: /api/patterns + /api/status + bundle, status every 2s)`);
const busy = BUSY_S > 0 ? await phase("busy", BUSY_S, 2000, playgroundLoad(bundlePath)) : null;

const rows = [idle, tab, busy].filter(Boolean);
const report = [];
const say = (s) => { report.push(s); console.log(s); };

say(`\n=== ${LABEL} ===`);
say(`bundle cold GET (best of 3): ${f1(Math.min(...cold))} ms` + (cold.length ? ` (all: ${cold.map(f1).join(", ")})` : ""));
say("");
say("| phase | fps | out_fps | rescan_hz | repeats/min (exact) | repeat % of passes | repeats/min (est) | skips | dropped | eof_race | slow_path | status p50/p95/max ms |");
say("|---|---:|---:|---:|---:|---:|---:|---:|---:|");
for (const r of rows) {
  say(`| ${r.name} | ${f1(r.fps)} | ${f1(r.out_fps)} | ${f1(r.rescan_hz)} | **${f1(r.repeatsPerMinExact)}** | ${f1(r.repeatPct)} % | ${f1(r.repeatsPerMin)} | ${r.passSkips ?? "-"} | ${r.dropped} | ${r.eof_race} | ${r.slow_path} | ${f1(r.statusP50)}/${f1(r.statusP95)}/${f1(r.statusMax)} |`);
}
say("");
for (const r of rows) {
  const l = r.load;
  const thru = l.bytes / Math.max(1, r.elapsed) / 1024;
  say(`${r.name}: ${r.samples} samples, ${r.statusFail} status failures` +
      ` over ${f1(r.coveredS)} s of integrated span` +
      (l.reqs ? `, ${l.reqs} load reqs (${l.fails} failed), req p50 ${f1(pct(l.ms, 50))} ms / p95 ${f1(pct(l.ms, 95))} ms, ${f1(thru)} KiB/s` : "") +
      `, heap min ${r.heapMin}, fence_timeouts ${r.fence_timeouts}, longest fence wait ${r.fence_wait_us} us` +
      (r.reboot ? "  ** DEVICE REBOOTED DURING THIS PHASE **" : "") +
      (r.vmerr ? `  ** vmerr: ${r.vmerr} **` : ""));
}
say("");
say(`WiFi stability: ${idle.statusFail + busy.statusFail} status failures and ` +
    `${(idle.load.fails ?? 0) + (busy.load.fails ?? 0)} load failures over ${f1(idle.elapsed + busy.elapsed)} s; ` +
    `reboot detected: ${idle.reboot || busy.reboot ? "YES" : "no"}`);

if (OUT) {
  const { writeFileSync } = await import("node:fs");
  writeFileSync(OUT, report.join("\n") + "\n\n```json\n" + JSON.stringify({ label: LABEL, cold, idle, busy }, null, 2) + "\n```\n");
  console.log(`\nwrote ${OUT}`);
}
