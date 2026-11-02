// name: Meadow
// Original work from the prose idea spec docs/pattern-specs/patternflow/firefly-hollow.md (Patternflow preset, CC BY-SA 4.0 — prose-described idea only, no code).
// Dusk over a meadow: a sunset sky with first stars, lit clouds and a sinking
// sun, two rolling hill lines, and a field of grass blades that bend as gust
// fronts roll across. Fireflies wander over the grass, flashing with a fast
// attack and slow decay; now and then a flash ripples through neighbours.
// All strokes: fillGradient sky, fillRect hills, drawLine blades, splat glows.

var MAXB = 120, MAXF = 30
var bx = array(MAXB), bh = array(MAXB), bk = array(MAXB)
var fa = array(MAXF), fb = array(MAXF), fra = array(MAXF), frb = array(MAXF)
var fph = array(MAXF), frt = array(MAXF), fyc = array(MAXF)

var blades = 70
//# min=20 max=120 step=1 default=70
export function sliderBlades(v) { blades = clamp(floor(v), 20, MAXB) }

var flies = 14
//# min=0 max=30 step=1 default=14
export function sliderFireflies(v) { flies = clamp(floor(v), 0, MAXF) }

// Gust strength in percent: 0 = still air.
var windPct = 40
//# min=0 max=100 step=1 default=40
export function sliderWindPct(v) { windPct = clamp(v, 0, 100) }

// Firefly halo radius in pixels.
var glowPx = 3
//# min=1 max=6 step=0.5 default=3
export function sliderGlowPx(v) { glowPx = clamp(v, 1, 6) }

for (var k = 0; k < MAXB; k++) {
  bx[k] = (k + hash(k * 1.7)) / MAXB          // evenly spread, jittered
  bh[k] = 0.35 + 0.65 * hash(k * 3.1 + 0.4)
  bk[k] = 0.6 + 0.8 * hash(k * 5.3 + 0.9)     // stiffness
}
for (k = 0; k < MAXF; k++) {
  fa[k] = random(PI2)
  fb[k] = random(PI2)
  fra[k] = 0.05 + random(0.12)
  frb[k] = 0.15 + random(0.25)
  frt[k] = 1 / (2 + random(2.5))
  fph[k] = random(1)
  fyc[k] = 0.72 + random(0.16)
}

var t = 0, gust = 0, px = 1 / 31, HOR = 0.6, syncIn = 5

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  t = mod(t + dt, 3600)
  var gw = gridWidth()
  px = 1 / max((gw > 0 ? gw : 32) - 1, 8)
  gust = mod(gust + dt * 0.11, 1)
  for (var i = 0; i < flies; i++) {
    fa[i] = mod(fa[i] + fra[i] * PI2 * dt, PI2)
    fb[i] = mod(fb[i] + frb[i] * PI2 * dt, PI2)
    fph[i] += frt[i] * dt
    if (fph[i] >= 1) fph[i] -= 1
  }
  syncIn -= dt
  if (syncIn < 0 && flies > 2) {              // a flash runs through a neighbourhood
    syncIn = 6 + random(8)
    var c = floor(random(flies))
    for (i = 0; i < flies; i++) {
      var d = abs(sin(fa[i]) - sin(fa[c]))
      if (d < 0.5) fph[i] = -d * 1.2 * frt[i]
    }
  }
}

function hill(x, base, amp, f1, f2, p) {
  return base + amp * (sin((x * f1 + p) * PI2) + 0.5 * sin((x * f2 + p * 1.7) * PI2))
}

// Wind at x: a slow sway plus sharp gust fronts rolling left to right.
function wind(x) {
  var w = windPct / 100
  var front = wave(x * 0.6 - gust * 2.2 - 0.25)
  front = front * front * front * front
  return w * (0.25 * sin((t * 0.35 + x * 1.3) * PI2) + 1.1 * front) + 0.03 * sin(t * 1.3 + x * 9)
}

