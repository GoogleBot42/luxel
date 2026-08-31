// name: Grinch's Heist
// Curated original for the Luxel library.
//
// A string of Christmas bulbs hangs in the dark. A green glow creeps in from
// one end — sneaking, freezing, then darting — and every bulb it reaches
// flickers twice and goes out: stolen. When the last bulb is dark the thief
// stops at the far end, his glow warms from green to gold, and a bright ring
// swells outward from him, relighting every bulb it passes with an overshoot
// pop. The whole string shimmers for a beat, then the night goes dark and
// the heist starts over.
//
// Structure: bulbs sit on a regular grid along the strip, so a pixel only
// ever needs to look at ONE bulb (no per-pixel light loop). Each bulb's
// colour is converted from hue once per frame, not once per pixel. On a 2D
// map the same string is drawn in index order, so it strings back and forth
// across the panel row by row.

var MAXL = 40           // hard cap on bulbs (array sizing)
var lhue = array(MAXL)  // bulb hue, turns
var lbri = array(MAXL)  // current brightness, 0..~2 during the relight pop
var lst = array(MAXL)   // 0 lit, 1 stealing, 2 dark, 3 relighting
var lt = array(MAXL)    // 0..1 progress through the current transition
var lr = array(MAXL)    // per-frame resolved colour
var lg = array(MAXL)
var lb = array(MAXL)
var cvt = array(3)      // one shared hsv2rgb scratch

var STEAL_T = 0.45      // seconds for a bulb to flicker out
var RELIGHT_T = 0.6     // seconds for a bulb to pop back on
var HEART_T = 1.7       // seconds for the warm ring to cross the string
var HOLD_T = 1.6        // seconds the fully relit string shimmers
var RING_W = 3          // warm ring half-thickness, pixels
var SNEAK_NORM = 1.3727 // makes the sneak/dart profile average out to 1

// --- controls -------------------------------------------------------------
var wantLights = 14
//# min=4 max=40 step=1 default=14
export function sliderLights(v) { wantLights = clamp(floor(v), 2, MAXL) }

var creepSeconds = 16
//# min=4 max=60 step=1 default=16
export function sliderCreepSeconds(v) { creepSeconds = max(v, 1) }

var grinchHue = 105 / 360
//# min=0 max=360 step=1 default=105
export function sliderGrinchHue(v) { grinchHue = v / 360 }

var glow = 6
//# min=1 max=20 step=1 default=6
export function sliderGlowSize(v) { glow = max(floor(v), 1) }

// --- state ----------------------------------------------------------------
var nLights = 0
var spacing = 1
var bulbR = 1.4
var phase = 0           // 0 creep, 1 heart, 2 hold
var gpos = -10          // thief position, pixel units
var heartX = 0
var heartR = 0
var holdT = 0
var gBri = 1
var ghue = 105 / 360
var gr = 0, gg = 0, gb = 0
var ringR = 0, ringG = 0, ringB = 0
var ringBri = 0
var bgR = 0.004, bgG = 0.006, bgB = 0.02   // cold night
var inited = 0

function bulbCenter(k) { return (k + 0.5) * spacing }

function relayout() {
  var n = min(wantLights, floor(pixelCount / 2))
  n = clamp(n, 2, MAXL)
  if (n == nLights) return
  nLights = n
  spacing = pixelCount / nLights
  bulbR = clamp(spacing * 0.42, 0.8, 2.5)  // bulbs stay bulb-sized on long strips
  for (var k = 0; k < nLights; k++) lhue[k] = festiveHue(hash(k * 7.31 + nLights))
  relightAll()
}

// a five-colour strand: red, gold, green, blue, warm amber
function festiveHue(u) {
  var pick = floor(u * 5)
  return pick == 0 ? 0.0 : pick == 1 ? 0.09 : pick == 2 ? 0.33 : pick == 3 ? 0.58 : 0.05
}

function relightAll() {
  for (var k = 0; k < nLights; k++) {
    lst[k] = 0
    lt[k] = 0
    lbri[k] = 1
  }
}

