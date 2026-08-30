// Interactive review UI for the port-verification sweep: a local web app that
// runs BOTH sides of every pair live in the browser — the corpus original and
// its clean-room port, on the same engine wasm, same rig, same synthetic
// sensor feed — side by side, with the judge's verdict attached, so a human
// can watch them and record a decision per pattern.
//
// This supersedes tools/verify/report.mjs for triage: the report renders fixed
// GIFs, this renders live engines you can drive (controls, reset, fps) and
// records what you decided.
//
// Usage:
//   nix develop -c cargo build --release --target wasm32-unknown-unknown -p luxel-wasm
//   nix develop -c node tools/verify/review.mjs [--port 4183]
//
// Then open the printed URL. Nothing is generated ahead of time; every route
// assembles from the repo at request time, so editing library/*.js or a
// verdict JSON and reloading the page picks it up.
//
// Endpoints
//   GET  /                 the UI (tools/verify/review/index.html)
//   GET  /app.js /engine.js /style.css   UI modules (tools/verify/review/)
//   GET  /sensormodel.mjs  the shared beat120 sensor model, served for the UI
//   GET  /luxel.wasm       target/wasm32-unknown-unknown/release/luxel_wasm.wasm
//   GET  /api/data         every pair: sources, rig, verdict, current decision
//   GET  /api/decisions    the raw decisions.json
//   POST /api/decision     {slug, decision, forkName?, feedback?} — persists
//                          immediately; {slug, decision:null} clears the entry
//
// decisions.json schema (tools/verify/decisions.json, TRACKED — this is the
// persistent store Jeremy's decisions land in across sittings):
//   { "<slug>": { "decision": "delete"|"good"|"fork"|"needs-work",
//                 "forkName": "<string>",   // fork only, optional
//                 "feedback": "<string>",   // fork/needs-work, goes verbatim
//                                           // to the agent fixing the pattern
//                 "decidedAt": "<ISO 8601>" } }
// Writes are atomic (tmp file + rename) — the file is edited across many
// sittings and must never be left half-written.
//
// Corpus note: corpus/ is gitignored and may be absent (symlink it from the
// main checkout). A missing or unreadable .epe yields origSource:null plus an
// origError string; the UI shows that side as unavailable. Corpus source is
// read at REQUEST time and never written anywhere — see the clean-room rule in
// CLAUDE.md. Per-slug fixups (tools/verify/fixups.json) are applied to the
// original before it is served, exactly as snap.mjs and report.mjs apply them —
// including the per-side `vars` pins, which the UI pushes into each side's
// engine once after init (a client-driven original is black until it is
// written to, which reads as a dead pattern).
//
// SPDX-License-Identifier: Apache-2.0

import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { applySourceFixups, resolveRig, varsOverride } from "./fixups.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "../..");
const UI_DIR = path.join(HERE, "review");
const PAIRS = path.join(HERE, "pairs.json");
const RESULTS = path.join(HERE, "results");
const DECISIONS = path.join(HERE, "decisions.json");
const WASM = path.join(ROOT, "target/wasm32-unknown-unknown/release/luxel_wasm.wasm");
const CLOUD_SIDE = 5;

const DECISION_KINDS = new Set(["delete", "good", "fork", "needs-work"]);

// ---- CLI --------------------------------------------------------------------

let port = 4183;
{
  const argv = process.argv.slice(2);
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--port") port = Number(argv[++i]);
    else if (argv[i] === "--help" || argv[i] === "-h") {
      console.log("usage: node tools/verify/review.mjs [--port 4183]");
      process.exit(0);
    } else {
      console.error(`unknown option ${argv[i]}`);
      process.exit(2);
    }
  }
  if (!Number.isInteger(port) || port <= 0) {
    console.error(`bad --port ${port}`);
    process.exit(2);
  }
}

// ---- preflight --------------------------------------------------------------

if (!fs.existsSync(PAIRS)) {
  console.error(`${PAIRS} missing — run: node tools/verify/gen-pairs.mjs`);
  process.exit(1);
}
if (!fs.existsSync(WASM)) {
  console.error(
    `engine wasm missing at ${WASM}\n` +
      `build it: nix develop -c cargo build --release --target wasm32-unknown-unknown -p luxel-wasm`,
  );
  process.exit(1);
}
// A wasm built before the wall-clock ABI landed (#111) is missing exports the
// UI calls unconditionally — the page dies with a TypeError instead of telling
// anyone to rebuild. Refuse to start on a stale binary.
if (!fs.readFileSync(WASM).includes("lx_set_default_wall_clock")) {
  console.error(
    `engine wasm at ${WASM} is STALE (predates the wall-clock ABI, missing lx_set_default_wall_clock)\n` +
      `rebuild it: nix develop -c cargo build --release --target wasm32-unknown-unknown -p luxel-wasm`,
  );
  process.exit(1);
}

