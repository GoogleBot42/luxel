// name: Fire
// Clean-room reimplementation from the prose specs docs/pattern-specs/perlin-fire.md, perlin-fire-wind.md, fire-red.md, fire-blue.md, sparkfire.md, newfire.md — reimagined, not a replica.
// Panel: a full-width hearth rooted at the bottom edge. Two simplex octaves are
// filled a row at a time — each row sheared by the gusting wind, so tips lean
// further than the base — over slowly breathing per-column flame heights, on a
// canvas of at most 32 columns painted through a heat palette; sparks ride the
// updraft. Strip: a heat sim on at most 120 cells, interpolated onto the pixels —
// classic convection or long advected licks, fed by random heat or launched sparks.

var pal = array(24)            // 6 stops, rewritten in place (setPalette keeps a live reference)
var rgbT = array(3)
setPalette(pal)

var palMode = 0
// 0 classic | 1 blue gas | 2 green | 3 the picker's hue
//# min=0 max=3 step=1 default=0
export function sliderPalette(v) { palMode = clamp(floor(v), 0, 3); buildPalette() }

// Palette 3's hue. The picker's S and V are ignored — global brightness owns V.
var pickH = 0.8
export function hsvPickerCustomColor(h, s, v) { pickH = h; buildPalette() }

var flameH = 0.7
// How far the flames reach, as a percentage of the panel height / strip length.
//# min=20 max=100 step=1 default=70
export function sliderFlameHeightPct(v) { flameH = clamp(v, 20, 100) / 100 }

var wind = 0
// Lean of the flame tips (panel only); negative blows left. It gusts.
//# min=-100 max=100 step=1 default=0
export function sliderWindPct(v) { wind = clamp(v, -100, 100) / 100 }

var sparking = 0.5
//# min=0 max=100 step=1 default=50
export function sliderSparkingPct(v) { sparking = clamp(v, 0, 100) / 100 }

// Strip only. 0 from the start | 1 from the end | 2 from both ends | 3 from the middle
var dirMode = 0
//# min=0 max=3 step=1 default=0
export function sliderDirection(v) { dirMode = clamp(floor(v), 0, 3) }

// Strip only. 0 random heat at the base | 1 sparks launched up the strip
var ignition = 0
//# min=0 max=1 step=1 default=0
export function sliderIgnition(v) { ignition = clamp(floor(v), 0, 1) }

// Strip only. 0 classic convection | 1 long advected licks
var style = 0
//# min=0 max=1 step=1 default=0
export function sliderStyle(v) { style = clamp(floor(v), 0, 1) }

function stop(k, pos, r, g, b) {
  pal[k * 4] = pos
  pal[k * 4 + 1] = r
  pal[k * 4 + 2] = g
  pal[k * 4 + 3] = b
}

function stopHsv(k, pos, h, s, v) {
  hsv2rgb(h, s, v, rgbT)
  stop(k, pos, rgbT[0], rgbT[1], rgbT[2])
}

function buildPalette() {
  stop(0, 0, 0, 0, 0)
  if (palMode == 1) {
    stop(1, 0.16, 0, 0.01, 0.16)
    stop(2, 0.4, 0.03, 0.12, 0.85)
    stop(3, 0.62, 0.08, 0.45, 1)
    stop(4, 0.82, 0.45, 0.85, 1)
    stop(5, 1, 0.92, 1, 1)
  } else if (palMode == 2) {
    stop(1, 0.16, 0, 0.12, 0.01)
    stop(2, 0.4, 0.06, 0.65, 0.03)
    stop(3, 0.62, 0.4, 1, 0.06)
    stop(4, 0.82, 0.78, 1, 0.35)
    stop(5, 1, 1, 1, 0.9)
  } else if (palMode == 3) {
    stopHsv(1, 0.16, pickH, 1, 0.18)
    stopHsv(2, 0.4, pickH, 1, 0.85)
    stopHsv(3, 0.62, pickH + 0.04, 0.8, 1)
    stopHsv(4, 0.82, pickH + 0.08, 0.45, 1)
    stopHsv(5, 1, pickH, 0.08, 1)
  } else {
    stop(1, 0.16, 0.22, 0.015, 0)
    stop(2, 0.4, 1, 0.09, 0)
    stop(3, 0.62, 1, 0.4, 0.01)
    stop(4, 0.82, 1, 0.78, 0.12)
    stop(5, 1, 1, 1, 0.85)
  }
}
buildPalette()

