#!/usr/bin/env node
// mockdiff — a computed-style fidelity instrument for the web UI v2.
//
// The approved mockups (`docs/design/webui-v2/mockups.html`) are the visual
// spec: their `<style>` block is the design system, and each `.frame` is one
// screen at a fixed drawn width. This tool stops "does it look like the mock?"
// being an eyeball question. For every (mock frame → app state) pair in
// `tools/mockdiff.map.json` it
//
//   1. renders the mock frame in real chromium at its drawn width,
//   2. drives the app into the same state at the same width (the native
//      mirror by default, `--device http://ip` for a real board),
//   3. reads `getComputedStyle` for a fixed property list plus the bounding
//      box off BOTH sides for every element of the frame's element map,
//      including its `:hover` and `:focus` states, and
//   4. reports only what differs, ranked by visual weight, with the
//      `web/src` file:line the app's value comes from.
//
// Attribution is real, not guessed: the winning declaration is read back over
// CDP (`CSS.getMatchedStylesForNode`), its Svelte scope hash stripped
// (`.tile.svelte-1ab2c3` → `.tile`), and the authored selector located in
// `web/src`. A value with no rule behind it is reported as `inline`/`ua`.
//
// Usage (from web/, inside the devshell):
//   npm run build
//   E2E_PORT=7500 node tools/mockdiff.mjs                    # every frame
//   E2E_PORT=7500 node tools/mockdiff.mjs --frames S1,S2     # some frames
//   E2E_PORT=7500 node tools/mockdiff.mjs --sweep                 # + the non-CSS half
//   E2E_PORT=7500 node tools/mockdiff.mjs --device http://192.168.0.238
//   E2E_PORT=7500 node tools/mockdiff.mjs --device http://…  --device-bundle
//   E2E_PORT=7500 node tools/mockdiff.mjs --out /tmp/md --no-crops
//
// `--device` runs THIS checkout's bundle against a real board's API;
// `--device-bundle` loads the copy the board serves from its own flash, so the
// two together say whether a device is behind `web/dist`. Either way a real
// board is only given the frames the map marks `deviceSafe` — states whose
// whole recipe is navigation, hover, focus and opening a popover. Every other
// frame writes (typing code live-pushes it, installing a lattice POSTs a
// layout) and is refused with a message rather than pointed at the board.
//
// Output (in `--out`, default /tmp/mockdiff):
//   mockdiff-report.md   per frame, a ranked table of element·property·mock·app·source
//   mockdiff.json        the same as data, for re-runs and diffs between runs
//   mockdiff/<frame>-<element>.png   side-by-side crops of the worst offenders
//
// Tolerance (stated in the report): lengths match within 1px, colours within
// 4/255 per channel and 0.02 alpha, unitless numbers within 0.02. Deviations
// Jeremy asked for that the mocks do not show live in the map's per-frame
// `allow` list and are printed as ALLOWED, never as deltas.

