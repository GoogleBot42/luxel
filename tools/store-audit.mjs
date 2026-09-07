#!/usr/bin/env node
// Pattern-store auditor: exercises the on-device packed file log
// (firmware/src/patlog.rs + patterns.rs, Gitea #340) over the public HTTP
// API and checks the FULL name set after EVERY mutation.
//
// Written for Gitea #379, where a compaction silently dropped files: the
// device answered {"ok":true} to every save, the count went *down*, and
// nothing noticed until a fill run was counted by hand. The invariant that
// catches that class is cheap and belongs in a script —
//
//     expected set  =  previous set  +  what this call saved
//                                    -  what this call deleted
//
// — re-checked after every single save and delete, not at the end. A save
// that takes the count down fails the run at the operation that did it,
// with both sets printed, instead of being discovered 60 saves later.
//
// It also checks the packing arithmetic the store's whole design rests on:
// a save that does not compact must grow `store.used` by EXACTLY
// 48 + align4(name) + align4(source) + align4(bytecode), and a delete must
// move that same number from `used` into `dead`.
//
// Usage (inside `nix develop`; needs `npm run wasm` once for lxp.mjs):
//
//   tools/store-audit.mjs <ip> verify
//       Read every stored pattern back and compare byte-for-byte against
//       its library/ source. Read-only.
//
//   tools/store-audit.mjs <ip> fill [--limit N]
//       Save library/*.js in name order until the store refuses, auditing
//       after each. The refusal is expected and is not a failure; a save
//       that loses a name is.
//
//   tools/store-audit.mjs <ip> churn <pattern> [--pin <name>] [--rounds N]
//       Re-save one pattern until `--rounds` compactions have run (default
//       1), auditing after each. `--pin <name>` activates that stored
//       pattern first, so a compaction has to route around its frozen
//       pages; with no --pin an ad-hoc /api/code push runs instead, so
//       NOTHING is pinned. Reports the compaction's wall time and the
//       `dead` residue it leaves — 0 with nothing pinned, a sub-page hole
//       when something is pinned.
//
//   tools/store-audit.mjs <ip> wipe
//       Delete every stored pattern, checking the delete accounting.
//
// Common flags: --hammer keeps a concurrent GET /api/patterns/<id> running
// across every save (a compaction must never serve a truncated body), and
// --quiet prints only compactions, anomalies and the summary.
//
// Exit codes: 0 clean, 1 an audit/arithmetic failure, 2 bad usage,
// 3 the device is unreachable.

import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { compile, envelope } from "../web/tools/lxp.mjs";

const LIB = fileURLToPath(new URL("../library", import.meta.url));
const HDR = 48;
const align4 = (n) => (n + 3) & ~3;

// --- CLI ---------------------------------------------------------------

const argv = process.argv.slice(2);
const flag = (name, def = null) => {
  const i = argv.indexOf(name);
  if (i < 0) return def;
  const v = argv[i + 1];
  argv.splice(i, v && !v.startsWith("--") ? 2 : 1);
  return v && !v.startsWith("--") ? v : true;
};
const HAMMER = flag("--hammer") === true;
const QUIET = flag("--quiet") === true;
const LIMIT = Number(flag("--limit", Infinity));
const ROUNDS = Number(flag("--rounds", 1));
const PIN = flag("--pin", null);
const [ip, cmd, arg] = argv;

if (!ip || !cmd) {
  console.error("usage: store-audit.mjs <ip> verify|fill|churn <pattern>|wipe [flags]");
  process.exit(2);
}

// --- device ------------------------------------------------------------

// The device's web pool is small (3 connections), and a compaction holds
// the flash for seconds at a time, so under `--hammer` it drops requests
// rather than queueing them. That is back-pressure, not a fault — retry
// once before giving up, or a perfectly healthy run dies on an
// UND_ERR_SOCKET partway through.
async function api(path, opt, tries = 2) {
  let last;
  for (let i = 0; i < tries; i++) {
    try {
      const r = await fetch(`http://${ip}${path}`, opt);
      const t = await r.text();
      try {
        return JSON.parse(t);
      } catch {
        return { _raw: t, _status: r.status };
      }
    } catch (e) {
      last = e;
      // A retry must never re-send a mutation: only GETs are replayed.
      if (opt?.method && opt.method !== "GET") throw e;
      await new Promise((r) => setTimeout(r, 250));
    }
  }
  throw last;
}
const status = () => api("/api/status");
const store = async () => (await status()).store;
const list = async () => (await api("/api/patterns")).patterns ?? [];
const names = async () => (await list()).map((p) => p.name).sort();
const del = (id) => api(`/api/patterns/${id}`, { method: "DELETE" });
const activate = (id) => api(`/api/patterns/${id}/activate`, { method: "POST" });
const getSrc = (id) => api(`/api/patterns/${id}`);