var t = 0
var gust = 0

// ---- panel: canvas of at most 32 columns, an integer fraction of the grid ----
var CW = 32, CH = 64
var cv = array(CW * CH)       // heat, 0..0.999
var ra = array(CW)            // octave 1 row
var rb = array(CW)            // octave 2 row
var inv = array(CW)           // per-column 1 / (reach in rows)
var cw = 32, ch = 32, gw = 0
var oy = 0, z = 0

var NS = 24                   // panel sparks
var px = array(NS), py = array(NS), pvx = array(NS), pvy = array(NS), pl = array(NS)
var spawnAcc = 0

function panelStep(dt) {
  gw = gridWidth()
  var gh = gridHeight()
  if (gw > 0) {
    var k = ceil(gw / CW)
    cw = ceil(gw / k)
    ch = clamp(ceil(gh / k), 2, CH)
  } else {
    cw = 32
    ch = 32
  }
  // the noise is stretched 2:1 vertically and rises about one panel a second
  var sx = 4.5 / cw, sy = sx * 0.5
  oy = mod(oy + ch * sy * 0.95 * dt, 2048)
  z = mod(z + 0.55 * dt, 2048)
  var lean = wind * (0.55 + 0.45 * gust) + 0.07 * sin(t * 0.7) + 0.035 * sin(t * 1.9)
  fillNoise2D(inv, cw, 1, 3 / cw, 0, t * 0.3, 0, 7)
  var rows = ch - 1
  for (var c = 0; c < cw; c++) inv[c] = 1 / max(rows * flameH * (0.74 + 0.36 * inv[c]), 1)
  for (var r = 0; r < ch; r++) {
    var hb = (rows - r) / rows                 // 0 at the bottom row, 1 at the top
    var sh = lean * hb * hb * cw * 0.4         // shear, in canvas cells
    var si = round(sh)
    var amp = 0.3 + 0.3 * hb
    fillNoise3D(ra, cw, 1, sx, 0, -sh * sx, r * sy + oy, z, 3)
    fillNoise3D(rb, cw, 1, sx * 2.3, 0, -sh * sx * 2.3, (r * sy + oy) * 2.6, z * 1.7, 11)
    var up = (rows - r) * 0.74
    var base = r * cw
    for (var c2 = 0; c2 < cw; c2++) {
      var h = 0.88 - up * inv[clamp(c2 - si, 0, cw - 1)] + (ra[c2] + 0.5 * rb[c2]) * amp
      cv[base + c2] = clamp(h, 0, 0.999)
    }
  }
  // sparks pop out of the flame body and ride the updraft
  spawnAcc += sparking * 18 * dt * cw / 32
  for (var i = 0; i < NS; i++) {
    if (pl[i] > 0) {
      pl[i] -= dt * 0.75
      py[i] += pvy[i] * dt
      px[i] += (pvx[i] + lean * 0.35 + 0.08 * sin(t * 5 + i)) * dt
      if (py[i] < -0.05) pl[i] = 0
    } else if (spawnAcc >= 1) {
      spawnAcc -= 1
      pl[i] = 0.6 + random(0.4)
      px[i] = 0.1 + random(0.8)
      py[i] = 1 - flameH * (0.25 + random(0.45))
      pvx[i] = random(0.16) - 0.08
      pvy[i] = -(0.3 + random(0.4))
    }
  }
  spawnAcc = min(spawnAcc, 2)
}

