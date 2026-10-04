// Hardware soak + benchmark: run every gallery pattern on a real device,
// sampling fps and vmerr, then measure fps vs pixel count with a reference
// pattern. Writes a markdown report. (tools/soak.mjs is the host-side
// mirror soak; this one exercises the actual firmware + strip.)
//
// Usage (repo root, nix develop): node tools/hw-bench.mjs <device-ip> [report.md]
//
// --perf-only turns the same sweep into a PERFORMANCE sweep: no pixel-count
// curve, and the report is one row per pattern (fps + the per-stage frame
// timers from Gitea #260) sorted by vm µs, worst first, plus the median/p90
// of the distribution. That table is the baseline the interpreter work
// (#261 superinstructions, #265 two-core split) gets measured against — run
// it when the engine changes; it is not a stability check.
//
// Restores what it FOUND, not rainbow: a playing playlist (at its index), an
// active scene, or the running pattern's own LXP1 (`GET /api/pattern.lxp` up
// front — /api/status carries no pattern id, so a library pattern comes back
// as an ad-hoc copy of itself); rainbow only when nothing was running. Pixel
// count + brightness are re-POSTed only if the curve ran (it used to hardcode
// a 300 px restore regardless, and a brightness POST is a flash write).
//
// --no-curve skips the fps-vs-pixel-count curve; a panel board
// (`caps.panel`) skips it on its own — a panel's pixel count IS its layout,
// and `POST /api/config <n>` is the wrong knob there.
//
// A device that crashes mid-soak is a FINDING, not an abort (2026-09-05: the
// Seengreat panel hard-hung at pattern 75 and the run died with no report):
// an unreachable device after a push becomes a `crashed` row, the harness
// waits for it to come back (RECOVER_MS), and if it doesn't, runs
// HW_BENCH_RESET_CMD (e.g. `timeout 3 socat -u /dev/ttyACM0,raw,echo=0,b115200
// STDOUT` — the termios setup resets an S3's native USB-Serial/JTAG) before
// giving up on that pattern. HW_BENCH_FROM=<n> resumes at gallery index n
// (1-based). The report is written even if the curve/restore phase fails.
//
// A crash that reboots FAST is invisible to "unreachable": a HUB75 S3 is back
// in ~5 s answering, running nothing. So every sample is checked against the
// previous one — `core1.fences[0]` only grows within a boot (by hundreds per
// activation, so only a DECREASE means anything) and `slot` only changes
// across one — and a reboot becomes a `rebooted` row in the crash list. An
// accepted push that leaves `engines` 0 is `not running`.
//
// HW_BENCH_ONLY=<substring> runs only the gallery patterns whose name has it
// (replaying a finding); HW_BENCH_GAP_MS (default 2000) is slept before each
// push so a fragile board gets a breather between device calls.
// ~45 min for the current ~320-pattern gallery, one pattern after another on
// the actual strip.

import fs from "node:fs";
// devices take LXP1 envelopes (source + LXBC bytecode), not raw source —
// compile locally via the built playground wasm (needs `npm run wasm` once)
import { lxpBody } from "../web/tools/lxp.mjs";

const PERF = process.argv.includes("--perf-only");
const NO_CURVE = process.argv.includes("--no-curve");
const POS = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const IP = POS[0] ?? "192.168.0.205";
const OUT = POS[1] ?? (PERF ? "docs/perf-sweep.md" : "docs/bench-report.md");
const DEV = `http://${IP}`;
const DWELL_MS = PERF ? 3000 : 2500; // per pattern: settle + let the fps counter refill
const RECOVER_MS = 180_000; // how long to wait for a crashed device to return
const RESET_AFTER_MS = 90_000; // ...before trying HW_BENCH_RESET_CMD (if set)
const RESET_CMD = process.env.HW_BENCH_RESET_CMD;
const FROM = Number(process.env.HW_BENCH_FROM ?? 1);
const ONLY = process.env.HW_BENCH_ONLY;
const GAP_MS = Number(process.env.HW_BENCH_GAP_MS ?? 2000); // before each push
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function api(path, body) {
  // node's fetch pools connections; picoserve closes idle ones after 1s —
  // "connection: close" avoids that reuse race. A request abandoned by a
  // short client timeout PINS one of the device's three sockets until its
  // server-side read timeout reaps it, and retry storms then cascade — so:
  // a generous timeout, few retries, and a real pause before each one.
  for (let attempt = 0; ; attempt++) {
    try {
      const opts = {
        headers: { connection: "close" },
        signal: AbortSignal.timeout(30_000),
      };
      const r = await fetch(
        DEV + path,
        body === undefined ? opts : { ...opts, method: "POST", body },
      );
      return await r.json();
    } catch (e) {
      if (attempt >= 2) throw e;
      await sleep(8_000); // let the device reap any socket we just pinned
    }
  }
}

