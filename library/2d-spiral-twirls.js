// name: Spiral Twirls
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/2d-spiral-twirls.md, spiral-twirls-star-2d.md — reimagined, not a replica.
// Chromatic mode: original work from the prose idea spec docs/pattern-specs/patternflow/chromatic-vortex.md (Patternflow preset, CC BY-SA 4.0 — prose-described idea only, no code).
// A spinning pinwheel of rainbow comet arms that winds into a spiral, relaxes
// straight and winds the other way. Chromatic splits R, G and B along the arms
// (white where they meet, fringes that breathe); Mask cuts the vortex into a star
// or hexagon with a glowing rim. Each panel cell's (angle, radius) is packed once
// into a table; per frame a cell is one phase, and the arm profile is a palette
// rebuilt each frame and painted with one paintCanvas.

var CAP = 64                      // canvas cap per axis (4096 cells)
var up = 0                        // 1 = bilinear upscale (grid wider than the cap)

var arms = 2
//# min=1 max=8 step=1 default=2
export function sliderArms(v) { arms = clamp(floor(v), 1, 8) }

// Peak wind of an arm from centre to edge, in turns (the sign picks the hand).
var twist = 1.25
//# min=-3 max=3 step=0.05 default=1.25
export function sliderTwistTurns(v) { twist = clamp(v, -3, 3) }

// One full wind -> unwind -> counter-wind cycle; 0 holds the spiral at Twist.
var windSec = 24
//# min=0 max=120 step=1 default=24
export function sliderWindSeconds(v) { windSec = clamp(v, 0, 120) }

// Pinwheel rotation, revolutions per minute (negative turns it the other way).
var spinRpm = 6
//# min=-60 max=60 step=1 default=6
export function sliderSpinRPM(v) { spinRpm = clamp(v, -60, 60) }

// Seconds for the palette to travel once round the colour wheel.
var driftSec = 20
//# min=5 max=120 step=1 default=20
export function sliderColorDriftSeconds(v) { driftSec = clamp(v, 5, 120) }

// Light left in each arm's dark trailing half (0 = crisp comets on black).
var glowPct = 20
//# min=0 max=60 step=1 default=20
export function sliderGlowPercent(v) { glowPct = clamp(v, 0, 60) }

var chroma = 0
export function toggleChromatic(v) { chroma = v }

// Peak channel separation for Chromatic, as a percentage of one arm's width.
var splitPct = 8
//# min=0 max=15 step=0.5 default=8
export function sliderSplitPercent(v) { splitPct = clamp(v, 0, 15) }

// 0 none, 1 five-point star, 2 hexagon.
var mask = 0
//# min=0 max=2 step=1 default=0
export function sliderMask(v) { mask = clamp(floor(v), 0, 2); dirty = 1 }

var sizePct = 85
//# min=20 max=100 step=1 default=85
export function sliderMaskSizePercent(v) { sizePct = clamp(v, 20, 100); dirty = 1 }

var tab = array(CAP * CAP)        // per cell: floor(angle*2048) + radius/RMAX, or -(rim value)
var cv = array(CAP * CAP)         // per cell palette position
var NS = 24, TOP = 0.9            // palette stops for the arm, top of the arm's range
var pal = array((NS + 4) * 4)
setPalette(pal)
var c3 = array(3), tint = array(3)
var RMAX = 1.5
var CW = 0, CH = 0, dirty = 1
var windPh = 0.25, spin = 0, hue0 = 0, breathe = 0

