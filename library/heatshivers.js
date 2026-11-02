// name: Embers
// tags: both, nature, ambient
// Clean-room reimplementation from the prose specs docs/pattern-specs/heatshivers.md, fireblobs.md (plus the smear idea of the curated Ember Diffusion example) — reimagined, not a replica.
// Strip: ember pulses swell, drift and shiver along the strip, leaving a heat
// trail that cools through a blackbody ramp; Smear lets the heat creep along
// the strip like coals. One capped heat array (≤ 1,024 cells), no frame-sized
// buffers. Matrix: embers rise off a flickering coal bed as max-blended splats on
// the persistent frame, which fades for the afterglow (Smear diffuses it).

var pal = array(24)
setPalette(pal)
function stop(k, pos, r, g, b) {
  pal[k * 4] = pos
  pal[k * 4 + 1] = r
  pal[k * 4 + 2] = g
  pal[k * 4 + 3] = b
}
stop(0, 0, 0, 0, 0)
stop(1, 0.18, 0.3, 0.01, 0)
stop(2, 0.42, 0.85, 0.06, 0)
stop(3, 0.66, 1, 0.33, 0)
stop(4, 0.86, 1, 0.66, 0.1)
stop(5, 1, 1, 0.93, 0.55)

var rate = 4
// Pulses per second for every 60 px of strip (every 24 columns of a matrix).
//# min=0.5 max=20 step=0.5 default=4
export function sliderPulsesPerSecond(v) { rate = clamp(v, 0.5, 20) }

var drift = 0.3
// How far a pulse slides over its life, as a percentage of the strip (sideways wander on a matrix).
//# min=0 max=100 step=1 default=30
export function sliderDriftPct(v) { drift = clamp(v, 0, 100) / 100 }

var shiver = 0.4
// The per-frame tremble of position and heat.
//# min=0 max=100 step=1 default=40
export function sliderShiverPct(v) { shiver = clamp(v, 0, 100) / 100 }

var glow = 1.5
// Half-life of the heat left behind.
//# min=0.2 max=5 step=0.1 default=1.5
export function sliderAfterglowSeconds(v) { glow = clamp(v, 0.2, 5) }

var style = 0
// 0 pulses | 1 smear (the heat creeps outward like coals)
//# min=0 max=1 step=1 default=0
export function sliderStyle(v) { style = clamp(floor(v), 0, 1) }

var NP = 64
var ex = array(NP), ey = array(NP), ev = array(NP), ea = array(NP), el = array(NP), ep = array(NP)
var spawnAcc = 0, t = 0, smearAcc = 0, decay = 1, gw = 0

// ---- strip ----
var CAPS = 1024
var hc = array(CAPS)
var sl = 1

function spawnStrip(k) {
  var dir = random(1) < 0.5 ? -1 : 1
  el[k] = style ? 0.6 + random(0.6) : 1.6 + random(1.4)    // life, s
  ex[k] = random(sl)
  ev[k] = dir * drift * sl / el[k]                        // cells per second
  ea[k] = 0
  ep[k] = 0
  ey[k] = ex[k]
}

function stripStep(dt) {
  sl = clamp(pixelCount, 1, CAPS)
  feedback(hc, decay)
  if (style) {
    smearAcc += dt
    while (smearAcc > 0.04) {
      smearAcc -= 0.04
      blur1D(hc, 1)
    }
  }
  spawnAcc = min(spawnAcc + rate * dt * max(sl / 60, 0.5), 3)
  var half = style ? 1.5 : 2.6                             // px
  for (var k = 0; k < NP; k++) {
    if (el[k] <= 0) {
      if (spawnAcc >= 1) {
        spawnAcc -= 1
        spawnStrip(k)
      }
      continue
    }
    ea[k] += dt
    var f = ea[k] / el[k]
    if (f >= 1) { el[k] = 0; continue }
    ex[k] += ev[k] * dt
    var env = 1 - abs(2 * f - 1)                          // swell, then fade
    ep[k] = env * (1 - shiver * 0.35 * random(1))         // this frame's heat
    ey[k] = ex[k] + (random(2) - 1) * shiver * 1.2        // this frame's position
    // the heat it leaves behind starts orange and cools to red
    var lo = max(ceil(ey[k] - half), 0), hi = min(floor(ey[k] + half), sl - 1)
    for (var i = lo; i <= hi; i++) {
      var v = ep[k] * 0.6 * (1 - abs(i - ey[k]) / half)
      if (v > hc[i]) hc[i] = v
    }
  }
  hw = half
}

