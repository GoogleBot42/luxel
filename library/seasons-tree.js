// name: Seasons Tree
// Curated original (hand-written showcase of the Luxel language/builtins).
// A year in one minute: a fractal tree grows limb by limb, blossoms pink,
// sheds petals, greens, turns gold and red, drops its leaves and stands bare
// in falling snow, then sinks back and a new tree grows. The 127-segment
// tree is rebuilt per year; per frame it is one pass of angle sums (wind
// sways the tips most), 127 drawLine, <= 192 leaf discs, a sky gradient.

var year = 60       // seconds per year
var wind = 40       // % wind strength
var spread = 26     // degrees between sibling branches

const NS = 127, NL = 192, TIP0 = 63
var OFF = array(NS), LEN = array(NS)              // static shape, per year
var ANG = array(NS), EX = array(NS), EY = array(NS) // this frame
var FX = array(NL), FY = array(NL), FT = array(NL), DET = array(NL)
var SKY = array(24)   // 4 seasons x (h0 s0 v0 h1 s1 v1): top, horizon
arrayReplaceAt(SKY, 0, 0.56, 0.45, 0.55, 0.52, 0.22, 0.75)    // spring
arrayReplaceAt(SKY, 6, 0.60, 0.75, 0.55, 0.54, 0.35, 0.78)    // summer
arrayReplaceAt(SKY, 12, 0.68, 0.45, 0.35, 1.06, 0.45, 0.72)   // autumn dusk
arrayReplaceAt(SKY, 18, 0.63, 0.30, 0.30, 0.60, 0.10, 0.58)   // winter
var C6 = array(6)

var yr = 0, prev = 0, seed = 0, t = 0, grow = 0, snow = 0, flakes = 0, sway = 0
var pxu = 1 / 63, lr = 0.016, BX = 0.5, BY = 0.93

function build() {
  var i, d, h
  seed = random(100)
  LEN[0] = 0.2 + 0.03 * hash(seed)
  OFF[0] = 0.12 * (hash(seed + 1) - 0.5)
  for (i = 1; i < NS; i++) {
    d = floor(log2(i + 1))
    h = hash(i * 1.37 + seed)
    // one child stays upright-ish, its sibling leans out
    OFF[i] = (i & 1 ? -1 : 1) * spread * 0.01745 * (0.55 + 0.9 * h) + 0.15 * (hash(i + seed * 3) - 0.5)
    LEN[i] = LEN[(i - 1) >> 1] * (0.68 + 0.16 * hash(i * 2.9 + seed))
  }
}
build()

function ease(d) { return easeOutCubic(saturate(grow - d)) }

export function beforeRender(delta) {
  var dt = delta * 0.001, i, p, a, g, w
  t += dt
  if (t > 3600) t -= 3600
  prev = yr
  yr = mod(yr + dt / year, 1)
  if (yr < prev) { build(); for (i = 0; i < NL; i++) DET[i] = 0 }
  grow = 8 * (easeInOutSine(saturate(yr / 0.14)) - saturate((yr - 0.965) / 0.035))
  snow = yr > 0.5 ? smoothstep(0.78, 0.84, yr) : 1 - smoothstep(0, 0.07, yr)
  flakes = smoothstep(0.78, 0.81, yr) * (1 - smoothstep(0.94, 0.97, yr))
  pxu = 1 / (gridWidth() > 1 ? gridWidth() - 1 : 47)
  lr = max(pxu * 0.75, 0.016)       // leaf radius: 1 px at 64, grows on 128
  w = wind * 0.01
  sway = w * (0.07 * sin(t * 0.9) + 0.04 * sin(t * 2.3 + 1))
  // limbs: parent before child (breadth-first), so one pass sums the angles
  for (i = 0; i < NS; i++) {
    g = ease(floor(log2(i + 1)))
    p = (i - 1) >> 1
    a = (i == 0 ? 0 : ANG[p]) + OFF[i] + sway + w * 0.05 * (i > 6) * sin(t * 3.1 + i * 1.7)
    ANG[i] = a
    EX[i] = (i == 0 ? BX : EX[p]) + sin(a) * LEN[i] * g
    EY[i] = (i == 0 ? BY : EY[p]) - cos(a) * LEN[i] * g
  }
  // petals (det 1) and leaves (det 2) let go at their own moment
  for (i = 0; i < NL; i++) {
    a = hash(i * 0.731 + seed)
    if ((DET[i] == 0 && yr > 0.24 + 0.07 * a) || (DET[i] == 1 && yr > 0.62 + 0.17 * a)) {
      p = TIP0 + floor(i / 3)
      FX[i] = EX[p] + lr * 2.2 * (hash(i * 3.1 + seed) - 0.5) * 2
      FY[i] = EY[p] + lr * 2.2 * (hash(i * 5.3 + seed) - 0.5) * 2
      FT[i] = yr
      DET[i]++
    }
  }
}

