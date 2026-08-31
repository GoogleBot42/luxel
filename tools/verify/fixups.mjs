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
//   controls            UI control values invoked on a side once after init,
//                       the way a user turning the dial would. Declared PER
//                       SIDE, because a port routinely names (or splits) its
//                       controls differently from the original. This is the
//                       declarative form of --controls-orig/--controls-port,
//                       and exists so a pair whose sides expose the SAME input
//                       through different surfaces can still be swept
//                       like-for-like (Gitea #177 item 3).
//
//   pins                Digital input pins DRIVEN into a side once after init
//                       (the lx_set_pin injection ABI — Gitea #177 item 2).
//                       Also per side, for the same reason: a corpus original
//                       reads a physical button with digitalRead(26) while its
//                       clean-room port offers a UI toggle instead. Pinning
//                       `{"orig": {"26": 0}}` alongside
//                       `{"port": {"toggleButton": 1}}` in `controls` presses
//                       BOTH sides' buttons, which is the only way such a pair
//                       is comparable at all: without it the original idles at
//                       "not pressed" forever and the two sides are showing
//                       different states, not different renderings.
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
//                 "controls": { "orig": {"<name>": <number>|[<n>,<n>,<n>], ...},
//                               "port": { ... } },
//                 "pins": { "orig": {"<pin 0..63>": 0|1|true|false|null, ...},
//                           "port": { ... } },
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
  const raw = perSide(slug, "vars", fixupFor(slug)?.vars);
  if (!raw) return null;
  const out = {};
  for (const side of SIDES) {
    out[side] = {};
    for (const [name, v] of Object.entries(raw[side])) {
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

/** Shared shape check for the per-side blocks (`vars`, `controls`, `pins`):
 *  an object keyed by `orig`/`port`, each holding a name→value map. Returns
 *  `{orig, port}` of raw maps (both always present) or null when the key is
 *  absent. Throws on anything malformed — see `varsOverride`. */
function perSide(slug, key, spec) {
  if (spec == null) return null;
  if (typeof spec !== "object" || Array.isArray(spec)) {
    throw new Error(`fixups.json: ${slug}.${key} must be an object of {orig,port}`);
  }
  for (const side of Object.keys(spec)) {
    if (!SIDES.includes(side)) {
      throw new Error(`fixups.json: ${slug}.${key} has unknown side "${side}" (want orig/port)`);
    }
  }
  const out = {};
  for (const side of SIDES) {
    const vals = spec[side] ?? {};
    if (typeof vals !== "object" || Array.isArray(vals)) {
      throw new Error(`fixups.json: ${slug}.${key}.${side} must be an object`);
    }
    out[side] = vals;
  }
  return out;
}

/** This slug's per-side UI control pins, or null: `{orig, port}` of
 *  `name → [v0, v1?, v2?]` (the shape `Engine.setControl` wants, and the same
 *  shape `--controls-orig/--controls-port` parse to). A bare number is
 *  accepted and wrapped, since almost every control is a one-value slider.
 *
 *  Values are engine-ABI 16.16 like everything else, and a control taking more
 *  than three components does not exist — both are rejected loudly, because a
 *  control pin that quietly fails to apply looks exactly like a dead dial. */
export function controlsOverride(slug) {
  const raw = perSide(slug, "controls", fixupFor(slug)?.controls);
  if (!raw) return null;
  const out = {};
  for (const side of SIDES) {
    out[side] = {};
    for (const [name, v] of Object.entries(raw[side])) {
      const vals = Array.isArray(v) ? v : [v];
      if (vals.length === 0 || vals.length > 3) {
        throw new Error(
          `fixups.json: ${slug}.controls.${side}.${name} must be 1..3 numbers (got ${vals.length})`,
        );
      }
      for (const n of vals) {
        if (typeof n !== "number" || !Number.isFinite(n)) {
          throw new Error(`fixups.json: ${slug}.controls.${side}.${name} must be numbers`);
        }
        if (Math.abs(n) >= 32768) {
          throw new Error(
            `fixups.json: ${slug}.controls.${side}.${name} = ${n} is outside the 16.16 range (±32768)`,
          );
        }
      }
      out[side][name] = vals;
    }
  }
  return Object.keys(out.orig).length || Object.keys(out.port).length ? out : null;
}

/** Highest pin the engine tracks — mirrors `MAX_TRACKED_PIN` in
 *  crates/luxel-core/src/vm.rs. Anything above it has nowhere to store a
 *  level, so `setPin` would reject it at render time; catching it here names
 *  the manifest entry instead. */
export const MAX_PIN = 63;

/** This slug's per-side digital pin pins, or null: `{orig, port}` of
 *  `pin → true|false|null` (null = explicitly released to the pin's `pinMode`
 *  idle level). JSON object keys are strings, so the pin number is parsed and
 *  range-checked here; `0`/`1` are accepted alongside `false`/`true` because
 *  patterns spell pin levels as LOW/HIGH numerics. */
export function pinsOverride(slug) {
  const raw = perSide(slug, "pins", fixupFor(slug)?.pins);
  if (!raw) return null;
  const out = {};
  for (const side of SIDES) {
    out[side] = {};
    for (const [key, v] of Object.entries(raw[side])) {
      const pin = Number(key);
      if (!Number.isInteger(pin) || pin < 0 || pin > MAX_PIN) {
        throw new Error(
          `fixups.json: ${slug}.pins.${side} key "${key}" must be an integer pin 0..${MAX_PIN}`,
        );
      }
      let level;
      if (v === null) level = null;
      else if (typeof v === "boolean") level = v;
      else if (v === 0 || v === 1) level = v === 1;
      else {
        throw new Error(
          `fixups.json: ${slug}.pins.${side}.${key} must be 0, 1, true, false or null`,
        );
      }
      out[side][pin] = level;
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
