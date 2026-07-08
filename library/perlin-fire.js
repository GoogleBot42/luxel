// name: Perlin fire
// Clean-room reimplementation from a prose functional description of the
// community pattern "Perlin fire"; original source never consulted.

// A noise-driven fire on a 2D matrix: a hot column centered horizontally,
// brightest at the base and thinning to black away from it, flames rising and
// slowly morphing. A mode slider chooses among four smooth-noise flavors
// (billowing perlin, ridged tendrils, layered fbm, rolling turbulence). Motion
// is a steady upward scroll plus a slower shape morph; both loop seamlessly by
// scrolling exactly one noise repeat-period. Stateless beyond the ambient clock.

const WRAP = 256          // noise lattice repeat period (seamless looping)

var mode = 3              // 1..4 noise flavor
var fireScale = 3         // zoom of flame detail
var riseSpeed = 0.5       // 0..1 slider (inverted, squared inside)
var morphSpeed = 0.5      // 0..1 slider (inverted, squared inside)

var scrollOff = 0         // vertical scroll offset (noise units)
var morphOff = 0          // shape-morph offset (noise units)

export function sliderMode(v) { //# min=0 max=1 step=0.01 default=0.66
  mode = 1 + floor(clamp(v, 0, 0.999) * 4)   // 1..4
}
export function showNumberMode() { return mode }

export function sliderScale(v) { //# min=0 max=1 step=0.01 default=0.25
  fireScale = 1 + v * 9        // ~life-size to ~10x zoom
}

// Inverted (right = faster), squared response, ~25x range.
export function sliderRisingSpeed(v) { //# min=0 max=1 step=0.01 default=0.5
  var s = v * v
  riseSpeed = 0.4 + s * 9.6
}
export function sliderMorphSpeed(v) { //# min=0 max=1 step=0.01 default=0.5
  var s = v * v
  morphSpeed = 0.4 + s * 9.6
}

export function beforeRender(delta) {
  setPerlinWrap(WRAP, WRAP, WRAP)
  setPalette([
    0.0,  0, 0, 0,     // black embers
    0.2,  1, 0, 0,     // deep red
    0.55, 1, 0.4, 0,   // orange
    0.8,  1, 1, 0,     // yellow
    1.0,  1, 1, 1      // white-hot
  ])
  // Sawtooths over exactly the repeat-period so the loop is seamless. Rise
  // completes its full period in ~1-2 min / riseSpeed; morph is several times
  // slower.
  scrollOff = time(120 / 65.536 / riseSpeed) * WRAP
  morphOff  = time(600 / 65.536 / morphSpeed) * WRAP
}

function sampleNoise(x, y, z) {
  if (mode <= 1) return perlin(x, y, z, 0) * 0.5 + 0.5             // billowing
  if (mode == 2) return saturate((perlinRidge(x, y, z, 0, 4, 0.5, 2) - 1.0) * 4) // tendrils
  if (mode == 3) return perlinFbm(x, y, z, 0, 4, 0.5, 2) * 0.5 + 0.5             // layered fbm
  return saturate(perlinTurbulence(x, y, z, 0, 4, 0.5, 2) * 1.16)  // rolling fireball
}

export function render2D(index, x, y) {
  // Centered, zoomed sample coordinates; third dim (morph) is time.
  var nx = (x - 0.5) * fireScale
  var ny = y * fireScale + scrollOff
  var n = sampleNoise(nx, ny, morphOff)

  // Horizontal envelope: peaks at center, falls linearly to ~zero past edges.
  var env = 1 - abs(x - 0.5) * 1.8
  if (env < 0) env = 0

  // Vertical ramp: intensity grows from nothing at the top to full at the base.
  var vramp = y

  var val = n * env * vramp
  if (val > 0.999) val = 0.999   // clamp so the palette does not wrap

  paint(val, val)
}