/** Per-stage frame timing from /api/status, as a compact console field.
 *  Empty string on firmware that predates the counters (Gitea #260). The
 *  native mirror reports them too since Gitea #262 — measured, with out_us
 *  always 0 (no output driver to time). */
const stages = (st) =>
  st.frame_us === undefined
    ? ""
    : `[${st.frame_us}µs vm ${st.vm_us} pipe ${st.pipe_us} out ${st.out_us}]`;

// raw body (the LXP1 envelope of the running pattern); null on any failure
async function getBytes(path) {
  try {
    const r = await fetch(DEV + path, {
      headers: { connection: "close" },
      signal: AbortSignal.timeout(30_000),
    });
    return r.ok ? new Uint8Array(await r.arrayBuffer()) : null;
  } catch {
    return null;
  }
}

/** The reboot oracle: did the device reboot between two status samples?
 *  Returns "reset <reason>, slot <slot>" or null. Firmware without `core1`
 *  only gets the slot check. */
function rebootedBetween(prev, st) {
  const f0 = prev?.core1?.fences?.[0];
  const f1 = st?.core1?.fences?.[0];
  const fenceDrop = typeof f0 === "number" && typeof f1 === "number" && f1 < f0;
  const slotMoved = prev?.slot !== undefined && st.slot !== prev.slot;
  if (!fenceDrop && !slotMoved) return null;
  return `reset ${st.core1?.last?.reset ?? "?"}, slot ${st.slot}`;
}

// one quick probe, no retries — for "is it back yet?" polling
async function probe() {
  try {
    const r = await fetch(DEV + "/api/status", {
      headers: { connection: "close" },
      // a starved device (1–2 fps pattern, #259) answers in ~10–20 s
      signal: AbortSignal.timeout(20_000),
    });
    return await r.json();
  } catch {
    return null;
  }
}
// Wait for a crashed device to come back; returns the status or null.
async function recover(why) {
  const t0 = Date.now();
  let reset = false;
  console.log(`   device unreachable (${why}) — waiting up to ${RECOVER_MS / 1000}s`);
  while (Date.now() - t0 < RECOVER_MS) {
    await sleep(5_000);
    const st = await probe();
    if (st) {
      console.log(`   back after ${Math.round((Date.now() - t0) / 1000)}s on ${st.slot}${reset ? " (after reset cmd)" : ""}`);
      return { st, reset, secs: Math.round((Date.now() - t0) / 1000) };
    }
    if (!reset && RESET_CMD && Date.now() - t0 > RESET_AFTER_MS) {
      console.log(`   running HW_BENCH_RESET_CMD`);
      try {
        (await import("node:child_process")).execSync(RESET_CMD, { stdio: "ignore", timeout: 30_000 });
      } catch {}
      reset = true;
    }
  }
  return null;
}

