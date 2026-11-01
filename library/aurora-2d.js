// name: Aurora 2D
// Clean-room reimplementation from the prose specs docs/pattern-specs/1d-aurora-borealis.md,
// aurorashivers.md, coolaura.md — reimagined, not a replica (the panel curtains are original).
// Panel: rayed curtains hang from a wavy hem under a starry sky, bright at the hem and
// shading to the palette's top colour toward their tops. Only per-column tables are
// noise (four native fillNoise2D rows); a canvas of at most 64x32 goes on with one
// paintCanvas. Strip: soft lights drift, bloom and fade, added in RGB into three capped
// arrays shown with one fillRGB. Styles: curtains | shiver (flares, afterglow) | aqua swell.

var MAXW = 64, MAXH = 32, CAP = 340, POOL = 10
// Allocated on first use, so a strip never pays for the panel's canvas or vice versa.
var vC = 0, bC = 0, nA = 0, nB = 0, nR = 0, nL = 0, base = 0, ih = 0, amp = 0, pA = 0, pB = 0
var skyB = 0, rA = 0, gA = 0, bA = 0, glow = 0          // rA..glow: strip cells, or panel columns
var px = array(POOL), pv = array(POOL), age = array(POOL), life = array(POOL), hw = array(POOL)
var cr = array(POOL), cg = array(POOL), cb = array(POOL)

// Panel palettes run sky (0.12) -> hem green (0.5) -> curtain top (1.0).
var PALS = [
  [0, 0, 0, 0, 0.12, 0.05, 0.08, 0.3, 0.32, 0, 0.3, 0.32, 0.5, 0.1, 1, 0.35,
   0.75, 0.15, 0.9, 0.6, 1, 0, 0.45, 0.65],                      // green
  [0, 0, 0, 0, 0.12, 0.05, 0.08, 0.3, 0.32, 0, 0.3, 0.3, 0.5, 0.15, 1, 0.3,
   0.66, 0.2, 0.85, 0.4, 0.84, 0.75, 0.25, 0.75, 1, 0.5, 0.15, 0.95], // violet-green
  [0, 0, 0, 0, 0.12, 0.04, 0.06, 0.3, 0.32, 0, 0.2, 0.45, 0.5, 0, 0.85, 0.85,
   0.75, 0.1, 0.4, 1, 1, 0.4, 0.15, 0.9]                         // aqua
]
// Strip light colours per palette (r, g, b x 5): 0-2 everyday, 3 the shiver flare, 4 its rival.
var LIGHTS = [
  [0.1, 1, 0.3, 0.35, 1, 0.45, 0.6, 1, 0.15, 0, 0.75, 0.6, 0, 0.55, 0.8],
  [0.1, 1, 0.3, 0, 0.8, 0.55, 0.95, 0.25, 0.55, 0.55, 0.2, 0.95, 0, 0.8, 0.8],
  [0, 0.85, 0.85, 0, 0.5, 1, 0.1, 1, 0.6, 0.4, 0.2, 1, 0, 0.9, 0.7]
]

var style = 0, palMode = 1, cycle = 30, seeded = 0
// 0 curtains, 1 shiver (flares that leave a blue afterglow), 2 aqua swell (slow blooms, breathing).
//# min=0 max=2 step=1 default=0
export function sliderStyle(v) {
  var s = clamp(floor(v), 0, 2)
  if (s != style) { style = s; seeded = 0 }
}
// 0 green, 1 violet-green, 2 aqua.
//# min=0 max=2 step=1 default=1
export function sliderPalette(v) {
  palMode = clamp(floor(v), 0, 2)
  setPalette(PALS[palMode])
  seeded = 0
}
// Seconds for the sky to change its whole shape; lower is livelier.
//# min=5 max=120 step=1 default=30
export function sliderCycleSeconds(v) { cycle = clamp(v, 5, 120) }
setPalette(PALS[palMode])

// Mapless strips stay strips: this export suppresses the default grid. Luxel runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var zt = random(50), ph = random(50), clock = 0, nextSpawn = 0, fam = 0
var W = 32, H = 32, gw = 0, gh = 0, m = 0, breath = 1, panel = -1

function tint(k, j) {
  var L = LIGHTS[palMode]
  cr[k] = L[j * 3]
  cg[k] = L[j * 3 + 1]
  cb[k] = L[j * 3 + 2]
}

