// Hardware soak + benchmark: run every gallery pattern on a real device,
// sampling fps and vmerr, then measure fps vs pixel count with a reference
// pattern. Writes a markdown report. (tools/soak.mjs is the host-side
// mirror soak; this one exercises the actual firmware + strip.)
//
// Usage (repo root, nix develop): node tools/hw-bench.mjs <device-ip> [report.md]
//
// Restores: rainbow, and the pixel count + brightness it FOUND (it used to
// hardcode a 300 px restore regardless, which quietly reconfigured the rig).
//
// A device that crashes mid-soak is a FINDING, not an abort (2026-09-05: the
// Seengreat panel hard-hung at pattern 75 and the run died with no report):
// an unreachable device after a push becomes a `crashed` row, the harness
// waits for it to come back (RECOVER_MS), and if it doesn't, runs
// HW_BENCH_RESET_CMD (e.g. a USB port open, which resets an S3's native
// USB-Serial/JTAG) before giving up on that pattern. HW_BENCH_FROM=<n>
// resumes at gallery index n (1-based). The report is written even if the
// curve/restore phase fails.
// ~45 min for the current ~320-pattern gallery, one pattern after another on
// the actual strip.

import fs from "node:fs";
// devices take LXP1 envelopes (source + LXBC bytecode), not raw source —
// compile locally via the built playground wasm (needs `npm run wasm` once)
import { lxpBody } from "../web/tools/lxp.mjs";

const IP = process.argv[2] ?? "192.168.0.205";
const OUT = process.argv[3] ?? "docs/bench-report.md";
const DEV = `http://${IP}`;
const DWELL_MS = 2500; // per pattern: settle + let the fps counter refill
const RECOVER_MS = 180_000; // how long to wait for a crashed device to return
const RESET_AFTER_MS = 90_000; // ...before trying HW_BENCH_RESET_CMD (if set)
const RESET_CMD = process.env.HW_BENCH_RESET_CMD;
const FROM = Number(process.env.HW_BENCH_FROM ?? 1);
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
console.log(`device ${IP}: v${status0.version}, ${status0.pixels}px, brightness ${bright0} — soaking ${gallery.length} patterns`);

const rows = [];
const crashes = []; // { after: name, downSecs, reset }
let i = 0;
for (const p of gallery) {
  i++;
  if (i < FROM) continue;
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
    rows.push({ name: p.name, kind: p.kind, fail: back ? `crashed (device back after ${back.secs}s)` : "crashed (device did not come back)" });
    console.log(`${String(i).padStart(3)}/${gallery.length}  -- fps  ${p.name} CRASHED`);
    if (!back) {
      console.log("device did not come back — stopping the sweep, writing what we have");
      break;
    }
    continue;
  }
  rows.push({ name: p.name, kind: p.kind, fps: st.fps, heap: st.heap_free, vmerr: st.vmerr });
  const flag = st.vmerr ? `VMERR ${st.vmerr}` : st.fps < 30 ? "SLOW" : "";
  console.log(`${String(i).padStart(3)}/${gallery.length} ${String(st.fps).padStart(3)} fps  ${p.name} ${flag}`);
}

// fps vs pixel count with the reference pattern. Wrapped so a device that
// dies here still gets the soak table written below.
const curve = [];
const rainbowBody = await lxpBody("", rainbow);
try {
  console.log("-- pixel-count curve --");
  await api("/api/code", rainbowBody);
  for (const n of [60, 150, 300, 600, 1024, 2048]) {
    await api("/api/config", String(n));
    await sleep(3500);
    const st = await api("/api/status");
    curve.push({ pixels: n, fps: st.fps });
    console.log(`${n} px → ${st.fps} fps`);
  }
  // restore exactly what we found — the sweep ran at status0.pixels, and the
  // curve above left the device on 2048
  await api("/api/config", String(status0.pixels));
  await api("/api/brightness", String(bright0));
  await api("/api/code", rainbowBody);
} catch (e) {
  console.log(`curve/restore aborted: ${String(e).slice(0, 80)} — device may need a manual restore`);
}

// report
const errs = rows.filter((r) => r.fail || r.vmerr);
const slow = rows.filter((r) => !r.fail && !r.vmerr && r.fps < 30).sort((a, b) => a.fps - b.fps);
const ok = rows.filter((r) => !r.fail && !r.vmerr);
const fpss = ok.map((r) => r.fps).sort((a, b) => a - b);
const pct = (q) => fpss[Math.min(fpss.length - 1, Math.floor(q * fpss.length))] ?? 0;
const minHeap = Math.min(...ok.map((r) => r.heap));
const lines = [];
lines.push(`# Hardware soak + benchmark — ${new Date().toISOString().slice(0, 10)}`);
lines.push("");
lines.push(`*Device ${IP}, firmware v${status0.version}, ${status0.pixels} px ${proto0}, brightness ${bright0}.*`);
lines.push(`*Regenerate: \`node tools/hw-bench.mjs <ip>\` (≈45 min; runs every gallery pattern on the strip).*`);
lines.push("");
lines.push(`## Summary`);
lines.push("");
lines.push(`- ${gallery.length} patterns: **${ok.length} clean**, ${errs.length} with errors, ${slow.length} under 30 fps.`);
// The sweep runs at whatever count the device was ALREADY on, which is not
// necessarily 300 — a hardcoded "at 300 px" here sent Gitea #193 chasing a
// 300 px repro for a bug that only bites at the 60 px the run actually used.
lines.push(`- fps at ${status0.pixels} px (the count the sweep ran at): median **${pct(0.5)}**, p10 ${pct(0.1)}, p90 ${pct(0.9)}.`);
lines.push(`- lowest heap_free seen while soaking: ${minHeap} bytes.`);
if (crashes.length) {
  lines.push(`- **${crashes.length} device crash${crashes.length === 1 ? "" : "es"}** (unreachable after a push): ` + crashes.map((c) => `after \"${c.after}\" (${c.downSecs === null ? "did not return" : `back in ${c.downSecs}s${c.reset ? ", needed the reset cmd" : ""}`})`).join("; ") + ".");
}
if (FROM > 1) lines.push(`- resumed at pattern ${FROM} (HW_BENCH_FROM) — earlier rows are not in this report.`);
lines.push("");
lines.push(`## fps vs pixel count (rainbow reference)`);
lines.push("");
lines.push(`| pixels | fps |`);
lines.push(`|---:|---:|`);
for (const c of curve) lines.push(`| ${c.pixels} | ${c.fps} |`);
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
lines.push(`| pattern | kind | fps |`);
lines.push(`|---|---|---:|`);
for (const r of rows) lines.push(`| ${r.name} | ${r.kind} | ${r.fail ? "—" : r.fps} |`);
fs.writeFileSync(OUT, lines.join("\n") + "\n");
console.log(`wrote ${OUT}: ${ok.length} ok, ${errs.length} errors, ${slow.length} slow`);