const gallery = JSON.parse(fs.readFileSync("web/public/gallery.json", "utf8"));
const rainbow = fs.readFileSync("library/rainbow.js", "utf8");
const status0 = await api("/api/status");
const bright0 = (await api("/api/brightness")).brightness;
// the LED protocol is not in /api/status — the header used to hardcode
// "SK9822" and mislabelled the ws2812 Athom rig on every report it wrote
const proto0 = (await api("/api/config")).protocol ?? "unknown";
// What to put back at the end, most specific first: a playing playlist owns
// the device (an /api/code push stops it), then an active scene, then the
// running pattern's own envelope. Each read is best-effort.
const playlist0 = await api("/api/playlist").catch(() => null);
const scene0 = (await api("/api/scenes").catch(() => null))?.active ?? null;
const pattern0 = status0.engines === 0 ? null : await getBytes("/api/pattern.lxp");
const found = playlist0?.playing
  ? `playlist (item ${playlist0.index})`
  : scene0
    ? `scene ${scene0}`
    : pattern0?.length
      ? "the running pattern"
      : "nothing running";
// a panel's pixel count is its layout — never sweep /api/config there
const curveSkip = PERF ? null : NO_CURVE ? "--no-curve" : status0.caps?.panel ? "panel board" : null;
const todo = gallery.filter((p, k) => k + 1 >= FROM && (!ONLY || p.name.includes(ONLY)));
console.log(`device ${IP}: v${status0.version}, ${status0.pixels}px, slot ${status0.slot}, brightness ${bright0}, found ${found} — ${PERF ? "perf sweep over" : "soaking"} ${todo.length} patterns${ONLY ? ` (HW_BENCH_ONLY=${ONLY})` : ""}`);

const rows = [];
const crashes = []; // { after: name, downSecs, reset } or { after, between, reboot: reason }
let prev = status0; // the last status sample — the reboot oracle's baseline
let prevName = "(start)";
let i = 0;
let pushes = 0;
for (const p of gallery) {
  i++;
  if (i < FROM) continue;
  if (ONLY && !p.name.includes(ONLY)) continue;
  if (pushes++ > 0) await sleep(GAP_MS);
  let res;
  try {
    res = await api("/api/code", await lxpBody("", p.source));
  } catch (e) {
    rows.push({ name: p.name, kind: p.kind, fail: `push failed: ${e}` });
    continue;
  }
  if (!res.ok) {
    rows.push({ name: p.name, kind: p.kind, fail: `rejected: ${res.error}` });
    continue;
  }
  await sleep(DWELL_MS);
  let st;
  try {
    st = await api("/api/status");
  } catch (e) {
    // the device went away after accepting the pattern — a crash. Record it,
    // wait for it to reboot (or reset it), and carry on with the next one.
    let back = await recover(String(e).slice(0, 60));
    if (!back) {
      // last try with the patient api() (30 s × 3) before giving up on the run
      try {
        const st2 = await api("/api/status");
        back = { st: st2, reset: true, secs: Math.round(RECOVER_MS / 1000) };
      } catch {}
    }
    crashes.push({ after: p.name, downSecs: back?.secs ?? null, reset: back?.reset ?? false });
    if (back) {
      prev = back.st; // a new boot: its fences restart, compare against it
      prevName = p.name;
    }
    rows.push({ name: p.name, kind: p.kind, fail: back ? `crashed (device back after ${back.secs}s)` : "crashed (device did not come back)" });
    console.log(`${String(i).padStart(3)}/${gallery.length}  -- fps  ${p.name} CRASHED`);
    if (!back) {
      console.log("device did not come back — stopping the sweep, writing what we have");
      break;
    }
    continue;
  }
  // `drops` (why the output task did not show a frame: no hand-off buffer,
  // overwritten before it was taken, refused by the driver) is the key to a
  // row that reads `pipe 0 out 0` — on 2026-10-04 six of seven crashes
  // followed such a row, so the counters travel with every row
  const extra = {
    heap_largest: st.heap_largest,
    engines: st.engines,
    jit: st.jit?.state,
    out_fps: st.out_fps,
    drops: st.drops ? `${st.drops.handoff}/${st.drops.overwrite}/${st.drops.refused}` : undefined,
    // the HUB75 ring's own health on this row: late/torn slots, DMA and LCD
    // restarts, DMA errors — a panel board only
    ring: st.pass?.ring
      ? `late ${st.pass.ring.late} torn ${st.pass.ring.torn} dma_rs ${st.pass.ring.dma_restarts} lcd_rs ${st.pass.ring.lcd_restarts} dma_err ${st.pass.ring.dma_errors}`
      : undefined,
  };
  // a fast reboot (back before the sample) — invisible to the catch above
  const reboot = rebootedBetween(prev, st);
  const between = prevName;
  prev = st;
  prevName = p.name;
  if (reboot) {
    crashes.push({ after: p.name, between, reboot });
    rows.push({ name: p.name, kind: p.kind, fail: `rebooted (${reboot})`, ...extra });
    console.log(`${String(i).padStart(3)}/${gallery.length}  -- fps  ${p.name} REBOOTED (${reboot}; since "${between}")`);
    continue;
  }
  if (st.engines === 0) {
    // accepted, but nothing resident — the push never became a running engine
    rows.push({ name: p.name, kind: p.kind, fail: "not running (engines 0)", ...extra });
    console.log(`${String(i).padStart(3)}/${gallery.length}  -- fps  ${p.name} NOT RUNNING (engines 0)`);
    continue;
  }
  rows.push({
    name: p.name,
    kind: p.kind,
    fps: st.fps,
    heap: st.heap_free,
    vmerr: st.vmerr,
    ...extra,
    // per-stage frame timing (µs/frame), firmware ≥ the /api/status
    // frame_us field — undefined only on firmware that predates it (#262 gave
    // the mirror the same four)
    frame_us: st.frame_us,
    vm_us: st.vm_us,
    pipe_us: st.pipe_us,
    out_us: st.out_us,
  });
  const flag = st.vmerr ? `VMERR ${st.vmerr}` : st.fps < 30 ? "SLOW" : "";
  console.log(
    `${String(i).padStart(3)}/${gallery.length} ${String(st.fps).padStart(3)} fps ${stages(st)} lb ${st.heap_largest ?? "—"} ${st.jit?.state ?? ""}${extra.drops && extra.drops !== "0/0/0" ? ` drops ${extra.drops}` : ""} ${p.name} ${flag}`,
  );
}