const post = (path, body) =>
  api(path, { method: "POST", body, headers: { "content-type": "application/octet-stream" } });

/** Save `name`; returns the device's reply and the record's exact byte size. */
async function save(name, source, pixels) {
  const bc = await compile(source, pixels);
  const bytes = HDR + align4(Buffer.byteLength(name)) + align4(Buffer.byteLength(source)) + align4(bc.length);
  return { r: await post("/api/patterns", envelope(name, source, bc)), bytes };
}

/** An ad-hoc push, so no STORED pattern is pinned during a compaction. */
async function livePush(pixels) {
  const src = "export function render(i){ hsv(time(.1)+i/pixelCount,1,1) }";
  return post("/api/code", envelope("", src, await compile(src, pixels)));
}

// --- the audit ---------------------------------------------------------

/** Multiset difference: what `got` is missing, and what it gained. */
function diffSets(expected, got) {
  const count = (a) => a.reduce((m, x) => ((m[x] = (m[x] || 0) + 1), m), {});
  const A = count(expected);
  const B = count(got);
  const lost = [];
  const extra = [];
  for (const k of new Set([...Object.keys(A), ...Object.keys(B)])) {
    const d = (B[k] || 0) - (A[k] || 0);
    for (let i = 0; i < -d; i++) lost.push(k);
    for (let i = 0; i < d; i++) extra.push(k);
  }
  return { lost, extra, ok: !lost.length && !extra.length };
}

let failures = 0;
const fail = (msg) => {
  failures++;
  console.log(`!!! ${msg}`);
};

/**
 * Run one mutation with the full audit around it.
 *
 * `expect` says what the name set should become: {saved} / {deleted}.
 * Returns the store before/after plus whether a compaction ran (the only
 * thing that can legitimately break the packing arithmetic).
 */
async function audited(label, expect, fn, state) {
  const before = await store();
  let hammering = HAMMER && state.other;
  let stop = false;
  let reads = 0;
  const hammer = hammering
    ? (async () => {
        while (!stop) {
          reads++;
          let j;
          try {
            j = await getSrc(state.other.id);
          } catch {
            // The device closes connections when its small web pool is
            // saturated during a compaction. A refused/dropped request is
            // back-pressure, not corruption — count it and keep going.
            state.dropped++;
            continue;
          }
          // Either the correct body, or a clean "no such pattern" while it
          // moves. Never a truncated or foreign one.
          if (j.ok === false) continue;
          if (j.source !== state.otherSource) fail(`${label}: concurrent read returned an altered body`);
        }
      })()
    : null;

  const t0 = Date.now();
  const out = await fn();
  const ms = Date.now() - t0;
  if (hammer) {
    stop = true;
    await hammer;
  }

  const after = await store();
  const got = await names();
  const want = [...state.expected];
  if (expect.saved && !want.includes(expect.saved)) want.push(expect.saved);
  if (expect.deleted) {
    const at = want.indexOf(expect.deleted);
    // indexOf -1 would splice the LAST name off instead — a silently wrong
    // expectation is worse than a loud one.
    if (at < 0) fail(`${label}: deleted a name that was not in the expected set`);
    else want.splice(at, 1);
  }
  want.sort();

  const d = diffSets(want, got);
  // `used + dead` IS the write cursor, so a compaction is the only thing
  // that can make it fall. Caveat: on a log so full that every save has to
  // compact, the cursor returns to the same place each time and this reads
  // false — the giveaway is then the save's wall time (~1 s instead of
  // ~70 ms), which is printed alongside.
  const compacted = after.used + after.dead < before.used + before.dead;
  if (!d.ok) {
    fail(`${label}: name set changed — lost ${JSON.stringify(d.lost)} extra ${JSON.stringify(d.extra)}`);
    console.log(`    store before ${JSON.stringify(before)}`);
    console.log(`    store after  ${JSON.stringify(after)}`);
  }
  state.expected = want;
  state.reads += reads;
  return { out, before, after, ms, compacted };
}