import { spawn } from "node:child_process";
import { execSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import puppeteer from "puppeteer-core";
import { NO_NETIN, PORT as E2E } from "./e2e-common.mjs";
import { lxpBody } from "./lxp.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const WEB = resolve(HERE, "..");
const REPO = resolve(WEB, "..");
const MOCKUPS = join(REPO, "docs/design/webui-v2/mockups.html");

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ── CLI ───────────────────────────────────────────────────────────────────
const argv = process.argv.slice(2);
const flag = (name, def = null) => {
  const i = argv.indexOf(name);
  return i === -1 ? def : (argv[i + 1] ?? true);
};
const has = (name) => argv.includes(name);

const OUT = resolve(String(flag("--out", "/tmp/mockdiff")));
const MAP_PATH = resolve(String(flag("--map", join(HERE, "mockdiff.map.json"))));
const ONLY = flag("--frames", null);
const DEVICE = flag("--device", null); // a real board, e.g. http://192.168.0.238
const CROPS = !has("--no-crops");
// `--device` keeps THIS checkout's bundle and only points it at a real board's
// API, which is what you want while iterating. `--device-bundle` instead loads
// the copy the board serves out of its own flash, so the two runs together say
// whether the device is behind `web/dist`. Both are read-only.
const DEVICE_BUNDLE = has("--device-bundle");
const CHROMIUM =
  process.env.CHROMIUM ?? execSync("command -v chromium", { encoding: "utf8" }).trim();
const WEB_PORT = E2E.web.mockdiff;

// ── the property list ─────────────────────────────────────────────────────
// Everything that carries a visible decision. `cursor` is deliberately out:
// the mock's "buttons" are inert `<div>`s, so it differs on every one of them
// for a reason that is not a design gap.
const PROPS = [
  // colour
  "color",
  "background-color",
  "border-top-color",
  "border-right-color",
  "border-bottom-color",
  "border-left-color",
  "box-shadow",
  "opacity",
  // border box
  "border-top-width",
  "border-right-width",
  "border-bottom-width",
  "border-left-width",
  "border-top-style",
  "border-left-style",
  "border-top-left-radius",
  "border-top-right-radius",
  "border-bottom-right-radius",
  "border-bottom-left-radius",
  // type
  "font-family",
  "font-size",
  "font-weight",
  "line-height",
  "letter-spacing",
  "text-transform",
  // box model
  "padding-top",
  "padding-right",
  "padding-bottom",
  "padding-left",
  "margin-top",
  "margin-right",
  "margin-bottom",
  "margin-left",
  "row-gap",
  "column-gap",
  "min-width",
  "max-width",
  // layout
  "display",
  "flex-direction",
  "flex-wrap",
  "flex-grow",
  "flex-shrink",
  "flex-basis",
  "grid-template-columns",
  "align-items",
  "align-self",
  "justify-content",
  "white-space",
  "overflow-x",
  "overflow-y",
  "position",
  "text-overflow",
];

/** Which family a property belongs to — the report's primary sort. */
const FAMILY = (p) => {
  if (/color|shadow|opacity|background/.test(p)) return "colour";
  if (/^font|line-height|letter-spacing|text-transform/.test(p)) return "type";
  return "layout";
};
const FAMILY_RANK = { missing: 0, extra: 0, layout: 1, colour: 2, type: 3, text: 4 };

/** How much a one-step difference in this property shows on screen. */
const WEIGHT = {
  display: 9,
  width: 9,
  height: 9,
  "grid-template-columns": 8,
  "flex-direction": 8,
  "min-width": 7,
  "max-width": 7,
  "row-gap": 7,
  "column-gap": 7,
  "flex-wrap": 6,
  "align-items": 6,
  "justify-content": 6,
  "align-self": 5,
  "flex-grow": 5,
  "flex-basis": 5,
  "flex-shrink": 4,
  position: 6,
  "white-space": 5,
  "overflow-x": 4,
  "overflow-y": 4,
  "text-overflow": 4,
  "background-color": 6,
  color: 6,
  opacity: 5,
  "box-shadow": 5,
  "font-size": 4,
  "font-weight": 3,
  "font-family": 3,
  "line-height": 3,
  "text-transform": 3,
  "letter-spacing": 2,
};
const weightOf = (p) => {
  if (WEIGHT[p] !== undefined) return WEIGHT[p];
  if (p.startsWith("padding")) return 7;
  if (p.startsWith("margin")) return 6;
  if (p.endsWith("-radius")) return 4;
  if (p.endsWith("-width")) return 5;
  if (p.endsWith("-style")) return 4;
  if (p.endsWith("-color")) return 5;
  return 3;
};

// ── value normalisation + tolerance ───────────────────────────────────────
const TOL = { px: 1, channel: 4, alpha: 0.02, number: 0.02 };

const parseColour = (v) => {
  const m = String(v).match(/^rgba?\(([^)]+)\)$/);
  if (!m) return null;
  const parts = m[1].split(/[,/]/).map((s) => parseFloat(s.trim()));
  if (parts.length < 3 || parts.some((n) => Number.isNaN(n))) return null;
  return [parts[0], parts[1], parts[2], parts.length > 3 ? parts[3] : 1];
};
const parsePx = (v) => {
  const m = String(v).match(/^(-?[\d.]+)px$/);
  return m ? parseFloat(m[1]) : null;
};
const normText = (v) =>
  String(v)
    .replace(/\s+/g, " ")
    .replace(/"/g, "")
    .replace(/,\s*/g, ", ")
    .trim();

/**
 * null when the two values agree within tolerance, else a human delta note.
 *
 * `ctx` is the pair of style maps, so a comparison can ask about a sibling
 * property: the colour of a border nobody draws is not a difference anyone
 * can see, and every `<a>`/`<div>` in the mock reports its border colour as
 * `currentColor` while the app's `<button>` reports `--border`.
 */
function differs(prop, a, b, ctx) {
  if (a === undefined || b === undefined) return null;
  if (String(a) === String(b)) return null;

  // `min-width:auto` on a flex item and `min-width:0px` on a block are the
  // same absence of a floor; neither is an authored value.
  if (prop === "min-width" || prop === "max-width") {
    const soft = (v) => v === "auto" || v === "none" || v === "0px";
    if (soft(a) && soft(b)) return null;
  }

  // a border-side colour only matters if that side is actually drawn
  const side = prop.match(/^border-(top|right|bottom|left)-color$/);
  if (side && ctx) {
    const w = `border-${side[1]}-width`;
    if (parsePx(ctx.mock?.[w]) === 0 && parsePx(ctx.app?.[w]) === 0) return null;
  }

  const pa = parsePx(a);
  const pb = parsePx(b);
  if (pa !== null && pb !== null) {
    const d = Math.abs(pa - pb);
    return d <= TOL.px ? null : { magnitude: Math.min(6, d / 3), note: `${d.toFixed(1)}px` };
  }

  const ca = parseColour(a);
  const cb = parseColour(b);
  if (ca && cb) {
    // fully transparent on both sides is the same paint whatever the channels
    if (ca[3] === 0 && cb[3] === 0) return null;
    const ch = Math.max(...[0, 1, 2].map((i) => Math.abs(ca[i] - cb[i])));
    const al = Math.abs(ca[3] - cb[3]);
    if (ch <= TOL.channel && al <= TOL.alpha) return null;
    const note = al > TOL.alpha ? `Δ${Math.round(ch)}/255, alpha ${ca[3]}→${cb[3]}` : `Δ${Math.round(ch)}/255`;
    return { magnitude: Math.min(6, ch / 25 + al * 4), note };
  }

  if (prop === "opacity" || prop === "flex-grow" || prop === "flex-shrink") {
    const d = Math.abs(parseFloat(a) - parseFloat(b));
    if (!Number.isNaN(d)) return d <= TOL.number ? null : { magnitude: Math.min(6, d * 5), note: "" };
  }

  if (prop === "line-height") {
    // `normal` vs a px value is a real difference; two px values fall through
    // to the px branch above.
    if (normText(a) === normText(b)) return null;
  }

  if (normText(a) === normText(b)) return null;
  return { magnitude: 2, note: "" };
}

// ── the in-page measurement ───────────────────────────────────────────────
/* eslint-disable no-undef */
function measureInPage(rootSel, entries, props) {
  const rootEl = rootSel ? document.querySelector(rootSel) : document.body;
  const rb = rootEl ? rootEl.getBoundingClientRect() : { left: 0, top: 0 };
  const out = {};
  for (const e of entries) {
    let el = null;
    let n = 0;
    try {
      const all = document.querySelectorAll(e.sel);
      n = all.length;
      el = all[e.nth ?? 0] ?? null;
    } catch (err) {
      out[e.id] = { found: false, error: String(err) };
      continue;
    }
    if (!el) {
      out[e.id] = { found: false, count: n };
      continue;
    }
    const cs = getComputedStyle(el);
    const styles = {};
    for (const p of props) styles[p] = cs.getPropertyValue(p);
    // `getComputedStyle` resolves an auto margin to its USED pixels, so the
    // mock's `margin-left:auto` reads back as e.g. "351.094px" and every
    // element pushed right by one looks like a 351px difference from an app
    // that does the same thing with a spacer. Typed OM returns the COMPUTED
    // value, where `auto` is still `auto` — which is the decision we mean to
    // compare.
    if (el.computedStyleMap) {
      const map = el.computedStyleMap();
      for (const q of props) {
        if (!q.startsWith("margin-")) continue;
        const v = map.get(q);
        if (v && String(v) === "auto") styles[q] = "auto";
      }
    }
    const r = el.getBoundingClientRect();
    out[e.id] = {
      found: true,
      count: n,
      tag: el.tagName.toLowerCase(),
      styles,
      box: {
        w: +r.width.toFixed(2),
        h: +r.height.toFixed(2),
        rx: +(r.left - rb.left).toFixed(2),
        ry: +(r.top - rb.top).toFixed(2),
        vx: +r.left.toFixed(2),
        vy: +r.top.toFixed(2),
      },
      text: (el.innerText ?? el.textContent ?? "").replace(/\s+/g, " ").trim(),
      placeholder: el.getAttribute ? (el.getAttribute("placeholder") ?? "") : "",
      value: "value" in el ? String(el.value ?? "") : "",
    };
  }
  return out;
}
/* eslint-enable no-undef */

async function measure(page, rootSel, entries) {
  return page.evaluate(measureInPage, rootSel, entries, PROPS);
}

// ── source attribution ────────────────────────────────────────────────────
/**
 * An index of the authored stylesheets, so a computed value can be traced to
 * the line that declares it. Svelte's scope hash is stripped before the
 * lookup, which is what makes this work at all: the compiled selector
 * `.tile.svelte-1ab2c3 .nm.svelte-1ab2c3` is the authored `.tile .nm`.
 */
function buildSourceIndex(roots) {
  const files = [];
  const walk = (d) => {
    for (const name of readdirSync(d)) {
      const p = join(d, name);
      if (name === "node_modules" || name === "dist" || name.startsWith(".")) continue;
      const st = statSync(p);
      if (st.isDirectory()) walk(p);
      else if (/\.(svelte|css|html)$/.test(name)) files.push(p);
    }
  };
  for (const r of roots) {
    if (!existsSync(r)) continue;
    if (statSync(r).isDirectory()) walk(r);
    else files.push(r);
  }
  return files.map((p) => ({ path: p, lines: readFileSync(p, "utf8").split("\n") }));
}

const stripScope = (sel) => sel.replace(/\.svelte-[a-z0-9]+/g, "").replace(/\s+/g, " ").trim();

/** The shorthands that can set `prop`, longest first (most specific wins). */
function declarersOf(prop) {
  const out = [prop];
  if (/^font-(size|family|weight)$|^line-height$/.test(prop)) out.push("font");
  if (prop.startsWith("padding-")) out.push("padding");
  if (prop.startsWith("margin-")) out.push("margin");
  if (prop.endsWith("-radius")) out.push("border-radius");
  if (/^border-(top|right|bottom|left)-(width|style|color)$/.test(prop)) {
    const [, side, kind] = prop.split("-");
    out.push(`border-${side}`, `border-${kind}`, "border");
  }
  if (prop === "background-color") out.push("background");
  if (prop === "row-gap" || prop === "column-gap") out.push("gap");
  if (prop === "flex-grow" || prop === "flex-shrink" || prop === "flex-basis") out.push("flex");
  if (prop === "overflow-x" || prop === "overflow-y") out.push("overflow");
  if (prop === "grid-template-columns") out.push("grid-template", "grid");
  return out;
}

/**
 * Locate `selector { … prop … }` in the indexed sources. Returns
 * `path:line` of the DECLARATION (not the rule head) when it can find it,
 * `path:line` of the rule head otherwise, or null.
 */
function findInSource(index, selectorText, prop, hashFile) {
  const wanted = declarersOf(prop);
  // A Svelte-scoped rule can only apply inside the component that authored it,
  // and its scope hash says which one. Without this, `.search` (authored in
  // BOTH Patterns.svelte and PatternPicker.svelte) is attributed to whichever
  // file the index happened to walk first.
  const hash = selectorText.match(/\.svelte-([a-z0-9]+)/)?.[1];
  const home = hash && hashFile?.get(hash);
  if (home) {
    const only = index.filter((f) => f.path === home);
    const hit = only.length ? findInSource(only, selectorText.replace(/\.svelte-[a-z0-9]+/g, ""), prop) : null;
    if (hit) return hit;
  }
  const parts = stripScope(selectorText)
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  for (const part of parts) {
    const needle = part.replace(/\s+/g, " ");
    for (const f of index) {
      for (let i = 0; i < f.lines.length; i++) {
        const line = f.lines[i].replace(/\s+/g, " ").trim();
        // prettier puts one selector per line in a list, and the brace on the
        // last one: `.menu,` / `.pop {`
        if (line !== needle && line !== `${needle},` && line !== `${needle} {` && line !== `${needle}{`)
          continue;
        // walk forward to the rule's closing brace looking for the property
        for (let j = i; j < Math.min(f.lines.length, i + 120); j++) {
          const l = f.lines[j];
          const m = l.match(/^\s*([a-z-]+)\s*:/);
          if (m && wanted.includes(m[1])) return `${short(f.path)}:${j + 1}`;
          if (/^\s*}/.test(l) && j > i) break;
        }
        return `${short(f.path)}:${i + 1}`;
      }
    }
  }
  return null;
}