function spawn(k, a0) {
  life[k] = style == 0 ? 12 + random(12) : style == 1 ? 2 : 4 + random(2)
  age[k] = a0 * life[k]
  px[k] = random(1)
  pv[k] = 0
  if (style == 0) {
    hw[k] = 0.04 + random(0.08)
    pv[k] = (random(1) < 0.5 ? -1 : 1) * (0.008 + random(0.012))
    var r = random(1)
    tint(k, r < 0.3 ? 0 : r < 0.5 ? 1 : r < 0.62 ? 2 : r < 0.82 ? 3 : 4)
  } else if (style == 1) {
    fam = 1 - fam                  // the two families alternate and drift apart
    hw[k] = panel ? 0.035 : 0.065
    px[k] = fam ? random(0.8) : 0.2 + random(0.8)
    pv[k] = fam ? 0.2 : -0.2
    tint(k, fam ? 3 : 4)
  } else {
    hw[k] = 0.05
    tint(k, floor(random(3)))
  }
  var o = style == 0 ? 0.5 + random(0.5) : 1
  if (panel) { cr[k] = o; cg[k] = 0; cb[k] = 0 }   // a panel only needs the intensity
  else { cr[k] *= o; cg[k] *= o; cb[k] *= o }
}

function seed() {
  for (var k = 0; k < POOL; k++) {
    life[k] = 0
    if (style == 0 && k < 8) spawn(k, random(0.8))
    if (style > 0 && k < 4) spawn(k, k / 4)
  }
  if (style == 1 && !glow) glow = array(m)
  if (glow) feedback(glow, 0)
  seeded = 1
}

// Add every live light into rA/gA/bA over m cells (strip pixels or panel columns).
function deposit(dt) {
  feedback(rA, 0)
  feedback(gA, 0)
  feedback(bA, 0)
  if (style > 0 && clock >= nextSpawn) {
    for (var j = 0; j < POOL; j++) if (age[j] >= life[j]) { spawn(j, 0); break }
    nextSpawn = clock + (style == 1 ? 0.3 + random(0.2) : 1.1 + random(0.8))
  }
  for (var k = 0; k < POOL; k++) {
    if (age[k] >= life[k]) {
      if (style == 0 && k < 8) spawn(k, 0)
      continue
    }
    age[k] += dt
    var a = min(age[k] / life[k], 1), env = 0, w = hw[k], c = px[k]
    if (style == 0) {
      env = smoothstep(0, 0.25, a) * (1 - smoothstep(0.35, 1, a))   // bloom, then a long fade
      if (random(1) < dt * 0.15) pv[k] = -pv[k]
      px[k] += pv[k] * dt
      if (c < -w || c > 1 + w) life[k] = 0
    } else if (style == 1) {
      env = triangle(a)
      c += pv[k] * a + (random(1) + random(1) - 1) * 0.012   // drift plus a per-frame shiver
    } else {
      env = sin(a * PI) * breath
      w = 0.05 + a * 0.5
    }
    if (!panel) env *= env                 // squared light: soft fades, deep darks
    var ck = c * m, hk = max(w * m, 0.6)
    var lo = max(0, ceil(ck - hk)), hi = min(m - 1, floor(ck + hk))
    var kr = cr[k] * env, kg = cg[k] * env, kb = cb[k] * env, st = 1 / hk, i = lo, f = 0
    for (; i <= hi; i++) {
      f = (i - ck) * st
      f = 1 - f * f
      f *= f
      rA[i] += f * kr
      gA[i] += f * kg
      bA[i] += f * kb
    }
  }
  if (panel) return
  if (style == 1) {
    // afterglow: a decaying running max of the flares, shown deep blue
    for (var q = 0; q < m; q++) {
      var l = max(rA[q], max(gA[q], bA[q])), g = glow[q]
      if (l > g) glow[q] = l
      gA[q] += g * 0.1
      bA[q] += g * 0.45
    }
  } else if (style == 2) {
    // the breathing blue field the swells rise out of
    var gg = 0.05 * breath, gb = 0.16 * breath
    for (var q2 = 0; q2 < m; q2++) {
      gA[q2] += gg
      bA[q2] += gb
    }
  }
}

