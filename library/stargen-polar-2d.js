// name: StarGen polar 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "StarGen polar 2D"; original source never consulted.
//
// A self-advancing playlist of twelve polar "star/figure" animations for a
// disc whose pixel map is stored in normalized SPHERICAL/POLAR coordinates:
// first coordinate = radius (0 center .. 1 rim), second = angle in turns,
// third = azimuth from the pole (0.5 = equatorial plane). render2D takes
// the equatorial slice; render(index) is a degenerate radial fallback.
// Consecutive modes crossfade with a stochastic per-pixel "shimmer" dither.

var NUMMODES = 12
var FADESECS = 2        // constant absolute crossfade duration

var dwell = 8           // seconds per mode
var modeOverride = 0    // 0 = auto playlist, 1..12 = pinned mode
var rotOffset = 0       // constant added to the angular coordinate
var baseW = 0.125       // default proximity half-width (~1/8 unit)

var clock = 0           // playlist position, in mode units
var dt = 0.016          // seconds elapsed this frame
var curMode = 0
var nextMode = 1
var inFade = 0
var fadeEase = 0

//# min=0 max=1 step=0.0769 default=0
export function sliderModeOverride(v) {
  // 0 = playlist; 1..12 pins a mode
  modeOverride = floor(v * 12.001)
  if (modeOverride > NUMMODES) modeOverride = NUMMODES
}

//# min=0 max=1 step=0.01 default=0.36
export function sliderDwellTime(v) {
  dwell = 0.3 + v * v * 60   // nearly instant .. about a minute
}

//# min=0 max=1 step=0.01 default=0
export function sliderRotationOffset(v) {
  rotOffset = v
}

//# min=0 max=1 step=0.01 default=0.35
export function sliderLineWidth(v) {
  baseW = 0.02 + v * 0.3
}

// ---- helpers -------------------------------------------------------------

// Slow sinusoidal oscillator, 0..1, with a period in seconds
function osc(period) {
  return wave(time(period / 65.536))
}

// Sawtooth 0..1 with a period in seconds
function saw(period) {
  return time(period / 65.536)
}

// Proximity line helper: 1 when v == target, falling to 0 once they differ
// by more than half-width w; falloff squared for a gamma-corrected edge.
function near(v, target, w) {
  var d = abs(v - target)
  if (d >= w) return 0
  var t = 1 - d / w
  return t * t
}

// Wrapped variant for angular quantities (values in turns): triangle-wave
// distance so values just below 1 and just above 0 read as close.
function nearWrap(v, target, w) {
  var d = triangle(v - target) * 0.5
  if (d >= w) return 0
  var t = 1 - d / w
  return t * t
}

// Star-polygon family: n points, pinch factor k (k=1 is the regular n-gon,
// larger k pinches the edges inward into a star), circumradius scl.
// Reciprocal cosine of a scaled arccos-of-cosine of n times the angle.
function starR(a, n, k, scl) {
  var d = acos(cos(n * a * PI2)) / n        // 0..PI/n from nearest point
  return scl * cos(PI * k / n) / cos(k * (PI / n - d))
}

// ---- mode 0: orbits / ellipse --------------------------------------------

var e0a = 0.4, e0b = 0.4, e0rot = 0, e0skew = 0, e0hue = 0.07, e0sat = 1

function setup0() {
  e0a = 0.3 + 0.35 * osc(7)                 // axes breathe independently
  e0b = 0.3 + 0.35 * osc(11)
  e0rot = frac(e0rot + (0.02 + 0.06 * osc(23)) * dt)  // slowly varying spin
  e0skew = max(0, osc(31) - 0.8) * 4        // occasional galaxy-spiral smear
  e0hue = 0.07 + max(0, osc(17) - 0.9) * 0.8 // hue drifts only at one extreme
  e0sat = 0.75 + 0.25 * min(1, abs(e0a - e0b) * 6) // dips when near-circular
}

function draw0(index, r, a) {
  if (r < 0.001) {
    // the exact center pixel: a little colorful heart, half bright
    hsv(saw(20), 1, 0.5)
    return
  }
  var aa = (a - e0rot) * PI2 + e0skew * r
  var er = e0a * e0b / hypot(e0b * cos(aa), e0a * sin(aa))
  hsv(e0hue, e0sat, near(r, er, baseW))
}

// ---- mode 1: six-lobed sinus star ----------------------------------------

var s1base = 0.5

function setup1() {
  s1base = 0.42 + 0.18 * osc(9)             // slowly breathing base radius
}

function draw1(index, r, a) {
  var target = s1base + 0.13 * cos(PI2 * (a - saw(5)) * 6)
  hsv(0, 0, near(r, target, baseW * 1.3))   // pure white snowflake
}

// ---- mode 2: sinus shimmer ------------------------------------------------

function draw2(index, r, a) {
  // same construction, angular frequency several times higher — reads as
  // snowy sparkling texture near the rim rather than a figure
  var target = 0.82 + 0.1 * cos(PI2 * (a - saw(4)) * 24)
  hsv(0, 0, near(r, target, baseW))
}

