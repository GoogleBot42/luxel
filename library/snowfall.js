// name: Snowfall
// Curated original (hand-written showcase of the Luxel language/builtins).
// Panel: flakes in continuous depth — tiny slow far ones, big swaying near
// ones — fall through a dusky blue night (one fillGradient, a splat per
// flake) and settle into drifts: a height per column that slumps to an angle
// of repose, slowly melts so it never fills, and gets scoured by gusts that
// lift powder off the crests. Strip: gentle snowfall past a window, index 0
// at the top, a little pile building at the far end. No frame-sized buffers.

var snowPct = 50
//# min=10 max=100 step=1 default=50
export function sliderSnowPercent(v) { snowPct = clamp(v, 10, 100) }

var windPct = 30          // steady breeze and how often gusts come
//# min=0 max=100 step=1 default=30
export function sliderWindPercent(v) { windPct = clamp(v, 0, 100) }

var driftPct = 20         // tallest a drift may grow, % of the panel height
//# min=0 max=40 step=1 default=20
export function sliderDriftMaxPercent(v) { driftPct = clamp(v, 0, 40) }

// A mapless strip stays a strip: this export suppresses the default grid.
export function render(index) { hsv(0, 0, 0) }

var NF = 200
var fx = array(NF), fy = array(NF), fd = array(NF), fp = array(NF), lift = array(NF)
var MC = 128, H = array(MC)
var n = 0, t = 0, dt = 0, wind = 0, gustT = 0, gustLen = 1, gust = 0, panel = -1
var cols = 32, rows = 32, pw = 1 / 31, ph = 1 / 31, pile = 0

function spawn(i, anywhere) {
  fd[i] = random(1)
  fp[i] = random(PI2)
  lift[i] = 0
  if (panel) { fx[i] = random(1); fy[i] = anywhere ? random(1) : -0.03 - random(0.1) }
  else { fx[i] = anywhere ? random(pixelCount) : -random(4); fy[i] = 0 }
}

function setup() {
  var w = gridWidth(), h = gridHeight()
  cols = clamp(panel && w > 0 ? w : 32, 2, MC)
  rows = panel && h > 0 ? h : 32
  pw = 1 / max(cols - 1, 1)
  ph = 1 / max(rows - 1, 1)
  for (var c = 0; c < MC; c++) H[c] = driftPct / 100 * (0.25 + 0.15 * sin(c * 0.21 + 1) + 0.1 * hash(c))
  for (var i = 0; i < NF; i++) spawn(i, 1)
  pile = pixelCount * 0.02
}

export function beforeRender(delta) {
  dt = min(delta, 100) / 1000
  t = mod(t + dt, 3600)
  var p = has2DMap() ? 1 : 0
  if (p != panel || (p && gridWidth() != cols && gridWidth() > 0 && gridWidth() <= MC)) { panel = p; setup() }
  var wp = windPct / 100
  n = panel ? clamp(floor((pixelCount / 1000) * snowPct * 0.5), 12, NF)
            : clamp(floor((pixelCount / 100) * snowPct / 8), 3, 40)
  if (gustT <= 0 && random(1) < dt * (0.02 + 0.12 * wp)) { gustLen = 2 + random(2.5); gustT = gustLen }
  gust = 0
  if (gustT > 0) { gustT -= dt; gust = (0.1 + 0.5 * wp) * sin(PI * clamp(1 - gustT / gustLen, 0, 1)) }
  wind = (0.01 + 0.1 * wp) * (0.7 + 0.3 * simplex2(t * 0.1, 4)) + gust
  if (panel) stepPanel()
  else stepStrip()
}

