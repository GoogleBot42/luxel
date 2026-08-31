// name: firework nova
// Clean-room reimplementation from a prose functional description of the
// community pattern "firework nova"; original source never consulted.

// A thin bright spherical shell erupts from the center of the volume
// roughly once a second and expands outward over black, trailed by random
// pure-white sparks. Hue runs as a rainbow gradient along the cube's main
// diagonal and slowly drifts, so each blast is tinted differently.

const radiusScale = 0.5   // shrink the radius so the wave spans the volume

// Tunables. Each is initialized to the exact constant the port shipped with,
// so the untouched render is unchanged; the sliders below re-express those
// constants in real units.
var burstInterval = 0.013 // time() interval for the blast clock (~0.85 s)
var shellCut = 0.75       // triangle level below which the shell is dark
var shellGain = 4         // 1 / shell thickness — renormalizes the survivor
var sparkChance = 0.125   // peak per-pixel spark probability (12.5%)
var hueSpread = 1         // how much of the wheel the diagonal rainbow spans

// Seconds between blasts.
//# min=0.2 max=5 step=0.05 default=0.85
export function sliderBurstTime(v) {
  burstInterval = max(v, 0.2) / 65.536
}

// Thickness of the expanding shell, as a fraction of the blast cycle.
//# min=0.05 max=0.6 step=0.01 default=0.25
export function sliderShellThickness(v) {
  var th = clamp(v, 0.05, 0.6)
  shellCut = 1 - th
  shellGain = 1 / th
}

// Peak chance (percent, per pixel per frame) of a white spark at the
// brightest point of the trailing shell.
//# min=0 max=50 step=0.5 default=12.5
export function sliderSparkChance(v) {
  sparkChance = clamp(v, 0, 50) / 100
}

// How much of the color wheel the rainbow spans across the volume's
// diagonal: 1 = full rainbow, 0 = every blast a single drifting color.
//# min=0 max=1 step=0.05 default=1
export function sliderHueSpread(v) {
  hueSpread = clamp(v, 0, 1)
}

var t1, t2

export function beforeRender(delta) {
  t1 = time(burstInterval)   // fast phase, ~0.85 s: blast expansion
  t2 = time(0.08)            // slow phase, ~5 s: hue drift
}

function nova(x, y, z) {
  // recenter on the middle of the unit cube
  x -= 0.5
  y -= 0.5
  z -= 0.5
  var r = hypot3(x, y, z) * radiusScale

  // rainbow along the main diagonal, drifting over time
  var h = (x + y + z) / 3 * hueSpread + t2

  // expanding shell: only the top slice of the triangle wave survives
  var v = triangle(r - t1) - shellCut

  // trailing sparks: same clipped wave, phase-shifted slightly behind,
  // renormalized to 0..1 and used as a probability field that peaks at
  // sparkChance
  var s = triangle(r - t1 + 0.04) - shellCut
  if (s * shellGain * sparkChance > random(1)) {
    rgb(1, 1, 1)
    return
  }

  // renormalize the surviving slice to 0..1 and cube it: sharpens the
  // shell and (sign-preserving) clips the negative regions to black
  v = v * shellGain
  v = v * v * v
  hsv(h, 1, v)
}

export function render3D(index, x, y, z) {
  nova(x, y, z)
}

// fallbacks (the original is 3D-only; these treat the missing axes as
// centered so unmapped fixtures still show the blast)
export function render2D(index, x, y) {
  nova(x, y, 0.5)
}

export function render(index) {
  nova(index / pixelCount, 0.5, 0.5)
}
