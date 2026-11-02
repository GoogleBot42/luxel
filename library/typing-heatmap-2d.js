// name: Touch Glow
// tags: panel, ambient
// Curated original (hand-written showcase of the Luxel language/builtins).
// A keyboard heatmap: every touch deposits heat that spreads to its neighbours
// and cools with a set half-life, through a thermal ramp (black, navy, magenta,
// red, yellow, white) so hammered spots burn white. Touches are injected events
// (click the preview, POST /api/events, MQTT); a phantom typist fills the idle
// time. Layout 1 snaps heat to the keys of a drawn keyboard. Heat lives on a
// canvas of up to 32x32 cells (blur2D + arrayMix), painted bilinearly.

var halfLife = 3       // seconds for heat to halve
//# min=0.5 max=10 step=0.1 default=3
export function sliderCoolingHalfLifeSeconds(v) { halfLife = clamp(v, 0.5, 10) }

var layout = 0         // 0 free, 1 keyboard rows
//# min=0 max=1 step=1 default=0
export function sliderLayout(v) { layout = clamp(floor(v), 0, 1); cw = 0 }

var demoRate = 4       // phantom keys per second while no real touches arrive
//# min=0 max=20 step=1 default=4
export function sliderDemoKeysPerSecond(v) { demoRate = clamp(v, 0, 20) }

export function triggerKeys() { burst = 6 }

var CAP = 32
var heat = array(CAP * CAP), scratch = array(CAP * CAP), show = array(CAP * CAP), caps = array(CAP * CAP)
var cw = 0, chh = 0, acc = 2, burst = 0, quiet = 0   // acc: two keys on the first frame
var ev = array(4)

setPalette([
  0.0,  0,    0,    0,
  0.1,  0.02, 0.03, 0.3,
  0.3,  0.55, 0.05, 0.6,
  0.5,  1,    0.1,  0.1,
  0.72, 1,    0.65, 0,
  1.0,  1,    1,    1
])

// keyboard: 5 rows of key units — row offsets and counts; space bar on row 4
var rowOff = [0, 0.25, 0.5, 1, 2.75], rowN = [10, 10, 9, 7, 1]
var unitW = 1, unitH = 1, top = 0
function keyRect(r, k) {                     // -> kx0, ky0, kx1, ky1 in cells (exclusive end)
  var w = r == 4 ? 5 : 1
  kx0 = floor((rowOff[r] + k) * unitW + 0.5); kx1 = floor((rowOff[r] + k + w) * unitW + 0.5) - 1
  ky0 = floor(top + r * unitH + 0.5); ky1 = floor(top + (r + 1) * unitH + 0.5) - 1
}
var kx0 = 0, ky0 = 0, kx1 = 0, ky1 = 0

function setup() {
  var gw = gridWidth(), gh = gridHeight()
  cw = gw > 0 ? min(gw, CAP) : CAP
  chh = gh > 0 ? min(gh, CAP) : CAP
  unitW = cw / 10.5
  unitH = min(chh / 5, unitW * 1.6)
  top = (chh - unitH * 5) / 2
  for (var i = 0; i < CAP * CAP; i++) { heat[i] = 0; caps[i] = 0 }
  if (layout == 1) {
    for (var r = 0; r < 5; r++) for (var k = 0; k < rowN[r]; k++) {
      keyRect(r, k)
      for (var y = ky0; y < ky1; y++) for (var x = kx0; x < kx1; x++) if (x < cw) caps[y * cw + x] = 0.06
    }
  }
}

// add heat over a block of cells, saturating below the ramp's white end
function heatRect(x0, y0, x1, y1, a) {
  for (var y = max(y0, 0); y <= min(y1, chh - 1); y++)
    for (var x = max(x0, 0); x <= min(x1, cw - 1); x++) {
      var i = y * cw + x
      heat[i] = min(heat[i] + a, 0.94)
    }
}

function touch(x, y) {                       // normalized 0..1
  if (layout == 1) {
    var r = clamp(floor((y * chh - top) / unitH), 0, 4)
    var k = clamp(floor(x * cw / unitW - rowOff[r]), 0, rowN[r] - 1)
    hitKey(r, k)
  } else {
    var cx = floor(x * cw), cy = floor(y * chh), s = max(1, floor(cw / 16))
    heatRect(cx - s, cy - s, cx + s - 1, cy + s - 1, 0.9)
  }
}
function hitKey(r, k) { keyRect(r, k); heatRect(kx0, ky0, kx1 - 1, ky1 - 1, 0.75) }

function phantom() {
  if (layout == 1) {
    var p = random(1), r = p < 0.15 ? 4 : p < 0.5 ? 2 : p < 0.55 ? 0 : 1 + floor(random(3))   // home row and space most
    hitKey(r, floor(random(rowN[r])))
  } else touch(random(1), clamp(0.5 + (random(1) - 0.5) * 0.7, 0, 1))
}

export function beforeRender(delta) {
  var gw = gridWidth(), gh = gridHeight()
  if (cw == 0 || (gw > 0 && min(gw, CAP) != cw) || (gh > 0 && min(gh, CAP) != chh)) setup()
  var dt = min(delta, 100) / 1000
  while (readEvent(ev)) { touch(ev[1], ev[2]); quiet = 4 }
  quiet -= dt
  acc = quiet > 0 ? 0 : acc + dt * demoRate
  acc += burst
  burst = 0
  while (acc >= 1) { acc -= 1; phantom() }
  var n = cw * chh
  arrayMix(scratch, heat, 1)                 // diffuse toward the 3x3 blur
  blur2D(scratch, cw, chh, 1)
  arrayMix(heat, scratch, min(dt * 3, 0.2))
  arrayScale(heat, pow(0.5, dt / halfLife))
  arrayMix(show, heat, 1)
  if (layout == 1) arrayAdd(show, caps)
}

export function renderBulk() {
  paintCanvas(show, cw, chh, 1, 1)
}
