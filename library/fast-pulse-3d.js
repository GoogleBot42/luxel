// name: Fast Pulse
// tags: both, ambient
// Clean-room reimplementation from the prose specs docs/pattern-specs/fast-pulse-3d.md,
// fast-pulse.md — reimagined, not a replica.
// Razor-thin pulses with white-hot cores and rainbow fringes whip through the
// display, racing through the middle of their sweep and lingering at the ends.
// On a strip they are bands; on a panel, lines whose angle slowly turns; in 3D,
// planes that tumble. The sweep direction is always a unit vector (an angle,
// not three free sines), so no frame ever goes edge-on and dark. The falloff is
// an integer pow(), which the engine evaluates as repeated multiplication.

var cycleS = 5.2
// Seconds per sweep and per trip round the colour wheel.
//# min=1 max=30 step=0.1 default=5.2
export function sliderCycleSeconds(v) { cycleS = clamp(v, 1, 30) }

var sharp = 5
// Falloff power: higher is thinner.
//# min=1 max=12 step=1 default=5
export function sliderSharpness(v) { sharp = clamp(floor(v), 1, 12) }

var pulses = 1
// Pulse crests across the display.
//# min=1 max=8 step=1 default=1
export function sliderPulses(v) { pulses = clamp(floor(v), 1, 8) }

var hueSweep = 100
// How much of the colour wheel each cycle walks, in percent.
//# min=0 max=100 step=5 default=100
export function sliderHueSweepPercent(v) { hueSweep = clamp(v, 0, 100) }

var hueBase = 0
// Where the colour walk starts (hue only; the white core is the pattern's own).
export function hsvPickerColor(h, s, v) { hueBase = h }

var t = 0, hue = 0, off1 = 0, off = 0
var ax = 1, ay = 0, bx = 1, by = 0, bz = 0

export function beforeRender(delta) {
  t = mod(t + min(delta, 100) / 1000 / cycleS, 1)
  hue = hueBase + t * hueSweep / 100
  var s = sin(t * PI2)
  off1 = s * 2                      // strip: two folds per sweep
  off = 0.5 + s * 0.5               // panel/volume: edge to edge, lingering at each
  var th = time(0.07 * cycleS / 5.2) * PI2         // the slow turn
  var ph = 0.8 * sin(time(0.05 * cycleS / 5.2) * PI2)   // and a tilt for 3D
  ax = cos(th)
  ay = sin(th)
  bx = ax * cos(ph)
  by = ay * cos(ph)
  bz = sin(ph)
}

function shade(u, core) {
  var v = pow(triangle(u), sharp)
  hsv(hue, v < core, v)             // the top of the crest goes white-hot
}

export function render(index) {
  shade(off1 + index / pixelCount * pulses, 0.9)
}

export function render2D(index, x, y) {
  shade(off + ((x - 0.5) * ax + (y - 0.5) * ay) * pulses, 0.85)
}

export function render3D(index, x, y, z) {
  shade(off + ((x - 0.5) * bx + (y - 0.5) * by + (z - 0.5) * bz) * pulses, 0.8)
}
