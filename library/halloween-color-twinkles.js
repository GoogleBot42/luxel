// name: Halloween color twinkles
// Clean-room reimplementation from a prose functional description of the
// community pattern "Halloween color twinkles"; original source never
// consulted.

// Deterministic confetti in a strict two-color Halloween palette: deep
// purples and pumpkin oranges twinkling on black. Nested incommensurate
// sinusoids of the raw pixel index stand in for randomness, so adjacent
// pixels differ strongly. A fourth-power gamma plus a hard low-end gate
// turns the smooth wave field into discrete twinkles.

// hue bands (as fractions of the hue circle)
const PURPLE_BASE = 0.7     // violet band start
const PURPLE_SPAN = 0.13    // ...a bit wider than the orange band
const ORANGE_BASE = 0.02    // warm orange just above red
const ORANGE_SPAN = 0.06

const GATE = 0.1            // brightness below this snaps to black

var slowPhase = 0           // ~1.5 s cycle, expressed as a full-circle angle
var fastPhase = 0           // ~3.5x faster, drives the shimmer

export function beforeRender(delta) {
  slowPhase = PI2 * time(0.023)    // ~1.5 s period
  fastPhase = PI2 * time(0.0066)   // ~0.43 s period
}

export function render(index) {
  // signed hue driver: one short-period sine nested inside another —
  // this is what scrambles neighboring pixels
  var driver = sin(index / 3 + PI2 * sin(index / 2 + slowPhase))

  // brightness: short spatial wave, offset by another sine carrying the
  // fast phase; ^4 gamma crushes mid-tones so only near-peaks show
  var v = wave(index * 0.23 + sin(index / 2.7 + fastPhase))
  v = v * v * v * v
  if (v < GATE) v = 0     // twinkle gate

  // sign of the driver picks the family; a triangle fold keeps each
  // family inside its narrow band with no hue wrap-around
  var h
  if (driver >= 0) {
    h = PURPLE_BASE + PURPLE_SPAN * triangle(driver)
  } else {
    h = ORANGE_BASE + ORANGE_SPAN * triangle(-driver)
  }

  hsv(h, 1, v)
}
