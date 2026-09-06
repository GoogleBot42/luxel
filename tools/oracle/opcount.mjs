#!/usr/bin/env node
// Count the bytecode operations the PIXELBLAZE's own compiler emits for a
// pattern, and put them side by side with Luxel's (Gitea #312).
//
//   nix develop -c node tools/oracle/opcount.mjs                # the #312 benchmarks
//   nix develop -c node tools/oracle/opcount.mjs library/foo.js …
//   nix develop -c node tools/oracle/opcount.mjs --md out.md --json out.json
//   nix develop -c node tools/oracle/opcount.mjs --cache pb-webui.html --no-luxel
//
// #312 measured that one iteration of `x += i * 0.5` costs 4.1 µs on the
// Pixelblaze and 5.0–6.2 µs on our boards, but the PB's cycles-per-OPERATION
// was a guess because nobody had counted the operations its compiler emits.
// This tool counts them.
//
// ---------------------------------------------------------------------------
// What it touches on the oracle, and what it does NOT
// ---------------------------------------------------------------------------
// ONE read-only HTTP GET of `http://<ip>/` — the device's web UI page. No
// websocket, no `setCode`, no live-coding, nothing written: the oracle keeps
// rendering whatever it was rendering. (`--cache` skips even that.) The
// compiler then runs LOCALLY in a node:vm sandbox, exactly as
// `tools/oracle/compiler.mjs` already does for every other oracle script —
// that module is the shared, blessed way we drive the PB's browser-side
// compiler, so opcount.mjs does not open a browser of its own.
//
// The oracle's FIRMWARE is never read, reversed or disassembled, and no PB
// code is copied into this repo. Everything below is derived at RUN TIME from
// the client-side JavaScript the device serves to a browser:
//
//   1. `window.compile(src, …)` — the pattern compiler (extracted by
//      compiler.mjs). Its return value `{compiled: int32[], exports: […]}` is
//      the exact word array the UI packs and sends over the websocket.
//   2. The compiler's own opcode table — a `{mnemonic: {stack, opcode, …}}`
//      object literal in that JavaScript, read out of the page text and
//      evaluated in a sandbox each run (never transcribed into this repo, so
//      a firmware update that renumbers opcodes updates us with it; if the
//      literal moves, we abort loudly rather than decode with a stale table).
//   3. The word ENCODING, which is stated by the compiler's own encoder:
//      an instruction word is `inline << 16 | stack << 8 | opcode << 1 | 1`
//      and a bare address word is `addr << 1`. So bit 0 tags the word:
//        bit0 = 1 → an instruction (opcode, its stack arity, one 16-bit
//                   inline operand — local/global slot, jump target, …),
//        bit0 = 0 → a data word (a 16.16 literal, or a function address).
//      Every instruction is exactly ONE 32-bit word; there are no
//      variable-length instructions to guess at.
//
// ---------------------------------------------------------------------------
// What it can measure, and what it cannot
// ---------------------------------------------------------------------------
// MEASURABLE (all of it from the client-side encoding above):
//   • total words, instruction words and data words per pattern;
//   • the same, per function — init / beforeRender / render / render2D /
//     each control handler — with function boundaries recovered from the
//     `<function address>; gstore <global>` pairs the top-level init code
//     emits, cross-referenced against the compiler's own export table;
//   • the decoded mnemonic + operand of every instruction (`--disasm`);
//   • loop spans, from backward branches (a `jmp`/`jmpz`/… whose inline
//     target is below its own address), and hence the STATIC word count of
//     one loop iteration — the number #312 needs.
//
// NOT MEASURABLE without crossing the black-box rule, and therefore NOT
// claimed here:
//   • what any of it COSTS. Cycles per operation on the PB are only ever
//     inferred here from a wall-clock measurement (#312's 4.1 µs/iteration)
//     divided by a counted operation count. We do not know the PB
//     interpreter's dispatch shape and will not look.
//   • whether the PB's VM dispatches a data word as its own "push constant"
//     operation or folds it into the consumer. The bit-0 tag makes a separate
//     dispatch overwhelmingly likely, but it is FIRMWARE behaviour, so both
//     numbers are reported (13 words vs 11 instruction words per iteration
//     for the microbench) and the cycles/op table gives a range, not a point.
//   • anything about PB semantics not visible in the emitted stream.
//
// ---------------------------------------------------------------------------
// The Luxel half
// ---------------------------------------------------------------------------
//   `luxel compile <f> --stats [--no-fuse]` → static instruction counts per
//   function, fused (what ships) and unfused (the #261 A/B baseline).
//   `luxel bench <f> --profile --json`      → dynamic instructions per pixel.
// Static counts are NOT comparable one-for-one with the PB's: Luxel fuses
// adjacent pairs into superinstructions (#261) and folds small literals into
// the instruction word, so one Luxel op can be two or three PB words. That is
// the point of the table — the same source, both compilers, counted the same
// way.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";
import { buildCompiler } from "./compiler.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