function buildTable() {
  var half = min(CW, CH) * 0.5, K = mask == 1 ? 5 : 6
  var R = sizePct * 0.0142, Ri = R * 0.42, al = PI / K, sa = sin(al), ck = cos(al)
  for (var r = 0; r < CH; r++) {
    var dy = (r - CH * 0.5 + 0.5) / half
    for (var c = 0; c < CW; c++) {
      var dx = (c - CW * 0.5 + 0.5) / half
      var d = hypot(dx, dy), a = mod(atan2(dy, dx) * 0.15915494, 1)
      var p = floor(a * 2048) + min(d / RMAX, 0.999)
      if (mask > 0) {
        // the outline in polar form: distance to one edge of the folded sector
        var u = a * K + 0.25
        var phi = abs(u - floor(u) - 0.5) * PI2 / K
        var b = mask == 1 ? R * Ri * sa / (Ri * sin(al - phi) + R * sin(phi)) : R * ck / cos(phi)
        var e = (b - d) * half
        if (e < 0.5) p = e < -3 ? -1 : -(TOP + 0.02 + 0.04 * saturate(1 + e / 3))
      }
      tab[r * CW + c] = p
    }
  }
  dirty = 0
}

// comet: dark seam, slow cubic rise, bright linear half, hard edge at the wrap
function comet(f) {
  f = f - floor(f)
  var s = f < 0.5 ? f * f * f * 2 : f
  return gw + (1 - gw) * s
}
var gw = 0.2

function stop(j, pos, r, g, b) {
  pal[j] = pos; pal[j + 1] = r; pal[j + 2] = g; pal[j + 3] = b
}

export function beforeRender(delta) {
  var dt = min(delta, 100) * 0.001
  var gwid = gridWidth(), ght = gridHeight()
  var w = gwid > 0 ? min(gwid, CAP) : 32, h = ght > 0 ? min(ght, CAP) : 32
  if (w != CW || h != CH) { CW = w; CH = h; dirty = 1 }
  up = gwid > CW || ght > CH ? 1 : 0    // bilinear only when the canvas is upscaled
  if (dirty) buildTable()

  if (windSec > 0) windPh = mod(windPh + dt / windSec, 1)
  spin = mod(spin + dt * spinRpm / 60, 1)
  hue0 = mod(hue0 + dt / driftSec, 1)
  breathe = mod(breathe + dt * 0.4, 1)
  gw = glowPct * 0.01

  // the arm profile as a palette over phase 0..1 (positions 0..TOP)
  var k = splitPct * 0.01 * (0.55 + 0.45 * sin(breathe * PI2))
  hsv2rgb(hue0, 0.75, 1, tint)
  for (var i = 0; i < NS; i++) {
    var f = i / (NS - 1) * 0.999
    if (chroma) {
      stop(i * 4, f * TOP, comet(f + k) * (0.25 + 0.75 * tint[0]),
           comet(f) * (0.25 + 0.75 * tint[1]), comet(f - k) * (0.25 + 0.75 * tint[2]))
    } else {
      hsv2rgb(hue0 + f * 0.5, 1 - f * f * f * 0.35, comet(f), c3)
      stop(i * 4, f * TOP, c3[0], c3[1], c3[2])
    }
  }
  // the mask rim: dim to bright in the complementary hue
  hsv2rgb(hue0 + 0.5, 0.6, 0.15, c3)
  stop(NS * 4, TOP + 0.02, c3[0], c3[1], c3[2])
  hsv2rgb(hue0 + 0.5, 0.6, 0.8, c3)
  stop(NS * 4 + 4, TOP + 0.06, c3[0], c3[1], c3[2])
  stop(NS * 4 + 8, TOP + 0.07, 0, 0, 0)            // outside the rim: black
  stop(NS * 4 + 12, 1, 0, 0, 0)
}

export function renderBulk() {
  var tw = twist * (windSec > 0 ? sin(windPh * PI2) : 1)
  var A = arms / 2048, B = arms * tw * RMAX, C = arms * spin
  var n = CW * CH
  for (var i = 0; i < n; i++) {
    var p = tab[i]
    if (p < 0) { cv[i] = -p; continue }
    var ai = floor(p)
    var ph = ai * A + (p - ai) * B - C
    cv[i] = (ph - floor(ph)) * TOP
  }
  paintCanvas(cv, CW, CH, 1, up)
}