function layout() {
  if (!vC) {
    vC = array(MAXW * MAXH)
    bC = array(MAXW * MAXH)
    nA = array(MAXW)
    nB = array(MAXW)
    nR = array(MAXW)
    nL = array(MAXW)
    base = array(MAXW)
    ih = array(MAXW)
    amp = array(MAXW)
    pA = array(MAXW)
    pB = array(MAXW)
    skyB = array(MAXH)
    rA = array(MAXW)
    gA = array(MAXW)
    bA = array(MAXW)
    glow = array(MAXW)
  }
  gw = gridWidth()
  gh = gridHeight()
  W = gw > 0 ? min(gw, MAXW) : 32
  H = gh > 0 ? min(gh, MAXH) : 32
  m = W
  for (var r = 0; r < H; r++) {
    var y = (r + 0.5) / H
    skyB[r] = 0.14 + 0.3 * y * y                 // airglow brightens toward the horizon
  }
  seeded = 0
}

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000 * 30 / cycle  // the sky's own clock
  clock += dt
  var p = has2DMap()
  if (p) {
    if (panel != 1 || gw != gridWidth() || gh != gridHeight()) { panel = 1; layout() }
  } else if (panel != 0) {
    panel = 0
    m = min(pixelCount, CAP)
    rA = array(m)
    gA = array(m)
    bA = array(m)
    seeded = 0
  }
  if (!seeded) seed()
  setBlur(panel && (W < gw || H < gh) ? 0.3 : 0)
  breath = style == 2 ? 0.55 + 0.45 * wave(clock / 8) : 1
  if (style == 1) feedback(glow, pow(0.5, dt / 1.5))   // afterglow half-life 1.5 s
  if (style > 0 || !panel) deposit(dt)
  if (!panel) return

  zt += dt * 0.2
  ph += dt * 0.04
  var sx = 1 / W, rf = max(4, W / 5.5)
  fillNoise2D(nA, W, 1, 1.7 * sx, 0, 0.85 * sx + ph, zt, 3)            // hem folds
  fillNoise2D(nB, W, 1, 1.1 * sx, 0, 0.55 * sx - ph * 0.6, zt * 0.8, 4) // curtain height
  fillNoise2D(nR, W, 1, rf * sx, 0, rf * 0.5 * sx + ph * 3, zt * 2.5, 5) // rays
  fillNoise2D(nL, W, 1, 2.6 * sx, 0, 1.3 * sx - ph, zt * 1.5, 6)       // brightness along it
  for (var c = 0; c < W; c++) {
    var ray = 0.4 + 0.6 * saturate(0.5 + 0.8 * nR[c])
    var a = saturate(0.6 + 0.7 * nL[c]) * ray, fr = 1
    var h = 0.44 + 0.16 * nB[c]
    base[c] = 0.68 + 0.18 * nA[c]
    if (style == 1) {
      var fresh = rA[c], g = glow[c] * 0.9
      if (fresh > glow[c]) glow[c] = fresh
      a = 0.2 * a + fresh * ray + g
      fr = fresh / max(fresh + g, 0.01)   // fresh flares climb the palette, afterglow stays low
      h *= 0.85
    } else if (style == 2) {
      a = (0.1 + 0.55 * rA[c]) * (0.85 + 0.15 * ray)
      h += 0.12
      base[c] = 0.8 + 0.07 * nA[c]
    }
    amp[c] = a
    ih[c] = 1 / h
    pA[c] = 0.22 + 0.28 * fr
    pB[c] = 0.47 * fr
  }
}

function sky() {
  var i = 0, hem = style == 2 ? 0.15 : 0.6
  for (var r = 0; r < H; r++) {
    var y = (r + 0.5) / H, sb = skyB[r]
    for (var c = 0; c < W; c++) {
      var u = (base[c] - y) * ih[c], a = 0
      if (u > -0.12 && u < 1) {
        // a bright hem, a long body, and a quick fade over the last fifth
        a = u < 0 ? 1 + u * 8.3 : min(1, (1 - u) * 5) * (0.35 + 0.4 * (1 - u)) + max(0, hem - u * 4)
        a *= amp[c]
      }
      if (a > 0.004) {
        var e = pA[c] + pB[c] * max(u, 0)
        vC[i] = a < 0.25 ? 0.12 + (e - 0.12) * a * 4 : e
        bC[i] = sb + a
      } else {
        vC[i] = 0.12
        bC[i] = sb
      }
      i++
    }
  }
  paintCanvas(vC, W, H, bC)
  // stars, only in the sky above each curtain's top
  var ns = min(40, floor(gw * gh / 100))
  for (var s = 0; s < ns; s++) {
    var sxn = hash(s * 1.37 + 0.5), syn = hash(s * 2.11 + 7) * 0.75
    var cc = floor(sxn * W)
    if (syn < base[cc] - 1.1 / ih[cc] || amp[cc] < 0.05) {
      hsv(0.6, 0.25, 0.2 + 0.25 * wave(zt * 2 + hash(s + 3)))
      setPixel(gridIndex(sxn * gw, syn * gh))
    }
  }
}

export function renderBulk() {
  if (panel) {
    sky()
  } else if (pixelCount <= CAP) {
    fillRGB(rA, gA, bA)
  } else {
    // a strip longer than the cells: each cell lights a short run
    var sc = m / pixelCount
    for (var i = 0; i < pixelCount; i++) {
      var k = floor(i * sc)
      rgb(rA[k], gA[k], bA[k])
      setPixel(i)
    }
  }
}
