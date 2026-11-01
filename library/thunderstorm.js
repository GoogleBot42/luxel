// name: Thunderstorm
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/lightning-clouds.md,
// rainstorm.md, b-lightning-flashes.md — reimagined, not a replica.
// Real lightning: a faint stepped leader, a one-frame return stroke, dimmer
// restrikes down the same channel, then a long afterglow; strikes cluster in
// bursts, and distant sheet lightning lights the clouds with no bolt.
// Panel: a drifting fillNoise3D cloud deck painted through a live palette the
// flash brightens, a forked bolt of 24 drawLine segments (midpoint
// displacement), wind-slanted rain streaks, a hill line the bolt lands on.
// Strip: a gradientRange cloud band, the jagged channel as fillRangeAA runs,
// and rain glints. No per-pixel buffers.

var strikesPerMin = 20
//# min=1 max=60 step=1 default=20
export function sliderStrikesPerMinute(v) { strikesPerMin = clamp(v, 1, 60) }

var rainRate = 40
//# min=0 max=200 step=5 default=40
export function sliderRainDropsPerSecond(v) { rainRate = clamp(v, 0, 200) }

var windDeg = 10
//# min=-45 max=45 step=1 default=10
export function sliderWindDegrees(v) { windDeg = clamp(v, -45, 45) }

// How bright the clouds stay between strikes, in % of full.
var floorPct = 15
//# min=0 max=40 step=1 default=15
export function sliderCloudFloorPercent(v) { floorPct = clamp(v, 0, 40) }

// Bolt colour; the core always whitens toward it. The picker's brightness is ignored.
var boltH = 0.64, boltS = 0.2
export function hsvPickerBoltColor(h, s, v) { boltH = h; boltS = s }

// A mapless strip stays a strip: this export suppresses the default grid.
export function render(index) { hsv(0, 0, 0) }

// --- shared storm state machine: 0 dark, 1 leader, 2 strokes, 3 sheet flash
var state = 0, timer = 0.4, eventAge = 0, strikesLeft = 0, strokeN = 0, leaderT = 0.1
var hot = 0, glow = 0, skyHot = 0, skyGlow = 0, sheetRate = 0
var flashX = 0.5, flashY = 0.2, t = random(50), drift = 0, dt = 0, panel = 0
var NS = 24, nSeg = 0, nMain = 0
var sx0 = array(NS), sy0 = array(NS), sx1 = array(NS), sy1 = array(NS), sa = array(NS)
var ptx = array(17), pty = array(17)

function nextGap() {             // mean is exactly 60/strikesPerMin; half the gaps cluster
  var base = 60 / strikesPerMin
  if (random(1) < 0.45) return base * (0.05 + random(0.2))
  return base * (0.7 + random(2))
}

// --- panel geometry ---------------------------------------------------------
var CMAX = 32
var nA = array(CMAX * CMAX), nB = array(CMAX * CMAX), base = array(CMAX * CMAX)
var hz = array(128), gw = -1, gh = 0, cw = 32, ch = 32, pw = 1 / 31, ph = 1 / 31
var pal = array(20)
setPalette(pal)

function resize() {
  var w = gridWidth(), h = gridHeight()
  if (w == gw && h == gh) return
  gw = w; gh = h
  cw = w > 0 ? min(w, CMAX) : 32
  ch = h > 0 ? min(h, CMAX) : 32
  pw = 1 / max((w > 0 ? w : 32) - 1, 1)
  ph = 1 / max((h > 0 ? h : 32) - 1, 1)
  for (var r = 0; r < ch; r++) {               // a denser deck up top, thinner low down
    var y = r / max(ch - 1, 1)
    for (var c = 0; c < cw; c++) base[r * cw + c] = 0.6 - 0.24 * y
  }
  for (var i = 0; i < 128; i++) {              // the hill line, in y
    var x = i / 127
    hz[i] = 0.9 + 0.035 * sin(x * 6.3 + 1.2) + 0.018 * sin(x * 17 + 4)
  }
}
function horizon(x) { return hz[clamp(floor(x * 127 + 0.5), 0, 127)] }

// --- the bolt ---------------------------------------------------------------
function addSeg(a, b, c, d, amp) {
  if (nSeg >= NS) return
  sx0[nSeg] = a; sy0[nSeg] = b; sx1[nSeg] = c; sy1[nSeg] = d; sa[nSeg] = amp
  nSeg++
}

