// name: Ocean
// Clean-room reimplementation from the prose specs docs/pattern-specs/oasis.md,
// green-ripple-reflections.md and docs/pattern-specs/patternflow/midsummer-sea.md (Patternflow
// preset, CC BY-SA 4.0 — prose-described idea only, no code) — reimagined, not a replica.
// Strip: open water (wave trains running both ways under a travelling set-swell, whitecaps
// where they crest together) or a shoreline (surf runs up the sand, leaves a wet band that
// dries, every seventh wave bigger). Panel: a seascape — sky, sun or moon, perspective swell
// bands, a swaying glittering sun path and surf on a beach — drawn in rows and strokes
// (fillRect per row, drawLine crests, splat glitter) through one colour ramp, so the cost
// follows the scene, not the pixel count.

// strip palettes, open water: deep -> crest -> foam
var OPEN = [
  [0, 0, 0.01, 0.07, 0.45, 0, 0.09, 0.32, 0.75, 0, 0.32, 0.46, 0.92, 0.25, 0.6, 0.58, 1, 0.9, 0.97, 0.92],
  [0, 0, 0.07, 0.2, 0.4, 0, 0.35, 0.55, 0.72, 0, 0.75, 0.7, 0.9, 0.5, 1, 0.85, 1, 1, 1, 1],
  [0, 0.01, 0.02, 0.03, 0.45, 0.07, 0.11, 0.13, 0.75, 0.2, 0.28, 0.3, 0.92, 0.5, 0.56, 0.56, 1, 0.95, 0.95, 0.95],
  [0, 0, 0.02, 0.02, 0.45, 0, 0.12, 0.08, 0.75, 0.04, 0.4, 0.25, 0.92, 0.4, 0.72, 0.56, 1, 0.95, 1, 0.95]
]
// strip palettes, shoreline: deep water, shallows, foam (0.5), wet sand, dry sand (1)
var SHORE = [
  [0, 0, 0.08, 0.25, 0.3, 0, 0.45, 0.55, 0.45, 0.4, 0.85, 0.8, 0.5, 1, 1, 1, 0.62, 0.35, 0.25, 0.1, 1, 0.95, 0.75, 0.45],
  [0, 0, 0.2, 0.4, 0.3, 0, 0.7, 0.7, 0.45, 0.5, 1, 0.9, 0.5, 1, 1, 1, 0.62, 0.45, 0.32, 0.12, 1, 1, 0.85, 0.55],
  [0, 0.02, 0.05, 0.07, 0.3, 0.12, 0.2, 0.22, 0.45, 0.45, 0.5, 0.5, 0.5, 0.9, 0.9, 0.9, 0.62, 0.2, 0.17, 0.12, 1, 0.55, 0.5, 0.4],
  [0, 0, 0.03, 0.06, 0.3, 0.05, 0.25, 0.3, 0.45, 0.4, 0.6, 0.65, 0.5, 0.85, 0.9, 0.95, 0.62, 0.08, 0.1, 0.12, 1, 0.35, 0.38, 0.42]
]
// panel ramps (sunset | tropical | stormy | moonlit), dark -> light, white at 1
var RAMP = [
  [0, 0, 0, 0, 0.2, 0.1, 0.04, 0.32, 0.4, 0.45, 0.1, 0.58, 0.6, 1, 0.35, 0.15, 0.8, 1, 0.72, 0.45, 1, 1, 1, 0.95],
  [0, 0, 0, 0, 0.15, 0, 0.08, 0.32, 0.35, 0, 0.35, 0.7, 0.5, 0, 0.72, 0.72, 0.65, 0.45, 0.75, 1, 0.8, 1, 0.85, 0.5, 1, 1, 1, 1],
  [0, 0, 0, 0, 0.2, 0.08, 0.1, 0.15, 0.45, 0.28, 0.33, 0.42, 0.65, 0.42, 0.5, 0.46, 0.8, 0.8, 0.58, 0.32, 1, 1, 1, 1],
  [0, 0, 0, 0, 0.2, 0.02, 0.05, 0.2, 0.45, 0.13, 0.28, 0.48, 0.7, 0.4, 0.58, 0.65, 0.85, 0.8, 0.86, 0.92, 1, 1, 1, 1]
]
// per ramp: sky top, sky at horizon, sea far, sea near, crest, path, sun halo, sand, clouds, disc, disc brightness
var LOOK = [
  [0.05, 0.62, 0.36, 0.22, 0.46, 0.8, 0.68, 0.7, 0.3, 0.93, 1],
  [0.62, 0.7, 0.33, 0.5, 0.58, 0.95, 0.85, 0.8, 0.75, 0.88, 1],
  [0.22, 0.5, 0.3, 0.22, 0.45, 0.72, 0.75, 0.62, 0.46, 0.92, 0.75],
  [0.08, 0.3, 0.22, 0.16, 0.4, 0.82, 0.6, 0.45, 0.22, 1, 1]
]

