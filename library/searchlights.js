// name: Searchlights
// tags: panel, ambient
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/all-lasers-fire.md,
// 3d-rotation-spotlights.md, spotlights-rotation-3d.md — reimagined, not a replica.
// Mode 0: soft searchlight cones sweep up from the bottom edge into a hazy night sky and
// light up drifting clouds where they cross. Mode 1: a moving emitter fires volleys of a
// rainbow laser fan (coloured halos, white-hot cores) with bolts racing out along it.
// Mode 2: a tumbling double cone, projected as stacked elliptical slices.
// renderBulk: a sky gradient, then a few chunked drawLine capsules per beam — no per-pixel math.

const NCL = 2
var mode = 0, beams = 3, sweepS = 8, haze = 8, coneDeg = 25, tumbleS = 8
var beamH = 0.12, beamS = 0.3, skyH = 0.66, skyS = 0.85
var T = 0, lw = 0.03
var clx = array(NCL), cly = array(NCL), clr = array(NCL)

//# min=0 max=2 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 2) }
//# min=1 max=8 step=1 default=3
export function sliderBeams(v) { beams = clamp(floor(v), 1, 8) }
//# min=2 max=30 step=0.5 default=8
export function sliderSweepSeconds(v) { sweepS = clamp(v, 2, 30) }
//# min=0 max=30 step=1 default=8
export function sliderHazePercent(v) { haze = clamp(v, 0, 30) }
//# min=5 max=60 step=1 default=25
export function sliderConeDeg(v) { coneDeg = clamp(v, 5, 60) }
//# min=2 max=30 step=0.5 default=8
export function sliderTumbleSeconds(v) { tumbleS = clamp(v, 2, 30) }
// Pickers set hue and saturation only; V is ignored (use the global brightness).
export function hsvPickerBeamColour(h, s, v) { beamH = h; beamS = s }
export function hsvPickerSkyColour(h, s, v) { skyH = h; skyS = s }

for (var i = 0; i < NCL; i++) { clx[i] = hash(i + 0.3); cly[i] = 0.12 + 0.25 * hash(i + 4.7); clr[i] = 0.09 + 0.04 * hash(i + 9.1) }

export function beforeRender(delta) {
  var dt = min(delta, 100) * 0.001
  T = mod(T + dt, 3600)
  for (var i = 0; i < NCL; i++) { clx[i] = mod(clx[i] + dt * (0.012 + 0.006 * i) + 0.3, 1.6) - 0.3 }
}

// A long capsule as short pieces, so each piece's bounding box stays small.
function seg(x0, y0, x1, y1, w) {
  var n = clamp(ceil(min(abs(x1 - x0), abs(y1 - y0)) / 0.12), 1, 8)
  for (var k = 0; k < n; k++) drawLine(x0 + (x1 - x0) * k / n, y0 + (y1 - y0) * k / n, x0 + (x1 - x0) * (k + 1) / n, y0 + (y1 - y0) * (k + 1) / n, w, 2)
}

function sky() {
  var h = haze / 100
  if (h <= 0) { clear(); return }
  hsv(skyH, skyS, h * 0.6)
  fill()
  for (var b = 1; b < 10; b++) {            // flat bands toward the glowing horizon
    hsv(skyH + b * 0.008, skyS * (1 - b * 0.02), h * (0.6 + b * b * 0.03))
    fillRect(0, 0.4 + b * 0.06, 1, 0.46 + b * 0.06)
  }
  hsv(skyH, skyS * 0.6, h * 1.5)
  for (var i = 0; i < NCL; i++) splat(clx[i], cly[i], clr[i], 2)
}

