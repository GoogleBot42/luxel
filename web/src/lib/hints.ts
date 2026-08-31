// Script-specified control bounds, PB-compatible by construction: a `//#`
// directive comment attached to the control's export. Pixel Blaze ignores the
// comment (its sliders always send 0..1); on Luxel the control sends the
// actual value in [min, max].
//
// Two placements are accepted, and they mean the same thing:
//
//   trailing, on the export line —
//     export function sliderSpeed(v) { speed = v }  //# min=0 max=5 step=0.5 default=2
//     export function toggleMirror(on) { … }        //# default=1
//     export function inputNumberCount(v) { … }     //# min=1 max=100 step=1 default=10
//
//   own line, directly above the export —
//     //# min=0 max=5 step=0.5 default=2
//     export function sliderSpeed(v) { speed = v }
//
// If a control carries both, the two are merged and the own-line directive
// wins on any key they share.
//
// Keep this in sync with the plain-JS twin in tools/verify/hints.mjs, which the
// verify harness (snap.mjs) and the review UI both use — web/tests/hints.test.mjs
// runs every case against both and fails on any divergence.

export interface ControlHint {
  min?: number;
  max?: number;
  step?: number;
  default?: number;
}

const TRAILING = /export\s+function\s+([A-Za-z_$][\w$]*)\s*\([^)]*\)[^\n]*?\/\/#([^\n]*)/g;
const OWN_LINE = /^[ \t]*\/\/#([^\n]*)\n[ \t]*export\s+function\s+([A-Za-z_$][\w$]*)\s*\(/gm;

export function parseControlHints(source: string): Map<string, ControlHint> {
  const hints = new Map<string, ControlHint>();
  const put = (name: string | undefined, body: string | undefined): void => {
    if (!name || body === undefined) return;
    const hint: ControlHint = hints.get(name) ?? {};
    for (const kv of body.matchAll(/(\w+)\s*=\s*(-?\d*\.?\d+)/g)) {
      const key = kv[1];
      const value = Number(kv[2]);
      if (Number.isNaN(value)) continue;
      if (key === "min") hint.min = value;
      else if (key === "max") hint.max = value;
      else if (key === "step") hint.step = value;
      else if (key === "default") hint.default = value;
    }
    hints.set(name, hint);
  };
  // `export function sliderX(v) { … }  //# min=0 max=5`
  for (const m of source.matchAll(TRAILING)) put(m[1], m[2]);
  // `//# min=0 max=5` on its own line, then `export function sliderX(v) { … }`
  for (const m of source.matchAll(OWN_LINE)) put(m[2], m[1]);
  return hints;
}