const short = (p) => relative(REPO, p);

/**
 * The rule that wins for `prop` on the node behind `selector`, over CDP.
 * CDP hands matched rules back in ascending specificity, so the last one that
 * declares the property (or a shorthand for it) is the one on screen.
 */
async function attribute(client, docNodeId, selector, nth, props) {
  let nodeId;
  try {
    if (nth) {
      const { nodeIds } = await client.send("DOM.querySelectorAll", {
        nodeId: docNodeId,
        selector,
      });
      nodeId = nodeIds[nth];
    } else {
      ({ nodeId } = await client.send("DOM.querySelector", { nodeId: docNodeId, selector }));
    }
  } catch {
    return {};
  }
  if (!nodeId) return {};
  let matched;
  try {
    matched = await client.send("CSS.getMatchedStylesForNode", { nodeId });
  } catch {
    return {};
  }
  const rules = (matched.matchedCSSRules ?? []).map((m) => m.rule);
  const inline = matched.inlineStyle;
  const out = {};
  for (const prop of props) {
    const wanted = declarersOf(prop);
    if (inline && (inline.cssProperties ?? []).some((c) => wanted.includes(c.name))) {
      out[prop] = { selector: "(inline style)", inline: true };
      continue;
    }
    for (let i = rules.length - 1; i >= 0; i--) {
      const r = rules[i];
      if (r.origin && r.origin !== "regular") continue;
      const decl = (r.style?.cssProperties ?? []).filter(
        (c) => wanted.includes(c.name) && !c.disabled,
      );
      if (!decl.length) continue;
      out[prop] = { selector: r.selectorList?.text ?? "" };
      break;
    }
  }
  return out;
}

/**
 * Which `.svelte-<hash>` belongs to which source file, learned from the live
 * DOM: a `data-role` is declared in exactly one component, and the element
 * carrying it wears that component's scope hash. Accumulates across frames.
 */
async function learnHashes(page, index, into) {
  const seen = await page
    .$$eval("[data-role]", (els) =>
      els.map((el) => ({
        role: el.getAttribute("data-role") ?? "",
        hashes: [...el.classList].filter((c) => c.startsWith("svelte-")),
      })),
    )
    .catch(() => []);
  const votes = new Map(); // hash -> Map(path -> count)
  for (const { role, hashes } of seen) {
    if (!role || !hashes.length) continue;
    const owners = index.filter((f) => f.lines.some((l) => l.includes(`data-role="${role}"`)));
    if (owners.length !== 1) continue; // ambiguous role: no vote
    for (const h of hashes) {
      const key = h.slice("svelte-".length);
      if (!votes.has(key)) votes.set(key, new Map());
      const m = votes.get(key);
      m.set(owners[0].path, (m.get(owners[0].path) ?? 0) + 1);
    }
  }
  for (const [hash, m] of votes) {
    if (into.has(hash)) continue;
    const [best] = [...m.entries()].sort((a, b) => b[1] - a[1]);
    if (best) into.set(hash, best[0]);
  }
}

// ── state driving ─────────────────────────────────────────────────────────
async function runSteps(page, steps = [], defaultTimeout = 15000) {
  for (const s of steps) {
    try {
      if (s.waitFor) await page.waitForSelector(s.waitFor, { timeout: s.timeout ?? defaultTimeout });
      if (s.waitGone)
        await page.waitForFunction((q) => !document.querySelector(q), { timeout: 15000 }, s.waitGone);
      if (s.click) await page.$eval(s.click, (el) => el.click());
      if (s.hover) await (await page.$(s.hover))?.hover();
      if (s.focus) await page.$eval(s.focus, (el) => el.focus());
      if (s.type) {
        await page.$eval(
          s.type,
          (el, v) => {
            el.value = v;
            el.dispatchEvent(new Event("input", { bubbles: true }));
            el.dispatchEvent(new Event("change", { bubbles: true }));
          },
          s.value ?? "",
        );
      }
      if (s.select) {
        await page.$eval(
          s.select,
          (el, v) => {
            el.value = v;
            el.dispatchEvent(new Event("change", { bubbles: true }));
          },
          s.value,
        );
      }
      if (s.scroll)
        await page.$eval(
          s.scroll,
          (el, y) => {
            el.scrollTop = y < 0 ? el.scrollHeight : y;
          },
          s.to ?? 0,
        );
      if (s.code) {
        // CodeMirror has no `value` to set, and TYPING a program in is a trap:
        // auto-close turns every `{` into `{}` and the source arrives mangled
        // (which then compiles to an error state and the Controls section this
        // frame exists to measure never mounts). Deliver a real paste instead —
        // CodeMirror's own paste handler replaces the selection verbatim.
        await page.click(`${s.code} .cm-content`);
        await page.keyboard.down("Control");
        await page.keyboard.press("KeyA");
        await page.keyboard.up("Control");
        await page.keyboard.press("Backspace");
        await page.$eval(
          `${s.code} .cm-content`,
          (el, t) => {
            const dt = new DataTransfer();
            dt.setData("text/plain", t);
            el.dispatchEvent(
              new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }),
            );
          },
          s.value ?? "",
        );
        await sleep(500);
      }
      if (s.keys) for (const k of s.keys) await page.keyboard.press(k);
      if (s.js) await page.evaluate((src) => eval(src), s.js); // eslint-disable-line no-eval
      if (s.sleep) await sleep(s.sleep);
    } catch (err) {
      console.warn(`    step failed (${JSON.stringify(s).slice(0, 90)}): ${err.message}`);
      if (s.required) throw err;
    }
  }
}