function blade(i, baseY, maxH, bright, tipV, wd) {
  var x = bx[i], h = maxH * bh[i]
  var lean = (wind(x) / bk[i] + (bk[i] - 1.3) * 0.35) * h * 0.9
  var mx = x + lean * 0.35, my = baseY - h * 0.55
  var tx = x + lean, ty = baseY - h * (1 - 0.3 * lean * lean / (h * h + 0.0001))
  hsv(0.3, 0.6, bright)
  drawLine(x, baseY, mx, my, px * wd, 0)
  drawLine(mx, my, tx, ty, px * wd * 0.8, 0)
  if (tipV > 0) {                             // the last light catches the seed head
    hsv(0.07, 0.75, tipV)
    drawLine(mix(mx, tx, 0.7), mix(my, ty, 0.7), tx, ty, px * 1.1, 2)
  }
}

export function renderBulk() {
  // sky: indigo overhead, through violet and rose, to amber at the horizon
  fillGradient(0.68, 0.8, 0.05, 0.68 + 0.4 / HOR, 0.85, 0.05 + 0.55 / HOR, 2)
  var i, k
  for (k = 0; k < 6 + 0.3 / px; k++) {        // first stars, one cell each, high and dim
    var sx = round(hash(k * 2.3) / px) * px, sy = round(hash(k * 4.7 + 1) * 0.3 / px) * px
    hsv(0.6, 0.2, (0.3 + 0.2 * sin(t * (1 + hash(k)) * 3 + k)) * (1 - sy * 2.2))
    splat(sx, sy, px * 0.9, 2)
  }
  hsv(0.07, 0.8, 0.3)                         // sinking sun and its haze
  splat(0.68, HOR - 0.05, 0.3, 1)
  hsv(0.12, 0.45, 1)
  fillCircle(0.68, HOR - 0.05, 0.06)
  for (k = 0; k < 3; k++) {                   // three clouds, lit from below
    var cx = mod(k * 0.37 + t * 0.006 * (1 + k * 0.3), 1.4) - 0.2, cy = 0.14 + k * 0.09
    for (i = 0; i < 7; i++) {
      var ox = (i - 3) * 0.04 * (1 + hash(k * 9 + i) * 0.4), oy = abs(i - 3) * 0.008
      hsv(0.8, 0.35, 0.1)
      splat(cx + ox, cy + oy - 0.012, 0.045 + 0.02 * hash(k + i), 2)
      hsv(0.04, 0.6, 0.2 - k * 0.03)
      splat(cx + ox, cy + oy + 0.012, 0.035, 1)
    }
  }
  // far hills (violet haze), then the near meadow (dark)
  for (var c = 0; c < 1 + 1 / px; c++) {
    var x = c * px
    hsv(0.74, 0.5, 0.11)
    fillRect(x - px * 0.45, hill(x, HOR + 0.03, 0.035, 0.9, 2.3, 0.2), x + px * 0.45, 1)
    hsv(0.33, 0.55, 0.035)
    fillRect(x - px * 0.45, hill(x, HOR + 0.2, 0.025, 1.4, 3.1, 0.6), x + px * 0.45, 1)
  }
  var back = floor(blades * 0.4)
  for (i = 0; i < back; i++) {                 // back row on the far ridge, hazy
    var j = floor(i * MAXB / back)
    blade(j, hill(bx[j], HOR + 0.03, 0.035, 0.9, 2.3, 0.2) + 0.02, 0.075, 0.06, 0, 0.75)
  }
  var front = blades - back
  for (i = 0; i < front; i++) {                // front row: black blades, lit tips
    j = floor(i * MAXB / front + MAXB / front / 2) % MAXB
    blade(j, 1.03, 0.34, 0.012, 0.3, 1.1)
  }
  // fireflies: a wide soft halo and a hot core, added
  for (i = 0; i < flies; i++) {
    var p = fph[i], e = 0
    if (p >= 0) e = p < 0.04 ? easeOutQuad(p / 0.04) : exp((0.04 - p) * 6)
    if (e < 0.02) continue
    var fx = 0.5 + 0.47 * sin(fa[i]), fy = fyc[i] + 0.1 * sin(fb[i]) + 0.03 * sin(fa[i] * 2.7)
    hsv(0.14, 0.9, e * 0.4)
    splat(fx, fy, px * glowPx, 1)
    hsv(0.15, 0.35, e)
    splat(fx, fy, px * 1.2, 1)
  }
}
