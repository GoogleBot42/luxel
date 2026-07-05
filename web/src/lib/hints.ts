// Script-specified control bounds, PB-compatible by construction: a `//#`
// directive comment on the control's export line. Pixel Blaze ignores the
// comment (its sliders always send 0..1); on Luxel the control sends the
// actual value in [min, max].
//
//   export function sliderSpeed(v) { speed = v }  //# min=0 max=5 step=0.5 default=2
//   export function toggleMirror(on) { … }        //# default=1
//   export function inputNumberCount(v) { … }     //# min=1 max=100 step=1 default=10

export interface ControlHint {
  min?: number;
  max?: number;
  step?: number;
  default?: number;
}

export function parseControlHints(source: string): Map<string, ControlHint> {
  const hints = new Map<string, ControlHint>();
  const line = /export\s+function\s+([A-Za-z_$][\w$]*)\s*\([^)]*\)[^\n]*?\/\/#([^\n]*)/g;
  for (const m of source.matchAll(line)) {
    const name = m[1];
    const body = m[2];
    if (!name || body === undefined) continue;
    const hint: ControlHint = {};
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
  }
  return hints;
}