// ── mirrors ───────────────────────────────────────────────────────────────
const MIRROR_ARGS = {
  panel: ["--board", "panel", "--pixels", "4096", "--name", "luxel-f6b0a8"],
  strip: ["--board", "strip", "--pixels", "300", "--name", "luxel-4ae0d4"],
  outputs: ["--board", "strip", "--pixels", "600", "--outputs", "2", "--name", "luxel-4ae0d4"],
  lattice: ["--board", "strip", "--pixels", "512", "--name", "luxel-lattice"],
};
/** The pixel count each mirror runs, so a seed pattern compiles for it. */
const MIRROR_PIXELS = { panel: 4096, strip: 300, outputs: 600, lattice: 512 };
const MIRROR_PORT = {
  panel: E2E.mirror.mdPanel,
  strip: E2E.mirror.mdStrip,
  outputs: E2E.mirror.mdOutputs,
  lattice: E2E.mirror.mdLattice,
};

/**
 * A bare `luxel serve` has an empty library and a stopped playlist, so a
 * Patterns page has no tiles, no playing tile and no captions, and a Playlist
 * page has no rows. The map's `seeds` block says what each mirror should hold;
 * this writes it over the same HTTP API the app uses.
 *
 * Mirrors only. A real board reached with `--device` is never written to —
 * see the `deviceSafe` gate below.
 */
async function seedMirror(base, seed, pixels) {
  if (!seed) return;
  const ids = new Map();
  for (const p of seed.patterns ?? []) {
    try {
      const body = await lxpBody(p.name, p.source, pixels);
      const r = await fetch(`${base}/api/patterns`, { method: "POST", body });
      const j = await r.json();
      if (j.id) ids.set(p.name, j.id);
      else console.warn(`    seed: ${p.name}: ${j.error ?? "no id"}`);
    } catch (err) {
      console.warn(`    seed: ${p.name}: ${err.message}`);
    }
  }
  if (seed.playlist?.length) {
    const lines = [`D ${seed.defaultSec ?? 8}`, `X ${seed.crossfadeMs ?? 500}`];
    for (const it of seed.playlist) {
      const [name, sec] = String(it).split("|");
      if (ids.has(name)) lines.push(`I ${ids.get(name)} ${sec ?? -1}`);
    }
    await fetch(`${base}/api/playlist`, { method: "POST", body: lines.join("\n") }).catch(() => {});
    // Long per-item durations on purpose: the playing tile and the playing row
    // have to stay the SAME one for the length of a run, or two frames measured
    // a minute apart disagree about which row is green.
    if (seed.play !== false)
      await fetch(`${base}/api/playlist/play`, { method: "POST" }).catch(() => {});
    await sleep(600);
  } else if (seed.activate && ids.has(seed.activate)) {
    await fetch(`${base}/api/patterns/${ids.get(seed.activate)}/activate`, {
      method: "POST",
    }).catch(() => {});
    await sleep(400);
  }
}

