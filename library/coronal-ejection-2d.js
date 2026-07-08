// name: Coronal Ejection 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// Clean-room reimplementation from a prose description of the community
// pattern "Coronal Mass Ejection 2D" (no source consulted). A white-hot
// core with turbulence-carved flares streaming outward. The noise wrap
// period is matched to the angular scale so the texture tiles seamlessly
// around the circle; a smooth threshold carves the field into discrete
// tongues; saturation grows with radius so the core whitens for free.
rDrift = 0
zt = 0

density = 6
export function sliderDensity(v) { density = 1 + floor(v * 11) } //# min=1 max=12 step=1 default=6
export function showNumberDensity() { return density }
mirror = 0
export function toggleMirror(v) { mirror = v }
cutoff = 0.35
export function sliderCutoff(v) { cutoff = 0.15 + v * 0.55 } //# min=0 max=1 step=0.01 default=0.35

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  hueT = time(0.2)          // full color-wheel lap, ~13 s
  rDrift += dt * 0.25       // flares stream outward
  if (rDrift > 512) rDrift -= 512
  zt += dt * 0.03
  if (zt > 256) zt -= 256
  lobes = density * (mirror ? 2 : 1)
  setPerlinWrap(lobes, 0, 0)
}

export function render2D(index, x, y) {
  dx = x - 0.5
  dy = y - 0.5
  r = hypot(dx, dy)
  a = (atan2(dy, dx) / PI2 + 0.5) * lobes  // spans the wrap period exactly
  n = 1 - perlinTurbulence(a, r * 3 - rDrift, zt, 2, 0.55, 3)
  flare = smoothstep(cutoff, 0.9, n) * saturate(1.3 - r * 1.9)
  core = saturate(1.3 - r * 8 * (0.35 + n * 0.5))  // edge nibbled by turbulence
  v = max(flare, core)
  v = v * v
  hsv(hueT - v * 0.05, saturate(r * 4.5 - v * 0.6), v)
}