const manifest = JSON.parse(fs.readFileSync(PAIRS, "utf8"));
const slugs = new Set(manifest.pairs.map((p) => p.slug));

// ---- decisions store --------------------------------------------------------

function readDecisions() {
  if (!fs.existsSync(DECISIONS)) return {};
  try {
    const parsed = JSON.parse(fs.readFileSync(DECISIONS, "utf8"));
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch (err) {
    // Never silently start from empty over a store we failed to parse — that
    // would clobber a sitting's worth of decisions on the next write.
    console.error(`refusing to continue: ${DECISIONS} is not valid JSON (${err.message})`);
    process.exit(1);
  }
}

/** tmp + rename, so a crash mid-write can never leave a truncated store. */
function writeDecisions(store) {
  const tmp = `${DECISIONS}.tmp-${process.pid}`;
  const ordered = Object.fromEntries(Object.entries(store).sort(([a], [b]) => a.localeCompare(b)));
  fs.writeFileSync(tmp, `${JSON.stringify(ordered, null, 2)}\n`);
  fs.renameSync(tmp, DECISIONS);
}

// ---- payload ----------------------------------------------------------------

function readVerdict(slug) {
  const file = path.join(RESULTS, `${slug}.json`);
  if (!fs.existsSync(file)) return null;
  try {
    const v = JSON.parse(fs.readFileSync(file, "utf8"));
    return {
      verdict: v.verdict ?? null,
      score: v.score ?? null,
      confidence: v.confidence ?? null,
      summary: v.summary ?? "",
      observations: v.observations ?? [],
      dials: v.dials ?? [],
      feedback: v.feedback ?? [],
      experiments: v.experiments ?? [],
    };
  } catch (err) {
    return { verdict: null, score: null, summary: `unreadable verdict: ${err.message}` };
  }
}

/** The ORIGINAL's source, fixups applied. Corpus is gitignored and optional,
 *  so every failure here is reported, not thrown. */
function readOrig(pair) {
  const file = path.join(ROOT, pair.epeFile);
  if (!fs.existsSync(file))
    return { origSource: null, origError: `corpus file missing: ${pair.epeFile}` };
  try {
    const main = JSON.parse(fs.readFileSync(file, "utf8"))?.sources?.main;
    if (typeof main !== "string")
      return { origSource: null, origError: `${pair.epeFile} has no sources.main` };
    const { source, applied } = applySourceFixups(pair.slug, main);
    return { origSource: source, origError: null, fixups: applied };
  } catch (err) {
    return { origSource: null, origError: `${pair.epeFile}: ${err.message}` };
  }
}

function buildData() {
  const decisions = readDecisions();
  const pairs = manifest.pairs.map((pair) => {
    const rig = resolveRig(pair, { cloudSide: CLOUD_SIDE });
    let portSource = null;
    let portError = null;
    try {
      portSource = fs.readFileSync(path.join(ROOT, pair.libFile), "utf8");
    } catch (err) {
      portError = `${pair.libFile}: ${err.message}`;
    }
    const { origSource, origError, fixups } = readOrig(pair);
    return {
      slug: pair.slug,
      epeName: pair.epeName,
      libFile: pair.libFile,
      epeFile: pair.epeFile,
      ambiguous: pair.ambiguous === true,
      epeFiles: pair.epeFiles ?? null,
      rig,
      fixups: fixups ?? null,
      // Per-side exported-var pins (tools/verify/fixups.json): the UI pushes
      // them into each engine after init, so a client-driven original renders
      // what it renders under the judge harness instead of sitting dark.
      vars: varsOverride(pair.slug),
      verdict: readVerdict(pair.slug),
      origSource,
      origError,
      portSource,
      portError,
      decision: decisions[pair.slug] ?? null,
    };
  });
  return { generatedAt: new Date().toISOString(), total: pairs.length, pairs };
}

// ---- http -------------------------------------------------------------------

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".wasm": "application/wasm",
};

const NO_CACHE = {
  "Cache-Control": "no-store, no-cache, must-revalidate",
  Pragma: "no-cache",
};