// fps vs pixel count with the reference pattern. Wrapped so a device that
// dies here still gets the soak table written below.
const curve = [];
const rainbowBody = await lxpBody("", rainbow);
try {
  if (curveSkip) console.log(`-- pixel-count curve skipped (${curveSkip}) --`);
  if (!PERF && !curveSkip) {
    console.log("-- pixel-count curve --");
    await sleep(GAP_MS);
    await api("/api/code", rainbowBody);
    for (const n of [60, 150, 300, 600, 1024, 2048]) {
      await api("/api/config", String(n));
      await sleep(3500);
      const st = await api("/api/status");
      curve.push({ pixels: n, fps: st.fps });
      console.log(`${n} px → ${st.fps} fps`);
    }
    // the sweep ran at status0.pixels, and the curve left the device on 2048.
    // Brightness is never changed by any of this — re-POSTed only here,
    // alongside the count, because that is what the curve's restore always did
    await api("/api/config", String(status0.pixels));
    await api("/api/brightness", String(bright0));
  }
  await sleep(GAP_MS);
  if (playlist0?.playing) await api("/api/playlist/play", String(playlist0.index ?? 0));
  else if (scene0) await api(`/api/scenes/${scene0}/activate`, "");
  else if (pattern0?.length) await api("/api/code", pattern0);
  else await api("/api/code", rainbowBody);
  console.log(`restored: ${pattern0?.length || scene0 || playlist0?.playing ? found : "rainbow (nothing was running)"}`);
} catch (e) {
  console.log(`curve/restore aborted: ${String(e).slice(0, 80)} — device may need a manual restore`);
}

