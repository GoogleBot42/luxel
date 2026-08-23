// Visual report for the port-verification sweep: renders every judged pair
// (or a chosen subset) as a pair of side-by-side animated GIFs — original vs
// clean-room port under the SAME conventions as snap.mjs (identical rig, seed,
// pinned wall clock, fixed frame delta, beat120 sensor feed on wants_sensors)
// — and writes one self-navigable index.html grouped by verdict, with each
// card expandable into the judge's full verdict (summary, observations, dials,
// feedback). Zero dependencies: the GIF89a encoder lives in this file (per
// frame the rigs have at most `pixels` unique colours ≤ 256, so frames are
// encoded EXACTLY with per-frame local colour tables — no quantization in the
// normal case).
//
// Usage:
//   node tools/verify/report.mjs [options]
//
//   --slugs a,b,c     only these slugs (default: every tools/verify/results/*.json)
//   --seconds N       simulated seconds per gif (default 6)
//   --fps N           simulation rate, matches the sweep default (default 20)
//   --gif-fps N       gif playback rate; frames are subsampled from the
//                     simulation, so timelines match the judged runs (default 10)
//   --skip N          warmup seconds discarded before capture (default 0)
//   --html-only       regenerate index.html from existing gifs/verdicts
//   --force           re-render gifs even if the file already exists
//   --out-root DIR    output root (default tools/verify/out)
//
// Writes <out-root>/report/index.html and <out-root>/report/gifs/<slug>-{orig,port}.gif.
// Output is regenerable and lives under the gitignored out/ tree; only this
// script is tracked. Rendering all 293 pairs takes a few minutes.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { load, cubeLattice, sensorSlots } from "./enginehost.mjs";
import { synthSensorFrame } from "./sensormodel.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "../..");
const PAIRS = path.join(HERE, "pairs.json");
const RESULTS = path.join(HERE, "results");

const WALL_CLOCK = 1756000000; // same pin as snap.mjs
const CLOUD_SIDE = 5;

// ---- CLI --------------------------------------------------------------------

const opts = {
  slugs: null,
  seconds: 6,
  fps: 20,
  gifFps: 10,
  skip: 0,
  htmlOnly: false,
  force: false,
  outRoot: path.join(HERE, "out"),
};
{
  const argv = process.argv.slice(2);
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const val = () => argv[++i];
    if (a === "--slugs") opts.slugs = val().split(",").map((s) => s.trim()).filter(Boolean);
    else if (a === "--seconds") opts.seconds = Number(val());
    else if (a === "--fps") opts.fps = Number(val());
    else if (a === "--gif-fps") opts.gifFps = Number(val());
    else if (a === "--skip") opts.skip = Number(val());
    else if (a === "--html-only") opts.htmlOnly = true;
    else if (a === "--force") opts.force = true;
    else if (a === "--out-root") opts.outRoot = val();
    else {
      console.error(`unknown option ${a}`);
      process.exit(2);
    }
  }
}
const REPORT = path.join(opts.outRoot, "report");
const GIFS = path.join(REPORT, "gifs");

// ---- GIF89a encoder ---------------------------------------------------------
//
// LZW is the classic compress-lineage variant (emit, THEN register the new
// table entry, growth check inside the emitter against the pre-registration
// free count) — the ordering every mainstream decoder mirrors.

function lzwEncode(minCodeSize, indices) {
  const out = [];
  let cur = 0;
  let curBits = 0;
  const CLEAR = 1 << minCodeSize;
  const EOI = CLEAR + 1;
  let nBits, maxCode, free, table;
  const reset = () => {
    nBits = minCodeSize + 1;
    maxCode = (1 << nBits) - 1;
    free = EOI + 1;
    table = new Map();
  };
  const emit = (code) => {
    cur |= code << curBits;
    curBits += nBits;
    while (curBits >= 8) {
      out.push(cur & 0xff);
      cur >>= 8;
      curBits -= 8;
    }
    if (free > maxCode && nBits < 12) {
      nBits++;
      maxCode = (1 << nBits) - 1;
    }
  };
  reset();
  emit(CLEAR);
  let ent = indices[0];
  for (let i = 1; i < indices.length; i++) {
    const c = indices[i];
    const key = (ent << 8) | c;
    const hit = table.get(key);
    if (hit !== undefined) {
      ent = hit;
      continue;
    }
    emit(ent);
    if (free < 4096) table.set(key, free++);
    else {
      emit(CLEAR);
      reset();
    }
    ent = c;
  }
  emit(ent);
  emit(EOI);
  if (curBits > 0) out.push(cur & 0xff);
  return out;
}

