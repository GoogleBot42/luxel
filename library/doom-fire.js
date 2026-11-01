// name: Doom Fire
// Clean-room reimplementation from the prose spec docs/pattern-specs/doom-fire.md — reimagined, not a replica.
// The PSX-era spread fire: no noise at all. Each step every cell copies the
// cell below it — nudged a column sideways at random, more often downwind —
// minus a cooling drawn in 1/36 steps, so heat climbs off a white-hot bed in
// ragged tongues through a 36-step ramp. The sim is sized from the grid (an
// integer fraction of it, at most 48 cells a side) and painted with one
// paintCanvas. Dragon makes the bed exhale in surges. Strip: one column.

var CAP = 48
var cv = array(CAP * CAP)
var pal = array(40)
setPalette(pal)
function stop(k, pos, r, g, b) {
  pal[k * 4] = pos
  pal[k * 4 + 1] = r
  pal[k * 4 + 2] = g
  pal[k * 4 + 3] = b
}
// black → soot → blood red → red → orange → amber → straw → white
stop(0, 0, 0, 0, 0)
stop(1, 0.08, 0.11, 0.02, 0.02)
stop(2, 0.2, 0.36, 0.05, 0.02)
stop(3, 0.33, 0.62, 0.1, 0.02)
stop(4, 0.46, 0.83, 0.24, 0.03)
stop(5, 0.58, 0.9, 0.4, 0.04)
stop(6, 0.7, 0.88, 0.56, 0.1)
stop(7, 0.82, 0.85, 0.72, 0.22)
stop(8, 0.92, 0.93, 0.88, 0.55)
stop(9, 1, 1, 1, 1)

var flameH = 0.75
// How far the tongues climb, as a percentage of the panel height.
//# min=20 max=100 step=1 default=75
export function sliderFlameHeightPct(v) { flameH = clamp(v, 20, 100) / 100 }

var steps = 30
//# min=10 max=60 step=1 default=30
export function sliderStepsPerSecond(v) { steps = clamp(v, 10, 60) }

var wind = 0
// Negative blows left. It gusts.
//# min=-100 max=100 step=1 default=0
export function sliderWindPct(v) { wind = clamp(v, -100, 100) / 100 }

var dragon = 0
//# default=0
export function toggleDragon(v) { dragon = v > 0.5 }

var DQ = 2                     // cooling quantum, in 1/36 steps
var cw = 32, ch = 32, acc = 0, t = 0, breath = 1, warm = 0

function size() {
  var gw = gridWidth(), gh = gridHeight()
  if (!has2DMap()) {
    cw = 1                                     // a strip is one column
    ch = clamp(pixelCount, 2, 64)
  } else if (gw > 0) {
    var k = ceil(gw / CAP)
    while (gw % k != 0 && k < 6) k++          // whole blocks: an even, crisp upscale
    cw = clamp(ceil(gw / k), 2, CAP)
    ch = clamp(ceil(gh / k), 2, CAP)
  } else {
    cw = 32
    ch = 32
  }
}

function step() {
  var rows = ch - 1
  var q = 36 / max(rows * flameH, 1) * 0.85   // mean cooling, in 1/36 steps per row
  if (dragon) q *= 1.5 - 0.8 * breath          // an exhale also burns taller
  var qs = cw == 1 ? 1 : DQ                     // a strip cools in finer steps (less flicker)
  var dq = qs / 36, p = min(q / qs, 1)
  var w = wind * (0.6 + 0.4 * wave(t / 9)) + 0.12 * sin(t * 0.6)
  var pr = clamp(0.25 + 0.3 * w, 0, 0.6)       // chance a parcel drifts a column right
  var pl = 1 - clamp(0.25 - 0.3 * w, 0, 0.6)
  var last = cw - 1
  for (var r = 0; r < rows; r++) {
    var o = r * cw, b = o + cw
    for (var c = 0; c < cw; c++) {
      var j = random(1)                       // one draw picks the drift and the cooling
      var dc = c
      if (j < pr) dc = min(c + 1, last)
      else if (j > pl) dc = max(c - 1, 0)
      var dec = frac(j * 13.7) < p ? dq : 0
      if (cw == 1 && j < 0.3 && r < rows - 1) dec = 0   // strip: sometimes a parcel leaps
      cv[o + dc] = max(0, cv[b + c] - dec)
    }
  }
  // the bed: white-hot with drifting cooler patches (the gaps between tongues), or a dragon's breath
  var o2 = rows * cw
  for (var c2 = 0; c2 < cw; c2++) {
    var s = clamp(0.8 + 0.4 * simplex2(c2 * 5 / cw, t * 0.5, 3), 0.4, 0.995)
    cv[o2 + c2] = dragon ? s * breath * (0.7 + 0.3 * wave(c2 / max(cw, 2) + t * 0.2)) : s
  }
}

export function beforeRender(delta) {
  var dt = min(delta, 100)
  t = mod(t + dt / 1000, 3600)
  var e = wave(t / 4.5)
  breath = 0.5 + 0.5 * e * e * e
  size()
  if (warm != cw * 1000 + ch) {                  // new size: burn in, so it never starts dark
    warm = cw * 1000 + ch
    for (var i = 0; i < ch; i++) step()
  }
  setBlur(cw == 1 ? 0.5 : 0)
  var rate = cw == 1 ? steps * ch / 32 : steps
  acc = min(acc + dt, 4000 / rate)
  while (acc >= 1000 / rate) {
    acc -= 1000 / rate
    step()
  }
}

export function renderBulk() {
  if (cw > 1) {
    paintCanvas(cv, cw, ch)
    return
  }
  // strip: the column runs from index 0 (the bed) to the far end
  var n = pixelCount
  var sc = (ch - 1) / max(n - 1, 1)
  for (var i = 0; i < n; i++) {
    var u = (ch - 1) - i * sc
    var k = floor(u)
    var h = cv[k] + (cv[min(k + 1, ch - 1)] - cv[k]) * (u - k)
    paint(min(h, 0.999))
    setPixel(i)
  }
}

// declares the strip form (no default grid on a mapless strip); renderBulk wins
export function render(index) { paint(0.5) }
