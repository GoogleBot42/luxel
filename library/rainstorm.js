// name: Rainstorm
// Clean-room reimplementation from a prose functional description of the
// community pattern "Rainstorm"; original source never consulted.

// Deep-blue rain: vertical columns of short bright streaks, each column
// with its own stable speed / brightness / spacing, plus irregular
// near-white lightning blooming from the center of the display.

var rate = 1.4             // rain fall rate
//# min=0 max=1 step=0.01 default=0.4
export function sliderSpeed(v) {
  rate = 0.3 + 2.7 * v     // slow drizzle .. fast downpour
}

var angle = 0
//# min=0 max=1 step=0.01 default=0
export function sliderAngle(v) {
  angle = v * PI2          // full turn of slant
}

var colW = 0.325           // streak column width (in transformed units)
//# min=0 max=1 step=0.01 default=0.5
export function sliderScale(v) {
  colW = 0.15 + 0.35 * v   // right = wider columns, fewer of them
}

var lightningAmt = 0.3
//# min=0 max=1 step=0.01 default=0.3
export function sliderLightning(v) {
  lightningAmt = v         // fraction of each cycle flashes may occur in
}

var tAcc = 0
var rainClock = 0
var flash = 0
var ca = 1
var sa = 0

export function beforeRender(delta) {
  // wrapped accumulator so speed changes never jump and precision holds
  tAcc = mod(tAcc + delta / 1000 * rate, 3600)
  rainClock = tAcc * 10

  // lightning: smooth noise moving fast on one axis, doubled, clamped,
  // cubed -> rare sharp spikes; then gated to the tail of a repeating cycle
  var spike = clamp(simplex2(tAcc * 7, tAcc * 0.3) * 2, 0, 1)
  spike = spike * spike * spike
  var gate = mod(tAcc, 3) > 3 * (1 - lightningAmt)
  flash = gate ? spike : 0

  ca = cos(angle)
  sa = sin(angle)
}

export function render2D(index, x, y) {
  // center the origin, scale up ~2x with the vertical axis flipped, rotate
  var ux = (x - 0.5) * 2
  var uy = (0.5 - y) * 2
  var px = ux * ca - uy * sa
  var py = ux * sa + uy * ca

  // flash radius: positive near center while a flash is active; stronger
  // strikes light a wider area
  var fr = 1.5 * flash - hypot(px, py)

  // perspective trick: slight radial stretch toward a point a couple of
  // units below the display keeps streaks from spanning the full height
  var f = 1 + 0.15 * hypot(px, py + 2.5)
  px = px * f
  py = py * f

  // column personality: stable per-column random draw (same every frame)
  var q = hash(floor((px + 16) / colW))

  // streaks: |sin| of (clock * column speed + y * column spacing), scaled
  // by column brightness, then ^4 to sharpen into short dashes
  var b = clamp(abs(sin(rainClock * (0.6 + 0.8 * q) + py * (4 + 4 * q))), 0, 1)
  b = b * (0.35 + 0.65 * q)
  var v = b * b * b * b

  // pure blue; streak cores whiten slightly, lightning washes toward white
  var bloom = max(fr, 0)
  var bright = v + flash * bloom * bloom * bloom
  hsv(0.66, clamp(1 - 0.2 * v - flash, 0, 1), clamp(bright, 0, 1))
}