// ---- perf-only report: one row per pattern, sorted by vm µs (worst first)
if (PERF) {
  // Only rows that actually rendered: a pattern the engine refused (fail) or
  // that faulted (vmerr) reports fps from the last good pattern and 0 µs
  // timers — counting those would drag the median toward zero.
  const timed = rows.filter(
    (r) => !r.fail && !r.vmerr && r.vm_us !== undefined && r.frame_us > 0,
  );
  const by = (f) => timed.map(f).sort((x, y) => x - y);
  const q = (arr, p) => arr[Math.min(arr.length - 1, Math.floor(p * arr.length))] ?? 0;
  const vms = by((r) => r.vm_us);
  const frames = by((r) => r.frame_us);
  const fpsv = by((r) => r.fps);
  const out = [];
  out.push(`# Pattern performance sweep \u2014 ${new Date().toISOString().slice(0, 10)}`);
  out.push("");
  out.push(`*Device ${IP}, firmware v${status0.version}, ${status0.pixels} px ${proto0}, brightness ${bright0}.*`);
  out.push("*Regenerate: `node tools/hw-bench.mjs <ip> <report.md> --perf-only`.*");
  out.push("");
  out.push(`- ${timed.length} of ${todo.length} gallery patterns measured (${rows.length - timed.length} rejected, crashed or untimed).`);
  out.push(`- vm \u00b5s/frame: median **${q(vms, 0.5)}**, p90 **${q(vms, 0.9)}**, max ${vms[vms.length - 1] ?? 0}.`);
  out.push(`- frame \u00b5s: median ${q(frames, 0.5)}, p90 ${q(frames, 0.9)}. fps: median **${q(fpsv, 0.5)}**, p10 ${q(fpsv, 0.1)}.`);
  out.push("");
  out.push("| # | pattern | kind | fps | frame \u00b5s | vm \u00b5s | pipe \u00b5s | out \u00b5s | heap free |");
  out.push("|---:|---|---|---:|---:|---:|---:|---:|---:|");
  const sorted = [...timed].sort((x, y) => y.vm_us - x.vm_us);
  sorted.forEach((r, i) => out.push(`| ${i + 1} | ${r.name} | ${r.kind} | ${r.fps} | ${r.frame_us} | ${r.vm_us} | ${r.pipe_us} | ${r.out_us} | ${r.heap} |`));
  const bad = rows.filter((r) => r.fail || r.vmerr);
  if (bad.length) {
    out.push("");
    out.push("## Not measured");
    out.push("");
    out.push("| pattern | kind | why |");
    out.push("|---|---|---|");
    for (const r of bad) out.push(`| ${r.name} | ${r.kind} | ${r.fail ?? r.vmerr} |`);
  }
  fs.writeFileSync(OUT, out.join("\n") + "\n");
  console.log(`wrote ${OUT}: ${timed.length} measured, ${bad.length} not measured`);
  process.exit(0);
}