function skyAt() {           // season blend of the SKY table into C6
  var f = mod(yr - 0.12, 1) * 4, s0 = floor(f), m = smoothstep(0.55, 1, f - s0), j
  for (j = 0; j < 6; j++) C6[j] = mix(SKY[s0 * 6 + j], SKY[(s0 + 1) % 4 * 6 + j], m)
}

export function renderBulk() {
  var i, d, p, a, x, y, tau, h, b, lf, tn, r
  skyAt()
  fillGradient(C6[0], C6[1], C6[2], C6[3], C6[4], C6[5], 2)
  // ground: a low hill, grass -> straw -> snow
  oklch(mix(mix(0.42, 0.5, smoothstep(0.5, 0.7, yr)), 0.92, snow), 0.13 * (1 - snow), mix(0.38, 0.22, smoothstep(0.45, 0.7, yr)))
  fillCircle(0.5, 2.1, 1.18)
  // limbs, trunk first; snow rides on the upper side in winter
  for (i = 0; i < NS; i++) {
    d = floor(log2(i + 1))
    if (grow <= d) break
    p = (i - 1) >> 1
    x = i == 0 ? BX : EX[p]
    y = i == 0 ? BY : EY[p]
    r = max(pxu * 0.6, 0.042 * pow(0.7, d))
    oklch(0.27 + 0.03 * d, 0.05, 0.16)
    drawLine(x, y, EX[i], EY[i], r, 0)
    if (d < 3) {                                    // sunlit bark on the left
      oklch(0.38, 0.05, 0.14)
      drawLine(x - r * 0.4, y, EX[i] - r * 0.4, EY[i], r * 0.4, 0)
    }
    if (snow > 0.05 && d < 6 && abs(sin(ANG[i])) > 0.4) {   // snow on the level limbs
      oklch(0.95, 0.01, 0.65)
      drawLine(x, y - r * 0.7, EX[i], EY[i] - r * 0.7, r * 0.55 * snow, 0)
    }
  }
  // blossoms, leaves, petals and fallen leaves
  for (i = 0; i < NL; i++) {
    h = hash(i * 0.731 + seed)
    b = hash(i * 1.913 + seed)
    p = TIP0 + floor(i / 3)
    x = EX[p] + lr * 2.2 * (hash(i * 3.1 + seed) - 0.5) * 2
    y = EY[p] + lr * 2.2 * (hash(i * 5.3 + seed) - 0.5) * 2
    lf = saturate((yr - 0.2 - 0.09 * h) / 0.05)          // leaf has opened
    if (DET[i] == 0) {
      if (grow < 6.5) continue
      a = saturate((yr - 0.08 - 0.07 * h) / 0.03)          // blossom opens
      if (lf > 0.4) { oklch(0.6, 0.15, 0.38 + 0.05 * b); fillCircle(x, y, lr * 1.05) }
      if (a > 0) { oklch(0.82 + 0.1 * b, 0.11 - 0.08 * b, 0.97); fillCircle(x, y, lr * (0.5 + 0.7 * a)) }
    } else if (DET[i] == 1) {
      tau = (yr - FT[i]) * year
      if (lf > 0) {
        tn = saturate((yr - 0.5 - 0.12 * h) / 0.07)        // autumn turn
        oklch(0.58 + 0.1 * tn, 0.15 + 0.03 * tn, mix(0.38 + 0.05 * b, 0.07 + 0.16 * b, tn))
        fillCircle(x, y, lr * 1.05 * max(lf, 0.5))
      }
      if (tau < 5) {
        oklch(0.85 * (1 - tau / 5), 0.1, 0.97)
        splat(FX[i] + 0.05 * tau * (0.5 + wind * 0.01) + 0.02 * sin(tau * 2.5 + i), FY[i] + 0.035 * tau, lr * 1.2, 2)
      }
    } else {
      if (snow > 0.1 + 0.8 * b) continue                   // buried
      tau = (yr - FT[i]) * year
      d = 0.93 + 0.06 * h                                  // where it settles
      a = min(tau, max(0, (d - FY[i]) / 0.08))
      oklch(0.62 + 0.1 * b - 0.22 * saturate(tau / 12), 0.17, 0.07 + 0.16 * b)
      fillCircle(FX[i] + (0.04 * wind * 0.01 + 0.006) * a + 0.025 * sin(a * 3 + i), FY[i] + 0.08 * a, lr * 0.9)
    }
  }
  // snowfall
  if (flakes > 0) {
    oklch(0.97, 0.01, 0.65)
    for (i = 0; i < 48; i++) {
      if (hash(i + 99.5) > flakes) continue
      y = frac(hash(i + 0.5) + t * (0.06 + 0.05 * hash(i + 7.5)))
      x = frac(hash(i + 3.5) + 0.03 * sin(t * 1.3 + i) + sway * y)
      splat(x, y, pxu * 1.3, 2)
    }
  }
}

//# min=20 max=300 step=5 default=60
export function sliderYear(v) { year = clamp(v, 20, 300) }

//# min=0 max=100 step=5 default=40
export function sliderWind(v) { wind = clamp(v, 0, 100) }

//# min=15 max=45 step=1 default=26
export function sliderSpread(v) { spread = clamp(v, 15, 45); build() }