// ---- measured wall-clock, from #312 (µs per loop iteration, 240 MHz) -------
// These are inputs, not measurements this tool makes; they are what the
// counted op counts get divided into.
const MHZ = 240;
const MEASURED_US = [
  ["Pixelblaze oracle (ESP32, fw 3.67)", 4.11, "pb"],
  ["Luxel, Athom (classic ESP32), master", 6.2, "lx"],
  ["Luxel, Seengreat S3, d267836", 5.02, "lx"],
];

// ---- argv -----------------------------------------------------------------
const argv = process.argv.slice(2);
const flag = (n, d = null) => {
  const i = argv.indexOf(n);
  if (i < 0) return d;
  const [, v] = argv.splice(i, 2);
  return v ?? d;
};
const has = (n) => {
  const i = argv.indexOf(n);
  if (i < 0) return false;
  argv.splice(i, 1);
  return true;
};
const opts = {
  ip: flag("--ip", "192.168.0.140"),
  cache: flag("--cache"),
  saveCache: flag("--save-cache"),
  md: flag("--md"),
  json: flag("--json"),
  loopK: (flag("--loop", "16,64") ?? "").split(",").map(Number),
  pixels: Number(flag("--pixels", "256")),
  frames: Number(flag("--frames", "20")),
  grid: flag("--grid", "16x16"),
  disasm: has("--disasm"),
  noLuxel: has("--no-luxel"),
};
const files = argv.filter((a) => !a.startsWith("--"));

/** The #312 benchmark set. perlin-fire is here to be REPORTED as rejected. */
const DEFAULT_FILES = [
  "library/rainbow.js",
  "library/snake.js",
  "library/snake-2d.js",
  "library/perlin-fire.js",
  // Stand-in for perlin-fire as the builtin-heavy case: same clean-room noise
  // model, but written without the `const` declarations PB's parser rejects.
  "library/perlin-fire-wind-tunnel.js",
].map((f) => path.join(root, f));

/** The #312 microbenchmark, verbatim in shape: K iterations of x += i * 0.5. */
const loopSource = (k) =>
  `export function render(index) { var x = 0; for (var i = 0; i < ${k}; i++) { x += i * 0.5 } hsv(x, 1, 1) }\n`;

// ---- the PB compiler + its opcode table, both from the served page --------
async function loadWebUI() {
  if (opts.cache) return fs.readFileSync(opts.cache, "utf8");
  // node's fetch handles the page's gzip Content-Encoding; a raw curl does not.
  const res = await fetch(`http://${opts.ip}/`);
  if (!res.ok) throw new Error(`GET http://${opts.ip}/ → ${res.status}`);
  const html = await res.text();
  if (opts.saveCache) fs.writeFileSync(opts.saveCache, html);
  return html;
}

/**
 * The compiler's opcode table, evaluated out of the served page.
 *
 * It is a plain object literal `{end: {stack, opcode}, return: {…}, …}` built
 * with a running counter, so it evaluates standalone given that counter. We
 * read it rather than hardcode it: the numbering is the PB's, not ours, and a
 * firmware update is free to change it.
 */