// ---- mode 3: Star of David (kaleidoscopic) --------------------------------

var s3scale = 0.35, s3rot = 0, s3drift = 0

function setup3() {
  s3scale = 0.28 + 0.18 * osc(13)           // chord scale breathes
  s3rot = saw(37)                           // rotation drifts
  s3drift = (osc(19) - 0.5) * 0.55          // radians
}

function draw3(index, r, a) {
  // twelve alternating sectors; alternate sectors mirror the polar line
  // equation of a straight chord: scale / cos(offset angle)
  var aa = frac(a + s3rot)
  var sector = floor(aa * 12)
  var offR = (frac(aa * 12) - 0.5) * PI2 / 12
  if (mod(sector, 2) == 1) offR = -offR
  var target = s3scale / cos(offR + s3drift)
  hsv(0, 0, near(r, target, baseW))
}

// ---- mode 4: Star over Bethlehem ------------------------------------------

var s4pow = 1, s4diag = 0

function setup4() {
  s4pow = 0.5 + 2.5 * osc(9)                // breathing radial shaping power
  s4diag = triangle(saw(6))                 // diagonal rays fade in and out
}

function draw4(index, r, a) {
  var dA = triangle(a * 8) / 16             // turns to nearest of 8 rays
  var v = near(dA, 0, baseW * 0.35)         // thin radial rays
  v = v * pow(max(0, 1 - r * 0.92), s4pow)  // length/spread ~ radius^power
  var k = floor(frac(a + 1 / 16) * 8)       // which ray is nearest
  if (mod(k, 2) == 1) v = v * s4diag        // diagonals: 4-ray <-> 8-ray star
  // white-hot center shading to orange, warmer toward the rim
  hsv(0.04 + 0.05 * r, min(1, r * r * 4), v)
}

// ---- mode 5: pentagram ------------------------------------------------------

var s5scale = 0.6, s5walk = 0.1

function setup5() {
  // scale swells/shrinks over many minutes: damped-sinc-like function of a
  // very slow triangle LFO — gentle drift punctuated by big swings
  var u = (triangle(saw(280)) - 0.5) * 14
  s5scale = 0.6 + 0.3 * sin(u) / (abs(u) + 1.2)
  // stroke weight wanders randomly within bounds
  s5walk = clamp(s5walk + random(0.02) - 0.01, 0.04, 0.18)
}

function draw5(index, r, a) {
  var target = starR(a - saw(50), 5, 2, s5scale)
  hsv(0, 0, near(r, target, s5walk * (s5scale + 0.5)))
}

// ---- mode 6: decagram -------------------------------------------------------

function draw6(index, r, a) {
  var v
  if (r < 0.28) {
    v = 1                                    // inner third filled solid
  } else {
    var target = starR(a - saw(300), 10, 3, 0.78) // extremely slow spin
    v = near(r, target, baseW * 2.2)         // quite thick stroke
  }
  hsv(0, 0, v)
}

// ---- mode 7: hexagram -------------------------------------------------------

function draw7(index, r, a) {
  var target = starR(a - saw(35), 6, 2, 0.72)
  hsv(0, 0, near(r, target, baseW * 1.5))
}

// ---- mode 8: heart ----------------------------------------------------------

var h8scale = 0.55, h8sat = 1

function setup8() {
  var pulse = osc(8)
  h8scale = 0.45 + 0.25 * pulse
  h8sat = 0.55 + 0.45 * pulse               // whitens as it shrinks
}

// A known polar heart: sine terms plus a square-root-of-absolute-cosine lobe
function heartR(th) {
  var s = sin(th)
  return s * sqrt(abs(cos(th))) / (s + 1.4) - 2 * s + 2
}

function draw8(index, r, a) {
  var th = (a + 0.5) * PI2                  // half-turn shift: point it right
  var target = h8scale * heartR(th) * 0.25
  hsv(0, h8sat, near(r, target, baseW))
}

// ---- mode 9: bird flap ------------------------------------------------------

var b9dih = 0, b9bend = 1, b9glow = 1

function setup9() {
  b9dih = (abs(cos(PI2 * saw(4))) - 0.35) * 1.1  // wingbeat dihedral angle
  b9bend = 0.75 + 0.25 * osc(7)                   // wing bend breathes
  b9glow = 0.6 + 0.4 * osc(4.5)                   // ember brightness breathes
}

function draw9(index, r, a) {
  var da = frac(a - 0.25 + 0.5) - 0.5       // signed turns from the heading
  var arg = abs(da) * PI2 * b9bend - b9dih
  var co = cos(arg)
  var v = 0
  if (co > 0.12) {                          // cull the chord's blow-up zone
    var target = 0.22 / co
    // line width grows with radius: wingtips softer/broader
    v = near(r, target, baseW * (0.5 + 1.8 * r)) * b9glow
  }
  hsv(0.05, 0.9, v)                         // warm ember orange
}

// ---- mode 10: rainbow spirals -----------------------------------------------

var sp1 = 1, sp2 = 0.3, sp3 = 2, spPhase = 0

