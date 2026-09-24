#!/usr/bin/env node
// The on-metal JIT differential: native vs interpreted pixels on a REAL
// board, over HTTP (Gitea #665, docs/jit-design.md §7.3 step 1).
//
// This is the hardware counterpart of `tools/qemu/jit-test.py`, and it
// answers the one question that gate cannot: does the emitted code compute
// the interpreter's pixels on the actual S3 that carries the JIT — its
// PSRAM, its cache, its two cores, its render task — rather than on QEMU's
// classic-ESP32 model. Same shape as the QEMU gate: for each pattern, run
// it with the JIT off, snapshot the frame, run the SAME blob with the JIT
// on, snapshot again, compare bit for bit. `POST /api/jit` applies at the
// NEXT activation (docs/api.md), which is exactly why the differential is
// "set the switch, push the pattern, read" and not "flip it mid-frame".
//
// WHY HTTP IS A WEAKER ORACLE THAN THE QEMU GATE
// ----------------------------------------------
// **The clock cannot be frozen, or even held still.** Under QEMU the guest
// clock is virtual and both sides reach frame one at t=0 of their own boot,
// so most clock-reading patterns still matched bit for bit; the only skew
// was the native side's `compile_us`. Here there is one long-lived engine
// on a device that has been up for hours, the render loop free-runs, and
// the two snapshots are a `--settle` apart — SECONDS, not microseconds. So
// any pattern that is a function of `time()`, of `beforeRender`'s `delta`,
// or of `random()` will differ every single run, and that is not a finding.
// It is filtered the same way jit-test.py filters it, by the same list plus
// `random(` (the QEMU gate can leave `random()` out because a fresh boot
// re-seeds it identically; two activations of one long-lived engine cannot
// promise that). A MATCH is never explained away, and a mismatch in a
// pattern that reads no clock is a failure — the same rule, and it is the
// only assertion here that binds.
//
// Before declaring that failure the harness re-runs the interpreted side
// and reads `/api/pixels` twice: if the interpreter's own two frames
// disagree, the pattern has a time or state dependence the source scan
// missed and there is nothing to compare, so the row is `unstable` rather
// than a bogus `native-mismatch`. That check is lazy — it costs nothing
// except on a row that was about to fail.
//
// WHAT IT CANNOT PROVE
// --------------------
// * **Not the serial narration.** The QEMU gate reads `jit: native, N fns,
//   B B code …` off the console; the S3's USB-Serial/JTAG resets the chip
//   when opened (CLAUDE.md), so everything here comes from `/api/status`'s
//   `jit` object — state, reason, `code_bytes`, `compile_us`. Enough to
//   know what ran and why; not the function count or the pool split.
// * **Not frame-to-frame state.** One snapshot per side, taken after the
//   settle. `crates/luxel-jit/tests/library_diff.rs` and `engine_diff.rs`
//   cover multi-frame state on the host with a frozen clock; this covers
//   one real frame off a real board.
// * **Not a crash-free verdict beyond the patterns it ran.** A codegen bug
//   on metal is a core-1 fault and a watchdog reboot (§7.3), which arrives
//   here as a `crashed` row — recorded, waited out for three minutes like
//   `hw-bench.mjs` does, and the sweep carries on.
// * `/api/jit` is not persisted: a reboot comes back with the JIT ON. The
//   interpreted side therefore ASSERTS `jit.state == "interp"` with reason
//   `disabled` — that assertion is what catches a reboot that slipped
//   between the two halves, and a row whose baseline is not the
//   interpreter is `no-baseline`, never a comparison.
// * `/api/pixels` is the ENGINE's frame, before the device output chain
//   (docs/api.md), so Settings blur/glow/palette cannot perturb the
//   comparison. A pattern's own `setBlur` is inside the snapshot and
//   applies identically to both sides.
//
// OUTCOMES (one per pattern; the summary counts them all)
//   native-identical  ran natively, pixels byte-identical — the win
//   native-clock      ran natively, pixels differ, and the source reads a
//                     wall clock or `random()` — reported, not asserted
//   native-mismatch   ran natively, pixels differ, no such input — FAILURE
//   unstable          the interpreter's own frame moves between two reads,
//                     so the pattern is not comparable over HTTP at all
//   refused:<reason>  the JIT declined it and it ran interpreted — a
//                     CORRECT outcome (docs/jit-design.md §4a), not a bug
//   vmerr             a runtime error on the native side — FAILURE
//   both-vmerr        the same runtime error on both sides — the pattern's,
//                     not the JIT's
//   no-pixels         one or both sides published no frame, so nothing to
//                     compare (an empty `/api/pixels` body is "no snapshot
//                     right now", including the heap-pressure degrade) —
//                     and it is a FAILURE in the one asymmetric case, a
//                     native side that published nothing while the
//                     interpreter did: that is a dead render task
//   no-baseline       the interpreted side did not report interp/disabled —
//                     the switch did not take, or the device rebooted
//   push-rejected     the device refused the blob (an LXBC format bump
//                     across an OTA looks like this — Gitea #643)
//   crashed           the device went away after a push
//
// Exit 0 iff there is no `native-mismatch`, no native-side `vmerr`, no
// `crashed`, no `no-baseline` and no blank native frame.
//
// USAGE (repo root, inside nix develop)
//   node tools/jit-diff.mjs <ip> [--patterns a.js,b.js | @five] [--from N]
//                                [--pixels N] [--settle MS]
//                                [--json out.json] [--report out.md]
//
//   --patterns  comma-separated `library/` names, or @five for the
//               docs/jit-design.md §7.1 set. Default: every library/*.js,
//               the same list gen-gallery and hw-bench sweep.
//   --from N    resume at 1-based index N (hw-bench's HW_BENCH_FROM).
//   --pixels N  run at this pixel count; the found count is restored.
//   --settle MS wait after each push before reading (default 5000).
//
// The device is left as it was found: the pattern that was running
// (`GET /api/pattern.lxp` up front, re-POSTed at the end), the pixel count,
// and the JIT switch back to whatever `jit.state` said at the start.
// Needs `web/public/luxel.wasm` (`npm run wasm` once) — patterns are
// compiled locally into LXP1 envelopes, exactly as hw-bench does it.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { lxpBody } from "../web/tools/lxp.mjs";