function opcodeTable(html) {
  const START = "{end:{stack:0,opcode:";
  const s = html.indexOf(START);
  if (s < 0) throw new Error("opcode table not found in the served web UI");
  const e = html.indexOf("}};", s);
  if (e < 0) throw new Error("opcode table end marker not found");
  const table = vm.runInNewContext(
    // `n` is the running opcode counter, `t` the module's exports object the
    // literal incidentally writes through; both are local to this sandbox.
    `(function () { var n = 0, t = {}; return ${html.slice(s, e + 2)}; })()`,
    {},
    { timeout: 5000 },
  );
  const byOp = new Map();
  for (const [name, d] of Object.entries(table)) {
    if (!byOp.has(d.opcode)) byOp.set(d.opcode, name);
  }
  if (byOp.size < 60) throw new Error(`opcode table looks wrong (${byOp.size} entries)`);
  return { table, byOp };
}

// ---- decoding the emitted word stream -------------------------------------
const isInsn = (w) => (w & 1) === 1;
const opOf = (w) => (w & 0xff) >> 1;
const stackOf = (w) => (w >> 8) & 0xff;
const inlineOf = (w) => w >> 16; // signed: negative = a param slot below the frame
const BRANCHES = new Set(["jmp", "jmpz", "jmpnz", "pjmpz", "pjmpnz"]);

/** One decoded word, ready to print or to count. */
function decodeWord(w, byOp) {
  if (!isInsn(w)) {
    return { kind: "data", word: w, addr: w >> 1, fx: w / 65536 };
  }
  return {
    kind: "insn",
    word: w,
    op: byOp.get(opOf(w)) ?? `?${opOf(w)}`,
    stack: stackOf(w),
    inline: inlineOf(w),
  };
}

/**
 * Split the word stream into the top-level init block plus one region per
 * function.
 *
 * The compiler materializes a function by pushing its entry address as a data
 * word (`addr << 1`) and `gstore`-ing it into a global; every function body
 * starts with `sinit`. So: a data word whose `>> 1` lands on a `sinit` and
 * which is immediately followed by a `gstore` is a function reference, and the
 * global it is stored into names the function via the compiler's export table.
 * Bodies are laid out consecutively after the init block, so each region ends
 * where the next begins.
 */
function functionRegions(compiled, exportsByGlobal, byOp) {
  const entries = new Map(); // entry address → global slot
  for (let i = 0; i < compiled.length; i++) {
    const w = compiled[i];
    if (isInsn(w)) continue;
    const a = w >> 1;
    if (a <= 0 || a >= compiled.length) continue;
    if (!isInsn(compiled[a]) || byOp.get(opOf(compiled[a])) !== "sinit") continue;
    const next = compiled[i + 1];
    if (next === undefined || !isInsn(next) || byOp.get(opOf(next)) !== "gstore") continue;
    entries.set(a, inlineOf(next));
  }
  const addrs = [...entries.keys()].sort((a, b) => a - b);
  const regions = [{ name: "(init)", start: 0, end: addrs[0] ?? compiled.length }];
  addrs.forEach((a, i) => {
    const g = entries.get(a);
    regions.push({
      name: exportsByGlobal.get(g) ?? `(local fn @g${g})`,
      start: a,
      end: addrs[i + 1] ?? compiled.length,
    });
  });
  return regions;
}

/** Words / instruction words / data words in `[start, end)`. */
function countRange(compiled, start, end) {
  let insns = 0;
  let data = 0;
  for (let i = start; i < end; i++) (isInsn(compiled[i]) ? insns++ : data++);
  return { words: end - start, insns, data };
}

/**
 * Backward branches in a region — one per loop. The span `[target, branch]`
 * inclusive is everything one iteration re-executes: the condition, the body
 * and the branch itself.
 */
