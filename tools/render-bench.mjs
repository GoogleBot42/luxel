// Render-core benchmark: fps, heap and web-serving latency under load, on a
// real device — the #259 / #260 measurement in one run.
//
// For each pixel count it pushes three reference patterns (rainbow — the
// default; library/snake.js — a 1D per-pixel loop; library/snake-2d.js with
// Smartness at 100 — Jeremy's named "intensive" benchmark, render2D per
// pixel), lets the fps counter settle, samples /api/status, then downloads
// the playground's main bundle (/assets/index-*.js, ~228 KB) a few times
// WHILE the pattern runs and reports the median wall time. The bundle time
// is the #259 number: how badly the render loop starves the web task.
//
// Usage (repo root, nix develop; needs `npm run wasm` once for lxp.mjs):
//   node tools/render-bench.mjs <device-ip> [--px 60,2048] [--runs 3] [--dwell 5]
//
// Restores the pixel count it found and the default rainbow. Never touches
// brightness or output settings. ~2 min per pixel count at the defaults.
import fs from "node:fs";
import { lxpBody } from "../web/tools/lxp.mjs";

const args = process.argv.slice(2);
const IP = args.find((a) => !a.startsWith("--")) ?? "192.168.0.183";
const opt = (name, dflt) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] !== undefined ? args[i + 1] : dflt;
};
const PX = opt("px", "60,2048").split(",").map(Number);
const RUNS = Number(opt("runs", "3"));
const DWELL_MS = Number(opt("dwell", "5")) * 1000;
const DEV = `http://${IP}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function api(path, body) {
  // connection: close — picoserve reaps idle keep-alive sockets after 1 s
  // and node's pool would race that (same shape as tools/hw-bench.mjs).
  for (let attempt = 0; ; attempt++) {
    try {
      const opts = { headers: { connection: "close" }, signal: AbortSignal.timeout(60_000) };
      const r = await fetch(DEV + path, body === undefined ? opts : { ...opts, method: "POST", body });
      return await r.json();
    } catch (e) {
      if (attempt >= 2) throw e;
      await sleep(5_000);
    }
  }
}

async function bundleUrl() {
  const r = await fetch(DEV + "/", { headers: { connection: "close" } });
  const html = await r.text();
  const m = html.match(/assets\/index-[A-Za-z0-9_-]+\.js/);
  if (!m) throw new Error("no /assets/index-*.js in the device's index page (hosted-ui build?)");
  return `${DEV}/${m[0]}`;
}

async function timeDownload(url) {
  const t0 = performance.now();
  const r = await fetch(url, { headers: { connection: "close" }, signal: AbortSignal.timeout(180_000) });
  const buf = await r.arrayBuffer();
  // wire bytes, not the decompressed body: node's fetch inflates the
  // gzip-served bundle transparently (~228 KB on the wire, ~680 KB inflated)
  const bytes = Number(r.headers.get("content-length")) || buf.byteLength;
  return { ms: performance.now() - t0, bytes, status: r.status };
}

const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};

const src = (f) => fs.readFileSync(f, "utf8");
const patterns = [
  { name: "rainbow", source: src("library/rainbow.js") },
  { name: "snake (1D)", source: src("library/snake.js") },
  {
    name: "snake-2d (smart=100)",
    // the "AI level raised" variant: Smartness 100 = the flood-fill
    // look-ahead runs on every step
    source: src("library/snake-2d.js").replace(/^var smart = [0-9.]+$/m, "var smart = 1"),
  },
];
for (const p of patterns) {
  if (p.name.startsWith("snake-2d") && !/^var smart = 1$/m.test(p.source)) {
    throw new Error("snake-2d.js: could not raise the Smartness default — pattern header changed?");
  }
  p.body = await lxpBody("", p.source);
}

const status0 = await api("/api/status");
const px0 = status0.pixels;
const url = await bundleUrl();
console.log(
  `device ${IP}: v${status0.version} slot ${status0.slot}, ${px0} px, heap ${status0.heap_free} B idle — bundle ${url.slice(DEV.length)}`,
);

const rows = [];
for (const px of PX) {
  await api("/api/config", String(px));
  await sleep(1500);
  for (const p of patterns) {
    const res = await api("/api/code", p.body);
    if (res.ok === false || res.error) {
      rows.push({ px, name: p.name, error: JSON.stringify(res) });
      console.log(`  ${px} px ${p.name}: push failed ${JSON.stringify(res)}`);
      continue;
    }
    await sleep(DWELL_MS);
    const fps = [];
    // per-stage frame timers (µs/frame, firmware >= the #260 pass-1 build;
    // absent on older firmware — reported as "–")
    const us = { frame_us: [], vm_us: [], pipe_us: [], out_us: [] };
    let heap = Infinity;
    let vmerr = null;
    let core1 = null;
    for (let i = 0; i < 5; i++) {
      const st = await api("/api/status");
      fps.push(st.fps);
      for (const k of Object.keys(us)) if (st[k] !== undefined) us[k].push(st[k]);
      heap = Math.min(heap, st.heap_free);
      vmerr = st.vmerr ?? vmerr;
      core1 = st.core1 ?? core1;
      await sleep(1000);
    }
    const usMed = Object.fromEntries(Object.entries(us).map(([k, v]) => [k, v.length ? median(v) : null]));
    const dl = [];
    for (let i = 0; i < RUNS; i++) dl.push(await timeDownload(url));
    const stAfter = await api("/api/status");
    const row = {
      px,
      name: p.name,
      fps: median(fps),
      fpsDuringDl: stAfter.fps,
      heap,
      ms: median(dl.map((d) => d.ms)),
      bytes: dl[0].bytes,
      vmerr,
      ...usMed,
      core1,
    };
    rows.push(row);
    console.log(
      `  ${px} px ${p.name.padEnd(22)} fps ${row.fps} (during download ${row.fpsDuringDl}), frame/vm/pipe/out ${row.frame_us ?? "–"}/${row.vm_us ?? "–"}/${row.pipe_us ?? "–"}/${row.out_us ?? "–"} µs, heap ${heap} B, bundle ${(row.ms / 1000).toFixed(2)} s (${(row.bytes / row.ms).toFixed(1)} KB/s)${vmerr ? ` vmerr=${vmerr}` : ""}${core1 ? ` core1=${JSON.stringify(core1)}` : ""}`,
    );
  }
}

// restore what we found
await api("/api/config", String(px0));
await api("/api/code", patterns[0].body);
const status1 = await api("/api/status");

console.log("");
console.log(`*${IP}, v${status0.version} (${status0.slot}), idle heap ${status0.heap_free} B, bundle ${rows.find((r) => r.bytes)?.bytes ?? "?"} B.*`);
console.log("");
console.log("| px | pattern | fps | fps during download | frame_us | vm_us | pipe_us | out_us | heap_free (min) | bundle download | rate |");
console.log("|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
for (const r of rows) {
  if (r.error) {
    console.log(`| ${r.px} | ${r.name} | push failed: ${r.error} | | | | | | | | |`);
    continue;
  }
  console.log(
    `| ${r.px} | ${r.name} | ${r.fps} | ${r.fpsDuringDl} | ${r.frame_us ?? "–"} | ${r.vm_us ?? "–"} | ${r.pipe_us ?? "–"} | ${r.out_us ?? "–"} | ${r.heap} B | ${(r.ms / 1000).toFixed(2)} s | ${(r.bytes / r.ms).toFixed(1)} KB/s |`,
  );
}
console.log("");
console.log(
  `restored: ${status1.pixels} px, rainbow, slot ${status1.slot}, heap ${status1.heap_free} B${status1.core1 ? `, core1 ${JSON.stringify(status1.core1)}` : ""}`,
);