function stepPanel() {
  var maxH = driftPct / 100, slope = ph * 0.6
  for (var i = 0; i < n; i++) {
    var d = fd[i]
    var vx = wind * (0.4 + 0.6 * d) + 0.05 * (0.3 + d) * sin(t * (0.8 + d) + fp[i])
    var vy = 0.035 + 0.13 * d
    if (lift[i] > 0) { lift[i] -= dt; vx = wind * 1.8; vy = -0.08 - gust * 0.2 }
    fx[i] = mod(fx[i] + vx * dt, 1)
    fy[i] += vy * dt
    var c = floor(fx[i] / pw + 0.5) % cols
    if (fy[i] >= 1 - H[c] && lift[i] <= 0) {
      if (d > 0.3 && maxH > 0) H[c] = min(H[c] + ph * d * (0.4 + 1.6 * max(0, sin(c * pw * 6.5 + 1.3))), maxH * 1.15)   // drifts favour the hollows
      spawn(i, 0)
    }
  }
  for (c = 0; c < cols - 1; c++) {             // slump to the angle of repose
    if (c > 0) H[c] = H[c] * 0.96 + (H[c - 1] + H[c + 1]) * 0.02   // wind smooths the crest
    var df = H[c] - H[c + 1]
    if (df > slope) { df = (df - slope) * 0.5; H[c] -= df; H[c + 1] += df }
    else if (df < -slope) { df = (-df - slope) * 0.5; H[c] += df; H[c + 1] -= df }
  }
  for (c = 0; c < cols; c++) {
    var floorH = ph * (1.5 + 0.8 * hash(c * 0.37))
    H[c] = max(min(floorH, maxH + ph * 1.5), H[c] - dt * (0.0004 + H[c] * 0.004) - (H[c] > maxH ? dt * 0.02 : 0))
  }
  if (gust > 0.12) {                           // powder lifted off the crests
    for (var k = 0; k < 2; k++) {
      if (random(1) > gust * dt * 30) continue
      var cc = floor(random(cols)), j = floor(random(n))
      if (H[cc] < ph * 3) continue
      H[cc] -= ph * 0.25
      fx[j] = cc * pw; fy[j] = 1 - H[cc]; fd[j] = 0.25 + random(0.3); lift[j] = 0.25 + random(0.4)
    }
  }
}

function stepStrip() {
  var px = pixelCount
  for (var i = 0; i < n; i++) {
    var d = fd[i]
    var v = px * (0.025 + 0.06 * d) * (0.65 + 0.35 * sin(t * (1.1 + d) + fp[i]))
    if (gust > 0.1) v = v * (1 - 1.6 * gust * (0.5 + 0.5 * sin(t * 3 + fp[i])))
    fx[i] += v * dt
    if (fx[i] >= px - pile) { pile = min(pile + 0.25, px * 0.1); spawn(i, 0) }
    else if (fx[i] < -6) spawn(i, 0)
  }
  pile = max(px * 0.01, pile - dt * pile * 0.012)
}

function drawPanel() {
  fillGradient(0.65, 0.85, 0.035, 0.62, 0.55, 0.13, 2)
  for (var i = 0; i < n; i++) {
    var d = fd[i], v = 0.22 + 0.72 * d
    hsv(0.6, 0.18 - 0.12 * d, v)
    splat(fx[i], fy[i], pw * (0.75 + 1.3 * d), 2)
  }
  for (var c = 0; c < cols; c++) {
    var x = c * pw, top = 1 - H[c], r = top / ph, r0 = floor(r)
    hsv(0.6, 0.35, 0.3)                        // drift body, shaded blue, lit near the crest
    fillRect(x - pw * 0.45, (r0 + 3.5) * ph, x + pw * 0.45, 1)
    hsv(0.6, 0.25, 0.5)
    fillRect(x - pw * 0.45, (r0 + 1.5) * ph, x + pw * 0.45, (r0 + 3.4) * ph)
    hsv(0.58, 0.12, 0.85)                      // the crest, anti-aliased
    setPixel(gridIndex(c, r0 + 1))
    hsv(0.58, 0.12, 0.85 * (1 - (r - r0)))
    setPixel(gridIndex(c, r0))
  }
}

function drawStrip() {
  var px = pixelCount
  gradientRange(0, px, 0.65, 0.85, 0.04, 0.6, 0.55, 0.12, 0)
  for (var i = 0; i < n; i++) {
    var d = fd[i]
    hsv(0.6, 0.15, 0.3 + 0.7 * d)
    splatIndex(fx[i], 0.8 + 1.4 * d, 2)
  }
  hsv(0.58, 0.15, 0.8)
  fillRangeAA(px - pile, px, 0)
}

export function renderBulk() {
  if (panel) drawPanel()
  else drawStrip()
}