/** Index a canvas frame against its own exact palette; quantize (channel
 *  masking) only in the never-expected >256-colour case. */
function indexFrame(rgb) {
  for (const mask of [0xff, 0xfe, 0xfc, 0xf8, 0xf0]) {
    const pal = new Map();
    const idx = new Uint8Array(rgb.length / 3);
    let ok = true;
    for (let i = 0, p = 0; i < rgb.length; i += 3, p++) {
      const key =
        ((rgb[i] & mask) << 16) | ((rgb[i + 1] & mask) << 8) | (rgb[i + 2] & mask);
      let j = pal.get(key);
      if (j === undefined) {
        if (pal.size === 256) {
          ok = false;
          break;
        }
        pal.set(key, (j = pal.size));
      }
      idx[p] = j;
    }
    if (ok) return { pal: [...pal.keys()], idx };
  }
  throw new Error("frame exceeds 256 colours even at 4-bit channels");
}

const u16 = (arr, v) => arr.push(v & 0xff, (v >> 8) & 0xff);

/** frames: array of {rgb: Uint8Array(w*h*3)}; delay in 1/100 s. */
function encodeGif(w, h, frames, delay) {
  const out = [0x47, 0x49, 0x46, 0x38, 0x39, 0x61]; // "GIF89a"
  u16(out, w);
  u16(out, h);
  out.push(0x70, 0, 0); // no global colour table
  // Netscape loop-forever extension
  out.push(0x21, 0xff, 0x0b);
  for (const ch of "NETSCAPE2.0") out.push(ch.charCodeAt(0));
  out.push(0x03, 0x01, 0x00, 0x00, 0x00);
  for (const f of frames) {
    const { pal, idx } = indexFrame(f.rgb);
    let tableBits = 2;
    while (1 << tableBits < pal.length) tableBits++;
    // graphic control: disposal "leave in place", no transparency
    out.push(0x21, 0xf9, 0x04, 0x04);
    u16(out, delay);
    out.push(0x00, 0x00);
    // image descriptor + local colour table
    out.push(0x2c);
    u16(out, 0);
    u16(out, 0);
    u16(out, w);
    u16(out, h);
    out.push(0x80 | (tableBits - 1));
    for (let i = 0; i < 1 << tableBits; i++) {
      const c = pal[i] ?? 0;
      out.push((c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff);
    }
    out.push(tableBits);
    const data = lzwEncode(tableBits, idx);
    for (let i = 0; i < data.length; i += 255) {
      const n = Math.min(255, data.length - i);
      out.push(n);
      for (let j = 0; j < n; j++) out.push(data[i + j]);
    }
    out.push(0x00);
  }
  out.push(0x3b);
  return Buffer.from(out);
}

// ---- layout: engine frame → upscaled RGB canvas -----------------------------

const GAP_RGB = [16, 16, 20]; // separator colour between cloud slices

function makeLayout(rig) {
  if (rig.kind === "grid") {
    const cell = Math.max(2, Math.floor(128 / Math.max(rig.gridW, rig.gridH)));
    const w = rig.gridW * cell;
    const h = rig.gridH * cell;
    return {
      w,
      h,
      paint(frame, rgb) {
        for (let p = 0; p < rig.pixels; p++) {
          const gx = (p % rig.gridW) * cell;
          const gy = Math.floor(p / rig.gridW) * cell;
          blit(rgb, w, gx, gy, cell, cell, frame, p * 3);
        }
      },
    };
  }
  if (rig.kind === "cloud") {
    const s = Math.round(Math.cbrt(rig.pixels));
    const cell = 10;
    const gap = 3;
    const w = s * s * cell + (s - 1) * gap;
    const h = s * cell;
    return {
      w,
      h,
      base(rgb) {
        for (let i = 0; i < rgb.length; i += 3) {
          rgb[i] = GAP_RGB[0];
          rgb[i + 1] = GAP_RGB[1];
          rgb[i + 2] = GAP_RGB[2];
        }
      },
      paint(frame, rgb) {
        for (let p = 0; p < rig.pixels; p++) {
          const x = p % s;
          const y = Math.floor(p / s) % s;
          const z = Math.floor(p / (s * s));
          const gx = z * (s * cell + gap) + x * cell;
          const gy = y * cell;
          blit(rgb, w, gx, gy, cell, cell, frame, p * 3);
        }
      },
    };
  }
  // strip: a horizontal LED bar
  const cell = Math.max(2, Math.floor(360 / rig.pixels));
  const w = rig.pixels * cell;
  const h = 18;
  return {
    w,
    h,
    paint(frame, rgb) {
      for (let p = 0; p < rig.pixels; p++) blit(rgb, w, p * cell, 0, cell, h, frame, p * 3);
    },
  };
}

function blit(rgb, canvasW, x0, y0, cw, ch, src, si) {
  const r = src[si];
  const g = src[si + 1];
  const b = src[si + 2];
  for (let y = y0; y < y0 + ch; y++) {
    let o = (y * canvasW + x0) * 3;
    for (let x = 0; x < cw; x++) {
      rgb[o++] = r;
      rgb[o++] = g;
      rgb[o++] = b;
    }
  }
}

// ---- rendering --------------------------------------------------------------

function rigFor(pair) {
  const kind = pair.rig;
  if (kind === "grid") return { kind, gridW: 16, gridH: 16, pixels: 256 };
  if (kind === "cloud") {
    const points = cubeLattice(CLOUD_SIDE);
    return { kind, points, pixels: points.length };
  }
  return { kind: "strip", pixels: 60 };
}

/** Render one side into gif frames. Returns {frames, error} — a compile
 *  failure yields a single placeholder frame plus the error string. */
function renderSide(host, source, rig, layout) {
  const res = host.compile(source, rig.pixels, 1);
  const mk = () => {
    const rgb = new Uint8Array(layout.w * layout.h * 3);
    layout.base?.(rgb);
    return rgb;
  };
  if (res.compileError) {
    const rgb = mk();
    for (let i = 0; i < rgb.length; i += 3) {
      rgb[i] = 44;
      rgb[i + 1] = 14;
      rgb[i + 2] = 18;
    }
    return { frames: [{ rgb }], error: `compile: ${res.compileError}` };
  }
  const eng = res;
  let error = null;
  const frames = [];
  try {
    if (rig.kind === "grid") eng.setMapGrid(rig.gridW, rig.gridH);
    else if (rig.kind === "cloud") eng.setMap3D(rig.points);
    eng.setWallClock(WALL_CLOCK);
    const wants = eng.wantsSensors();
    const delta = 1000 / opts.fps;
    const warm = Math.round(opts.skip * opts.fps);
    const total = warm + Math.round(opts.seconds * opts.fps);
    const keep = Math.max(1, Math.round(opts.fps / opts.gifFps));
    for (let i = 0; i < total; i++) {
      if (wants) eng.setSensors(sensorSlots(synthSensorFrame(i, opts.fps)));
      const frame = eng.frame(delta);
      const err = eng.takeError();
      if (err && !error) error = `runtime @f${i}: ${err.message ?? err}`;
      if (i >= warm && (i - warm) % keep === 0) {
        const rgb = mk();
        layout.paint(frame, rgb);
        frames.push({ rgb });
      }
    }
  } finally {
    eng.free();
  }
  return { frames, error };
}

// ---- HTML -------------------------------------------------------------------

const VERDICT_ORDER = ["match", "close", "divergent", "broken", "orig-unrenderable"];
const VERDICT_BLURB = {
  match: "a viewer would accept them as the same pattern",
  close: "same pattern, minor visible differences",
  divergent: "recognizably related but wrong in a major axis",
  broken: "port errors, dies, or bears no resemblance",
  "orig-unrenderable": "the ORIGINAL fails on our engine — no comparison possible (not a port score)",
};
const esc = (s) =>
  String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

function buildHtml(entries) {
  const groups = new Map(VERDICT_ORDER.map((v) => [v, []]));
  for (const e of entries) (groups.get(e.verdict) ?? groups.set(e.verdict, []).get(e.verdict)).push(e);
  for (const g of groups.values()) g.sort((a, b) => b.score - a.score || a.slug.localeCompare(b.slug));

  const counts = VERDICT_ORDER.map((v) => `${v} ${groups.get(v)?.length ?? 0}`).join(" · ");
  const sections = [];
  for (const v of VERDICT_ORDER) {
    const g = groups.get(v);
    if (!g?.length) continue;
    const cards = g
      .map((e) => {
        const gif = (side, err) => `
        <figure>
          <img loading="lazy" src="gifs/${e.slug}-${side}.gif" alt="${side}">
          <figcaption>${side === "orig" ? "original" : "port"}${err ? ` <span class="err" title="${esc(err)}">⚠ ${esc(err.slice(0, 60))}</span>` : ""}</figcaption>
        </figure>`;
        const list = (title, items, fmt = esc) =>
          items?.length
            ? `<h4>${title}</h4><ul>${items.map((x) => `<li>${fmt(x)}</li>`).join("")}</ul>`
            : "";
        const dialFmt = (d) =>
          `<b>${esc(d.name)}</b> — ${d.matches ? "✓ matches" : "✗ differs"}<br>orig: ${esc(d.origEffect)}<br>port: ${esc(d.portEffect)}`;
        return `
      <div class="card" id="${e.slug}" data-slug="${e.slug}">
        <div class="head">
          <a class="slug" href="#${e.slug}">${e.slug}</a>
          <span class="badges">
            <span class="score s${e.score}">${e.score}/10</span>
            <span class="conf">${esc(e.confidence ?? "")}</span>
          </span>
        </div>
        <div class="gifs">${gif("orig", e.origError)}${gif("port", e.portError)}</div>
        <details>
          <summary>${esc(e.summary)}</summary>
          ${list("Observations", e.observations)}
          ${list("Dials", e.dials, dialFmt)}
          ${list("Feedback for the fix pass", e.feedback)}
          ${e.experiments?.length ? `<h4>Experiments</h4><p class="exp">${esc(e.experiments.join(", "))}</p>` : ""}
        </details>
      </div>`;
      })
      .join("\n");
    sections.push(`
    <section id="v-${v}">
      <h2>${v} <span class="count">${g.length}</span> <span class="blurb">${VERDICT_BLURB[v] ?? ""}</span></h2>
      <div class="grid">${cards}</div>
    </section>`);
  }

  return `<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<title>Luxel port verification — visual report</title>
<style>
  :root { color-scheme: dark; }
  body { margin: 0; background: #101014; color: #d8d8e0; font: 14px/1.45 system-ui, sans-serif; }
  header { position: sticky; top: 0; background: #16161cee; backdrop-filter: blur(4px);
           padding: 10px 18px; border-bottom: 1px solid #2a2a33; z-index: 2;
           display: flex; gap: 16px; align-items: baseline; flex-wrap: wrap; }
  header h1 { font-size: 17px; margin: 0; }
  header .counts { color: #9a9aa8; font-size: 13px; }
  header input { background: #0c0c10; border: 1px solid #33333d; color: inherit;
                 border-radius: 6px; padding: 4px 10px; min-width: 220px; }
  header nav a { color: #8ab4ff; margin-right: 10px; text-decoration: none; font-size: 13px; }
  main { padding: 12px 18px 60px; }
  section h2 { margin: 26px 0 10px; font-size: 16px; text-transform: capitalize; }
  section h2 .count { background: #2a2a33; border-radius: 10px; padding: 1px 9px; font-size: 13px; }
  section h2 .blurb { color: #8a8a98; font-size: 12px; font-weight: normal; margin-left: 8px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(430px, 1fr)); gap: 12px; }
  .card { background: #17171d; border: 1px solid #26262f; border-radius: 10px; padding: 10px 12px; }
  .head { display: flex; justify-content: space-between; align-items: baseline; gap: 8px; }
  .slug { font-family: ui-monospace, monospace; font-size: 13px; color: #e6e6ee; text-decoration: none; }
  .score { border-radius: 5px; padding: 1px 7px; font-size: 12px; font-weight: 600; background: #333; }
  .score.s10,.score.s9 { background: #14532d; } .score.s8,.score.s7 { background: #3f6212; color:#e7ffd0; }
  .score.s6,.score.s5 { background: #713f12; } .score.s4,.score.s3 { background: #7c2d12; }
  .score.s2,.score.s1,.score.s0 { background: #7f1d1d; }
  .conf { color: #7d7d8c; font-size: 11px; }
  .gifs { display: flex; gap: 8px; margin: 8px 0 4px; }
  .gifs figure { margin: 0; flex: 1; min-width: 0; text-align: center; }
  .gifs img { width: 100%; height: auto; image-rendering: pixelated; border-radius: 4px;
              background: #000; border: 1px solid #202027; }
  .gifs figcaption { font-size: 11px; color: #8a8a98; margin-top: 3px; }
  .err { color: #ff9d9d; }
  details { margin-top: 6px; font-size: 13px; }
  summary { cursor: pointer; color: #c9c9d6; }
  details h4 { margin: 10px 0 4px; font-size: 12px; text-transform: uppercase; letter-spacing: .04em; color: #9a9aa8; }
  details ul { margin: 0; padding-left: 18px; }
  details li { margin: 3px 0; color: #bdbdca; }
  .exp { color: #74748a; font-size: 11px; font-family: ui-monospace, monospace; }
  .hidden { display: none; }
</style></head>
<body>
<header>
  <h1>Luxel port verification — 293 pairs</h1>
  <span class="counts">${esc(counts)}</span>
  <input id="q" type="search" placeholder="filter by slug…">
  <nav>${VERDICT_ORDER.map((v) => `<a href="#v-${v}">${v}</a>`).join("")}</nav>
</header>
<main>
<p style="color:#8a8a98;max-width:70em">Each pair renders ${opts.seconds} s from t=0 under the sweep's exact conventions (same rig, seed&nbsp;1, pinned wall clock, ${opts.fps} fps simulation shown at ${opts.gifFps} fps, beat120 sensor feed where a side binds sensors). Click a card's summary line for the judge's full verdict. Note some ORIGINALS have warm-up transients — the first seconds are not always the steady state the verdict describes.</p>
${sections.join("\n")}
</main>
<script>
  const q = document.getElementById("q");
  q.addEventListener("input", () => {
    const needle = q.value.trim().toLowerCase();
    for (const c of document.querySelectorAll(".card"))
      c.classList.toggle("hidden", needle && !c.dataset.slug.includes(needle));
  });
</script>
</body></html>`;
}

// ---- main -------------------------------------------------------------------

const manifest = JSON.parse(fs.readFileSync(PAIRS, "utf8"));
const verdictFiles = fs.readdirSync(RESULTS).filter((f) => f.endsWith(".json"));
const wanted = opts.slugs ? new Set(opts.slugs) : null;

fs.mkdirSync(GIFS, { recursive: true });

const entries = [];
let host = null;
let done = 0;
for (const f of verdictFiles.sort()) {
  const slug = f.replace(/\.json$/, "");
  const verdict = JSON.parse(fs.readFileSync(path.join(RESULTS, f), "utf8"));
  const entry = {
    slug,
    verdict: verdict.verdict,
    score: verdict.score ?? 0,
    confidence: verdict.confidence,
    summary: verdict.summary ?? "",
    observations: verdict.observations,
    dials: verdict.dials,
    feedback: verdict.feedback,
    experiments: verdict.experiments,
    origError: null,
    portError: null,
  };
  entries.push(entry);
  if (wanted && !wanted.has(slug)) continue;

  const pair = manifest.pairs.find((p) => p.slug === slug);
  if (!pair) {
    entry.origError = entry.portError = "slug missing from pairs.json";
    continue;
  }
  const sidePaths = {
    orig: path.join(GIFS, `${slug}-orig.gif`),
    port: path.join(GIFS, `${slug}-port.gif`),
  };
  const errPath = path.join(GIFS, `${slug}.err.json`);
  const cached =
    !opts.force && fs.existsSync(sidePaths.orig) && fs.existsSync(sidePaths.port);
  if (opts.htmlOnly || cached) {
    if (fs.existsSync(errPath)) Object.assign(entry, JSON.parse(fs.readFileSync(errPath, "utf8")));
    continue;
  }

  host ??= await load();
  const rig = rigFor(pair);
  const layout = makeLayout(rig);
  const sources = {
    orig: JSON.parse(fs.readFileSync(path.join(ROOT, pair.epeFile), "utf8"))?.sources?.main,
    port: fs.readFileSync(path.join(ROOT, pair.libFile), "utf8"),
  };
  for (const side of ["orig", "port"]) {
    const { frames, error } = renderSide(host, sources[side], rig, layout);
    fs.writeFileSync(
      sidePaths[side],
      encodeGif(layout.w, layout.h, frames, Math.round(100 / opts.gifFps)),
    );
    if (side === "orig") entry.origError = error;
    else entry.portError = error;
  }
  if (entry.origError || entry.portError)
    fs.writeFileSync(
      errPath,
      JSON.stringify({ origError: entry.origError, portError: entry.portError }),
    );
  else if (fs.existsSync(errPath)) fs.unlinkSync(errPath);
  done++;
  if (done % 20 === 0) console.log(`  rendered ${done} pairs…`);
}

fs.writeFileSync(path.join(REPORT, "index.html"), buildHtml(entries));
console.log(
  `report: ${entries.length} verdicts, ${done} pair(s) rendered → ${path.join(REPORT, "index.html")}`,
);
