// name: DNA Helix 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// A rotating double helix: two phase-offset sine strands, cos() depth
// dimming whichever strand is "behind", and rungs on a square() mask
// between them. Pure per-pixel math — no state.
export function beforeRender(delta) {
  t1 = time(0.05) * PI2
}

export function render2D(index, x, y) {
  ph = x * PI2 + t1  // one helix turn across the panel
  y1 = 0.5 + sin(ph) * 0.3
  y2 = 0.5 - sin(ph) * 0.3
  d1 = 0.55 + 0.45 * cos(ph)  // depth cue
  d2 = 1.1 - d1
  v1 = saturate(1 - abs(y - y1) * 7)
  v1 = v1 * v1 * d1
  v2 = saturate(1 - abs(y - y2) * 7)
  v2 = v2 * v2 * d2
  // rungs: bars fixed to the helix so they ride around with it
  lo = min(y1, y2)
  hi = max(y1, y2)
  vr = 0
  if (y > lo + 0.04 && y < hi - 0.04) {
    vr = square(ph / PI2 * 6, 0.28) * 0.6 * min(d1, d2)
  }
  if (v1 > v2 && v1 > vr) hsv(0.5, 0.85, v1)        // cyan strand
  else if (v2 > vr) hsv(0.88, 0.85, v2)             // magenta strand
  else hsv(0.12, 0.35, vr)                          // pale gold rungs
}