const ROOT = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const argv = process.argv.slice(2);
const opt = (name, dflt) => {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 && argv[i + 1] !== undefined ? argv[i + 1] : dflt;
};
// every flag here takes a value, so a token is positional unless it starts
// with `--` or directly follows one
const positional = argv.filter(
  (a, i) => !a.startsWith("--") && !(i > 0 && argv[i - 1].startsWith("--")),
);
const IP = positional[0];
if (!IP) {
  console.error(
    "usage: node tools/jit-diff.mjs <ip> [--patterns a.js,b.js|@five] [--from N] " +
      "[--pixels N] [--settle MS] [--json out.json] [--report out.md]",
  );
  process.exit(2);
}
const DEV = `http://${IP}`;
const SETTLE = Number(opt("settle", "5000"));
const FROM = Number(opt("from", "1"));
const PIXELS = opt("pixels", null) === null ? null : Number(opt("pixels", "0"));
const JSON_OUT = opt("json", null);
const REPORT_OUT = opt("report", null);

// One HTTP call's budget. A 1–2 fps pattern makes /api/status take ~11 s on
// the panel (.claude/skills/seengreat-panel, Gitea #259) and anything under
// that reads a live device as dead.
const CALL_TIMEOUT_MS = 20_000;
const RECOVER_MS = 180_000; // how long a crashed device gets to come back

