// Per-slug fixups for the port-verification harness: the small, declared set
// of adjustments a pair needs before its two sides can be compared at all.
//
// Two kinds, both driven by tools/verify/fixups.json:
//
//   stripLinesMatching  Applied to the ORIGINAL side only, at load time. Every
//                       source line containing one of the substrings is
//                       dropped. This exists for author-planted tripwire lines
//                       (deliberately invalid identifiers the pattern's README
//                       tells the user to delete) — without stripping them the
//                       original never compiles and the pair is unjudgeable.
//                       It is NOT a place to patch patterns into working: the
//                       only sanctioned use is removing lines the author
//                       intended the user to remove.
//
//   rig / pixels / grid Rig overrides applied to BOTH sides, so the comparison
//                       stays like-for-like. These exist for originals that
//                       only render on a specific geometry (a fixed-size
//                       canvas, a sprite LUT, a pixel count that must divide
//                       by N) and error out on the sweep's default rig.
//
//   vars                Exported-var values pushed into a side once after init,
//                       the way an external client writes them over PB's vars
//                       API. Declared PER SIDE, because the two sides may name
//                       the same variable differently. This exists for
//                       originals that are DRIVEN by a companion app (a
//                       mapper, a home-automation bridge) and render nothing
//                       at their default value — without a pinned var the
//                       original is black and the pair is unjudgeable.
//
//   nonVisual           A one-string REASON marking the pair as excluded from
//                       the output-verification sweep: the ORIGINAL is not a
//                       visual pattern at all, so "the port should render the
//                       same pixels" is not a fidelity target and no score is
//                       meaningful. Unlike the keys above this changes nothing
//                       about a render — snap.mjs still renders both sides on
//                       demand, it just says loudly that the pair is not
//                       scoreable, and report.mjs files it under its own
//                       `non-visual` heading instead of a verdict bucket.
//                       (Gitea #123. Note the annotation lives HERE and not in
//                       pairs.json: gen-pairs.mjs regenerates pairs.json from
//                       library/ + corpus/ and would drop it.)
//
// Every entry carries a `note` explaining why, usually citing SWEEP-NOTES.md
// or the slug's verdict in results/.
//
// Consumers: snap.mjs (judge harness), report.mjs (visual report), review.mjs
// (interactive review UI). Fixups are part of a run's identity — snap.mjs
// records them in meta.json's `provenance`.
//
// Schema (tools/verify/fixups.json):
//   { "<slug>": { "stripLinesMatching": ["<substring>", ...],
//                 "rig": "strip"|"grid"|"cloud",
//                 "pixels": <n>, "grid": [w, h],
//                 "vars": { "orig": {"<name>": <number>, ...},
//                           "port": {"<name>": <number>, ...} },
//                 "nonVisual": "<reason the pair is not scoreable>",
//                 "note": "why" } }
//
// SPDX-License-Identifier: Apache-2.0

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const FIXUPS_PATH = path.join(HERE, "fixups.json");

let cache = null;

/** The whole manifest, keyed by slug. Read once, cached. */
export function loadFixups() {
  if (cache) return cache;
  cache = fs.existsSync(FIXUPS_PATH) ? JSON.parse(fs.readFileSync(FIXUPS_PATH, "utf8")) : {};
  return cache;
}

/** This slug's entry, or null. */
export function fixupFor(slug) {
  return loadFixups()[slug] ?? null;
}

/** Strip the slug's tripwire lines out of an ORIGINAL's source.
 *
 *  Returns `{ source, applied }` — `applied` is null when the slug has no
 *  line fixups or none of them matched, otherwise
 *  `{ stripLinesMatching: [<substrings that actually matched>], removed: n }`,
 *  which is what callers stamp into their provenance. */
export function applySourceFixups(slug, source) {
  const fx = fixupFor(slug);
  const needles = fx?.stripLinesMatching;
  if (!Array.isArray(needles) || needles.length === 0 || typeof source !== "string")
    return { source, applied: null };

  const matched = new Set();
  const kept = [];
  for (const line of source.split("\n")) {
    const hit = needles.find((n) => line.includes(n));
    if (hit === undefined) kept.push(line);
    else matched.add(hit);
  }
  const removed = source.split("\n").length - kept.length;
  if (removed === 0) return { source, applied: null };
  return {
    source: kept.join("\n"),
    applied: { stripLinesMatching: [...matched], removed },
  };
}