// --- commands ----------------------------------------------------------

const libFiles = () =>
  readdirSync(LIB)
    .filter((f) => f.endsWith(".js"))
    .sort()
    .map((f) => ({ name: f.replace(/\.js$/, ""), src: readFileSync(`${LIB}/${f}`, "utf8") }));

async function cmdVerify() {
  const l = await list();
  let ok = 0;
  let missing = 0;
  for (const p of l) {
    const got = await getSrc(p.id);
    let want;
    try {
      want = readFileSync(`${LIB}/${p.name}.js`, "utf8");
    } catch {
      missing++;
      continue;
    }
    if (got.source === want) ok++;
    else fail(`${p.name} (${p.id}) does not match library/${p.name}.js`);
  }
  console.log(`verify: ${l.length} stored, ${ok} byte-identical, ${missing} with no library/ file`);
}

async function cmdFill(state, pixels) {
  let saved = 0;
  let compactions = 0;
  for (const f of libFiles()) {
    if (saved >= LIMIT) break;
    let res;
    try {
      res = await save(f.name, f.src, pixels);
    } catch (e) {
      console.log(`  skip ${f.name}: ${e.message}`);
      continue;
    }
    // save() already ran; re-do it through the audit wrapper would double
    // the write, so audit the state it produced directly.
    const after = await store();
    const got = await names();
    const want = state.expected.includes(f.name) ? [...state.expected] : [...state.expected, f.name].sort();
    if (res.r.ok === false) {
      console.log(`REFUSED at ${f.name} (needs ${res.bytes} B): ${res.r.error}`);
      console.log(`  store ${JSON.stringify(after)}`);
      const d = diffSets(state.expected, got);
      if (!d.ok) fail(`a REFUSED save changed the name set: ${JSON.stringify(d)}`);
      break;
    }
    const d = diffSets(want, got);
    if (!d.ok) {
      fail(`save ${f.name}: lost ${JSON.stringify(d.lost)} extra ${JSON.stringify(d.extra)}`);
      console.log(`  store before ${JSON.stringify(state.last)} after ${JSON.stringify(after)}`);
      break;
    }
    const compacted = after.used + after.dead < state.last.used + state.last.dead;
    if (compacted) compactions++;
    else if (after.used !== state.last.used + res.bytes) {
      fail(`save ${f.name}: used grew by ${after.used - state.last.used}, expected exactly ${res.bytes}`);
    }
    if (compacted || !QUIET) {
      console.log(
        `save#${saved + 1} ${f.name} rec=${res.bytes} used ${state.last.used}->${after.used}` +
          ` dead ${state.last.dead}->${after.dead} pat ${state.last.patterns}->${after.patterns}` +
          (compacted ? "  <<< COMPACTION" : "  exact"),
      );
    }
    state.expected = want;
    state.last = after;
    saved++;
  }
  console.log(`fill: ${saved} saves accepted, ${compactions} compactions, ${(await list()).length} patterns present`);
}