function searchlights() {
  var L = 1.3, K = 4
  for (var b = 0; b < beams; b++) {
    var ox = (b + 0.5) / beams + 0.04 * sin(T * 0.3 + b)
    var a = 0.72 * sin(T * PI2 / sweepS * (1 + 0.17 * b) + b * 2.1)
    var dx = sin(a), dy = -cos(a)
    for (var k = 0; k < K; k++) {             // the soft cone: widening, dimming pieces
      var t0 = L * k / K, t1 = L * (k + 1) / K, tm = (k + 0.5) / K
      hsv(beamH, beamS, 0.95 - 0.6 * tm)
      drawLine(ox + dx * t0, 1 + dy * t0, ox + dx * t1, 1 + dy * t1, 0.03 + 0.12 * tm, 2)
    }
    hsv(beamH, beamS * 0.5, 1)
    splat(ox, 1, 0.07, 2)                     // the lamp
    for (var i = 0; i < NCL; i++) {           // clouds the beam passes through light up
      var cx = clx[i] - ox, cy = cly[i] - 1
      var along = cx * dx + cy * dy, perp = abs(cx * dy - cy * dx)
      if (along > 0 && perp < clr[i]) {
        hsv(beamH, beamS * 0.7, 0.45 * (1 - perp / clr[i]))
        splat(ox + dx * along, 1 + dy * along, clr[i] * 0.9, 1)
      }
    }
  }
}

function lasers() {
  var ph = mod(T / sweepS, 1)
  var rest = min(0.15, 0.8 / sweepS)
  var ex = 0.5 + 0.32 * sin(T * 0.45), ey = 0.94
  hsv(beamH, beamS * 0.5, 0.9)
  splat(ex, ey, 0.06, 2)
  if (ph > 1 - rest) return                   // a short breath between volleys
  var n = beams + 2
  var base = 0.55 * sin(ph * PI2 * 2), spread = 0.35 + 0.3 * sin(ph * PI2 + 1)
  for (var b = 0; b < n; b++) {
    var a = base + spread * (n > 1 ? b / (n - 1) - 0.5 : 0) * 2
    var dx = sin(a), dy = -cos(a), L = 1.5
    var x1 = ex + dx * L, y1 = ey + dy * L
    hsv(beamH + b / n, 1, 1); seg(ex, ey, x1, y1, lw * 1.5)      // coloured halo…
    rgb(1, 1, 1); seg(ex, ey, x1, y1, lw * 0.6)                  // …white-hot core
    var bt = mod(T * 1.3 + b * 0.37, 1) * L    // a bolt racing out along the beam
    hsv(0, 0, 1)
    splat(ex + dx * bt, ey + dy * bt, lw * 2.2, 1)
  }
}

function cones() {
  var w = T * PI2 / tumbleS
  var ax = sin(w * 0.83), ay = sin(w + 1), az = cos(w * 0.61) + 0.2
  var n = hypot3(ax, ay, az)
  ax /= n; ay /= n; az /= n
  var tn = tan(coneDeg * PI / 180), L = min(0.62, 0.36 / tn), K = 6
  var pl = hypot(ax, ay), ux = 1, uy = 0
  if (pl > 0.01) { ux = -ay / pl; uy = ax / pl }
  for (var side = -1; side <= 1; side += 2) {
    var lit = 0.7 + 0.3 * side * az           // the half facing you is brighter
    for (var k = K; k >= 1; k--) {            // far, wide, dim slices first
      var s = L * k / K, rho = s * tn
      var cx = 0.5 + side * ax * s, cy = 0.5 + side * ay * s
      // Additive slices whose tents tile the projected axis: light summed along the view.
      var ps = L / K * pl, wmin = max(max(rho * abs(az), ps), lw)
      var hl = max(0, rho - wmin)
      hsv(beamH, 0.35 + 0.6 * k / K, lit * (1 - 0.5 * k / K) * clamp(ps / wmin, 0.3, 1))
      drawLine(cx - ux * hl, cy - uy * hl, cx + ux * hl, cy + uy * hl, wmin, 1)
    }
  }
  hsv(beamH, 0.2, 1)
  splat(0.5, 0.5, 0.06, 2)
}

export function renderBulk() {
  lw = 1.6 / (gridWidth() > 0 ? gridWidth() : 32)
  sky()
  if (mode == 0) searchlights()
  else if (mode == 1) lasers()
  else cones()
}
