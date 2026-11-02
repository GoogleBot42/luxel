// name: 2D Fireworks Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "2D Fireworks Fade"; original source never consulted.
//
// DELIBERATELY REDESIGNED 2026-09-01 (library review pass 2). Fidelity to the
// original was waived by Jeremy: the original is a red/white/blue holiday
// playlist that never actually shows a firework. Everything below is a
// ground-up 2D firework show and matches the original only in name.
//
// A real shell lifecycle on a 2D map: rockets lift off the ground with a
// flickering comet trail, decelerate under gravity, flash at apogee and throw
// a burst of sparks that drag against the air, droop back down and gutter out.
// Six shell types (Mode) each break differently — peony, willow, crackle,
// ring, comet, finale — and Loop (on by default) walks through them so an
// unattended rig shows the whole repertoire.
//
// Physics runs in normalized sky coordinates (x right, y UP, ground at y = 0)
// and is deposited into a square virtual canvas that render2D samples
// bilinearly. That makes the show rig-independent: exactly 1:1 on a 16x16
// grid, smoothly upscaled on a 64x64 panel, still sensible on any other map.

var NMODES = 6
var COLS = 5              // colors per scheme

// ---- virtual canvas --------------------------------------------------------

var CW = clamp(round(sqrt(pixelCount)), 8, 32)
var CH = CW
var CELLS = CW * CH
var rBuf = array(CELLS)
var gBuf = array(CELLS)
var bBuf = array(CELLS)

// ---- controls (real units) -------------------------------------------------

var mode = 0              // shell type, or the starting type when looping
var loopModes = 1         // auto-cycle the types
var loopSeconds = 12      // seconds per type while looping
var launchRate = 32       // shells per minute
var gravityScale = 1      // 1 = earth-ish for a sky one map tall
var fadeSeconds = 1.4     // how long an ember burns
var scheme = 0            // color scheme

// 0 peony, 1 willow, 2 crackle, 3 ring, 4 comet, 5 finale
//# min=0 max=5 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, NMODES - 1) }

// On: Mode is where the show starts and it walks the types from there.
//# min=0 max=1 step=1 default=1
export function toggleLoop(v) { loopModes = (v > 0.5) }

//# min=4 max=60 step=1 default=12
export function sliderLoopSeconds(v) { loopSeconds = max(2, v) }

//# min=4 max=90 step=1 default=32
export function sliderLaunchRate(v) { launchRate = clamp(v, 1, 200) }

//# min=0.3 max=2.5 step=0.1 default=1
export function sliderGravity(v) { gravityScale = clamp(v, 0.1, 4) }

//# min=0.4 max=4 step=0.1 default=1.4
export function sliderFadeSeconds(v) { fadeSeconds = max(0.15, v) }

// 0 classic, 1 red/white/blue, 2 warm gold, 3 icy, 4 drifting rainbow
//# min=0 max=4 step=1 default=0
export function sliderColorScheme(v) { scheme = clamp(floor(v), 0, 4) }

// A hand-fired volley, for a button or a companion app.
export function triggerFinale() { volleyLeft = 12; launchTimer = 0 }

// ---- palette ---------------------------------------------------------------
// Five colors per scheme; entry 1 is always the scheme's soft/pale one, which
// is what willows and crackling salutes are drawn in.

var schemeH = array(4 * COLS)
var schemeS = array(4 * COLS)
// classic: red, gold, green, blue, violet
schemeH[0] = 0;    schemeS[0] = 1
schemeH[1] = 0.09; schemeS[1] = 0.8
schemeH[2] = 0.33; schemeS[2] = 1
schemeH[3] = 0.6;  schemeS[3] = 1
schemeH[4] = 0.8;  schemeS[4] = 0.9
// red / white / blue
schemeH[5] = 0;    schemeS[5] = 1
schemeH[6] = 0.08; schemeS[6] = 0.12
schemeH[7] = 0.62; schemeS[7] = 1
schemeH[8] = 0;    schemeS[8] = 1
schemeH[9] = 0.62; schemeS[9] = 1
// warm gold
schemeH[10] = 0.06; schemeS[10] = 1
schemeH[11] = 0.1;  schemeS[11] = 0.55
schemeH[12] = 0.03; schemeS[12] = 0.9
schemeH[13] = 0.12; schemeS[13] = 0.8
schemeH[14] = 0.01; schemeS[14] = 1
// icy
schemeH[15] = 0.5;  schemeS[15] = 1
schemeH[16] = 0.55; schemeS[16] = 0.25
schemeH[17] = 0.62; schemeS[17] = 1
schemeH[18] = 0.75; schemeS[18] = 0.85
schemeH[19] = 0.45; schemeS[19] = 0.9