async function cmdChurn(state, pixels) {
  if (!arg) {
    console.error("churn needs a pattern name");
    process.exit(2);
  }
  const src = readFileSync(`${LIB}/${arg}.js`, "utf8");
  if (PIN) {
    const p = (await list()).find((x) => x.name === PIN);
    if (!p) {
      console.error(`no stored pattern named "${PIN}" to pin`);
      process.exit(2);
    }
    await activate(p.id);
    state.pin = p;
    state.pinSource = (await getSrc(p.id)).source;
    console.log(`pinned: "${PIN}" (${p.id}) activated and running`);
  } else {
    await livePush(pixels);
    console.log("nothing pinned (ad-hoc /api/code push running)");
  }

  let compactions = 0;
  for (let i = 1; compactions < ROUNDS && i <= 200; i++) {
    let bytes = 0;
    const { out, before, after, ms, compacted } = await audited(
      `churn save#${i}`,
      { saved: arg },
      async () => {
        const s = await save(arg, src, pixels);
        bytes = s.bytes;
        return s.r;
      },
      state,
    );
    if (compacted) compactions++;
    else if (after.used !== before.used && after.used !== before.used + bytes) {
      fail(`churn save#${i}: used grew by ${after.used - before.used}, expected 0 or ${bytes}`);
    }
    if (compacted || !QUIET) {
      console.log(
        `save#${i} ok=${out.ok} rec=${bytes} ${ms}ms | used ${before.used}->${after.used}` +
          ` dead ${before.dead}->${after.dead} pat ${before.patterns}->${after.patterns}` +
          (compacted ? "  <<< COMPACTION" : ""),
      );
    }
    if (compacted) {
      // The churn record's own superseded copy is expected to be dead; the
      // rest of `dead` is the hole the compaction could not close. Zero
      // with nothing pinned; a sub-page residue when something is.
      const hole = after.dead - bytes;
      console.log(`  compaction took ${ms} ms, dead residue beyond the churn record: ${hole} B`);
      if (!PIN && hole !== 0) console.log(`  NOTE: nothing was pinned, so this should be 0 (Gitea #388)`);
    }
    if (out.ok === false) {
      console.log(`  save refused: ${out.error}`);
      break;
    }
    state.last = after;
  }
  if (state.pin) {
    const now = await getSrc(state.pin.id);
    if (now.source !== state.pinSource) fail(`the PINNED file "${PIN}" changed across the compaction`);
    else console.log(`pinned file "${PIN}" byte-identical after the compaction`);
  }
  console.log(`churn: ${compactions} compactions`);
}

async function cmdWipe(state) {
  const l = await list();
  console.log(`wipe: deleting ${l.length} patterns`);
  for (const p of l) {
    const sizeBefore = await store();
    const { after } = await audited(`delete ${p.name}`, { deleted: p.name }, () => del(p.id), state);
    const freed = sizeBefore.used - after.used;
    if (after.dead - sizeBefore.dead !== freed) {
      fail(`delete ${p.name}: used fell ${freed} B but dead rose ${after.dead - sizeBefore.dead} B`);
    }
    if (!QUIET) {
      console.log(
        `del ${p.name} (${p.id}) | used ${sizeBefore.used}->${after.used} (-${freed})` +
          ` dead ${sizeBefore.dead}->${after.dead} (+${after.dead - sizeBefore.dead}) pat ${after.patterns}`,
      );
    }
    state.last = after;
  }
  console.log(`wipe: ${(await list()).length} patterns left, store ${JSON.stringify(await store())}`);
}

// --- main --------------------------------------------------------------

let s0;
try {
  s0 = await status();
} catch (e) {
  console.error(`device ${ip} unreachable: ${e.message}`);
  process.exit(3);
}
if (!s0.store) {
  console.error(`device ${ip} reports no pattern store`);
  process.exit(3);
}
console.log(`device ${ip} slot=${s0.slot} v${s0.version} pixels=${s0.pixels} store=${JSON.stringify(s0.store)}`);

const l0 = await list();
const state = {
  expected: l0.map((p) => p.name).sort(),
  last: s0.store,
  other: l0[0] ?? null,
  otherSource: l0[0] ? (await getSrc(l0[0].id)).source : null,
  reads: 0,
  dropped: 0,
  pin: null,
};

switch (cmd) {
  case "verify":
    await cmdVerify();
    break;
  case "fill":
    await cmdFill(state, s0.pixels);
    break;
  case "churn":
    await cmdChurn(state, s0.pixels);
    break;
  case "wipe":
    await cmdWipe(state);
    break;
  default:
    console.error(`unknown command "${cmd}"`);
    process.exit(2);
}

const sf = await status();
console.log(
  `final: store ${JSON.stringify(sf.store)} vmerr=${sf.vmerr} fence_timeouts=${sf.core1?.fence_timeouts}` +
    ` reset=${sf.core1?.last?.reset}` +
    (state.reads ? ` | ${state.reads} concurrent reads, ${state.dropped} dropped` : ""),
);
if (sf.vmerr) fail(`vmerr is set: ${sf.vmerr}`);
console.log(failures ? `FAILED — ${failures} problem(s)` : "OK — every audit clean");
process.exit(failures ? 1 : 0);