var mode = 0, swellS = 8, scalePct = 100, foamPct = 40, pal = 0, sunPct = 30
// Strip only: 0 open water, 1 shoreline.
//# min=0 max=1 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 1); dirty = 1 }
// Seconds between the big swells (and, on the panel and shoreline, between surf runs).
//# min=2 max=30 step=0.5 default=8
export function sliderSwellSeconds(v) { swellS = clamp(v, 2, 30) }
// Wave length, % of the standard.
//# min=25 max=400 step=5 default=100
export function sliderWaveScalePct(v) { scalePct = clamp(v, 25, 400) }
// Whitecaps, foam lace and glitter, %.
//# min=0 max=100 step=1 default=40
export function sliderFoamPct(v) { foamPct = clamp(v, 0, 100) }
// 0 night sea / sunset, 1 tropical, 2 stormy, 3 moonlit.
//# min=0 max=3 step=1 default=0
export function sliderPalette(v) { pal = clamp(floor(v), 0, 3); dirty = 1 }
// Panel only: sun (or moon) height, from sitting on the horizon to high in the sky, %.
//# min=0 max=100 step=1 default=30
export function sliderSunHeightPct(v) { sunPct = clamp(v, 0, 100) }

// Mapless strips stay strips: this export suppresses the default grid. Luxel runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var dirty = 1, panel = -1, clock = 0, t1 = 0, t2 = 0, t3 = 0, t4 = 0, tS = 0, tB = 0
var wet = 0, m = 0, edge = 0.4, prevK = 0, NR = 48, NC = 48, wetMax = 0, fdt = 0
var L = LOOK[0]

// Surf run-up over one swell (0..1): a quick surge, then a long slide back.
function surf(f) {
  return f < 0.3 ? easeOutCubic(f / 0.3) : 1 - easeInOutSine((f - 0.3) / 0.7)
}
function reach() {
  var n = floor(tS), big = n % 7 == 6 ? 1.6 : 0.8 + 0.35 * hash(n)
  return big * surf(tS - n)
}

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  fdt = dt
  clock = (clock + dt) % 3600
  var p = has2DMap() ? 1 : 0
  if (p != panel || dirty) {
    panel = p
    dirty = 0
    if (panel) {
      setPalette(RAMP[pal])
      L = LOOK[pal]
      NR = gridHeight() > 1 ? gridHeight() : 48
      NC = gridWidth() > 1 ? gridWidth() : 48
    } else {
      setPalette(mode ? SHORE[pal] : OPEN[pal])
      if (mode && !wet) {
        m = min(pixelCount, 1024)
        wet = array(m)
      }
    }
  }
  var k = 8 / swellS
  t1 = (t1 + dt * 0.32 * k) % 1
  t2 = (t2 + dt * 0.23 * k) % 1
  t3 = (t3 + dt * 0.18 * k) % 1
  t4 = (t4 + dt * 0.12 * k) % 1
  tS = (tS + dt / swellS) % 7000
  tB = (tB + dt * 0.35 * k) % 100
  if (!panel && mode) {
    // shoreline: the edge position and the sand it has wetted
    var e = 0.42 + 0.3 * reach() + 0.02 * simplex2(clock * 0.4, 3.3)
    var ek = floor(e * m), a = min(prevK, ek), b = min(max(prevK, ek), m - 1)
    feedback(wet, pow(0.5, dt / 2.5))
    for (var i = max(a, 0); i <= b; i++) wet[i] = 1
    prevK = ek
    edge = e
  }
}