var col = array(3)

function pickColor(idx) {
  if (scheme >= 4) {
    // drifting rainbow: the whole show walks the wheel over ~20 s
    hsv2rgb(frac(time(0.3) + idx * 0.11), idx == 1 ? 0.3 : 1, 1, col)
  } else {
    var i = scheme * COLS + idx
    hsv2rgb(schemeH[i], schemeS[i], 1, col)
  }
}

// ---- shells and sparks -----------------------------------------------------

var MAXSHELLS = 5
var PERSHELL = 22
var MAXSPARKS = MAXSHELLS * PERSHELL

var stage = array(MAXSHELLS)     // 0 idle, 1 rising, 2 broken
var shX = array(MAXSHELLS)
var shY = array(MAXSHELLS)
var shVX = array(MAXSHELLS)
var shVY = array(MAXSHELLS)
var shType = array(MAXSHELLS)
var shApogee = array(MAXSHELLS)
var shFlash = array(MAXSHELLS)   // burst-flash timer, seconds
var shBreak = array(MAXSHELLS)   // countdown to the next break, 0 = none
var shBreaks = array(MAXSHELLS)  // breaks still owed
var shUsed = array(MAXSHELLS)    // sparks this shell threw
var shR = array(MAXSHELLS)
var shG = array(MAXSHELLS)
var shB = array(MAXSHELLS)
var shDrag = array(MAXSHELLS)    // air brake, per second
var shDroop = array(MAXSHELLS)   // ember gravity scale
var shFade = array(MAXSHELLS)    // ember lifetime scale
var shStrobe = array(MAXSHELLS)  // 1 = crackling embers
var shBri = array(MAXSHELLS)     // payload split over the star count
var shFuse = array(MAXSHELLS)    // comet: seconds aloft

var pX = array(MAXSPARKS)
var pY = array(MAXSPARKS)
var pVX = array(MAXSPARKS)
var pVY = array(MAXSPARKS)
var pLife = array(MAXSPARKS)

var launchTimer = 0.4
var volleyLeft = 0
var cycleT = 0
var cycleIdx = 0
var activeMode = 0

// ---- canvas deposit --------------------------------------------------------

// Bilinear splat: a particle drifting a fraction of a cell per frame moves
// smoothly instead of stepping. y is flipped here (and only here) so the sky
// is up in map space.
function splat(x, y, r, g, b) {
  var fx = x * CW - 0.5
  var fy = (1 - y) * CH - 0.5
  var ix = floor(fx)
  var iy = floor(fy)
  var wx = fx - ix
  var wy = fy - iy
  // smoothstep the weights: the deposit still slides smoothly between cells,
  // but a star sitting near a cell centre keeps its energy there instead of
  // being smeared into a permanent 2x2 blob
  wx = wx * wx * (3 - 2 * wx)
  wy = wy * wy * (3 - 2 * wy)
  var okL = (ix >= 0 && ix < CW)
  var okR = (ix + 1 >= 0 && ix + 1 < CW)
  var i, w
  if (iy >= 0 && iy < CH) {
    if (okL) {
      i = iy * CW + ix
      w = (1 - wx) * (1 - wy)
      rBuf[i] += r * w; gBuf[i] += g * w; bBuf[i] += b * w
    }
    if (okR) {
      i = iy * CW + ix + 1
      w = wx * (1 - wy)
      rBuf[i] += r * w; gBuf[i] += g * w; bBuf[i] += b * w
    }
  }
  if (iy + 1 >= 0 && iy + 1 < CH) {
    if (okL) {
      i = (iy + 1) * CW + ix
      w = (1 - wx) * wy
      rBuf[i] += r * w; gBuf[i] += g * w; bBuf[i] += b * w
    }
    if (okR) {
      i = (iy + 1) * CW + ix + 1
      w = wx * wy
      rBuf[i] += r * w; gBuf[i] += g * w; bBuf[i] += b * w
    }
  }
}

