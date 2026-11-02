// name: Newton's Cradle
// tags: both, nature
// Curated original (hand-written showcase of the Luxel language/builtins).
// Five polished balls hang in a row. The end ball swings out, falls back and
// clacks into the row with a flash, and the far ball leaps away; each swing
// keeps a little less height until a hand lifts the first ball again. Strip:
// each ball is a shaded sphere of gradientRange spans with an anti-aliased
// position. Panel: seen side-on, strings from a top bar, balls as layered splats.

var swingS = 0.9
// Seconds for one ball's swing out and back.
//# min=0.4 max=3 step=0.1 default=0.9
export function sliderSwingSeconds(v) { swingS = clamp(v, 0.4, 3) }

var swings = 14
// Swings before the energy runs out and the first ball is lifted again.
//# min=3 max=40 step=1 default=14
export function sliderSwings(v) { swings = clamp(floor(v), 3, 40) }

var finish = 0
// 0 chrome, 1 brass, 2 candy colours
//# min=0 max=2 step=1 default=0
export function sliderFinish(v) { finish = clamp(floor(v), 0, 2) }

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var MAXA = 0.62                  // release angle, radians
var tt = 0, phase = 0, swingN = 0, angL = 0, angR = 0, flash = 0, flashSide = 0
var LIFT = 1.6                   // seconds the hand takes to raise the ball

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  tt += dt
  flash = max(0, flash - dt * 6)
  angL = 0
  angR = 0
  if (phase == 0) {
    // the lift: ease the first ball up and hold it a moment
    var u = min(1, tt / LIFT)
    angL = -MAXA * easeInOutSine(min(1, u * 1.25))
    if (u >= 1) { phase = 1; tt = 0; swingN = 0 }
    return
  }
  var a = MAXA * pow(0.82, swingN * 14 / swings)   // height left this swing
  var u = tt / (swingS / 2)                         // 0..1 = out and back
  if (phase == 1) angL = -a * cos(u * PI / 2)        // falling in from the left
  if (phase == 2) angR = a * sin(u * PI / 2)         // right ball out ...
  if (phase == 3) angR = a * cos(u * PI / 2)         // ... and back
  if (phase == 4) angL = -a * sin(u * PI / 2)        // left ball out ...
  if (u >= 1) {
    tt = 0
    if (phase == 1 || phase == 3) {                  // a clack
      flash = min(1, a / MAXA * 1.4)
      flashSide = phase == 1 ? -1 : 1
      swingN++
      if (swingN >= swings) { phase = 0; return }
      phase = 2 + (phase == 3 ? 2 : 0)
    } else if (phase == 2) phase = 3
    else phase = 1                                   // ... falling back in
  }
}

// ball colour: hue, saturation of the ball body
var bh = 0, bs = 0
function tint(k) {
  if (finish == 1) { bh = 0.11; bs = 0.75 }
  else if (finish == 2) { bh = k * 0.2 + 0.02; bs = 0.8 }
  else { bh = 0.6; bs = 0.12 }
}

function ballAngle(k) { return k == 0 ? angL : k == 4 ? angR : 0 }

// ---- strip ----
function sphere1D(c, r, k) {
  tint(k)
  var hl = c - r * 0.25                            // highlight a little left of centre
  gradientRange(c - r, hl, bh, bs, 0.12, bh, bs * 0.35, 1, 1)
  gradientRange(hl, c + r, bh, bs * 0.35, 1, bh, bs, 0.1, 1)
}

function strip() {
  clear()
  var n = pixelCount
  var D = n / 9, r = D * 0.47, L = D * 3.2
  var left = n / 2 - 2 * D
  for (var k = 0; k < 5; k++) {
    var c = left + k * D + L * sin(ballAngle(k))
    sphere1D(c, r, k)
  }
  if (flash > 0) {
    hsv(0.12, 0.1, flash)
    splatIndex(left + (flashSide < 0 ? 0.5 : 3.5) * D, D * (0.4 + 0.5 * flash), 1)
  }
}

// ---- panel ----
function panel() {
  var gw = gridWidth()
  if (gw < 1) gw = 32
  var px = 1 / gw
  fillGradient(0.64, 0.7, 0.16, 0.7, 0.9, 0, 2)    // a night-blue backdrop
  oklch(0.4, 0.02, 0.6)                           // the frame's top bar
  fillRect(0.06, 0.17, 0.94, 0.17 + 1.5 * px)
  var r = 0.07, D = 0.14, py = 0.18, L = 0.52
  for (var k = 0; k < 5; k++) {
    var a = ballAngle(k) * 0.5                     // a gentler arc fits the square
    var ax = (floor((0.5 + (k - 2) * D) * gw) + 0.5) * px   // strings on pixel centres
    var x = ax + L * sin(a), y = py + L * cos(a)
    oklch(0.5, 0.01, 0.6)                          // the string
    drawLine(ax, py, x, y - r, 0.7 * px, 2)
    tint(k)
    hsv(bh, bs, 0.22)                              // the ball: dark rim ...
    fillCircle(x, y, r)
    hsv(bh, bs * 0.8, 0.8)                         // ... lit body ...
    splat(x - r * 0.15, y - r * 0.2, r * 1.05, 2)
    hsv(0.08, 0.6, 0.35)                           // ... warm light off the floor ...
    splat(x, y + r * 0.55, r * 0.5, 1)
    hsv(bh, bs * 0.15, 1)                          // ... and a hot highlight
    splat(x - r * 0.35, y - r * 0.35, r * 0.45, 2)
  }
  if (flash > 0) {
    hsv(0.12, 0.2, flash)
    splat(0.5 + (flashSide < 0 ? -1.5 : 1.5) * D, py + L, r * (0.7 + 1.2 * flash), 1)
  }
}

export function renderBulk() {
  if (has2DMap()) panel()
  else strip()
}