function openWater() {
  var n = pixelCount, ku = 100 / (150 * scalePct), thr = 0.74 - foamPct * 0.0025
  for (var i = 0; i < n; i++) {
    var u = i * ku
    var a = wave(u * 3 + t1), b = wave(u * 5 - t2), c = wave(u * 8 + t3)
    var s = (0.32 * a * a + 0.26 * b + 0.2 * c * c + 0.22 * wave(u * 2 - t4))
    s *= 0.62 + 0.38 * wave(u * 0.45 - tS)   // the set-swell rolling through
    var f = saturate((s - thr) * 7)           // whitecaps where the trains crest together
    paint(saturate(s * 1.1 + f * 0.3), 0.3 + 0.7 * s + f)
    setPixel(i)
  }
}

function shoreline() {
  var n = pixelCount, sc = m / n, fw = 0.015 + foamPct * 0.0004
  var ph = tS - floor(tS), wl = ph > 0.55 ? (ph - 0.55) / 0.45 * 0.42 : -1  // next breaker coming in
  var tick = floor(clock * 6)
  for (var i = 0; i < n; i++) {
    var x = i / n
    if (x < edge) {
      var dep = edge - x, sh = saturate(1 - dep * 3)
      var t = 0.06 + 0.32 * sh + 0.05 * wave(x * 9 - clock * 0.6)
      var lace = saturate(1 - dep / fw) * (0.55 + 0.45 * hash2(i, tick))
      if (wl > 0) lace = max(lace, saturate(1 - abs(x - wl) * 40) * 0.8)
      paint(t + (0.5 - t) * lace, 0.55 + 0.45 * sh)
    } else {
      var w = wet[floor(i * sc)]
      var g = 0.92 + 0.08 * hash(i * 1.7)
      var fz = w > 0.8 && hash2(i, tick) < foamPct * 0.006 ? 0.5 : 0   // bubbles left behind
      paint(fz ? 0.5 : 0.62 + 0.38 * (1 - w), fz ? 0.7 : g * (0.75 + 0.25 * (1 - w)))
    }
    setPixel(i)
  }
}

// one panel row (r of NR) in the current brush
function row(r) {
  var y = r / (NR - 1), hh = 0.5 / (NR - 1)
  fillRect(0, y - hh, 1, y + hh)
}