function loops(compiled, region, byOp) {
  const out = [];
  for (let i = region.start; i < region.end; i++) {
    const w = compiled[i];
    if (!isInsn(w)) continue;
    const op = byOp.get(opOf(w));
    if (!BRANCHES.has(op)) continue;
    const t = inlineOf(w);
    if (t >= i || t < region.start) continue;
    out.push({ op, from: i, to: t, ...countRange(compiled, t, i + 1) });
  }
  return out;
}

/** Everything opcount knows about one PB compilation. */
function analysePB(source, compile, byOp) {
  const r = compile(source);
  if (!r.ok) return { ok: false, error: r.error };
  const compiled = r.compiled;
  const exportsByGlobal = new Map(r.exports.map((e) => [e.address, e.name]));
  const regions = functionRegions(compiled, exportsByGlobal, byOp);
  return {
    ok: true,
    exports: r.exports.map((e) => e.name),
    total: countRange(compiled, 0, compiled.length),
    bytes: 4 * compiled.length,
    fns: regions.map((g) => ({
      name: g.name,
      start: g.start,
      end: g.end,
      ...countRange(compiled, g.start, g.end),
      loops: loops(compiled, g, byOp),
    })),
    compiled,
  };
}

function disasm(compiled, byOp) {
  return compiled
    .map((w, i) => {
      const d = decodeWord(w, byOp);
      return d.kind === "insn"
        ? `${String(i).padStart(5)}  ${d.op.padEnd(12)} stack=${d.stack} inline=${d.inline}`
        : `${String(i).padStart(5)}  ${"<data>".padEnd(12)} 16.16=${d.fx}  (as addr ${d.addr})`;
    })
    .join("\n");
}

// ---- the Luxel half -------------------------------------------------------
/** Build luxel-cli with the dynamic profiler, in its own target dir (#261). */
function buildLuxel() {
  const targetDir = path.join(root, "target/profile");
  execFileSync(
    "cargo",
    ["build", "--release", "-p", "luxel-cli", "--features", "profile", "--target-dir", targetDir],
    { cwd: root, stdio: ["ignore", "ignore", "inherit"] },
  );
  return path.join(targetDir, "release/luxel");
}

const jsonLine = (out) => JSON.parse(out.trim().split("\n").filter((l) => l.startsWith("{")).pop());

function luxelStats(bin, file, fused) {
  const args = ["compile", file, "--stats"];
  if (!fused) args.push("--no-fuse");
  try {
    return jsonLine(execFileSync(bin, args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }));
  } catch (e) {
    return { error: String(e.stderr ?? e.message).trim() };
  }
}

