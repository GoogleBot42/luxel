// name: Bouncing Columns
// tags: panel, ambient
// Clean-room reimplementation from the prose spec docs/pattern-specs/bouncing-balls-2d.md — reimagined, not a replica.
// One white ball per panel column, bouncing under real gravity on top of a
// rainbow column of light, like a graphic equaliser that obeys physics. Every
// ball loses a little speed per bounce and is relaunched on its own once it is
// spent, so the wall ripples forever instead of settling. Three fillRects per
// column (body, glowing cap, ball): never a per-pixel loop.

var MAXC = 128
var hgt = array(MAXC), vel = array(MAXC), wait = array(MAXC), el = array(MAXC)
for (var k = 0; k < MAXC; k++) {
  hgt[k] = 1
  wait[k] = hash(k + 0.5)
  el[k] = 0.8 + 0.1 * hash(k + 7.5)
}

// Panel heights per second squared.
var gravity = 3
//# min=0.5 max=10 step=0.1 default=3
export function sliderGravity(v) { gravity = clamp(v, 0.5, 10) }

// Longest random pause, in seconds, before a spent ball is relaunched.
var stagger = 1
//# min=0 max=3 step=0.05 default=1
export function sliderStagger(v) { stagger = clamp(v, 0, 3) }

var hue0 = 0, cols = 32

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  var gw = gridWidth()
  cols = clamp(gw > 0 ? gw : 32, 1, MAXC)
  var v0 = sqrt(2 * gravity)                   // just reaches the top
  for (var c = 0; c < cols; c++) {
    if (wait[c] > 0) { wait[c] -= dt; continue }
    vel[c] -= gravity * dt
    hgt[c] += vel[c] * dt
    if (hgt[c] <= 0) {
      hgt[c] = 0
      vel[c] = -vel[c] * el[c]
      if (vel[c] < v0 * 0.5) {                // spent: rest, then relaunch
        vel[c] = v0 * (0.75 + 0.25 * hash2(c, hue0 * 97))
        var w = hash2(c + 0.5, hue0 * 61)
        wait[c] = stagger * w * w
      }
    }
    if (hgt[c] > 1) { hgt[c] = 1; vel[c] = min(vel[c], 0) }
  }
  hue0 = time(0.12)
}

export function renderBulk() {
  var gw = gridWidth(), gh = gridHeight()
  if (gw == 0) { gw = 32; gh = 32 }
  var kx = 1 / max(gw - 1, 1), ky = 1 / max(gh - 1, 1)
  var cw = gw / cols                            // cells per column (1 unless clamped)
  clear()
  var cap = max(1, gh / 16)
  for (var c = 0; c < cols; c++) {
    var x0 = (c * cw - 0.45) * kx, x1 = ((c + 1) * cw - 0.55) * kx
    var row = round((1 - hgt[c]) * (gh - 1))      // the floor is y = 1
    var h = hue0 + 0.8 * c / cols
    hsv(h, 1, 0.6)
    fillRect(x0, (row + 0.5) * ky, x1, (gh - 0.5) * ky)
    hsv(h, 0.55, 1)
    fillRect(x0, (row + 0.5) * ky, x1, (row + cap + 0.45) * ky)
    hsv(0, 0, 1)
    fillRect(x0, (row - 0.45) * ky, x1, (row + 0.45) * ky)
  }
}