// The five of docs/jit-design.md §7.1, same set as tools/qemu/jit-test.py:
// two strip patterns, a 2D one, the #260 noise probe, and a renderFrame one.
const FIVE = [
  "rainbow.js",
  "snake.js",
  "snake-2d.js",
  "perlin-fire-wind-tunnel.js",
  "bulk-canvas-ripples-2d.js",
];

// Inputs that are NOT a function of the pattern across two activations of a
// long-lived engine. jit-test.py's list, plus `random(`: that gate can leave
// it out because each side is a fresh boot with the same constant seed, and
// here both sides share one engine whose RNG has advanced by an unknown
// number of draws.
const WALL_CLOCK = [
  "time(",
  "random(",
  "clockHour",
  "clockMinute",
  "clockSecond",
  "clockYear",
  "clockMonth",
  "clockDay",
  "clockWeekday",
];
// `beforeRender(delta)` is the other one, and it is a parameter rather than
// a builtin: `delta` is real elapsed milliseconds, so anything integrating
// it has moved by a whole settle between the two reads.
const BEFORE_RENDER = /function\s+beforeRender\s*\(\s*([A-Za-z_$][\w$]*)\s*\)/;

/** Which non-deterministic inputs this pattern reads, by name. Empty means
 *  its frame is a function of the pattern alone, and a native/interpreted
 *  mismatch is therefore a bug. */
