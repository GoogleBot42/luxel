// name: cube fire 3D
// Clean-room reimplementation from a prose functional description of the
// community pattern "cube fire 3D"; original source never consulted.

// Roiling volumetric blobs of colored flame: the product of three phase-
// offset axis waves, overdriven so only coincident crests read as blobs.
// Three incommensurate time phases keep the motion from ever locking; the
// cell size slowly breathes; hue cycles globally with a gentle spatial
// gradient; the hottest cores bleach toward white.
// (The original declared sound-sensor bindings it never used — omitted here,
// as the spec allows.)

const speed = 1   // source-only constant, divides all three time periods

var t1 = 0
var t2 = 0
var t3 = 0
var breathe = 0.5

export function beforeRender(delta) {
  // three sawtooth phases in a ~10 : 13 : 8.5 ratio (several seconds each)
  t1 = time(0.10 / speed)
  t2 = time(0.13 / speed)
  t3 = time(0.085 / speed)
  // cell size breathes between roughly one quarter and three quarters
  breathe = 0.25 + 0.5 * triangle(time(0.09))
}

export function render3D(index, x, y, z) {
  // slow global hue cycle plus a mild positional gradient
  var h = t1 + (x + y + z) * 0.2

  // separable product of three axis waves, each drifted by a wave of its own
  // time phase; amplified ~10x so only coincident crests are visible
  var i = wave(x * breathe + wave(t1))
        * wave(y * breathe + wave(t2))
        * wave(z * breathe + wave(t3)) * 10

  // fringes stay saturated; past roughly twice unity the core bleaches
  // toward white, like heat
  var s = clamp(3 - i, 0, 1)

  // cubed brightness: hard black between blobs, blown-out cores clamp at full
  hsv(h, s, i * i * i)
}

// planar slice of the volume
export function render2D(index, x, y) {
  render3D(index, x, y, 0)
}

// a line through the volume
export function render(index) {
  render3D(index, index / pixelCount, 0, 0)
}