export function beforeRender(delta) {
  var dt = delta / 1000
  if (!inited) { inited = 1; nLights = 0; gpos = -glow - 2 }
  relayout()

  // ---- the thief -------------------------------------------------------
  if (phase == 0) {
    var travel = pixelCount + glow * 2 + 4
    var sneak = (0.25 + 1.75 * pow(wave(time(0.05)), 4)) * SNEAK_NORM
    gpos += (travel / creepSeconds) * sneak * dt
    gBri = 1
    ghue = grinchHue
    for (var k = 0; k < nLights; k++) {
      if (lst[k] == 0 && abs(gpos - bulbCenter(k)) < max(bulbR, glow * 0.35)) {
        lst[k] = 1
        lt[k] = 0
      }
    }
    if (gpos > pixelCount + glow) {
      phase = 1
      heartX = pixelCount - 1
      heartR = 0
      gpos = pixelCount + glow * 0.5
    }
  } else if (phase == 1) {
    // his heart grows: green -> gold, beating, and a ring rolls out
    heartR += (pixelCount / HEART_T) * dt
    var grown = saturate(heartR / pixelCount)
    ghue = mix(grinchHue, 0.09, grown)
    gBri = 1 + 0.9 * pow(wave(time(0.012)), 3)
    for (var k = 0; k < nLights; k++) {
      if (lst[k] == 2 && abs(bulbCenter(k) - heartX) <= heartR) {
        lst[k] = 3
        lt[k] = 0
      }
    }
    if (heartR > pixelCount + RING_W * 2) { phase = 2; holdT = 0 }
  } else {
    holdT += dt
    ghue = 0.09
    gBri = max(1 - holdT / (HOLD_T * 0.6), 0)
    if (holdT > HOLD_T) {
      phase = 0
      gpos = -glow - 2
      relightAll()
    }
  }

  // ---- bulb animation --------------------------------------------------
  for (var k = 0; k < nLights; k++) {
    var st = lst[k]
    var white = 0
    if (st == 0) {
      // lit and gently shimmering
      lbri[k] = 0.82 + 0.18 * wave(time(0.03) + k * 0.13)
    } else if (st == 1) {
      lt[k] += dt / STEAL_T
      // once it is dark, restock the socket: it comes back a different colour
      if (lt[k] >= 1) { lst[k] = 2; lbri[k] = 0; lhue[k] = festiveHue(random(1)) }
      else lbri[k] = (1 - lt[k]) * (0.4 + 0.6 * square(lt[k] * 7, 0.55))
    } else if (st == 2) {
      lbri[k] = 0
    } else {
      lt[k] += dt / RELIGHT_T
      if (lt[k] >= 1) { lst[k] = 0; lbri[k] = 1; lt[k] = 0 }
      else {
        var u = 1 - lt[k]
        lbri[k] = 1 + 1.5 * u * u   // pop, then settle
        white = u * u
      }
    }
    hsv2rgb(lhue[k], 1 - white * 0.85, 1, cvt)
    lr[k] = cvt[0]
    lg[k] = cvt[1]
    lb[k] = cvt[2]
  }

  // ---- thief + ring colours (once per frame) ---------------------------
  hsv2rgb(ghue, phase == 0 ? 1 : 0.75, 1, cvt)
  gr = cvt[0] * gBri
  gg = cvt[1] * gBri
  gb = cvt[2] * gBri

  ringBri = phase == 1 ? saturate(1.15 - heartR / (pixelCount * 1.15)) : 0
  hsv2rgb(0.11, 0.55, 1, cvt)
  ringR = cvt[0] * ringBri
  ringG = cvt[1] * ringBri
  ringB = cvt[2] * ringBri
}

export function render(index) {
  var r = bgR, g = bgG, b = bgB

  // the one bulb that can reach this pixel
  var k = clamp(floor(index / spacing), 0, nLights - 1)
  var d = abs(index - bulbCenter(k))
  if (d < bulbR) {
    var p = 1 - d / bulbR
    var v = lbri[k] * p * p
    r += lr[k] * v
    g += lg[k] * v
    b += lb[k] * v
  }

  // the thief's glow, plus a hot core
  var gd = abs(index - gpos)
  if (gd < glow) {
    var q = 1 - gd / glow
    q = q * q * 0.75
    if (gd < 1.6) q += (1 - gd / 1.6) * 0.7
    r += gr * q
    g += gg * q
    b += gb * q
  }

  // the ring of the growing heart
  if (ringBri > 0) {
    var rd = abs(abs(index - heartX) - heartR)
    if (rd < RING_W) {
      var w = 1 - rd / RING_W
      w = w * w
      r += ringR * w
      g += ringG * w
      b += ringB * w
    }
  }

  rgb(r, g, b)
}

// On a 2D map the string is drawn in index order — it hangs back and forth
// across the panel, row by row, the way a real light string does on a wall.
export function render2D(index, x, y) {
  render(index)
}