// ---- launch and break ------------------------------------------------------

function launch(t) {
  var s = -1
  for (var i = 0; i < MAXSHELLS; i++) if (stage[i] == 0) { s = i; break }
  if (s < 0) return 0

  var g = 0.62 * gravityScale
  // the lift charge is fixed, so a heavier sky means lower bursts
  var h = clamp((0.46 + random(0.3)) / gravityScale, 0.18, 0.92)
  shApogee[s] = h
  shX[s] = 0.14 + random(0.72)
  shY[s] = 0
  shVX[s] = (random(2) - 1) * 0.04
  shVY[s] = sqrt(2 * g * h)      // exactly enough to coast to a stop up there
  shFuse[s] = 0
  if (t == 4) {
    // a comet does not go straight up: it is lobbed across the sky and the
    // long arc IS the effect, with only a small terminal break at the end
    var dir = random(1) < 0.5 ? -1 : 1
    shVX[s] = dir * (0.16 + random(0.13))
    shX[s] = dir > 0 ? random(0.2) : 1 - random(0.2)
    shVY[s] = sqrt(2 * g * clamp((0.6 + random(0.25)) / gravityScale, 0.2, 0.95))
  }
  stage[s] = 1
  shType[s] = t
  shFlash[s] = 0
  shBreak[s] = 0
  shBreaks[s] = 0
  shUsed[s] = 0

  var ci = floor(random(COLS))
  if (t == 1) ci = 1                      // willows burn in the pale/warm entry
  pickColor(ci)
  shR[s] = col[0]; shG[s] = col[1]; shB[s] = col[2]
  if (t == 2) {
    // a salute is a titanium flash: keep a tint, push it most of the way white
    shR[s] = col[0] * 0.3 + 0.7
    shG[s] = col[1] * 0.3 + 0.7
    shB[s] = col[2] * 0.3 + 0.7
  }
  splat(shX[s], 0.008, 0.9, 0.6, 0.22)    // muzzle flash on the ground
  return 1
}

function burst(s) {
  stage[s] = 2
  var t = shType[s]
  var n = 18
  var base = 0.85            // burst speed; drag stops a star at ~base/drag
  var tBri = 1               // per-type payload trim
  var jitter = 1
  var even = 0
  shFlash[s] = 0.1
  shStrobe[s] = 0
  shDrag[s] = 2.4
  shDroop[s] = 0.5
  shFade[s] = 1

  if (t == 1) {
    // willow: a soft slow break whose heavy embers hang, then pour downward
    n = 12; base = 0.8; shDrag[s] = 3.4; shDroop[s] = 1.7; shFade[s] = 1.7; tBri = 0.5
  } else if (t == 2) {
    // crackle: a hard fast salute that sizzles and breaks twice more
    n = 22; base = 1.15; shDrag[s] = 3.4; shDroop[s] = 0.4; shFade[s] = 0.5
    shStrobe[s] = 1; shFlash[s] = 0.16
    shBreak[s] = 0.2 + random(0.12); shBreaks[s] = 2
  } else if (t == 3) {
    // ring: identical speeds on evenly spaced angles, barely any drag or
    // gravity, so the front stays a clean expanding circle
    n = 20; base = 0.45; shDrag[s] = 1.2; shDroop[s] = 0.2; shFade[s] = 1.6
    even = 1; jitter = 0
  } else if (t == 4) {
    // comet: only a small terminal break — the arc was the show
    n = 9; base = 0.5; shDrag[s] = 2.6; shDroop[s] = 0.75; shFade[s] = 0.9
    shFlash[s] = 0.07
  } else if (t == 5) {
    // finale: a fat break that re-colors and re-throws itself once
    n = 16; base = 0.9; shDrag[s] = 2.2; shDroop[s] = 0.6; shFade[s] = 1.05
    shBreak[s] = 0.3 + random(0.16); shBreaks[s] = 1
  }

  n = min(n, PERSHELL)
  shUsed[s] = n
  shBri[s] = clamp(22 / n, 0.7, 1.8) * tBri
  for (var k = 0; k < n; k++) {
    var i = s * PERSHELL + k
    var a = (k + 0.5) / n * PI2
    if (even == 0) a += (random(1) - 0.5) * (PI2 / n)
    var sp = base
    if (jitter) sp = base * (0.5 + random(0.62))    // a ragged, filled break
    if (t == 2) sp = base * (0.3 + random(0.8))     // a salute fills its cloud
    pX[i] = shX[s]; pY[i] = shY[s]
    pVX[i] = cos(a) * sp + shVX[s] * 0.5
    pVY[i] = sin(a) * sp + shVY[s] * 0.3
    pLife[i] = 1
  }
}