/** Why this pair is excluded from the sweep, or null.
 *
 *  A non-empty string means the ORIGINAL is not a visual pattern, so pixel
 *  fidelity is not a target and no verdict/score applies (Gitea #123).
 *  Anything other than a non-empty string throws: a silently-ignored
 *  exclusion marker would put the pair back in the scored population without
 *  anyone noticing. */
export function nonVisualReason(slug) {
  const reason = fixupFor(slug)?.nonVisual;
  if (reason == null) return null;
  if (typeof reason !== "string" || reason.trim() === "") {
    throw new Error(`fixups.json: ${slug}.nonVisual must be a non-empty reason string`);
  }
  return reason;
}

/** This slug's rig override, or null: `{ rig?, pixels?, grid? }`. */
export function rigOverride(slug) {
  const fx = fixupFor(slug);
  if (!fx) return null;
  const out = {};
  if (typeof fx.rig === "string") out.rig = fx.rig;
  if (Number.isFinite(fx.pixels)) out.pixels = fx.pixels;
  if (Array.isArray(fx.grid) && fx.grid.length === 2) out.grid = [fx.grid[0], fx.grid[1]];
  return Object.keys(out).length ? out : null;
}

/** This slug's per-side exported-var pins, or null: `{ orig: {}, port: {} }`.
 *
 *  Both sides are always present (empty when unpinned) so callers can spread
 *  them without null checks. The manifest is tracked, hand-edited and small —
 *  a malformed entry throws rather than silently pinning nothing, because a
 *  var that quietly fails to apply looks exactly like a black pattern. */
const SIDES = ["orig", "port"];
export function varsOverride(slug) {
  const fx = fixupFor(slug);
  const spec = fx?.vars;
  if (spec == null) return null;
  if (typeof spec !== "object" || Array.isArray(spec)) {
    throw new Error(`fixups.json: ${slug}.vars must be an object of {orig,port}`);
  }
  for (const key of Object.keys(spec)) {
    if (!SIDES.includes(key)) {
      throw new Error(`fixups.json: ${slug}.vars has unknown side "${key}" (want orig/port)`);
    }
  }
  const out = {};
  for (const side of SIDES) {
    const vals = spec[side] ?? {};
    if (typeof vals !== "object" || Array.isArray(vals)) {
      throw new Error(`fixups.json: ${slug}.vars.${side} must be an object of name=number`);
    }
    out[side] = {};
    for (const [name, v] of Object.entries(vals)) {
      if (typeof v !== "number" || !Number.isFinite(v)) {
        throw new Error(`fixups.json: ${slug}.vars.${side}.${name} must be a finite number`);
      }
      // Vars cross the engine ABI as 16.16; anything wider would wrap into an
      // unrelated value instead of failing.
      if (Math.abs(v) >= 32768) {
        throw new Error(
          `fixups.json: ${slug}.vars.${side}.${name} = ${v} is outside the 16.16 range (±32768)`,
        );
      }
      out[side][name] = v;
    }
  }
  return Object.keys(out.orig).length || Object.keys(out.port).length ? out : null;
}

/** The sweep's default rig geometry for a pair, with any fixup override
 *  applied — the single definition every consumer shares.
 *
 *  Returns a plain descriptor (no coordinate arrays, so it serializes):
 *    { kind: "strip", pixels }
 *    { kind: "grid",  pixels, gridW, gridH }
 *    { kind: "cloud", pixels, cloudSide }   // pixels === cloudSide ** 3
 *  `overridden` is true when fixups.json moved it off the default. */
export function resolveRig(pair, { gridSide = 16, cloudSide = 5, stripPixels = 60 } = {}) {
  const ov = rigOverride(pair.slug) ?? {};
  const kind = ov.rig ?? pair.rig;
  const overridden = Object.keys(ov).length > 0;
  if (kind === "grid") {
    const gw = ov.grid?.[0] ?? gridSide;
    const gh = ov.grid?.[1] ?? (ov.pixels ? Math.max(1, Math.round(ov.pixels / gw)) : gridSide);
    return { kind, gridW: gw, gridH: gh, pixels: gw * gh, overridden };
  }
  if (kind === "cloud") {
    const side = ov.pixels ? Math.max(1, Math.round(Math.cbrt(ov.pixels))) : cloudSide;
    return { kind, cloudSide: side, pixels: side ** 3, overridden };
  }
  return { kind: "strip", pixels: ov.pixels ?? stripPixels, overridden };
}