function buildBolt2D() {
  nSeg = 0
  var x0 = 0.15 + random(0.7), y0 = 0.1 + random(0.14)
  var x1 = clamp(x0 + random(0.5) - 0.25, 0.05, 0.95), y1 = horizon(x1) + 0.01
  ptx[0] = x0; pty[0] = y0; ptx[16] = x1; pty[16] = y1
  var disp = 0.24 * (y1 - y0)
  for (var step = 16; step > 1; step = step / 2) {
    var hs = step / 2
    for (var i = hs; i < 16; i += step) {
      ptx[i] = (ptx[i - hs] + ptx[i + hs]) / 2 + (random(2) - 1) * disp
      pty[i] = (pty[i - hs] + pty[i + hs]) / 2 + (random(2) - 1) * disp * 0.25
    }
    disp = disp * 0.56
  }
  for (i = 0; i < 16; i++) addSeg(ptx[i], pty[i], ptx[i + 1], pty[i + 1], 0.85 + random(0.15))
  nMain = nSeg
  for (var b = 0; b < 2; b++) {                // two forks, 4 segments each, thinning out
    var k = 2 + floor(random(9)), bx = ptx[k], by = pty[k]
    var dir = random(1) < 0.5 ? -1 : 1, amp = 0.75
    for (var j = 0; j < 4; j++) {
      var nx = bx + dir * (0.03 + random(0.05)), ny = by + 0.03 + random(0.06)
      addSeg(bx, by, nx, ny, amp)
      bx = nx; by = ny; amp = amp * 0.72
    }
  }
  flashX = x0; flashY = y0
}

function buildBoltStrip() {      // a jagged run of the strip plus dim stubs past its ends
  nSeg = 0
  var span = max(3, floor(pixelCount * (0.45 + random(0.55))))
  span = min(span, pixelCount)
  var a = floor(random(pixelCount - span + 1)), K = clamp(floor(span / 3), 2, 20)
  var c = 0.7 + random(0.3)
  for (var k = 0; k < K; k++) {
    c = clamp(c + random(0.36) - 0.18, 0.35, 1)
    var v = random(1) < 0.08 ? c * (0.2 + random(0.3)) : c   // dark kinks
    addSeg(a + k * span / K, 0, a + (k + 1) * span / K, 0, v)
  }
  nMain = nSeg
  if (random(1) < 0.6) addSeg(a - 2 - random(4), 0, a, 0, 0.25 + random(0.2))
  if (random(1) < 0.6) addSeg(a + span, 0, a + span + 2 + random(4), 0, 0.25 + random(0.2))
  flashX = (a + span / 2) / pixelCount
}

function stroke(amp) {
  for (var i = 0; i < nSeg; i++) sa[i] = clamp(sa[i] * (0.88 + random(0.24)), 0.15, 1)
  hot = max(hot, amp)
  glow = min(0.3, glow + 0.14 * amp)
  skyHot = min(1, skyHot + 0.7 * amp)
  skyGlow = min(0.35, skyGlow + 0.12 * amp)
}

function beginEvent() {
  eventAge = 0
  if (random(1) < 0.22) {        // sheet lightning: a soft cloud-wide glow, no bolt
    state = 3
    timer = 0.06 + random(0.08)
    sheetRate = (0.35 + random(0.4)) / timer
    flashX = 0.1 + random(0.8); flashY = 0.08 + random(0.25)
    return
  }
  if (panel) buildBolt2D()
  else buildBoltStrip()
  leaderT = 0.05 + random(0.09)
  timer = leaderT
  strokeN = 0
  strikesLeft = random(1) < 0.78 ? 1 + floor(random(3)) : 0
  state = 1
}

function endEvent() {
  state = 0
  timer = max(0.03, nextGap() - eventAge)
}