function luxelProfile(bin, file) {
  try {
    const out = execFileSync(
      bin,
      ["bench", file, "--pixels", String(opts.pixels), "--map-grid", opts.grid,
        "--frames", String(opts.frames), "--profile", "--json"],
      { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
    );
    return jsonLine(out);
  } catch (e) {
    return { error: String(e.stderr ?? e.message).trim() };
  }
}

// ---- reporting ------------------------------------------------------------
const n = (v, d = 0) => (v === undefined || v === null || Number.isNaN(v) ? "—" : v.toFixed(d));

function table(headers, rows) {
  const w = headers.map((h, i) => Math.max(h.length, ...rows.map((r) => String(r[i]).length)));
  const line = (cells) => cells.map((c, i) => String(c).padEnd(w[i])).join("  ");
  return [line(headers), w.map((x) => "-".repeat(x)).join("  "), ...rows.map(line)].join("\n");
}

function mdTable(headers, rows) {
  return [
    `| ${headers.join(" | ")} |`,
    `|${headers.map(() => "---").join("|")}|`,
    ...rows.map((r) => `| ${r.join(" | ")} |`),
  ].join("\n");
}

async function main() {
  const html = await loadWebUI();
  const { byOp } = opcodeTable(html);
  const compile = buildCompiler(html);
  const bin = opts.noLuxel ? null : buildLuxel();
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "luxel-opcount-"));

  const results = [];
  const analyse = (label, file, source) => {
    const pb = analysePB(source, compile, byOp);
    const lx = bin
      ? {
          fused: luxelStats(bin, file, true),
          unfused: luxelStats(bin, file, false),
          dyn: luxelProfile(bin, file),
        }
      : null;
    const r = { label, file, pb, lx };
    results.push(r);
    return r;
  };

  for (const f of files.length ? files.map((f) => path.resolve(f)) : DEFAULT_FILES) {
    if (!fs.existsSync(f)) {
      console.error(`skip (no such file): ${f}`);
      continue;
    }
    analyse(path.relative(root, f), f, fs.readFileSync(f, "utf8"));
  }

  // The K-sweep microbenchmark, generated so it is reproducible from this file
  // alone rather than from a session scratchpad.
  const loopRuns = [];
  for (const k of opts.loopK) {
    const src = loopSource(k);
    const file = path.join(tmp, `loop-k${k}.js`);
    fs.writeFileSync(file, src);
    loopRuns.push({ k, ...analyse(`loop microbench K=${k}`, file, src) });
  }

  // ---------------- text report ----------------
  const out = [];
  out.push(
    opts.cache
      ? `Pixelblaze compiler: from cached web UI ${opts.cache}`
      : `Pixelblaze compiler: from http://${opts.ip}/ (read-only GET; device state untouched)`,
  );
  out.push(`Luxel: ${bin ?? "(skipped, --no-luxel)"}`);
  out.push(`Luxel dynamic rig: ${opts.pixels} px, grid ${opts.grid}, ${opts.frames} frames\n`);

  // A pattern the PB compiler rejects still gets its Luxel columns — the row
  // is the honest answer to "how does this one compare", not a blank.
  const rows = [];
  for (const r of results) {
    const render = r.pb.ok
      ? (r.pb.fns.find((f) => f.name === "render2D") ?? r.pb.fns.find((f) => f.name === "render"))
      : null;
    const lxRender = r.lx?.fused?.fns?.find((f) => f.name === "render2D")
      ?? r.lx?.fused?.fns?.find((f) => f.name === "render");
    rows.push([
      r.label,
      r.pb.ok ? r.pb.total.words : "REJECTED",
      r.pb.ok ? r.pb.total.insns : "—",
      render ? `${render.words}/${render.insns}` : "—",
      r.lx?.fused?.insns ?? "—",
      lxRender?.insns ?? "—",
      r.lx?.dyn?.insns_per_px !== undefined ? n(r.lx.dyn.insns_per_px, 1) : "—",
    ]);
  }
  out.push("PER PATTERN — PB static words vs Luxel static fused ops");
  out.push(
    table(
      ["pattern", "PB words", "PB insn-words", "PB render w/i", "Lx static ops", "Lx render ops", "Lx dyn ops/px"],
      rows,
    ),
  );

  for (const r of results) {
    if (!r.pb.ok) {
      out.push(`\n${r.label}: PB compiler REJECTED — ${r.pb.error}`);
      continue;
    }
    out.push(`\n${r.label} — per function (PB)`);
    out.push(
      table(
        ["function", "words", "insn", "data", "loops"],
        r.pb.fns.map((f) => [
          f.name,
          f.words,
          f.insns,
          f.data,
          f.loops.map((l) => `${l.to}..${l.from} (${l.words}w/${l.insns}i)`).join(" ") || "—",
        ]),
      ),
    );
    if (opts.disasm) out.push(disasm(r.pb.compiled, byOp));
  }

  // ---------------- the microbenchmark ----------------
  const mdBlocks = [];
  if (loopRuns.length >= 2 && loopRuns.every((r) => r.pb.ok)) {
    const a = loopRuns[0];
    const b = loopRuns[loopRuns.length - 1];
    const sameShape = a.pb.total.words === b.pb.total.words;
    const pbLoop = a.pb.fns
      .find((f) => f.name === "render")
      ?.loops.sort((x, y) => y.words - x.words)[0];
    const dk = b.k - a.k;
    const lxPerIter =
      a.lx?.dyn?.insns_per_px !== undefined && b.lx?.dyn?.insns_per_px !== undefined
        ? (b.lx.dyn.insns_per_px - a.lx.dyn.insns_per_px) / dk
        : undefined;

    out.push("\nLOOP MICROBENCHMARK — x += i * 0.5, per iteration");
    out.push(
      `  PB bytecode is ${sameShape ? "IDENTICAL" : "NOT identical"} for K=${a.k} and K=${b.k} ` +
        `(${a.pb.total.words} vs ${b.pb.total.words} words) — the PB compiler does not unroll, so the ` +
        `per-iteration count comes from the loop's backward branch, not from a K difference.`,
    );
    if (pbLoop) {
      out.push(
        `  PB loop span words ${pbLoop.to}..${pbLoop.from}: ${pbLoop.words} words = ` +
          `${pbLoop.insns} instruction words + ${pbLoop.data} data words`,
      );
    }
    out.push(
      `  Luxel dynamic: (${n(b.lx?.dyn?.insns_per_px, 1)} − ${n(a.lx?.dyn?.insns_per_px, 1)}) / ${dk} = ` +
        `${n(lxPerIter, 2)} fused ops per iteration`,
    );

    // Cycles per op: measured µs/iteration (from #312) ÷ the counted ops.
    const cyc = (us) => (us * MHZ).toFixed(0);
    const perOp = (us, ops) => (ops ? ((us * MHZ) / ops).toFixed(0) : "—");
    const cycRows = MEASURED_US.map(([name, us, side]) => {
      const ops = side === "pb" ? pbLoop?.insns : lxPerIter;
      const opsAll = side === "pb" ? pbLoop?.words : lxPerIter;
      return [
        name,
        us.toFixed(2),
        cyc(us),
        side === "pb" ? `${pbLoop?.insns ?? "—"} insn / ${pbLoop?.words ?? "—"} words` : n(lxPerIter, 1),
        perOp(us, ops),
        perOp(us, opsAll),
      ];
    });
    const cycHead = [
      "device",
      "µs/iter",
      "cycles/iter",
      "ops/iter",
      "cycles/op (insn words)",
      "cycles/op (all words)",
    ];
    out.push("\n  cycles per operation, from #312's measured µs/iteration at 240 MHz");
    out.push(table(cycHead, cycRows));
    mdBlocks.push({ head: cycHead, rows: cycRows, pbLoop, lxPerIter, sameShape, a, b });
  }

  const text = out.join("\n");
  console.log(text);

  if (opts.json) {
    fs.writeFileSync(
      opts.json,
      `${JSON.stringify(
        {
          ip: opts.cache ? null : opts.ip,
          rig: { pixels: opts.pixels, grid: opts.grid, frames: opts.frames },
          results: results.map((r) => ({ ...r, pb: { ...r.pb, compiled: undefined } })),
        },
        null,
        1,
      )}\n`,
    );
    console.error(`wrote ${opts.json}`);
  }
  if (opts.md) {
    const md = [
      "### Pixelblaze vs Luxel — static bytecode operation counts",
      "",
      mdTable(
        ["pattern", "PB words", "PB insn-words", "PB render w/i", "Lx static ops", "Lx render ops", "Lx dyn ops/px"],
        rows,
      ),
      "",
      ...mdBlocks.flatMap((b) => [
        `Loop microbench (\`x += i * 0.5\`), per iteration: PB ${b.pbLoop?.words} words ` +
          `(${b.pbLoop?.insns} instruction + ${b.pbLoop?.data} data), Luxel ${n(b.lxPerIter, 1)} fused ops.`,
        "",
        mdTable(b.head, b.rows),
      ]),
    ].join("\n");
    fs.writeFileSync(opts.md, `${md}\n`);
    console.error(`wrote ${opts.md}`);
  }

  fs.rmSync(tmp, { recursive: true, force: true });
}

main().catch((e) => {
  console.error(`error: ${e.message}`);
  process.exit(1);
});