const procs = [];
async function startMirror(kind) {
  const port = MIRROR_PORT[kind];
  const p = spawn(
    join(REPO, "target/debug/luxel"),
    ["serve", "--port", String(port), ...NO_NETIN, ...MIRROR_ARGS[kind]],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  procs.push(p);
  await new Promise((res, rej) => {
    p.stdout.on("data", (d) => String(d).includes("luxel serve:") && res());
    p.on("exit", () => rej(new Error(`${kind} mirror died`)));
    setTimeout(() => rej(new Error(`${kind} mirror start timeout`)), 30000);
  });
  return `http://127.0.0.1:${port}`;
}

// ── the non-CSS sweep (`--sweep`) ─────────────────────────────────────────
/**
 * Fidelity is not only computed style. This walks each console screen at the
 * five widths the design has to survive and reports, with numbers:
 *
 *   - horizontal scroll on the document (there must be none at any width),
 *   - text clipped without an ellipsis (`scrollWidth > clientWidth` under
 *     `overflow:hidden` and no `text-overflow`),
 *   - children escaping a clipping parent's content box,
 *   - controls under the 24px touch-target floor on a phone,
 *   - the tab order, as the roles Tab actually visits,
 *   - every `[disabled]` without a `data-reason` (the §5.7 invariant).
 */
/* eslint-disable no-undef */
function sweepInPage() {
  const vw = window.innerWidth;
  const out = { docScroll: document.documentElement.scrollWidth, vw, clipped: [], escaped: [], tiny: [] };
  const name = (el) =>
    el.getAttribute("data-role") ||
    `${el.tagName.toLowerCase()}.${(el.className || "").toString().split(" ").filter((c) => c && !c.startsWith("svelte-")).slice(0, 2).join(".")}`;
  for (const el of document.querySelectorAll("body *")) {
    const cs = getComputedStyle(el);
    if (cs.display === "none" || cs.visibility === "hidden" || !el.getClientRects().length) continue;
    const r = el.getBoundingClientRect();
    // text clipped with nothing to say it is clipped
    if (
      el.children.length === 0 &&
      (el.textContent ?? "").trim() &&
      el.scrollWidth > el.clientWidth + 1 &&
      cs.overflowX === "hidden" &&
      cs.textOverflow !== "ellipsis"
    )
      out.clipped.push({ el: name(el), text: el.textContent.trim().slice(0, 40), over: el.scrollWidth - el.clientWidth });
    // a child escaping a clipping parent
    const par = el.parentElement;
    if (par) {
      const pcs = getComputedStyle(par);
      const pr = par.getBoundingClientRect();
      if (
        (pcs.overflowX === "hidden" || pcs.overflow === "hidden") &&
        r.width > 0 &&
        (r.right > pr.right + 2 || r.left < pr.left - 2)
      )
        out.escaped.push({ el: name(el), parent: name(par), by: +Math.max(r.right - pr.right, pr.left - r.left).toFixed(1) });
    }
    // touch targets on a phone
    if (vw <= 420 && /^(BUTTON|A|INPUT|SELECT)$/.test(el.tagName) && r.height > 0 && r.height < 24)
      out.tiny.push({ el: name(el), h: +r.height.toFixed(1) });
  }
  out.disabledNoReason = [...document.querySelectorAll('[disabled], [aria-disabled="true"]')]
    .filter((el) => !el.hasAttribute("data-reason"))
    .map((el) => name(el));
  return out;
}
/* eslint-enable no-undef */

async function tabOrder(page, limit = 24) {
  await page.evaluate(() => document.body.focus());
  const seen = [];
  for (let i = 0; i < limit; i++) {
    await page.keyboard.press("Tab");
    const who = await page.evaluate(() => {
      const el = document.activeElement;
      if (!el || el === document.body) return null;
      return (
        el.getAttribute("data-role") ||
        `${el.tagName.toLowerCase()}.${(el.className || "").toString().split(" ").filter((c) => c && !c.startsWith("svelte-"))[0] ?? ""}`
      );
    });
    if (!who) break;
    seen.push(who);
  }
  return seen;
}

// ── report helpers ────────────────────────────────────────────────────────
const md = (s) => String(s).replace(/\|/g, "\\|").replace(/\n/g, " ");
const trunc = (s, n) => (String(s).length > n ? `${String(s).slice(0, n - 1)}…` : String(s));

// ── main ──────────────────────────────────────────────────────────────────
const MAP = JSON.parse(readFileSync(MAP_PATH, "utf8"));
const frameIds = ONLY
  ? String(ONLY)
      .split(",")
      .map((s) => s.trim())
  : Object.keys(MAP.frames);

mkdirSync(OUT, { recursive: true });
if (CROPS) mkdirSync(join(OUT, "mockdiff"), { recursive: true });

const srcIndex = buildSourceIndex([join(WEB, "src")]);
/** A Svelte scope hash -> the file that authored it (learned from the live DOM). */
const hashFile = new Map();
const mockIndex = buildSourceIndex([MOCKUPS]);

// `--device` points the `panel` frames at a REAL board. Most frames in the map
// drive the app into their state by writing (typing code live-pushes it,
// installing a lattice POSTs a layout, changing a data pin queues a reboot), so
// a real board is only ever given the frames the map marks `deviceSafe` — the
// ones whose whole recipe is navigation, hover, focus and opening a popover.
// Everything else is refused rather than silently pointed at the mirror.
let runIds = frameIds;
if (DEVICE) {
  const unsafe = frameIds.filter(
    (f) => MAP.frames[f] && MAP.frames[f].app?.target === "panel" && !MAP.frames[f].deviceSafe,
  );
  if (unsafe.length)
    console.log(
      `--device: skipping ${unsafe.length} frame(s) that would WRITE to the board ` +
        `(${unsafe.join(", ")}). Run those against the mirror.`,
    );
  runIds = frameIds.filter((f) => !unsafe.includes(f));
}

// mirrors this run actually needs
const needed = new Set(
  runIds
    .filter((f) => MAP.frames[f])
    .map((f) => MAP.frames[f].app?.target)
    .filter((t) => t && t !== "playground"),
);
const bases = {};
if (DEVICE) console.log(`device mode: ${DEVICE} (read-only console frames only)`);
for (const kind of needed) {
  if (DEVICE && kind === "panel") {
    bases[kind] = String(DEVICE);
    continue;
  }
  process.stdout.write(`starting ${kind} mirror… `);
  bases[kind] = await startMirror(kind);
  await seedMirror(bases[kind], MAP.seeds?.[kind], MIRROR_PIXELS[kind] ?? 300);
  console.log(bases[kind]);
}

const web = spawn("npx", ["vite", "preview", "--port", String(WEB_PORT), "--strictPort"], {
  stdio: "ignore",
  cwd: WEB,
});
procs.push(web);
process.on("exit", () => procs.forEach((p) => p.kill()));
await sleep(2000);

const browser = await puppeteer.launch({
  executablePath: CHROMIUM,
  headless: true,
  args: [
    "--no-sandbox",
    "--disable-gpu",
    "--font-render-hinting=none",
    // a classic-headless scrollbar is 15px of layout width the mock frames do
    // not have; without this every full-width element reads 15px narrow
    "--hide-scrollbars",
    "--window-size=1440,1000",
  ],
});

// One mock page for the whole run: the frames are fixed-width blocks, so the
// viewport only has to be wide enough for the widest of them.
const mock = await browser.newPage();
await mock.setViewport({ width: 1440, height: 1000, deviceScaleFactor: 1 });
await mock.goto(pathToFileURL(MOCKUPS).href, { waitUntil: "networkidle0" });
// `.frame` is the mockup PAGE's furniture, not the app's: its 1px border makes
// every full-width child 2px narrower than the same element in the app, and
// its radius/shadow have no counterpart at all. Drop them before measuring so
// a `.w1200` frame really is 1200px of content.
await mock.addStyleTag({
  content: ".frame{border:0 !important;border-radius:0 !important;box-shadow:none !important}",
});
await sleep(800); // the mock's own <script> paints its canvases
const mockClient = await mock.createCDPSession();
await mockClient.send("DOM.enable");
await mockClient.send("CSS.enable");

const results = {};
const allDeltas = [];
/** The one page kept alive across --device-bundle frames (see below). */
let devicePage = null;

for (const frameId of runIds) {
  const F = MAP.frames[frameId];
  if (!F) {
    console.warn(`! no map entry for ${frameId}`);
    continue;
  }
  console.log(`\n── ${frameId} · ${F.title}`);
  const app = F.app ?? {};
  const width = F.width ?? 1200;
  const height = F.height ?? 950;
  const allow = new Set(
    (F.allow ?? []).map((a) => (typeof a === "string" ? a : `${a.element}.${a.property ?? "*"}`)),
  );
  const allowed = (id, prop) => allow.has(`${id}.*`) || allow.has(`${id}.${prop}`) || allow.has(id);

  const entries = (F.elements ?? []).map((e) => ({
    id: e.id,
    mock: e.mock,
    appSel: e.app,
    nth: e.nth ?? 0,
    mockNth: e.mockNth ?? 0,
    hover: !!e.hover || !!e.hoverOnly,
    // the mock paints some hover states permanently (`.tile.hov .actions`), so
    // the RESTING comparison would be a false positive: compare hover only
    hoverOnly: !!e.hoverOnly,
    // an element whose size is its TEXT (the fps readout, a tab label) has a
    // bounding box that tracks copy, not CSS — comparing it is noise
    noBox: !!e.noBox,
    // a container that fills the viewport has a height nobody authored, and a
    // list's height is its item count — suppress one axis without losing the
    // other (the editor rail's 360px WIDTH is a real finding; its height is not)
    noBoxH: !!e.noBox || !!e.noBoxH,
    noBoxW: !!e.noBox || !!e.noBoxW,
    // what to put the pointer on — default the element, but a tile's action
    // strip is revealed by hovering the TILE, and hovering the button itself
    // would also fire the button's own `:hover`
    hoverMock: e.hoverMock ?? null,
    hoverApp: e.hoverApp ?? null,
    focus: !!e.focus,
    text: !!e.text,
    props: e.props ?? null,
    note: e.note ?? "",
  }));

  // ---- mock side --------------------------------------------------------
  // Several app STATES share one mock frame (the S2 frame draws the overflow
  // menu open, the compile-error strip and a healthy rail at once, which no
  // single app state can be in), so a map entry may name its mock frame.
  const mockId = F.mock ?? frameId;
  const scope = (sel) => (sel.startsWith("#") ? sel : `#${mockId} ${sel}`);
  const mockEntries = entries.map((e) => ({ id: e.id, sel: scope(e.mock), nth: e.mockNth }));
  await mock.$eval("#" + mockId, (el) => el.scrollIntoView({ block: "center" })).catch(() => {});
  const mockBase = await measure(mock, `#${mockId}`, mockEntries);

  const mockHover = {};
  for (const e of entries.filter((x) => x.hover)) {
    const h = await mock.$(scope(e.hoverMock ?? e.mock));
    if (!h) continue;
    await h.hover().catch(() => {});
    await sleep(260); // the mock's .actions fade is .12s — settle past it
    Object.assign(
      mockHover,
      await measure(mock, `#${mockId}`, [{ id: e.id, sel: scope(e.mock), nth: e.mockNth }]),
    );
  }
  await mock.mouse.move(0, 0);

  // ---- app side ---------------------------------------------------------
  // In --device-bundle mode every frame would otherwise cold-load the whole
  // bundle out of the board's flash, and chromium opens several parallel
  // connections per load: after a handful of frames the device's small HTTP
  // socket pool is exhausted and the next goto is refused outright (which then
  // reads as a page full of ABSENT rows rather than as the load failure it is).
  // So keep ONE page for the whole run and change screens the way a person
  // does — by fragment.
  const reusing = DEVICE_BUNDLE && devicePage !== null && (F.app?.target === "panel");
  const page = reusing ? devicePage : await browser.newPage();
  await page.setViewport({ width, height, deviceScaleFactor: 1 });
  const base = app.target && app.target !== "playground" ? bases[app.target] : null;
  if (app.target && app.target !== "playground" && !base) {
    console.warn(`  ! no base for target ${app.target}; skipping`);
    await page.close();
    continue;
  }
  const onDeviceBundle = DEVICE_BUNDLE && base === String(DEVICE);
  if (reusing) {
    // leave whatever the last frame opened, then go to this frame's screen
    await page.keyboard.press("Escape").catch(() => {});
    await page.evaluate((r) => {
      location.hash = r.startsWith("#") ? r.slice(1) : r;
    }, app.route ?? "#/");
    await sleep(1500);
  }
  const url = onDeviceBundle
    ? `${base}/${app.route ?? "#/"}`
    : `http://localhost:${WEB_PORT}/` +
      (base ? `?device=${encodeURIComponent(base)}` : "") +
      (app.route ?? "#/");
  // A console polls, so `networkidle0` never arrives when the page is served
  // BY the device; and the board's socket pool serves a cold bundle slowly
  // enough that the first paint can be 10s of seconds out. Wait on `load` and
  // give the steps room rather than timing out into a page of ABSENT rows.
  if (!reusing)
    await page
    .goto(url, {
      waitUntil: onDeviceBundle ? "load" : "networkidle0",
      timeout: onDeviceBundle ? 90000 : 45000,
    })
    .catch((e) => {
      console.warn(`  ! goto: ${e.message}`);
    });
  if (onDeviceBundle && !reusing) {
    await page.waitForSelector("#app > *", { timeout: 60000 }).catch(() => {});
    await sleep(3000);
  }
  await runSteps(page, app.steps, onDeviceBundle ? 45000 : 15000);
  if (onDeviceBundle) devicePage = page;
  await sleep(app.settle ?? 500);

  await learnHashes(page, srcIndex, hashFile);

  const appEntries = entries.map((e) => ({ id: e.id, sel: e.appSel, nth: e.nth }));
  const appBase = await measure(page, app.root ?? "body", appEntries);

  const appClient = await page.createCDPSession();
  await appClient.send("DOM.enable");
  await appClient.send("CSS.enable");

  const appHover = {};
  for (const e of entries.filter((x) => x.hover)) {
    const h = await page.$(e.hoverApp ?? e.appSel);
    if (!h) continue;
    await h.hover().catch(() => {});
    await sleep(260);
    Object.assign(appHover, await measure(page, app.root ?? "body", [{ id: e.id, sel: e.appSel, nth: e.nth }]));
  }
  await page.mouse.move(0, 0);

  const mockFocus = {};
  const appFocus = {};
  for (const e of entries.filter((x) => x.focus)) {
    await mock.$eval(scope(e.mock), (el) => el.focus?.()).catch(() => {});
    Object.assign(mockFocus, await measure(mock, `#${mockId}`, [{ id: e.id, sel: scope(e.mock), nth: e.mockNth }]));
    await page.$eval(e.appSel, (el) => el.focus?.()).catch(() => {});
    Object.assign(appFocus, await measure(page, app.root ?? "body", [{ id: e.id, sel: e.appSel, nth: e.nth }]));
  }

  // ---- diff -------------------------------------------------------------
  const deltas = [];
  const push = (d) => {
    deltas.push(d);
    allDeltas.push({ frame: frameId, ...d });
  };
  const { root: appDoc } = await appClient.send("DOM.getDocument", { depth: 1 });
  const { root: mockDoc } = await mockClient.send("DOM.getDocument", { depth: 1 });

  for (const e of entries) {
    const m = mockBase[e.id];
    const a = appBase[e.id];
    if (!m?.found) {
      push({
        element: e.id,
        family: "missing",
        property: "(mock selector)",
        mock: `NOT FOUND: ${scope(e.mock)}`,
        app: "",
        weight: 50,
        note: "the MAP is wrong, not the app",
      });
      continue;
    }
    if (!a?.found) {
      if (allowed(e.id, "*")) continue;
      push({
        element: e.id,
        family: "missing",
        property: "(element)",
        mock: `present — ${trunc(m.text || m.tag, 48)}`,
        app: `ABSENT (${e.appSel})`,
        weight: 100,
        note: e.note,
      });
      continue;
    }

    const props = e.props ?? PROPS;
    const wanted = [];
    for (const p of props) {
      if (e.hoverOnly || allowed(e.id, p)) continue;
      const d = differs(p, m.styles[p], a.styles[p], { mock: m.styles, app: a.styles });
      if (!d) continue;
      wanted.push(p);
      push({
        element: e.id,
        family: FAMILY(p),
        property: p,
        mock: m.styles[p],
        app: a.styles[p],
        weight: weightOf(p) + d.magnitude,
        delta: d.note,
        note: e.note,
      });
    }

    // bounding box: size always, position relative to the frame root
    for (const [k, label, w] of [
      ["w", "box width", 10],
      ["h", "box height", 10],
    ]) {
      if (e.hoverOnly || allowed(e.id, label)) continue;
      if (k === "w" ? e.noBoxW : e.noBoxH) continue;
      const d = Math.abs(m.box[k] - a.box[k]);
      if (d > TOL.px)
        push({
          element: e.id,
          family: "layout",
          property: label,
          mock: `${m.box[k]}px`,
          app: `${a.box[k]}px`,
          weight: w + Math.min(6, d / 12),
          delta: `${d.toFixed(1)}px`,
          note: e.note,
        });
    }

    if (e.text && !allowed(e.id, "text") && normText(m.text) !== normText(a.text)) {
      push({
        element: e.id,
        family: "text",
        property: "copy",
        mock: trunc(m.text, 70),
        app: trunc(a.text, 70),
        weight: 8,
        note: e.note,
      });
    }
    if (e.text && m.placeholder && normText(m.placeholder) !== normText(a.placeholder || a.value)) {
      // the mock fakes a placeholder with `value` on an inert input
      if (normText(m.value || m.placeholder) !== normText(a.placeholder || a.value))
        push({
          element: e.id,
          family: "text",
          property: "placeholder",
          mock: trunc(m.value || m.placeholder, 50),
          app: trunc(a.placeholder || a.value, 50),
          weight: 6,
        });
    }

    // hover / focus states
    for (const [label, mSet, aSet] of [
      [":hover", mockHover, appHover],
      [":focus", mockFocus, appFocus],
    ]) {
      const mh = mSet[e.id];
      const ah = aSet[e.id];
      if (!mh?.found || !ah?.found) continue;
      for (const p of props) {
        if (allowed(e.id, p) || allowed(e.id, `${label}${p}`)) continue;
        // only report a hover delta the resting state does not already carry
        const ctx = { mock: mh.styles, app: ah.styles };
        const restDiffers =
          !e.hoverOnly && !!differs(p, m.styles[p], a.styles[p], { mock: m.styles, app: a.styles });
        if (!e.hoverOnly) {
          const mChanged = String(mh.styles[p]) !== String(m.styles[p]);
          if (!mChanged) continue; // the mock does not style hover here: nothing to compare
          if (restDiffers) continue; // already reported as a resting delta
        }
        const d = differs(p, mh.styles[p], ah.styles[p], ctx);
        if (!d) continue;
        if (!wanted.includes(p)) wanted.push(p);
        push({
          element: e.id,
          family: FAMILY(p),
          property: `${label} ${p}`,
          mock: mh.styles[p],
          app: ah.styles[p],
          weight: weightOf(p) + d.magnitude - 1,
          delta: d.note,
        });
      }
    }

    // attribution for the properties that actually differ
    if (wanted.length) {
      const appAttr = await attribute(appClient, appDoc.nodeId, e.appSel, e.nth, wanted);
      const mockAttr = await attribute(mockClient, mockDoc.nodeId, scope(e.mock), e.mockNth, wanted);
      for (const d of deltas) {
        if (d.element !== e.id) continue;
        // hover/focus deltas name the property as ":hover color"
        const p = d.property.replace(/^:\w+\s+/, "");
        const aa = appAttr[p];
        const ma = mockAttr[p];
        if (aa)
          d.appSource = aa.inline
            ? "(inline style)"
            : (findInSource(srcIndex, aa.selector, p, hashFile) ??
              `rule \`${stripScope(aa.selector)}\``);
        else if (PROPS.includes(p)) d.appSource = d.appSource ?? "(ua default)";
        if (ma)
          d.mockSource = ma.inline
            ? "(inline style)"
            : (findInSource(mockIndex, ma.selector, p) ?? `rule \`${ma.selector}\``);
      }
    }
  }

  deltas.sort(
    (x, y) => FAMILY_RANK[x.family] - FAMILY_RANK[y.family] || y.weight - x.weight,
  );

  // ---- crops ------------------------------------------------------------
  if (CROPS) {
    const worst = [...new Set(deltas.slice(0, 12).map((d) => d.element))].slice(0, 6);
    for (const id of worst) {
      const e = entries.find((x) => x.id === id);
      const m = mockBase[id];
      const a = appBase[id];
      if (!e || !m?.found || !a?.found) continue;
      const pad = 6;
      const shotA = await page
        .screenshot({
          encoding: "base64",
          clip: {
            x: Math.max(0, a.box.vx - pad),
            y: Math.max(0, a.box.vy - pad),
            width: Math.max(8, a.box.w + pad * 2),
            height: Math.max(8, a.box.h + pad * 2),
          },
        })
        .catch(() => null);
      const shotM = await mock
        .screenshot({
          encoding: "base64",
          clip: {
            x: Math.max(0, m.box.vx - pad),
            y: Math.max(0, m.box.vy - pad),
            width: Math.max(8, m.box.w + pad * 2),
            height: Math.max(8, m.box.h + pad * 2),
          },
        })
        .catch(() => null);
      if (!shotA || !shotM) continue;
      await composeSideBySide(browser, join(OUT, "mockdiff", `${frameId}-${id}.png`), shotM, shotA, `${frameId} · ${id}`);
    }
  }

  const counts = deltas.reduce((acc, d) => ((acc[d.family] = (acc[d.family] ?? 0) + 1), acc), {});
  console.log(
    `  ${deltas.length} deltas  ${Object.entries(counts)
      .map(([k, v]) => `${k} ${v}`)
      .join(" · ")}`,
  );
  results[frameId] = {
    bundle: onDeviceBundle ? "device flash" : "web/dist",
    title: F.title,
    width,
    target: app.target ?? "playground",
    base: base ?? "(playground)",
    route: app.route ?? "#/",
    elements: entries.length,
    counts,
    allow: F.allow ?? [],
    deltas,
  };
  if (page !== devicePage) await page.close();
}
if (devicePage) await devicePage.close();

// ── side-by-side crop composition (no image deps: chromium does it) ───────
async function composeSideBySide(br, path, mockB64, appB64, label) {
  const pg = await br.newPage();
  await pg.setViewport({ width: 1200, height: 600, deviceScaleFactor: 1 });
  await pg.setContent(
    `<style>body{margin:0;background:#0e1013;color:#d7dae0;font:12px/1.4 system-ui}
     .h{padding:6px 10px;font-weight:600}
     .r{display:flex;align-items:flex-start;gap:14px;padding:0 10px 12px}
     .c{display:flex;flex-direction:column;gap:4px}
     .c b{font:11px/1 ui-monospace,monospace;color:#8a90a0;text-transform:uppercase;letter-spacing:.08em}
     img{display:block;border:1px solid #2b303a;image-rendering:pixelated}</style>
     <div class="h">${label}</div>
     <div class="r">
       <div class="c"><b>mock</b><img src="data:image/png;base64,${mockB64}"></div>
       <div class="c"><b>app</b><img src="data:image/png;base64,${appB64}"></div>
     </div>`,
  );
  await sleep(120);
  const el = await pg.$("body");
  await el.screenshot({ path }).catch(() => {});
  await pg.close();
}

// ── the sweep run ─────────────────────────────────────────────────────────
const sweep = [];
if (has("--sweep")) {
  const target = DEVICE ? "panel" : (MAP.sweep?.target ?? "panel");
  const base = DEVICE ? String(DEVICE) : bases[target] ?? (bases[target] = await startMirror(target));
  if (!DEVICE && !MAP.seeds?.[target]?.seeded)
    await seedMirror(base, MAP.seeds?.[target], MIRROR_PIXELS[target] ?? 300);
  console.log(`\n── non-CSS sweep (${base})`);
  for (const screen of MAP.sweep?.screens ?? []) {
    for (const width of MAP.sweep?.widths ?? [1400, 1200, 1000, 760, 390]) {
      const pg = await browser.newPage();
      await pg.setViewport({ width, height: 900 });
      await pg
        .goto(`http://localhost:${WEB_PORT}/?device=${encodeURIComponent(base)}${screen.route}`, {
          waitUntil: "networkidle0",
          timeout: 45000,
        })
        .catch(() => {});
      await runSteps(pg, screen.steps);
      await sleep(1200);
      const r = await pg.evaluate(sweepInPage);
      const order = await tabOrder(pg);
      sweep.push({ screen: screen.name, route: screen.route, width, ...r, tabOrder: order });
      const bad =
        (r.docScroll > width + 1 ? 1 : 0) + r.clipped.length + r.escaped.length + r.tiny.length + r.disabledNoReason.length;
      console.log(`  ${screen.name} @ ${width}: ${bad === 0 ? "clean" : `${bad} finding(s)`}`);
      await pg.close();
    }
  }
}

// ── write the report ──────────────────────────────────────────────────────
const stamp = new Date().toISOString().slice(0, 19).replace("T", " ");
const total = allDeltas.length;
const lines = [];
lines.push(`# mockdiff report — web UI v2 vs the approved mockups`);
lines.push("");
lines.push(
  `Generated ${stamp} by \`web/tools/mockdiff.mjs\` (map \`${short(MAP_PATH)}\`). ` +
    `Spec: \`${short(MOCKUPS)}\`.`,
);
lines.push("");
lines.push(
  `**Tolerance.** Lengths match within **1px**; colours within **4/255 per channel** and ` +
    `**0.02 alpha**; unitless numbers within **0.02**. Values that match are omitted entirely. ` +
    `\`cursor\` is not compared (the mock's buttons are inert \`<div>\`s). Deviations Jeremy asked ` +
    `for that the mocks do not show are in each frame's \`allow\` list and never counted.`,
);
lines.push("");
lines.push(`**${total} deltas** over ${Object.keys(results).length} frames.`);
lines.push("");
lines.push(`| frame | screen | width | target | deltas | missing | layout | colour | type | copy |`);
lines.push(`|---|---|---:|---|---:|---:|---:|---:|---:|---:|`);
for (const [id, r] of Object.entries(results)) {
  const c = r.counts;
  lines.push(
    `| \`${id}\` | ${md(r.title)} | ${r.width} | ${r.target} | **${r.deltas.length}** | ` +
      `${c.missing ?? 0} | ${c.layout ?? 0} | ${c.colour ?? 0} | ${c.type ?? 0} | ${c.text ?? 0} |`,
  );
}
lines.push("");
lines.push(`## Top 40 deltas across every frame`);
lines.push("");
lines.push(`| # | frame | element | property | mock | app | app source |`);
lines.push(`|---:|---|---|---|---|---|---|`);
allDeltas
  .slice()
  .sort((x, y) => FAMILY_RANK[x.family] - FAMILY_RANK[y.family] || y.weight - x.weight)
  .slice(0, 40)
  .forEach((d, i) => {
    lines.push(
      `| ${i + 1} | \`${d.frame}\` | \`${md(d.element)}\` | ${md(d.property)} | ` +
        `\`${md(trunc(d.mock, 44))}\` | \`${md(trunc(d.app, 44))}\` | ${md(d.appSource ?? "—")} |`,
    );
  });
lines.push("");

for (const [id, r] of Object.entries(results)) {
  lines.push(`## ${id} — ${r.title}`);
  lines.push("");
  lines.push(
    `${r.width}px · target \`${r.target}\` · bundle \`${r.bundle}\` · route \`${r.route}\` · ${r.elements} mapped elements · ` +
      `**${r.deltas.length} deltas**`,
  );
  lines.push("");
  lines.push("```sh");
  lines.push(`E2E_PORT=${process.env.E2E_PORT ?? 4179} node tools/mockdiff.mjs --frames ${id}`);
  lines.push("```");
  lines.push("");
  if (r.allow.length) {
    lines.push(
      `Allowed (intentional, not counted): ${r.allow
        .map((a) => `\`${typeof a === "string" ? a : `${a.element}.${a.property ?? "*"}`}\`${typeof a === "object" && a.why ? ` — ${a.why}` : ""}`)
        .join("; ")}`,
    );
    lines.push("");
  }
  if (!r.deltas.length) {
    lines.push("_No differences outside tolerance._");
    lines.push("");
    continue;
  }
  lines.push(`| # | element | property | mock | app | app source |`);
  lines.push(`|---:|---|---|---|---|---|`);
  r.deltas.forEach((d, i) => {
    lines.push(
      `| ${i + 1} | \`${md(d.element)}\` | ${md(d.property)}${d.delta ? ` (${d.delta})` : ""} | ` +
        `\`${md(trunc(d.mock, 46))}\` | \`${md(trunc(d.app, 46))}\` | ${md(d.appSource ?? "—")} |`,
    );
  });
  lines.push("");
}

if (sweep.length) {
  lines.push(`## Non-CSS findings — responsive, overflow, touch targets, tab order`);
  lines.push("");
  lines.push(
    `Each console screen walked at ${(MAP.sweep?.widths ?? []).join(" · ")} px. ` +
      `A row is a finding; a screen/width with none is omitted.`,
  );
  lines.push("");
  lines.push(`| screen | width | horizontal scroll | clipped text | escapes its parent | <24px targets | disabled w/o data-reason |`);
  lines.push(`|---|---:|---|---|---|---|---|`);
  for (const s of sweep) {
    const scroll = s.docScroll > s.width + 1 ? `**YES (+${s.docScroll - s.width}px)**` : "no";
    const bad = scroll !== "no" || s.clipped.length || s.escaped.length || s.tiny.length || s.disabledNoReason.length;
    if (!bad) continue;
    lines.push(
      `| ${md(s.screen)} | ${s.width} | ${scroll} | ` +
        `${s.clipped.map((c) => `\`${md(c.el)}\` +${c.over}px ("${md(trunc(c.text, 24))}")`).join("<br>") || "—"} | ` +
        `${s.escaped.slice(0, 4).map((c) => `\`${md(c.el)}\` out of \`${md(c.parent)}\` by ${c.by}px`).join("<br>") || "—"} | ` +
        `${s.tiny.slice(0, 5).map((c) => `\`${md(c.el)}\` ${c.h}px`).join("<br>") || "—"} | ` +
        `${s.disabledNoReason.map((c) => `\`${md(c)}\``).join("<br>") || "—"} |`,
    );
  }
  lines.push("");
  lines.push(`### Tab order (the roles Tab visits, in order)`);
  lines.push("");
  for (const s of sweep.filter((x) => x.width === 1200))
    lines.push(`- **${md(s.screen)}** — ${s.tabOrder.map((r) => `\`${md(r)}\``).join(" → ") || "_nothing focusable_"}`);
  lines.push("");
}

writeFileSync(join(OUT, "mockdiff-report.md"), lines.join("\n"));
writeFileSync(
  join(OUT, "mockdiff.json"),
  JSON.stringify({ generated: stamp, map: short(MAP_PATH), tolerance: TOL, frames: results }, null, 2),
);
console.log(`\n${total} deltas → ${join(OUT, "mockdiff-report.md")}`);

await browser.close();
procs.forEach((p) => p.kill());
process.exit(0);