function sendJson(res, code, body) {
  const buf = Buffer.from(JSON.stringify(body));
  res.writeHead(code, {
    "Content-Type": TYPES[".json"],
    "Content-Length": buf.length,
    ...NO_CACHE,
  });
  res.end(buf);
}

function sendFile(res, file, { cache = false } = {}) {
  let buf;
  try {
    buf = fs.readFileSync(file);
  } catch {
    res.writeHead(404, { "Content-Type": "text/plain" });
    res.end(`not found: ${path.basename(file)}\n`);
    return;
  }
  res.writeHead(200, {
    "Content-Type": TYPES[path.extname(file)] ?? "application/octet-stream",
    "Content-Length": buf.length,
    // The UI itself is edited while the server runs; never let a reload
    // serve yesterday's app.js out of the browser cache.
    ...(cache ? {} : NO_CACHE),
  });
  res.end(buf);
}

const readBody = (req) =>
  new Promise((resolve, reject) => {
    const chunks = [];
    let n = 0;
    req.on("data", (c) => {
      n += c.length;
      if (n > 1_000_000) {
        reject(new Error("body too large"));
        req.destroy();
        return;
      }
      chunks.push(c);
    });
    req.on("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
    req.on("error", reject);
  });

function handleDecision(res, body) {
  let req;
  try {
    req = JSON.parse(body);
  } catch {
    return sendJson(res, 400, { error: "body is not JSON" });
  }
  const slug = req?.slug;
  if (typeof slug !== "string" || !slugs.has(slug))
    return sendJson(res, 400, { error: `unknown slug ${JSON.stringify(slug)}` });

  const store = readDecisions();
  if (req.decision === null || req.decision === undefined || req.decision === "") {
    delete store[slug];
    writeDecisions(store);
    return sendJson(res, 200, { slug, decision: null });
  }
  if (!DECISION_KINDS.has(req.decision))
    return sendJson(res, 400, {
      error: `decision must be one of ${[...DECISION_KINDS].join("|")} or null`,
    });

  const entry = { decision: req.decision, decidedAt: new Date().toISOString() };
  if (typeof req.forkName === "string" && req.forkName.trim())
    entry.forkName = req.forkName.trim();
  if (typeof req.feedback === "string" && req.feedback.trim())
    entry.feedback = req.feedback.trim();
  store[slug] = entry;
  writeDecisions(store);
  return sendJson(res, 200, { slug, ...entry });
}

const STATIC = new Set(["/app.js", "/engine.js", "/style.css", "/index.html"]);

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, `http://localhost:${port}`);
  const p = url.pathname;

  try {
    if (req.method === "POST" && p === "/api/decision")
      return handleDecision(res, await readBody(req));

    if (req.method !== "GET" && req.method !== "HEAD") {
      res.writeHead(405, { Allow: "GET, POST" });
      return res.end();
    }

    if (p === "/api/data") return sendJson(res, 200, buildData());
    if (p === "/api/decisions") return sendJson(res, 200, readDecisions());
    if (p === "/luxel.wasm") return sendFile(res, WASM);
    if (p === "/sensormodel.mjs") return sendFile(res, path.join(HERE, "sensormodel.mjs"));
    if (p === "/favicon.ico") {
      res.writeHead(204);
      return res.end();
    }
    if (p === "/") return sendFile(res, path.join(UI_DIR, "index.html"));
    if (STATIC.has(p)) return sendFile(res, path.join(UI_DIR, path.basename(p)));

    res.writeHead(404, { "Content-Type": "text/plain" });
    res.end(`not found: ${p}\n`);
  } catch (err) {
    console.error(`500 ${p}: ${err.stack ?? err}`);
    if (!res.headersSent) sendJson(res, 500, { error: String(err.message ?? err) });
    else res.end();
  }
});

server.listen(port, "0.0.0.0", () => {
  const data = buildData();
  const withOrig = data.pairs.filter((x) => x.origSource !== null).length;
  const decided = Object.keys(readDecisions()).length;
  console.log(`review: ${data.total} pairs, ${withOrig} with a readable original`);
  if (withOrig < data.total)
    console.log(
      "  (corpus/ missing or partial — symlink it: " +
        "ln -s /home/googlebot/workspace/pixler/corpus corpus)",
    );
  console.log(`  decisions: ${decided} recorded in ${path.relative(ROOT, DECISIONS)}`);
  console.log(`  → http://localhost:${port}/`);
});