function wallClockInputs(src) {
  const found = WALL_CLOCK.filter((b) => src.includes(b)).map((b) =>
    b.endsWith("(") ? `${b})` : b,
  );
  const m = BEFORE_RENDER.exec(src);
  if (m) {
    const name = m[1];
    const uses = src.match(new RegExp(`\\b${name.replace(/\$/g, "\\$")}\\b`, "g")) ?? [];
    // more than the declaration itself = the body actually uses it
    if (uses.length > 1) found.push(`beforeRender(${name})`);
  }
  return found.sort();
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** One device call. node's fetch pools connections and picoserve closes idle
 *  ones after 1 s, so every request says `connection: close` — the same
 *  reuse race hw-bench.mjs documents. A request abandoned early PINS one of
 *  the device's three sockets until its server-side read timeout reaps it,
 *  hence: generous timeout, few retries, a real pause before each one.
 *  Throws after the last attempt — the caller turns that into `crashed`. */
async function call(pathname, { body, ct, raw = false } = {}) {
  for (let attempt = 0; ; attempt++) {
    try {
      const init = {
        headers: { connection: "close", ...(ct ? { "content-type": ct } : {}) },
        signal: AbortSignal.timeout(CALL_TIMEOUT_MS),
      };
      const r = await fetch(DEV + pathname, body === undefined ? init : { ...init, method: "POST", body });
      if (raw) return Buffer.from(await r.arrayBuffer());
      return await r.json();
    } catch (e) {
      if (attempt >= 2) throw e;
      await sleep(8_000); // let the device reap any socket we just pinned
    }
  }
}
const get = (p) => call(p);
const getBytes = (p) => call(p, { raw: true });
const post = (p, body, ct) => call(p, { body, ct });

/** One quick probe, no retries — for "is it back yet?" polling. */
async function probe() {
  try {
    const r = await fetch(DEV + "/api/status", {
      headers: { connection: "close" },
      signal: AbortSignal.timeout(CALL_TIMEOUT_MS),
    });
    return await r.json();
  } catch {
    return null;
  }
}

/** Wait out a reboot. Returns the status it came back with, or null. */
async function recover(why) {
  const t0 = Date.now();
  console.log(`   device unreachable (${why}) — waiting up to ${RECOVER_MS / 1000}s`);
  while (Date.now() - t0 < RECOVER_MS) {
    await sleep(5_000);
    const st = await probe();
    if (st) {
      const secs = Math.round((Date.now() - t0) / 1000);
      console.log(`   back after ${secs}s on ${st.slot}`);
      return { st, secs };
    }
  }
  return null;
}

// ------------------------------------------------------------- patterns

function resolvePattern(name) {
  const file = name.includes("/") ? path.resolve(ROOT, name) : path.join(ROOT, "library", name.endsWith(".js") ? name : `${name}.js`);
  if (!fs.existsSync(file)) {
    console.error(`no such pattern: ${file}`);
    process.exit(2);
  }
  return file;
}

const patternsArg = opt("patterns", null);
let names;
if (patternsArg === "@five") names = [...FIVE];
else if (patternsArg) names = patternsArg.split(",").map((s) => s.trim()).filter(Boolean);
else names = fs.readdirSync(path.join(ROOT, "library")).filter((f) => f.endsWith(".js")).sort();
const files = names.map(resolvePattern);

// ---------------------------------------------------------- the two sides

/** Set the switch, push the blob, settle, read what happened. */
async function runSide(jitOn, body) {
  await post("/api/jit", JSON.stringify({ on: jitOn }), "application/json");
  const res = await post("/api/code", body, "application/octet-stream");
  if (res && res.ok === false) return { rejected: res.error ?? "rejected" };
  await sleep(SETTLE);
  const st = await get("/api/status");
  const px = await getBytes("/api/pixels");
  return { st, px };
}

/** Re-run the interpreted side and read the frame twice, to tell a real
 *  codegen mismatch from a pattern whose own frame will not sit still. */
async function interpIsStable(body, firstFrame) {
  const again = await runSide(false, body);
  if (again.rejected || !again.px?.length) return { stable: false, why: "the re-run published no frame" };
  const second = await getBytes("/api/pixels");
  if (!second.length || !again.px.equals(second)) {
    return { stable: false, why: "two interpreted reads a moment apart already differ" };
  }
  if (!again.px.equals(firstFrame)) {
    return { stable: false, why: "the interpreted frame moved between the two runs" };
  }
  return { stable: true };
}

const jitOf = (st) => st?.jit ?? {};
const num = (v) => (typeof v === "number" ? v : null);

// ------------------------------------------------------------------- run

const status0 = await get("/api/status");
const jit0 = jitOf(status0);
if (jit0.state === undefined) {
  console.error(`${IP}: /api/status carries no \`jit\` object — this firmware predates Gitea #658`);
  process.exit(2);
}
if (jit0.state === "off") {
  console.error(
    `${IP}: jit.state is "off" — this board carries no JIT backend at all, so there is ` +
      `nothing to diff (docs/api.md; only the two S3 boards have it).`,
  );
  process.exit(2);
}
// "interp" + "disabled" is the one combination that means the switch is off;
// every other state means someone (or a reboot) left it on.
const jitWasOn = !(jit0.state === "interp" && jit0.reason === "disabled");
const foundPattern = await getBytes("/api/pattern.lxp");
const foundPixels = status0.pixels;
console.log(
  `device ${IP}: v${status0.version}, ${foundPixels}px, slot ${status0.slot}, ` +
    `jit ${jit0.state}${jit0.reason ? `/${jit0.reason}` : ""} (switch ${jitWasOn ? "on" : "off"}) — ` +
    `${files.length} pattern${files.length === 1 ? "" : "s"}, settle ${SETTLE} ms`,
);

const rows = [];
let stopped = null;
try {
  if (PIXELS !== null && PIXELS !== foundPixels) {
    await post("/api/config", String(PIXELS), "text/plain");
    await sleep(2_500);
  }
  for (let i = 0; i < files.length; i++) {
    if (i + 1 < FROM) continue;
    const name = path.basename(files[i]);
    const src = fs.readFileSync(files[i], "utf8");
    const tag = `${String(i + 1).padStart(3)}/${files.length}`;
    const row = { name, outcome: null, note: "" };
    rows.push(row);

    let body;
    try {
      body = await lxpBody("", src);
    } catch (e) {
      row.outcome = "push-rejected";
      row.note = `local compile failed: ${String(e).slice(0, 120)}`;
      console.log(`${tag}  ${row.outcome}  ${name} — ${row.note}`);
      continue;
    }

    let interp, native;
    try {
      interp = await runSide(false, body);
      if (interp.rejected) {
        row.outcome = "push-rejected";
        row.note = `device refused the blob: ${interp.rejected}`;
        console.log(`${tag}  ${row.outcome}  ${name} — ${row.note}`);
        continue;
      }
      native = await runSide(true, body);
      if (native.rejected) {
        row.outcome = "push-rejected";
        row.note = `device refused the blob on the native side: ${native.rejected}`;
        console.log(`${tag}  ${row.outcome}  ${name} — ${row.note}`);
        continue;
      }
    } catch (e) {
      const back = await recover(String(e).slice(0, 60));
      row.outcome = "crashed";
      row.note = back ? `device back after ${back.secs}s` : "device did not come back";
      console.log(`${tag}  CRASHED  ${name} — ${row.note}`);
      if (!back) {
        stopped = `device did not come back after ${name}`;
        break;
      }
      continue;
    }

    const ji = jitOf(interp.st);
    const jn = jitOf(native.st);
    row.vm_us_interp = num(interp.st.vm_us);
    row.vm_us_native = num(native.st.vm_us);
    row.fps_interp = num(interp.st.fps);
    row.fps_native = num(native.st.fps);
    row.code_bytes = num(jn.code_bytes);
    row.compile_us = num(jn.compile_us);
    row.jit_state = jn.state ?? null;
    row.jit_reason = jn.reason ?? null;
    row.heap_free = num(native.st.heap_free);
    row.psram_free = num(native.st.psram_free);
    row.vmerr_interp = interp.st.vmerr ?? null;
    row.vmerr_native = native.st.vmerr ?? null;
    row.pixels_interp = interp.px.length;
    row.pixels_native = native.px.length;
    // Only where the differential actually happened: interpreter one side,
    // native code the other. Two interpreted runs (a refusal) or two native
    // ones (a switch that did not take) would print a meaningless 1.00x.
    const ran = jn.state === "native" && ji.state === "interp" && ji.reason === "disabled";
    if (ran && row.vm_us_interp && row.vm_us_native) row.speedup = row.vm_us_interp / row.vm_us_native;

    // The baseline has to actually BE the interpreter. `disabled`
    // short-circuits every other reason in firmware/src/jit.rs, so anything
    // else here means the switch did not take or the device rebooted
    // between the two halves (the switch is not persisted and comes back on).
    if (!(ji.state === "interp" && ji.reason === "disabled")) {
      row.outcome = "no-baseline";
      row.note = `jit off reported ${ji.state}/${ji.reason ?? "null"}, not interp/disabled`;
    } else if (row.vmerr_native && row.vmerr_native === row.vmerr_interp) {
      row.outcome = "both-vmerr";
      row.note = `both sides: ${row.vmerr_native}`;
    } else if (row.vmerr_native || row.vmerr_interp) {
      row.outcome = "vmerr";
      row.note = row.vmerr_native
        ? `native: ${row.vmerr_native}${row.vmerr_interp ? ` (interpreted: ${row.vmerr_interp})` : " (interpreted side clean)"}`
        : `interpreted only: ${row.vmerr_interp}`;
    } else if (jn.state === "interp") {
      row.outcome = `refused:${jn.reason ?? "unknown"}`;
      row.note = "ran interpreted both ways — a refusal is not a failed pattern";
      if (interp.px.length && native.px.length && !interp.px.equals(native.px)) {
        row.note += "; the two interpreted frames differ, so this pattern does not sit still";
      }
    } else if (jn.state !== "native") {
      row.outcome = "no-baseline";
      row.note = `jit on reported state ${jn.state ?? "missing"}`;
    } else if (!interp.px.length || !native.px.length) {
      row.outcome = "no-pixels";
      if (interp.px.length && !native.px.length) {
        // The asymmetric case is the #658 trap the QEMU gate shouts about:
        // the pattern compiled, started, and took the render task with it.
        // An empty body is also the documented heap-pressure degrade, so the
        // WORD stays `no-pixels` — but this half blocks the exit code.
        row.native_blank = true;
        row.note =
          `ran natively (${row.code_bytes} B in ${row.compile_us} us) and published NO frame ` +
          `while the interpreted side did — a dead render task, or the heap could not hold the response`;
      } else {
        row.note = native.px.length
          ? "the interpreted side published no frame, so there is no oracle"
          : "neither side published a frame";
      }
    } else if (interp.px.equals(native.px)) {
      row.outcome = "native-identical";
      row.note = native.px.some((b) => b !== 0)
        ? `${native.px.length} B frame identical`
        : `${native.px.length} B frame identical, but BOTH are all-black — the agreement is vacuous`;
    } else {
      const at = native.px.findIndex((b, k) => b !== interp.px[k]);
      const p0 = at - (at % 3);
      const detail =
        at < 0
          ? `frames are ${native.px.length} B native vs ${interp.px.length} B interpreted`
          : `first differs at byte ${at} (pixel ${Math.floor(at / 3)}): native ` +
            `${native.px.subarray(p0, p0 + 3).toString("hex")} interpreted ` +
            `${interp.px.subarray(p0, p0 + 3).toString("hex")}`;
      const clock = wallClockInputs(src);
      if (clock.length) {
        row.outcome = "native-clock";
        row.note = `reads ${clock.join(", ")} and the two reads are a settle apart — ${detail}`;
      } else {
        // About to call this a codegen bug: make the interpreter prove it
        // can reproduce its own frame first.
        let check;
        try {
          check = await interpIsStable(body, interp.px);
        } catch (e) {
          check = { stable: false, why: `the re-run failed: ${String(e).slice(0, 60)}` };
        }
        if (check.stable) {
          row.outcome = "native-mismatch";
          row.note = `no clock input, and the interpreted frame is reproducible — ${detail}`;
        } else {
          row.outcome = "unstable";
          row.note = `${check.why}, so the frame is not comparable over HTTP — ${detail}`;
        }
      }
    }
    const speed = row.speedup ? `${row.speedup.toFixed(2)}x` : "—";
    console.log(
      `${tag}  ${row.outcome.padEnd(17)} ${name}  vm ${row.vm_us_interp ?? "—"}→${row.vm_us_native ?? "—"} us (${speed})` +
        (row.code_bytes ? `  ${row.code_bytes} B in ${row.compile_us} us` : ""),
    );
  }
} finally {
  // Put the device back exactly as it was found: pixel count, the switch,
  // then the pattern — in that order, because the switch only takes effect
  // at the next activation and that push is it.
  try {
    if (PIXELS !== null && PIXELS !== foundPixels) await post("/api/config", String(foundPixels), "text/plain");
    await post("/api/jit", JSON.stringify({ on: jitWasOn }), "application/json");
    if (foundPattern.length) await post("/api/code", foundPattern, "application/octet-stream");
    console.log(`restored: ${foundPixels} px, jit switch ${jitWasOn ? "on" : "off"}, the pattern found running`);
  } catch (e) {
    console.log(`restore failed: ${String(e).slice(0, 120)} — the device may need a manual restore`);
  }
}

// ---------------------------------------------------------------- report

const count = (o) => rows.filter((r) => r.outcome === o).length;
const refused = rows.filter((r) => r.outcome?.startsWith("refused:")).length;
const mismatch = count("native-mismatch");
const nativeVmerr = rows.filter((r) => r.outcome === "vmerr" && r.vmerr_native).length;
const crashed = count("crashed");
const noBaseline = count("no-baseline");
const nativeBlank = rows.filter((r) => r.native_blank).length;
const summary =
  `${count("native-identical")} native-identical / ${count("native-clock")} native-clock / ` +
  `${refused} refused / ${mismatch} mismatch / ${count("vmerr")} vmerr / ${crashed} crashed`;
const extra = [
  ["both-vmerr", count("both-vmerr")],
  ["unstable", count("unstable")],
  ["no-pixels", count("no-pixels")],
  ["no-baseline", noBaseline],
  ["push-rejected", count("push-rejected")],
].filter(([, n]) => n > 0);
const summaryLine = summary + (extra.length ? ` (+ ${extra.map(([k, n]) => `${n} ${k}`).join(", ")})` : "");

const us = (v) => (v === null || v === undefined ? "—" : v);
const cell = (s) => String(s).replaceAll("|", "\\|");
// Every row that ran natively against a real interpreted baseline carries a
// valid timing ratio, whether or not its PIXELS were comparable.
const speedups = rows
  .filter((r) => r.speedup && (r.outcome === "native-identical" || r.outcome === "native-clock"))
  .map((r) => r.speedup)
  .sort((a, b) => a - b);
const median = speedups.length ? speedups[Math.floor(speedups.length / 2)] : null;

const md = [];
md.push(`# JIT differential on ${IP} — ${new Date().toISOString().slice(0, 10)}`);
md.push("");
md.push(
  `*Firmware v${status0.version}, slot ${status0.slot}, ${PIXELS ?? foundPixels} px, settle ${SETTLE} ms. ` +
    `Native vs interpreted pixels over HTTP (Gitea #665, docs/jit-design.md §7.3).*`,
);
md.push(`*Regenerate: \`node tools/jit-diff.mjs ${IP}${patternsArg ? ` --patterns ${patternsArg}` : ""}\`.*`);
md.push("");
md.push(`- ${rows.length} pattern${rows.length === 1 ? "" : "s"}: ${summaryLine}.`);
if (median) {
  md.push(
    `- median interp/native \`vm_us\` over the ${speedups.length} row${speedups.length === 1 ? "" : "s"} ` +
      `measured both ways: **${median.toFixed(2)}x**.`,
  );
}
if (FROM > 1) md.push(`- resumed at pattern ${FROM} (\`--from\`) — earlier rows are not in this report.`);
if (stopped) md.push(`- **run stopped early**: ${stopped}.`);
md.push("");
md.push("| pattern | outcome | code_bytes | compile_us | vm_us interp | vm_us native | speedup | note |");
md.push("|---|---|---:|---:|---:|---:|---:|---|");
for (const r of rows) {
  md.push(
    `| ${cell(r.name)} | ${cell(r.outcome ?? "—")} | ${us(r.code_bytes)} | ${us(r.compile_us)} | ` +
      `${us(r.vm_us_interp)} | ${us(r.vm_us_native)} | ${r.speedup ? `${r.speedup.toFixed(2)}x` : "—"} | ${cell(r.note)} |`,
  );
}
md.push("");
const report = md.join("\n") + "\n";
process.stdout.write("\n" + report);
if (REPORT_OUT) {
  fs.writeFileSync(REPORT_OUT, report);
  console.log(`wrote ${REPORT_OUT}`);
}
if (JSON_OUT) {
  fs.writeFileSync(
    JSON_OUT,
    JSON.stringify({ ip: IP, version: status0.version, pixels: PIXELS ?? foundPixels, settle: SETTLE, summary: summaryLine, rows }, null, 2),
  );
  console.log(`wrote ${JSON_OUT}`);
}
console.log(summaryLine);

// A mismatch, a native-side vmerr, a crash, a missing baseline or a native
// side that published nothing while the interpreter did: the differential
// either failed or never happened.
const bad = mismatch + nativeVmerr + crashed + noBaseline + nativeBlank;
process.exit(bad === 0 ? 0 : 1);