function setup10() {
  sp1 = 1 + 2 * osc(33)                     // radius scale LFO
  sp2 = 0.18 + 0.3 * osc(47)                // modulo LFO
  sp3 = floor(1 + osc(26) * 3.99)           // arm count LFO
  spPhase = saw(15)                         // winding motion
}

function draw10(index, r, a) {
  var ph = mod(r * sp1 + spPhase, sp2) / sp2
  var v = nearWrap(ph, frac(a * sp3), baseW * 1.2)
  // perceptual rainbow correction: sine reshaping that de-emphasizes the
  // overlong green band
  var h0 = frac(r * 0.7 + a + saw(30))
  hsv(frac(h0 - 0.1 * sin(PI2 * h0)), 1, v)
}

// ---- mode 11: snowglobe (1D sparks over the pixel index) ---------------------

var SPARKN = 20
var spkPos = array(SPARKN)
var spkVel = array(SPARKN)
var energy = array(pixelCount)  // per-pixel accumulation buffer

function respawnSpark(i) {
  spkPos[i] = random(pixelCount)
  spkVel[i] = (random(2) - 1) * pixelCount * 0.4  // either direction
}

var _si = 0
while (_si < SPARKN) {
  respawnSpark(_si)
  _si = _si + 1
}

function setup11() {
  var t = dt * 0.1                          // time step slowed 10x on purpose
  var friction = 600 / pixelCount           // friction ~ 1/pixelCount
  arrayReplace(energy, 0)                   // buffer cleared every frame
  for (var i = 0; i < SPARKN; i++) {
    spkVel[i] = spkVel[i] * (1 - friction * t)
    spkPos[i] = spkPos[i] + spkVel[i] * t
    if (spkPos[i] < 0 || spkPos[i] >= pixelCount || abs(spkVel[i]) < pixelCount * 0.02) {
      respawnSpark(i)
    } else {
      // deposit speed as energy
      energy[floor(spkPos[i])] += abs(spkVel[i]) * t
    }
  }
}

function draw11(index, r, a) {
  // ignores polar coordinates entirely; on a disc the pixel order reads as
  // concentric drifting snow sparkle
  var v = min(1, energy[index] * 6)
  // faint traces deep icy blue, hot ones white; brightness squared
  hsv(0.58, clamp(1 - v * 0.85, 0.1, 1), v * v)
}

// ---- playlist machinery -----------------------------------------------------

function needsSetup(m) {
  if (curMode == m) return 1
  return inFade && nextMode == m
}

export function beforeRender(delta) {
  dt = delta / 1000
  clock += delta / (dwell * 1000)
  if (clock >= NUMMODES) clock -= NUMMODES
  if (clock < 0 || clock >= NUMMODES) clock = 0

  curMode = floor(clock)
  var modeFrac = clock - curMode
  nextMode = curMode + 1
  if (nextMode >= NUMMODES) nextMode = 0

  // crossfade window: constant absolute duration at the end of each dwell
  var wf = min(0.5, FADESECS / dwell)
  inFade = modeFrac > 1 - wf
  fadeEase = 0
  if (inFade) {
    var p = (modeFrac - (1 - wf)) / wf
    fadeEase = (1 - cos(PI * p)) / 2        // smooth sinusoidal easing
  }

  if (modeOverride > 0) {
    curMode = modeOverride - 1
    inFade = 0
  }

  // run the current mode's per-frame setup; during a crossfade the incoming
  // mode's setup runs too so both are live
  if (needsSetup(0)) setup0()
  if (needsSetup(1)) setup1()
  if (needsSetup(3)) setup3()
  if (needsSetup(4)) setup4()
  if (needsSetup(5)) setup5()
  if (needsSetup(8)) setup8()
  if (needsSetup(9)) setup9()
  if (needsSetup(10)) setup10()
  if (needsSetup(11)) setup11()
}

function drawMode(m, index, r, a) {
  if (m == 0) draw0(index, r, a)
  else if (m == 1) draw1(index, r, a)
  else if (m == 2) draw2(index, r, a)
  else if (m == 3) draw3(index, r, a)
  else if (m == 4) draw4(index, r, a)
  else if (m == 5) draw5(index, r, a)
  else if (m == 6) draw6(index, r, a)
  else if (m == 7) draw7(index, r, a)
  else if (m == 8) draw8(index, r, a)
  else if (m == 9) draw9(index, r, a)
  else if (m == 10) draw10(index, r, a)
  else draw11(index, r, a)
}

// coordinates arrive as (radius, angle, azimuth), each normalized 0..1
export function render3D(index, r, a, az) {
  a = frac(a + rotOffset)
  var m = curMode
  // stochastic shimmer crossfade: each pixel independently and randomly
  // picks the outgoing or incoming mode each frame — never both
  if (inFade && random(1) < fadeEase) m = nextMode
  drawMode(m, index, r, a)
}

// 2D: equatorial slice (azimuth pinned to the midpoint)
export function render2D(index, r, a) {
  render3D(index, r, a, 0.5)
}

// 1D fallback: strip position as radius, angle zero
export function render(index) {
  render3D(index, index / pixelCount, 0, 0.5)
}
