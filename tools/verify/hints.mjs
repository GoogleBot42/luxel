// Script-specified control bounds — the `//#` directive — for the harness side
// of the house: snap.mjs reads it to probe a dial across its REAL range and to
// report the parsed bounds in meta.json, and the review UI imports it (served
// as /hints.mjs, the same way sensormodel.mjs is) so the browser and the
// headless harness can never disagree about what a directive means.
//
// This is the plain-JS twin of web/src/lib/hints.ts — that copy exists because
// the playground is a typed Svelte build with no path into tools/. Keep the two
// in sync; web/tests/hints.test.mjs runs every case against BOTH and fails on
// any divergence.
//
// Pixel Blaze ignores the comment (its sliders always send 0..1); on Luxel the
// control sends the actual value in [min, max]. Two placements, same meaning:
//
//   export function sliderSpeed(v) { speed = v }  //# min=0 max=5 step=0.5 default=2
//
//   //# min=0 max=5 step=0.5 default=2
//   export function sliderSpeed(v) { speed = v }
//
// If a control carries both, they merge and the own-line directive wins on any
// key they share.
//
// SPDX-License-Identifier: Apache-2.0

const TRAILING = /export\s+function\s+([A-Za-z_$][\w$]*)\s*\([^)]*\)[^\n]*?\/\/#([^\n]*)/g;
const OWN_LINE = /^[ \t]*\/\/#([^\n]*)\n[ \t]*export\s+function\s+([A-Za-z_$][\w$]*)\s*\(/gm;
const KEYS = ["min", "max", "step", "default"];

/** Parse every `//#` directive in a pattern source.
 *  @returns {Map<string, {min?: number, max?: number, step?: number, default?: number}>}
 *           keyed by the FULL exported function name (`sliderSpeed`, not `Speed`). */
export function parseControlHints(source) {
  const hints = new Map();
  const put = (name, body) => {
    if (!name || body === undefined) return;
    const hint = hints.get(name) ?? {};
    for (const kv of body.matchAll(/(\w+)\s*=\s*(-?\d*\.?\d+)/g)) {
      const v = Number(kv[2]);
      if (Number.isNaN(v)) continue;
      if (KEYS.includes(kv[1])) hint[kv[1]] = v;
    }
    hints.set(name, hint);
  };
  // same line: `export function sliderX(v) { … }  //# min=0 max=5`
  for (const m of source.matchAll(TRAILING)) put(m[1], m[2]);
  // line above: `//# min=0 max=5` \n `export function sliderX(v) { … }`
  for (const m of source.matchAll(OWN_LINE)) put(m[2], m[1]);
  return hints;
}

/** The numeric range a parsed directive actually describes: its ends and the
 *  midpoint a caller should use to sample the middle of it.
 *
 *  A directive naming only one end still describes a range — the other end
 *  takes the raw Pixel Blaze default (min 0, max 1), which is what the
 *  playground's slider does with a one-sided directive too. The midpoint snaps
 *  to the `step` grid when a step is declared and the snapped value lands
 *  strictly inside the range, so an integer dial is sampled at an integer
 *  rather than at x.5 (`min=1 max=8 step=1` → 5, not 4.5).
 *
 *  @returns {{min: number, max: number, mid: number} | null} null when there is
 *           nothing usable: no bounds at all, or max <= min — a malformed
 *           directive whose "range" is a single point, which a caller must not
 *           mistake for a real one.
 */
export function directiveRange(bounds) {
  if (!bounds) return null;
  const hasMin = Number.isFinite(bounds.min);
  const hasMax = Number.isFinite(bounds.max);
  if (!hasMin && !hasMax) return null;
  const min = hasMin ? bounds.min : 0;
  const max = hasMax ? bounds.max : 1;
  if (!(max > min)) return null;
  let mid = (min + max) / 2;
  if (Number.isFinite(bounds.step) && bounds.step > 0) {
    const snapped = min + Math.round((mid - min) / bounds.step) * bounds.step;
    if (snapped > min && snapped < max) mid = snapped;
  }
  // Kill float dust from the midpoint/step arithmetic, so the value reads as
  // the number a user would type (0.30000000000000004 → 0.3).
  return { min, max, mid: +mid.toFixed(6) };
}