export function beforeRender(delta) {
  dt = min(delta, 200) / 1000
  panel = has2DMap()
  if (panel) resize()
  t = mod(t + dt, 3600)
  drift += dt * (0.06 + 0.25 * sin(windDeg * PI / 180))
  if (drift > 1000) drift -= 1000
  hot = hot * exp(-dt / 0.035)
  glow = glow * exp(-dt / 0.45)
  skyHot = skyHot * exp(-dt / 0.07)
  skyGlow = skyGlow * exp(-dt / 0.6)
  timer -= dt
  if (state > 0) eventAge += dt
  if (state == 0) {
    if (timer <= 0) beginEvent()
  } else if (state == 1) {
    if (timer <= 0) {
      stroke(1)                  // the return stroke: full scale, inside one frame
      state = 2
      timer = strikesLeft > 0 ? 0.05 + random(0.25) : 0.15 + random(0.2)
    }
  } else if (state == 2) {
    if (timer <= 0) {
      if (strikesLeft > 0) {
        strikesLeft -= 1; strokeN += 1
        stroke(min(1, pow(0.62, strokeN) * (0.8 + random(0.3))))
        timer = strikesLeft > 0 ? 0.05 + random(0.25) : 0.15 + random(0.2)
      } else endEvent()
    }
  } else {
    skyHot = min(1, skyHot + sheetRate * dt)
    if (timer <= 0) { skyGlow = min(0.35, skyGlow + skyHot * 0.3); endEvent() }
  }
  stepRain()
}

// --- rain: a pool of drops (panel streaks, strip glints) ---------------------
var NR = 128, rx = array(NR), ry = array(NR), rv = array(NR), rl = array(NR), rainAcc = 0
for (var q = 0; q < NR; q++) rl[q] = -1

function stepRain() {
  var wr = windDeg * PI / 180, vx = sin(wr), vy = cos(wr)
  rainAcc = min(rainAcc + rainRate * dt, 8)
  for (var i = 0; i < NR; i++) {
    if (rl[i] < 0) {
      if (rainAcc < 1) continue
      rainAcc -= 1
      rl[i] = 1
      rv[i] = 0.6 + random(0.8)                 // depth: far drops slow and dim
      if (panel) { rx[i] = random(1.6) - 0.3 - vx * 0.4; ry[i] = -0.1 - random(0.15) }
      else { rx[i] = random(pixelCount); ry[i] = 0.18 + random(0.15) }
      continue
    }
    if (panel) {
      var s = 1.1 * rv[i] * dt
      rx[i] += vx * s; ry[i] += vy * s
      if (rl[i] == 1 && ry[i] >= horizon(clamp(rx[i], 0, 1))) rl[i] = 0.08   // splash
      else if (rl[i] < 1) rl[i] -= dt
      if (rl[i] <= 0) rl[i] = -1
    } else {
      rx[i] += rv[i] * dt * 24
      ry[i] -= dt
      if (ry[i] <= 0 || rx[i] >= pixelCount) rl[i] = -1
    }
  }
}

function boltColor(v) {
  var h = clamp(v / 0.6, 0, 1)
  hsv(boltH, mix(min(1, boltS + 0.35), boltS, h), min(v, 1))
}

// --- panel ------------------------------------------------------------------
function cloudStop(i, pos, lvl, lit) {
  pal[i * 4] = pos
  pal[i * 4 + 1] = min(1, 0.32 * lvl + 0.62 * lit)
  pal[i * 4 + 2] = min(1, 0.38 * lvl + 0.68 * lit)
  pal[i * 4 + 3] = min(1, 0.6 * lvl + 0.9 * lit)
}