// ---- frame -----------------------------------------------------------------

export function beforeRender(delta) {
  var dt = min(delta, 60) * 0.001

  // which shell type is being launched right now
  cycleT += dt
  if (cycleT >= loopSeconds) { cycleT -= loopSeconds; cycleIdx = mod(cycleIdx + 1, NMODES) }
  activeMode = loopModes ? mod(mode + cycleIdx, NMODES) : mode

  // afterglow: the trail length is part of the shell type's character
  var half = 0.14
  if (activeMode == 1) half = 0.24
  else if (activeMode == 2) half = 0.07
  else if (activeMode == 3) half = 0.12
  else if (activeMode == 4) half = 0.4
  else if (activeMode == 5) half = 0.16
  var decay = pow(0.5, dt / half)
  feedback(rBuf, decay)
  feedback(gBuf, decay)
  feedback(bBuf, decay)

  // schedule: jittered gaps, because a metronome never reads as a show
  launchTimer -= dt
  if (launchTimer <= 0) {
    var fired = launch(activeMode)
    if (volleyLeft > 0) {
      if (fired) { volleyLeft--; launchTimer = 0.25 + random(0.2) }
      else launchTimer = 0.1
    } else if (!fired) {
      launchTimer = 0.12
    } else {
      var rate = launchRate
      if (activeMode == 5) rate *= 2.2          // finales come in volleys
      else if (activeMode == 4) rate *= 0.7     // comets get the sky to themselves
      else if (activeMode == 1) rate *= 0.7     // willows hang, so give them room
      launchTimer = (60 / rate) * (0.65 + random(0.7))
    }
  }

  var g = 0.62 * gravityScale
  var lifeStep = dt / fadeSeconds

  for (var s = 0; s < MAXSHELLS; s++) {
    if (stage[s] == 0) continue

    if (stage[s] == 1) {
      // rising: a decelerating comet, drawn along the segment it covered so a
      // fast rocket leaves a streak instead of dashes
      var vy = shVY[s] - g * dt
      shVY[s] = vy
      var x0 = shX[s]
      var y0 = shY[s]
      var x1 = x0 + shVX[s] * dt
      var y1 = y0 + vy * dt
      shX[s] = x1; shY[s] = y1
      var comet = (shType[s] == 4)
      var steps = clamp(ceil(abs(y1 - y0) * CH + abs(x1 - x0) * CW), 1, 6)
      for (var k = 0; k < steps; k++) {
        var f = (k + 1) / steps
        var b = 0.55 + random(0.8)             // the fuse gutters as it climbs
        var xx = x0 + (x1 - x0) * f
        var yy = y0 + (y1 - y0) * f
        if (comet) splat(xx, yy, (shR[s] * 0.5 + 0.5) * b, (shG[s] * 0.5 + 0.35) * b, (shB[s] * 0.5 + 0.2) * b)
        else splat(xx, yy, b, 0.62 * b, 0.2 * b)
      }
      if (comet) {
        // a comet breaks where it lands, not at the top of its arc
        shFuse[s] += dt
        if (shFuse[s] > 0.5 && (y1 <= 0.12 || x1 < 0.04 || x1 > 0.96)) burst(s)
      } else if (y1 >= shApogee[s] || vy <= 0.02) {
        burst(s)
      }
      continue
    }

    // burst flash: a hot core that blooms a cell or two wide
    if (shFlash[s] > 0) {
      shFlash[s] -= dt
      var fb = shFlash[s] > 0 ? 1.5 : 0.6
      var e = 1.3 / CW
      splat(shX[s], shY[s], fb, fb, fb * 0.95)
      splat(shX[s] + e, shY[s], fb * 0.5, fb * 0.5, fb * 0.45)
      splat(shX[s] - e, shY[s], fb * 0.5, fb * 0.5, fb * 0.45)
      splat(shX[s], shY[s] + e, fb * 0.5, fb * 0.5, fb * 0.45)
      splat(shX[s], shY[s] - e, fb * 0.5, fb * 0.5, fb * 0.45)
    }

    // secondary break: the same cloud kicked outward again
    if (shBreak[s] > 0) {
      shBreak[s] -= dt
      if (shBreak[s] <= 0) {
        shBreaks[s]--
        shFlash[s] = 0.07
        var kick = shType[s] == 2 ? 0.5 : 0.34
        for (var k = 0; k < shUsed[s]; k++) {
          var i = s * PERSHELL + k
          if (pLife[i] <= 0) continue
          pVX[i] += (random(2) - 1) * kick
          pVY[i] += (random(2) - 1) * kick
          pLife[i] = max(pLife[i], 0.8)
        }
        if (shType[s] == 5) {
          pickColor(floor(random(COLS)))       // finale shells change color
          shR[s] = col[0]; shG[s] = col[1]; shB[s] = col[2]
        }
        shBreak[s] = shBreaks[s] > 0 ? 0.16 + random(0.12) : 0
      }
    }

    var dragStep = min(shDrag[s] * dt, 0.9)
    var drop = g * shDroop[s] * dt
    var step = lifeStep / shFade[s]
    var r = shR[s]
    var gg = shG[s]
    var b = shB[s]
    var strobe = shStrobe[s]
    var briScale = shBri[s]
    var living = 0
    for (var k = 0; k < shUsed[s]; k++) {
      var i = s * PERSHELL + k
      var life = pLife[i]
      if (life <= 0) continue

      var vx = pVX[i]
      var vy2 = pVY[i] - drop
      vx -= vx * dragStep
      vy2 -= vy2 * dragStep
      var px = pX[i] + vx * dt
      var py = pY[i] + vy2 * dt
      if (py <= 0) { py = 0; vy2 = 0; vx = 0; life -= step * 4 }  // burned out on the ground
      pVX[i] = vx; pVY[i] = vy2
      pX[i] = px; pY[i] = py

      life -= step
      pLife[i] = life
      if (life <= 0) continue
      living++
      if (px < -0.05 || px > 1.05) continue                   // drifted off the map

      var bri = life * life * briScale
      if (strobe) bri *= random(1) < 0.45 ? 0.12 : 1.6        // the sizzle
      else if (life < 0.4) bri *= random(1) < 0.35 ? 0.3 : 1.3 // guttering embers
      splat(px, py, r * bri, gg * bri, b * bri)
    }
    if (living == 0 && shFlash[s] <= 0) stage[s] = 0
  }
}

export function render2D(index, x, y) {
  rgb(
    saturate(canvasGet(rBuf, CW, x, y)),
    saturate(canvasGet(gBuf, CW, x, y)),
    saturate(canvasGet(bBuf, CW, x, y))
  )
}