// report
const errs = rows.filter((r) => r.fail || r.vmerr);
const slow = rows.filter((r) => !r.fail && !r.vmerr && r.fps < 30).sort((a, b) => a.fps - b.fps);
const ok = rows.filter((r) => !r.fail && !r.vmerr);
const fpss = ok.map((r) => r.fps).sort((a, b) => a - b);
const pct = (q) => fpss[Math.min(fpss.length - 1, Math.floor(q * fpss.length))] ?? 0;
const minHeap = Math.min(...ok.map((r) => r.heap));
const reboots = crashes.filter((c) => c.reboot);
const unreach = crashes.filter((c) => !c.reboot);
const lines = [];
lines.push(`# Hardware soak + benchmark — ${new Date().toISOString().slice(0, 10)}`);
lines.push("");
lines.push(`*Device ${IP}, firmware v${status0.version}, ${status0.pixels} px ${proto0}, brightness ${bright0}.*`);
lines.push(`*Regenerate: \`node tools/hw-bench.mjs <ip>\` (≈45 min; runs every gallery pattern on the strip).*`);
lines.push("");
lines.push(`## Summary`);
lines.push("");
lines.push(`- ${todo.length} patterns${todo.length < gallery.length ? ` (of ${gallery.length})` : ""}: **${ok.length} clean**, ${errs.length} with errors, ${slow.length} under 30 fps.`);
// The sweep runs at whatever count the device was ALREADY on, which is not
// necessarily 300 — a hardcoded "at 300 px" here sent Gitea #193 chasing a
// 300 px repro for a bug that only bites at the 60 px the run actually used.
lines.push(`- fps at ${status0.pixels} px (the count the sweep ran at): median **${pct(0.5)}**, p10 ${pct(0.1)}, p90 ${pct(0.9)}.`);
lines.push(`- lowest heap_free seen while soaking: ${minHeap} bytes.`);
if (crashes.length) {
  lines.push(`- **${crashes.length} device crash${crashes.length === 1 ? "" : "es"}** (${unreach.length} unreachable after a push, **${reboots.length} reboot${reboots.length === 1 ? "" : "s"}** caught by the fences/slot oracle).`);
  if (unreach.length) lines.push(`  - unreachable: ` + unreach.map((c) => `after \"${c.after}\" (${c.downSecs === null ? "did not return" : `back in ${c.downSecs}s${c.reset ? ", needed the reset cmd" : ""}`})`).join("; ") + ".");
  // the reboot happened somewhere between the two samples — either pattern
  if (reboots.length) lines.push(`  - rebooted: ` + reboots.map((c) => `at \"${c.after}\" (since \"${c.between}\"; ${c.reboot})`).join("; ") + ".");
}
if (FROM > 1) lines.push(`- resumed at pattern ${FROM} (HW_BENCH_FROM) — earlier rows are not in this report.`);
if (ONLY) lines.push(`- only patterns whose name contains \"${ONLY}\" (HW_BENCH_ONLY).`);
lines.push("");
lines.push(`## fps vs pixel count (rainbow reference)`);
lines.push("");
if (curveSkip) lines.push(`skipped (${curveSkip}).`);
else {
  lines.push(`| pixels | fps |`);
  lines.push(`|---:|---:|`);
  for (const c of curve) lines.push(`| ${c.pixels} | ${c.fps} |`);
}
lines.push("");
if (errs.length) {
  lines.push(`## Errors`);
  lines.push("");
  lines.push(`| pattern | kind | problem |`);
  lines.push(`|---|---|---|`);
  for (const r of errs) lines.push(`| ${r.name} | ${r.kind} | ${r.fail ?? r.vmerr} |`);
  lines.push("");
}
if (slow.length) {
  lines.push(`## Slowest (< 30 fps at ${status0.pixels} px)`);
  lines.push("");
  lines.push(`| pattern | kind | fps |`);
  lines.push(`|---|---|---:|`);
  for (const r of slow) lines.push(`| ${r.name} | ${r.kind} | ${r.fps} |`);
  lines.push("");
}
lines.push(`## All results`);
lines.push("");
// the µs columns only exist on firmware that reports the per-stage timers
const timed = rows.some((r) => r.frame_us !== undefined);
const us = (v) => (v === undefined ? "—" : v);
lines.push(timed ? `| pattern | kind | fps | heap largest | jit | drops | frame µs | vm µs | pipe µs | out µs |` : `| pattern | kind | fps | heap largest | jit | drops |`);
lines.push(timed ? `|---|---|---:|---:|---|---|---:|---:|---:|---:|` : `|---|---|---:|---:|---|---|`);
for (const r of rows) {
  const head = `| ${r.name} | ${r.kind} | ${r.fail ? "—" : r.fps} | ${us(r.heap_largest)} | ${us(r.jit)} | ${us(r.drops)}${r.ring ? ` (${r.ring})` : ""} |`;
  lines.push(
    timed ? `${head} ${us(r.frame_us)} | ${us(r.vm_us)} | ${us(r.pipe_us)} | ${us(r.out_us)} |` : head,
  );
}
fs.writeFileSync(OUT, lines.join("\n") + "\n");
console.log(`wrote ${OUT}: ${ok.length} ok, ${errs.length} errors, ${slow.length} slow`);
