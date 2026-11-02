// name: Crosstown Traffic
// tags: both, ambient
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/crosstown-traffic-2d.md, bustle.md — reimagined, not a replica.
// A long-exposure photo of a city grid: cars stream both ways in their own lanes,
// each leaving a fading light trail. On a panel half run across and half up and
// down, one additive drawLine per car per frame, so crossings flare; on a strip
// they become two-way comets (fillRangeAA, max-blended). The frame itself is the trail buffer
// (delta-based fade), so there are no pixel arrays; trails are pre-drawn at load.

var MAXC = 48
var pos = array(MAXC), spd = array(MAXC), lane = array(MAXC), dir = array(MAXC)
var prev = array(MAXC)

var cars = 32                     // cars on a panel; a strip gets one per 50 px
//# min=4 max=48 step=1 default=32
export function sliderCars(v) { cars = clamp(floor(v), 4, MAXC) }

var crossing = 2.5                // seconds for an average car to cross
//# min=1 max=20 step=0.5 default=2.5
export function sliderCrossingSeconds(v) { crossing = clamp(v, 1, 20) }

var widthPx = 1                   // panel line width
//# min=1 max=4 step=1 default=1
export function sliderWidthPx(v) { widthPx = clamp(floor(v), 1, 4) }

var trailS = 1.5                  // seconds for a trail to fade out
//# min=0.1 max=3 step=0.1 default=1.5
export function sliderTrailSeconds(v) { trailS = clamp(v, 0.1, 3) }

var hueCycle = 30                 // rainbow: seconds per hue turn, 0 holds
//# min=0 max=120 step=1 default=30
export function sliderHueCycleSeconds(v) { hueCycle = clamp(v, 0, 120) }

// 0 rainbow | 1 two-tone (the pickers, one per direction) | 2 headlights & taillights
var palette = 0
//# min=0 max=2 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 2) }

// Two-tone colours (hue and saturation; the picker's brightness is ignored).
var hA = 0.88, sA = 1, hB = 0.98, sB = 1
export function hsvPickerColorA(h, s, v) { hA = h; sA = s }
export function hsvPickerColorB(h, s, v) { hB = h; sB = s }

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

function spawn(j, anywhere) {
  dir[j] = floor(j / 2) % 2 ? -1 : 1
  spd[j] = 0.55 + random(1.1)
  lane[j] = random(1)
  pos[j] = anywhere ? random(1) : (dir[j] > 0 ? -0.02 : 1.02)
  prev[j] = pos[j]
}
for (var j = 0; j < MAXC; j++) spawn(j, 1)

var hue0 = 0, warm = 1, lastW = -1, fadeK = 1, stepDt = 0.05

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  stepDt = dt
  hue0 = hueCycle > 0 ? mod(hue0 + dt / hueCycle, 1) : hue0
  fadeK = pow(0.02, dt / trailS)
}

function count() {
  if (has2DMap()) return cars
  return clamp(round(cars * pixelCount / 1600), 2, MAXC)
}

function step(dt) {
  var n = count()
  for (var j = 0; j < n; j++) {
    prev[j] = pos[j]
    pos[j] += dir[j] * spd[j] * dt / crossing
    if (pos[j] > 1.05 || pos[j] < -0.05) { spawn(j, 0); prev[j] = pos[j] }
  }
}

function brush(j, n) {
  if (palette == 0) hsv(hue0 + j / n, 1, 1)
  else if (palette == 1) { if (dir[j] > 0) hsv(hA, sA, 1); else hsv(hB, sB, 1) }
  else { if (dir[j] > 0) hsv(0.13, 0.3, 1); else hsv(0.99, 1, 1) }
}

function draw() {
  var n = count()
  if (has2DMap()) {
    var gw = gridWidth() > 1 ? gridWidth() : 32
    var gh = gridHeight() > 1 ? gridHeight() : 32
    var odd = widthPx % 2
    var wx = (widthPx * 0.5 + 0.5) / (gw - 1), wy = (widthPx * 0.5 + 0.5) / (gh - 1)
    for (var j = 0; j < n; j++) {
      brush(j, n)
      if (j % 2) {                // vertical: the lane is a column
        var cx = (floor(lane[j] * (gw - odd)) + 0.5 - odd * 0.5) / (gw - 1)
        drawLine(cx, prev[j], cx, pos[j], wx, 1)
      } else {
        var cy = (floor(lane[j] * (gh - odd)) + 0.5 - odd * 0.5) / (gh - 1)
        drawLine(prev[j], cy, pos[j], cy, wy, 1)
      }
    }
  } else {
    var last = pixelCount - 1
    for (var j = 0; j < n; j++) {
      brush(j, n)
      var a = prev[j] * last, b = pos[j] * last   // this frame's stretch, sub-pixel
      fillRangeAA(min(a, b), max(a, b) + 1, 2)
    }
  }
}

export function renderBulk() {
  var w = gridWidth()
  if (warm || w != lastW) {       // load or resize: lay the trails down at once
    lastW = w
    warm = 0
    clear()
    var k = pow(0.02, 0.05 / trailS)
    for (var s = 0; s < 40; s++) { fade(k); step(0.05); draw() }
    return
  }
  fade(fadeK)
  step(stepDt)
  draw()
}
