// name: Coronal Mass Ejection
// Clean-room reimplementation from a prose functional description of the
// community pattern "Coronal Mass Ejection"; original source never
// consulted. A white-hot star at the panel center throws ragged plasma
// flares outward -- discrete writhing tongues and detached hot bits, not a
// smooth glow. Everything is polar: turbulent Perlin noise sampled in
// (angle, radius) space, streamed radially outward, with a permanently
// bright noisy core. The base hue drifts around the whole wheel over
// ~12 s. No particle system -- one max() expression per pixel does it all.

var CORE = 0.1                // core radius ~ a tenth of the half-width
var COFF = 0.025              // core offset ~ a quarter of the core size
var RSCALE = 4                // radial detail of the noise field
var SCROLL = 40               // large factor scaling the slow scroll phases

var huePhase = 0
var scroll1 = 0
var scroll2 = 0
var wrapped = 0

export function beforeRender(delta) {
  if (!wrapped) {
    // angle axis wraps at 1 turn -> seamless around the circle; the other
    // two axes wrap far away so they effectively never repeat
    setPerlinWrap(1, 4096, 4096)
    wrapped = 1
  }
  huePhase = time(0.183)                 // ~12 s per full hue revolution
  scroll1 = time(9.2) * SCROLL           // ~10 min period, radial stream
  scroll2 = time(10.3) * SCROLL          // ~11 min period, shape evolution
}

export function render2D(index, x, y) {
  var cx = x - 0.5
  var cy = y - 0.5

  // polar: first coord = angle (turns), second = radius
  var angle = atan2(cy, cx) / PI2 + 0.5
  var radius = hypot(cx, cy)

  // multi-octave turbulence, inverted; radius minus a scroll phase makes
  // the field stream outward, the third axis evolves the shapes
  var n = 1 - perlinTurbulence(angle, radius * RSCALE - scroll1, scroll2, 0)

  // flares: keep only the strongest ridges (top ~third) with soft edges
  var flare = smoothstep(0.66, 1, n)

  // noisy core: near the center this saturates full; strong flares
  // overlapping the core extend the bright zone outward (eruptions)
  var core = clamp(1 - (radius * n - COFF) / CORE, 0, 1)

  var bri = max(flare, core)
  bri = bri * bri * bri                  // deepen contrast

  var hue = huePhase - bri * 0.08        // hot regions tint slightly behind
  var sat = clamp(radius * 3 - bri, 0, 1) // core white; edges saturated
  hsv(hue, sat, bri)
}