function renderPanel() {
  paintCanvas(cv, cw, ch, 1, gw > 0 && cw < gw ? 1 : 0)   // bilinear upscale
  var r = max(1.2 / max(gw, 32), 0.01)
  for (var i = 0; i < NS; i++) {
    if (pl[i] > 0) {
      paint(0.5 + 0.32 * pl[i], min(1, pl[i] * 3))
      splat(px[i], py[i], r, 1)
    }
  }
}

// ---- strip: heat sim on at most 120 cells along the flame ----
var SC = 120
var sh = array(SC)
var sl = 2, along = 2
var acc = 0, warm = 0
var NK = 6                    // launched sparks
var kp = array(NK), kv = array(NK)

function stripStep(delta) {
  along = dirMode >= 2 ? ceil(pixelCount / 2) : pixelCount
  sl = clamp(along, 2, SC)
  var tick = clamp(1800 / sl, 12, 40)        // flames rise at a length-independent pace
  if (!warm) {                                 // start mid-burn, never on a dark strip
    warm = 1
    for (var w = 0; w < 80; w++) simStep()
  }
  acc = min(acc + delta, 160)
  while (acc >= tick) {
    acc -= tick
    simStep()
  }
}

function simStep() {
  var i
  if (style == 1) {
    // licks: every cell pulls from one or two cells nearer the root, minus a random cooling
    var coolL = 2.6 / (flameH * sl)
    for (i = sl - 1; i >= 1; i--) sh[i] = max(0, sh[max(i - 1 - floor(random(2)), 0)] - random(coolL))
    sh[0] = 0.75 + random(0.25)
  } else {
    // classic: random cooling, then each cell averages the two below it (the farther counted twice)
    var cool = 2 / (flameH * sl)
    for (i = 0; i < sl; i++) sh[i] = max(0, sh[i] - random(cool))
    for (i = sl - 1; i >= 2; i--) sh[i] = (sh[i - 1] + 2 * sh[i - 2]) / 3
  }
  if (ignition == 0) {
    if (random(1) < 0.15 + 0.85 * sparking) {
      var p = floor(random(max(2, sl * 0.12)))
      sh[p] = min(1, sh[p] + 0.45 + random(0.55))
    }
  } else {
    // sparks launch from the root, accelerate and leave heat along their path
    sh[0] = max(sh[0], 0.35 + random(0.3))
    for (var k = 0; k < NK; k++) {
      if (kv[k] <= 0) {
        if (random(1) < sparking * 0.05) {
          kp[k] = 0
          kv[k] = 0.4 + random(0.5)
        }
        continue
      }
      var from = floor(kp[k])
      kp[k] += kv[k]
      kv[k] += 0.06
      var to = min(floor(kp[k]), sl - 1)
      var dep = 0.15 * (1 - kp[k] / (sl * flameH * 0.85))
      if (dep <= 0 || kp[k] >= sl) { kv[k] = 0; continue }
      for (i = from; i <= to; i++) sh[i] = min(1, sh[i] + dep)
    }
  }
}

function renderStrip() {
  var n = pixelCount
  var sc = (sl - 1) / max(along - 1, 1)
  var mid = (n - 1) / 2
  for (var i = 0; i < n; i++) {
    var d = i
    if (dirMode == 1) d = n - 1 - i
    else if (dirMode == 2) d = min(i, n - 1 - i)
    else if (dirMode == 3) d = abs(i - mid)
    var u = d * sc
    var k = floor(u)
    var h = sh[k] + (sh[min(k + 1, sl - 1)] - sh[k]) * (u - k)
    paint(min(h, 0.999))
    setPixel(i)
  }
}

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  t = mod(t + dt, 3600)
  gust = 0.5 + 0.5 * simplex2(t * 0.45, 0.5, 5)
  if (has2DMap()) panelStep(dt)
  else stripStep(delta)
}

export function renderBulk() {
  if (has2DMap()) renderPanel()
  else renderStrip()
}

// declares the strip form (no default grid on a mapless strip); renderBulk wins
export function render(index) { paint(0.5) }
