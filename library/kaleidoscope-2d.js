// name: Kaleidoscope 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// Clean-room reimplementation from a prose description of the community
// pattern "Perlin Kaleidoscope 2D" (no source consulted).
//
// DELIBERATELY REDESIGNED in the 2026-09-01 pattern review: fidelity to the
// original was waived ("just run with the Kaleidoscope idea — make something
// cool"), so this is now an original stained-glass kaleidoscope rather than a
// close port. A live source texture — three orbiting colour blobs read through
// two octaves of domain-warped simplex noise — is folded into N mirrored
// wedges (the abs() against the wedge bisector is what makes true mirrors
// instead of rotated copies) and then quantised into hard-edged colour cells
// separated by dark leading lines. The whole figure spins while the source
// drifts underneath it at its own rate, so the reflections shimmer, collide
// and recombine instead of sitting still.
DEG = 0.01745329  // degrees → radians
FAN = 0.95        // radians of source texture each mirrored wedge stretches over

// --- controls -------------------------------------------------------------
folds = 6
wedgeK = FAN * 6 / PI  // FAN / (half wedge) = FAN * folds / PI
//# min=2 max=12 step=1 default=6
export function sliderFolds(v) {
  folds = clamp(floor(v + 0.5), 2, 12)
  wedgeK = FAN * folds / PI
}

// whole-figure rotation, degrees per second (negative spins the other way)
spinRate = 20 * DEG
//# min=-180 max=180 step=1 default=20
export function sliderSpin(v) { spinRate = v * DEG }

// how fast the source texture under the mirrors evolves, in texture units/sec
texRate = 0.18
//# min=0 max=0.6 step=0.01 default=0.18
export function sliderTexture(v) { texRate = v }

// hue rotation of the whole palette, degrees per second
hueRate = 25 / 360
//# min=-180 max=180 step=1 default=25
export function sliderColorDrift(v) { hueRate = v / 360 }

// how many colour cells the source field is quantised into
bands = 3
//# min=2 max=5 step=1 default=3
export function sliderBands(v) { bands = clamp(floor(v + 0.5), 2, 5) }

// --- state ----------------------------------------------------------------
spin = 0
tex = 0
hueBase = 0
// six independent orbit phases so the three blobs never fall into lockstep
q0 = 0
q1 = 1.9
q2 = 3.4
q3 = 5.1
q4 = 0.7
q5 = 2.8
b0x = 0.3
b0y = 0
b1x = 0.4
b1y = 0.1
b2x = 0.5
b2y = -0.1

export function beforeRender(delta) {
  dt = min(delta, 100) * 0.001
  spin = mod(spin + spinRate * dt, PI2)
  tex = mod(tex + texRate * dt, 512)
  hueBase = mod(hueBase + hueRate * dt, 1)

  // incommensurate frequencies, each phase wrapped on its own so nothing
  // accumulates into the range where fixed point loses angular resolution
  q0 = mod(q0 + texRate * dt * 4.1, PI2)
  q1 = mod(q1 + texRate * dt * 2.7, PI2)
  q2 = mod(q2 + texRate * dt * 3.3, PI2)
  q3 = mod(q3 + texRate * dt * 5.2, PI2)
  q4 = mod(q4 + texRate * dt * 2.1, PI2)
  q5 = mod(q5 + texRate * dt * 6.4, PI2)

  // blobs orbit in polar form so they always stay inside the sampled fan
  rr = 0.16 + 0.40 * (0.5 + 0.5 * sin(q0))
  aa = 0.44 * sin(q1)
  b0x = cos(aa) * rr
  b0y = sin(aa) * rr
  rr = 0.14 + 0.44 * (0.5 + 0.5 * sin(q2))
  aa = 0.44 * sin(q3)
  b1x = cos(aa) * rr
  b1y = sin(aa) * rr
  rr = 0.20 + 0.38 * (0.5 + 0.5 * sin(q4))
  aa = 0.44 * sin(q5)
  b2x = cos(aa) * rr
  b2y = sin(aa) * rr
}

export function render2D(index, x, y) {
  dx = x - 0.5
  dy = y - 0.5
  rad = hypot(dx, dy)
  seg = PI2 / folds
  // mirror fold + spin, then stretch the wedge over a fixed fan of the source
  // so the fold count changes the symmetry order without changing the picture
  wa = abs(mod(atan2(dy, dx) + spin, seg) - seg * 0.5) * wedgeK - FAN * 0.5
  sx = cos(wa) * rad
  sy = sin(wa) * rad

  // two octaves, the second domain-warped by the first
  n1 = simplex3(sx * 1.5 + tex * 0.7, sy * 1.5, tex * 0.45, 17)
  n2 = simplex3(sx * 2.4 + n1 * 0.5, sy * 2.4 - n1 * 0.5, tex * 0.8 + 60, 23)

  // three orbiting blobs; the squared reciprocal falloff keeps them hot cores
  // with short tails instead of a bright bed over the whole wedge
  e = sx - b0x
  f = sy - b0y
  g = 1 / (1 + 30 * (e * e + f * f))
  glo = g * g
  e = sx - b1x
  f = sy - b1y
  g = 1 / (1 + 40 * (e * e + f * f))
  glo = glo + 0.85 * g * g
  e = sx - b2x
  f = sy - b2y
  g = 1 / (1 + 25 * (e * e + f * f))
  glo = glo + 0.9 * g * g

  // one normalised source field: noise + blobs, dimming outward
  fld = saturate(0.7 + n1 * 0.35 + n2 * 0.14 + glo * 0.5 - rad * 1.45)

  // quantise into colour cells; the fractional part draws the leading lines
  qv = fld * bands + hueBase * 1.5
  ci = floor(qv)
  fr = qv - ci
  edg = smoothstep(0.02, 0.16, min(fr, 1 - fr) * 2)

  hsv(
    mod(ci * 0.29 + hueBase + wa * 0.12 + glo * 0.05, 1),
    saturate(1.02 - glo * 0.3),
    saturate((0.05 + 0.85 * fld + 0.8 * glo) * (0.1 + 0.9 * edg))
  )
}