function seascape() {
  var hz = 0.36, yb = 0.8, rp = max(1.5, 0.075 * min(NC, NR))
  var sx = 0.5, sy = hz - (sunPct / 100) * (hz - 0.08) + 0.01
  var rH = round(hz * (NR - 1)), rB = round(yb * (NR - 1))
  // sky
  for (var r = 0; r < rH; r++) {
    var q = r / rH
    paint(L[0] + (L[1] - L[0]) * q * q, 1)
    row(r)
  }
  var cl = pal == 2 ? 6 : 3
  for (var j = 0; j < cl; j++) {
    var cy = 0.04 + hz * 0.55 * hash(j + 0.3), cx = mod(hash(j + 9) + clock * 0.004 * (1 + j % 2), 1.6) - 0.3
    paint(L[8], 0.45 + 0.1 * (pal == 2))
    drawLine(cx, cy, cx + 0.45, cy + 0.06, 0.9 / NR, 2)
  }
  // sun or moon: halo, then the disc in pixel-round rows
  paint(L[6], pal == 2 ? 0.35 : 0.7)
  splat(sx, sy, rp * 4.5 / NC, 1)
  paint(L[9], L[10])
  for (var r2 = floor(sy * (NR - 1) - rp); r2 <= ceil(sy * (NR - 1) + rp); r2++) {
    var dy = r2 - sy * (NR - 1)
    if (abs(dy) < rp && r2 < rH) {
      var hx = sqrt(rp * rp - dy * dy) / (NC - 1)
      var y2 = r2 / (NR - 1), hh2 = 0.5 / (NR - 1)
      fillRect(sx - hx, y2 - hh2, sx + hx, y2 + hh2)
    }
  }
  // sea rows: depth shading, the band each row sits on, and the sun path across it
  var kk = 1.3 * 100 / scalePct, pathB = pal == 2 ? 0.45 : 0.9
  for (var r3 = rH; r3 < rB; r3++) {
    var d = (r3 - rH + 0.5) / (rB - rH), pp = 1 / (d + 0.1)
    var band = wave(pp * kk + tB)
    paint(L[2] + (L[3] - L[2]) * d + 0.09 * band, 0.75 + 0.25 * d)
    row(r3)
    var pw = rp / NC * 0.8 + d * 0.16, px = sx + 0.025 * sin(clock * 0.9 + d * 7) * (0.3 + d)
    if (band > 0.35) {
      var jit = (hash2(r3, floor(clock * 4)) - 0.5) * pw * 0.7, len = pw * (0.35 + 0.65 * band)
      paint(L[5], pathB * (band - 0.35) * (1.2 - 0.5 * d))
      drawLine(px - len + jit, r3 / (NR - 1), px + len + jit, r3 / (NR - 1), 0.9 / (NR - 1), 1)
    }
  }
  // crest lines: a crest sits where the band peaks, laid out with reciprocal depth
  var y0 = rH / (NR - 1), y1 = rB / (NR - 1)
  var nLo = ceil(kk * 0.909 + tB - 0.25), nHi = floor(kk * 7 + tB - 0.25)
  for (var cn = nLo; cn <= nHi; cn++) {
    var dc = 1 / ((cn + 0.25 - tB) / kk) - 0.1
    if (dc < 0.04 || dc > 1) continue
    var yc = y0 + dc * (y1 - y0), amp = 0.012 * dc * (100 / scalePct + 0.5), lw = (0.35 + 1.3 * dc) / NR
    paint(L[4] + 0.06 * dc, 0.55 + 0.45 * dc)
    var xa = 0, ya = yc + amp * sin(cn * 1.7 + clock * 0.8)
    for (var sgm = 1; sgm <= 6; sgm++) {
      var xb = sgm / 6, yb2 = yc + amp * sin(xb * 6 + cn * 1.7 + clock * 0.8)
      drawLine(xa, ya, xb, yb2, lw, 2)
      xa = xb
      ya = yb2
    }
  }
  // glitter: sparse sparkles re-dealt a few times a second, densest in the sun path
  var deal = floor(clock * 5), ng = floor((6 + foamPct * 0.3) * clamp(NC * NR / 4096, 0.3, 3))
  var gg = gridWidth() > 0
  for (var g = 0; g < ng; g++) {
    var hd = hash2(g, deal)
    var dg = 0.08 + 0.92 * hash2(g + 40, deal), inPath = hd < 0.7
    var gx = inPath ? sx + (hash2(g + 80, deal) - 0.5) * 2 * (rp / NC * 0.8 + dg * 0.16) : hash2(g + 80, deal)
    paint(1, 0.55 + 0.45 * hash2(g + 120, deal))
    var gy = y0 + dg * (y1 - y0)
    if (gg) setPixel(gridIndex(gx * NC, gy * NR))   // a crisp single pixel on a grid
    else splat(gx, gy, 0.7 / NC, 2)
  }
  // beach: wet sheet, wet sand, dry sand, then the foam line
  var e = yb + (1 - yb) * (0.08 + 0.3 * reach()), re = round(e * (NR - 1))
  wetMax = max(e, wetMax - fdt * 0.012)   // the wet band dries back
  var rw = round(wetMax * (NR - 1))
  for (var r4 = rB; r4 < NR; r4++) {
    if (r4 <= re) paint(L[3] + 0.08, 0.8)
    else if (r4 <= rw) paint(L[7] - 0.08, 0.55)
    else paint(L[7], 0.75 + 0.2 * (r4 - rB) / max(NR - rB, 1))
    row(r4)
  }
  paint(1, 0.55 + foamPct * 0.0045)
  var xf = 0, yf = e + 0.01 * sin(clock * 2)
  for (var sf = 1; sf <= 8; sf++) {
    var xg = sf / 8, yg = e + 0.012 * sin(xg * 11 + clock * 2 + sf)
    drawLine(xf, yf, xg, yg, 0.7 / NR, 2)
    xf = xg
    yf = yg
  }
}

export function renderBulk() {
  if (panel) seascape()
  else if (mode) shoreline()
  else openWater()
}