function drawPanel() {
  var fl = floorPct / 100, sky = min(1, skyHot + skyGlow)
  fillNoise3D(nA, cw, ch, 3.2 / cw, 3.2 / ch, drift, 0, t * 0.07, 7)
  fillNoise3D(nB, cw, ch, 7.5 / cw, 7.5 / ch, drift * 1.6, 3, t * 0.13, 8)
  arrayScale(nB, 0.45)
  arrayAdd(nA, nB)
  arrayScale(nA, 0.26)
  arrayAdd(nA, base)
  cloudStop(0, 0, fl * 0.15, sky * 0.06)       // gaps between clouds
  cloudStop(1, 0.38, fl * 0.6, sky * 0.18)
  cloudStop(2, 0.55, fl * 1.3, sky * 0.5)
  cloudStop(3, 0.72, fl * 2.1, sky * 0.85)     // thick cloud lights hardest
  cloudStop(4, 1, fl * 2.8, sky)
  paintCanvas(nA, cw, ch, 1, 1)

  if (sky > 0.02) {                            // the flash bleeding through the deck
    rgb(0.35 * sky, 0.38 * sky, 0.55 * sky)
    splat(flashX, flashY, 0.45, 1)
  }

  var wr = windDeg * PI / 180, vx = sin(wr), vy = cos(wr), lit = 0.6 + sky * 1.6
  for (var i = 0; i < NR; i++) {
    if (rl[i] < 0) continue
    var b = (0.06 + 0.1 * rv[i]) * lit
    rgb(b * 0.6, b * 0.75, b)
    if (rl[i] < 1) splat(rx[i], ry[i] - ph * 0.5, pw * 1.6, 1)
    else {
      var len = 0.05 + 0.05 * rv[i]
      drawLine(rx[i], ry[i], rx[i] - vx * len, ry[i] - vy * len, pw * 0.7, 1)
    }
  }

  var amp = hot + glow
  if (state == 1) {                            // the stepped leader creeps down
    var f = clamp(1 - timer / leaderT, 0, 1), front = floor(f * nMain)
    for (i = 0; i < front; i++) {
      boltColor((0.06 + random(0.12)) * (random(1) < 0.3 ? 0.3 : 1))
      drawLine(sx0[i], sy0[i], sx1[i], sy1[i], pw * 1.1, 2)
    }
  } else if (amp > 0.015) {
    if (hot > 0.1) for (i = 0; i < nSeg; i++) {   // glow pass while hot, then the core
      boltColor(hot * sa[i] * 0.3)
      drawLine(sx0[i], sy0[i], sx1[i], sy1[i], pw * 3, 1)
    }
    for (i = 0; i < nSeg; i++) {
      if (amp * sa[i] < 0.015) continue
      boltColor(amp * sa[i])
      drawLine(sx0[i], sy0[i], sx1[i], sy1[i], pw * (i < nMain ? 1.2 : 0.9), 2)
    }
  }

  var gl = 0.012 + 0.1 * sky                   // hills, lit by the flash
  rgb(gl * 0.35, gl * 0.45, gl * 0.6)
  var cols = gw > 0 ? gw : 32, st = ceil(cols / 48)   // ≤ 48 rects, stepped on big panels
  for (var c = 0; c < cols; c += st) {
    var x = c * pw
    fillRect(x - pw * 0.45, horizon(x + pw * (st - 1) / 2), x + pw * (st - 0.55), 1)
  }
}

// --- strip ------------------------------------------------------------------
function drawStrip() {
  var fl = floorPct / 100, sky = min(1, skyHot + skyGlow), K = 6
  var n = ceil(pixelCount / K), prev = 0
  for (var k = 0; k <= n; k++) {
    var d = clamp(0.5 + 0.6 * simplex2(k * 0.28 + drift * 4, t * 0.08, 3), 0, 1)
    var v = fl * (0.25 + 0.95 * d) + sky * (0.25 + 0.75 * d) * (1 - 0.5 * abs(k / n - flashX))
    if (k > 0) gradientRange((k - 1) * K, k * K, 0.62, 0.55 - 0.35 * sky, prev, 0.62, 0.55 - 0.35 * sky, v, 0)
    prev = v
  }
  for (var i = 0; i < NR; i++) {
    if (rl[i] < 0) continue
    var b = min(ry[i] * 3, 1) * (0.15 + 0.2 * rv[i]) * (1 + sky)
    hsv(0.6, 0.45, b)
    fillRangeAA(rx[i] - 1.5, rx[i] + 0.5, 2)
  }
  var amp = hot + glow
  if (state == 1) {
    var f = clamp(1 - timer / leaderT, 0, 1), front = floor(f * nMain)
    for (i = max(0, front - 2); i < front; i++) {
      boltColor((0.05 + random(0.12)) * (random(1) < 0.35 ? 0.3 : 1))
      fillRangeAA(sx0[i], sx1[i], 2)
    }
  } else if (amp > 0.01) {
    for (i = 0; i < nSeg; i++) {
      boltColor(amp * sa[i])
      fillRangeAA(sx0[i], sx1[i], 2)
    }
  }
}

export function renderBulk() {
  if (panel) drawPanel()
  else drawStrip()
}