var hw = 2.6
function renderStrip() {
  var n = pixelCount
  var sc = sl / n, ps = n / sl
  for (var i = 0; i < n; i++) {
    paint(min(hc[floor(i * sc)], 0.999))
    setPixel(i)
  }
  // the pulses themselves, over their trail: the hot core goes yellow-white
  for (var k = 0; k < NP; k++) {
    if (el[k] <= 0) continue
    var lo = max(ceil(ey[k] - hw), 0), hi = min(floor(ey[k] + hw), sl - 1)
    for (var c = lo; c <= hi; c++) {
      paint(min(hc[c] + ep[k] * (1 - abs(c - ey[k]) / hw), 0.999))
      fillRange(floor(c * ps), floor((c + 1) * ps))
    }
  }
}

// ---- matrix ----
function spawnRise(k) {
  el[k] = 1.6 + random(1.6)
  ex[k] = random(1)
  ey[k] = 1 - random(0.06)
  ev[k] = (style ? 0.08 : 0.18) + random(0.18)            // rise, panels per second
  ea[k] = 0
  ep[k] = random(6.28)
}

function riseStep(dt) {
  setBlur(style ? 0.45 : 0)                              // compounds on the persistent frame: diffusion
  spawnAcc = min(spawnAcc + rate * dt * max(gw, 16) / 24, 4)
  for (var k = 0; k < NP; k++) {
    if (el[k] <= 0) {
      if (spawnAcc >= 1) {
        spawnAcc -= 1
        spawnRise(k)
      }
      continue
    }
    ea[k] += dt
    if (ea[k] >= el[k] || ey[k] < -0.05) { el[k] = 0; continue }
    ey[k] -= ev[k] * dt
    ex[k] += drift * 0.25 * sin(ep[k] + t * 1.7) * dt
  }
}

function renderRise() {
  fade(decay)
  var r = max(1.6 / max(gw, 16), 0.012)
  var jit = shiver / max(gw, 16)
  // the coal bed: overlapping glows along the bottom edge, flickering
  for (var b = 0; b < 10; b++) {
    var fl = 0.5 + 0.5 * simplex2(b * 0.9, t * (1 + shiver * 2), 4)
    paint(0.2 + 0.3 * fl, 0.5 + 0.5 * fl)
    splat((b + 0.5) / 10, 1.03, 0.14, 2)
  }
  for (var k = 0; k < NP; k++) {
    if (el[k] <= 0) continue
    var f = ea[k] / el[k]
    var heat = (1 - f) * (1 - shiver * 0.3 * random(1))
    paint(0.35 + 0.42 * heat, min(1, f * 8))
    splat(ex[k] + (random(2) - 1) * jit, ey[k], r, 2)
  }
}

var warmed = 0
export function beforeRender(delta) {
  if (!warmed) {                                        // start mid-burn, never on a dark frame
    warmed = 1
    for (var w = 0; w < 40; w++) step(0.05)
  }
  step(min(delta, 100) / 1000)
}

function step(dt) {
  t = mod(t + dt, 3600)
  if (has2DMap()) {
    decay = pow(0.5, dt * 5 / glow)                     // trails: a fifth of the strip's afterglow
    gw = gridWidth()
    riseStep(dt)
  } else {
    decay = pow(0.5, dt / glow)
    setBlur(0)
    stripStep(dt)
  }
}

export function renderBulk() {
  if (has2DMap()) renderRise()
  else renderStrip()
}

// declares the strip form (no default grid on a mapless strip); renderBulk wins
export function render(index) { paint(0.5) }
