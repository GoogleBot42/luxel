// name: StarGen polar 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "StarGen polar 2D"; original source never consulted.

// A self-advancing playlist of twelve polar "star/figure" animations for a
// circular disc with a POLAR pixel map: coordinate 1 = radius (0 center,
// 1 rim), coordinate 2 = angle in turns, coordinate 3 = azimuth from the
// pole (0.5 = equatorial plane). Consecutive modes hand over via a
// stochastic per-pixel "shimmer" dissolve rather than an alpha blend.

var NUM_MODES = 12

var tNow = 0
var dwellS = 10          // seconds per mode
var modeOverride = 0     // 0 = auto playlist, 1..12 pins a mode
var rotOff = 0           // physical-orientation rotation, in turns
var lineHw = 0.1         // proximity half-width for the curve drawing

var curMode = 0
var nextMode = 1
var fadeP = 0            // probability a pixel is drawn by the next mode

//# min=0 max=1 step=0.077 default=0
export function sliderModeOverride(v) {
  modeOverride = floor(v * (NUM_MODES + 0.999))
}

//# min=0 max=1 step=0.01 default=0.15
export function sliderDwellTime(v) {
  dwellS = 0.5 + v * 63
}

//# min=0 max=1 step=0.01 default=0
export function sliderRotationOffset(v) {
  rotOff = v
}

//# min=0.02 max=0.3 step=0.01 default=0.1
export function sliderLineWidth(v) {
  lineHw = max(0.02, v)
}

// ---- shared helpers ------------------------------------------------

// Slow sinusoidal oscillator, 0..1, with a period in seconds.
function osc(period) {
  return wave(tNow / period)
}

// Proximity: 1 when v == target, falling to 0 once they differ by more
// than hw; falloff squared for a gamma-corrected soft edge.
function near(v, target, hw) {
  var d = abs(v - target)
  if (d >= hw) return 0
  d = 1 - d / hw
  return d * d
}

// Same, for wrapped (angular, 0..1) quantities via triangle distance.
function nearWrap(v, target, hw) {
  var d = v - target
  d -= floor(d)
  if (d > 0.5) d = 1 - d
  if (d >= hw) return 0
  d = 1 - d / hw
  return d * d
}

// Star-polygon family: n points, pinch = 1 gives the regular polygon,
// larger pinch pulls the edges inward into a star. Returns radius (max 1)
// at angle theta (radians).
function starR(theta, n, pinch) {
  var a = acos(cos(n * theta)) / n * pinch
  return cos(PI * pinch / n) / cos(a)
}

// ---- mode 0: orbits / ellipse --------------------------------------
var eA = 0.4, eB = 0.4, eRot = 0, eSkew = 0, eSat = 0.85, eHue = 0.075

// ---- mode 1: six-lobed sinus star ----------------------------------
var sBase = 0.5

// ---- mode 3: Star of David (kaleidoscopic) -------------------------
var sdC = 0.35, sdRot = 0

// ---- mode 4: Star over Bethlehem -----------------------------------
var bPw = 1, bMask = 0

// ---- mode 5: pentagram ----------------------------------------------
var pScale = 0.6, penHw = 0.07

// ---- mode 8: heart ---------------------------------------------------
var hScale = 0.45, hSat = 1

// ---- mode 9: bird flap -----------------------------------------------
var bFlap = 0, bBend = 0.6, bGlow = 1

// ---- mode 10: rainbow spirals ---------------------------------------
var spS = 1, spM = 0.3, spArms = 2

// ---- mode 11: snowglobe (1D sparks over pixel index) -----------------
var NSPARKS = 20
var skPos = array(NSPARKS)
var skVel = array(NSPARKS)
var energy = array(pixelCount)
var skMaxV = pixelCount / 2
var skInit = 0

function respawnSpark(i) {
  skPos[i] = random(pixelCount)
  skVel[i] = (random(2) - 1) * skMaxV   // either direction
}

function setupMode(m, delta) {
  if (m == 0) {
    eA = 0.28 + 0.22 * osc(7.3)
    eB = 0.28 + 0.22 * osc(11.1)
    eRot += delta / 1000 * (0.03 + 0.09 * osc(19))   // slowly varying spin
    eSkew = max(0, osc(31) - 0.85) * 6               // occasional spiral smear
    eSat = 0.68 + 0.27 * min(1, abs(eA - eB) * 5)    // dips when circular
    eHue = 0.075 + max(0, osc(13) - 0.9) * 0.8       // brief drift at one extreme
  } else if (m == 1) {
    sBase = 0.5 + 0.12 * osc(8.2)
  } else if (m == 3) {
    sdC = 0.28 + 0.16 * osc(12.7)   // chord scale breathes
    sdRot = 0.2 * osc(41)           // rotation drifts
  } else if (m == 4) {
    bPw = 0.6 + 1.8 * osc(16)       // ray spread exponent breathes
    bMask = triangle(tNow / 6)      // diagonal rays fade in/out
  } else if (m == 5) {
    // damped-sinc-flavored size envelope on a very slow triangle LFO:
    // long gentle drift punctuated by big swings
    var x = 0.4 + triangle(tNow / 420) * 9
    pScale = clamp(0.55 + 0.35 * sin(x * PI2) / x, 0.15, 0.95)
    // stroke weight wanders randomly within bounds
    penHw += random(0.006) - 0.003
    penHw = clamp(penHw, 0.03, 0.12)
  } else if (m == 8) {
    var pulse = osc(5.5)
    hScale = 0.35 + 0.18 * pulse
    hSat = 0.55 + 0.45 * pulse      // whitens as it shrinks
  } else if (m == 9) {
    bFlap = abs(cos(tNow * PI2 / 2.8))   // the wingbeat
    bBend = 0.5 + 0.35 * osc(9.3)
    bGlow = 0.55 + 0.45 * osc(6.1)
  } else if (m == 10) {
    spS = 0.8 + 2.2 * osc(27)
    spM = 0.18 + 0.35 * osc(43)
    spArms = 1 + floor(3 * osc(61))
  } else if (m == 11) {
    var i
    if (!skInit) {
      skInit = 1
      for (i = 0; i < NSPARKS; i++) respawnSpark(i)
    }
    var dt = delta / 1000 * 0.1     // deliberately slowed 10x
    var friction = pixelCount / 3   // friction inversely felt vs. strip length
    arrayReplace(energy, 0)         // accumulation buffer cleared every frame
    for (i = 0; i < NSPARKS; i++) {
      var v = skVel[i]
      if (v > 0) v = max(0, v - friction * dt)
      else v = min(0, v + friction * dt)
      skVel[i] = v
      skPos[i] += v * dt
      if (abs(v) < 0.4 || skPos[i] < 0 || skPos[i] >= pixelCount) {
        respawnSpark(i)
      } else {
        energy[floor(skPos[i])] += abs(v) / skMaxV * 1.4
      }
    }
  }
}

function drawMode(m, index, r, a) {
  var th, v, d
  if (m == 0) {
    // orbits: a breathing, spinning, occasionally skewed ellipse
    if (r < 0.02) {
      hsv(time(0.3), 1, 0.5)        // colorful little heart at dead center
      return
    }
    th = frac(a + eRot + eSkew * r * 0.15) * PI2
    var ca = eB * cos(th)
    var sa = eA * sin(th)
    var rE = eA * eB / sqrt(ca * ca + sa * sa)
    hsv(eHue, eSat, near(r, rE, lineHw))
  } else if (m == 1) {
    // six-lobed sinus star, pure white
    v = near(r, sBase + 0.14 * cos(PI2 * (a * 6 + tNow * 0.18)), lineHw)
    hsv(0, 0, v)
  } else if (m == 2) {
    // sinus shimmer: same construction, lobes many times finer, near rim
    v = near(r, 0.84 + 0.1 * cos(PI2 * (a * 37 + tNow * 0.3)), lineHw)
    hsv(0, 0, v)
  } else if (m == 3) {
    // Star of David: 12 alternating mirrored sectors of a chord equation
    var k = frac(a + sdRot) * 12
    var lo = frac(k) - 0.5
    if (mod(floor(k), 2) == 1) lo = -lo
    v = near(r, sdC / cos(lo * 1.9), lineHw)
    hsv(0, 0, v)
  } else if (m == 4) {
    // Star over Bethlehem: eight radial rays, warm, white-hot center
    var g = frac(a * 8)
    d = min(g, 1 - g)
    var sp = 0.45 * pow(1.01 - r, bPw) + 0.02
    v = near(d, 0, sp)
    if (mod(floor(a * 8 + 0.5), 2) == 1) v *= bMask   // diagonals breathe
    hsv(0.04 + 0.05 * r, min(1, pow(r, 1.5) * 2), v)
  } else if (m == 5) {
    // pentagram {5/2}, slow spin, wandering stroke weight
    th = frac(a + tNow / 45) * PI2
    v = near(r, starR(th, 5, 2) * pScale, penHw * (0.5 + pScale))
    hsv(0, 0, v)
  } else if (m == 6) {
    // decagram, thick stroke, glacial spin, solid inner fill
    th = frac(a + tNow / 300) * PI2
    v = near(r, starR(th, 10, 4) * 0.85, 0.12)
    if (r < 0.3) v = 1
    hsv(0, 0, v)
  } else if (m == 7) {
    // hexagram
    th = frac(a + tNow / 40) * PI2
    hsv(0, 0, near(r, starR(th, 6, 2) * 0.8, 0.08))
  } else if (m == 8) {
    // heart: polar heart curve, pulsing scale, whitening as it shrinks
    th = frac(a + 0.5) * PI2        // half-turn shift so it points right
    var sn = sin(th)
    var rH = (sn * sqrt(abs(cos(th))) / (sn + 1.4) - 2 * sn + 2) * hScale / 3.2
    hsv(0, hSat, near(r, rH, 0.09))
  } else if (m == 9) {
    // bird flap: chord-like wings off a fixed heading, animated dihedral
    d = a - 0.25
    d -= floor(d + 0.5)
    th = abs(d) * PI2 * bBend - (bFlap - 0.5) * 1.1
    v = near(r, 0.22 / cos(th), 0.05 + 0.09 * r)   // wider at wingtips
    hsv(0.045, 0.9, v * bGlow)
  } else if (m == 10) {
    // rainbow spirals: wrapped scaled radius vs. scaled angle
    var kk = mod(r * spS, spM) / spM
    v = nearWrap(kk, frac(a * spArms + tNow * 0.04), 0.28)
    // perceptual rainbow correction: sine reshaping squeezes the green band
    var h = frac(r * 0.7 + a + tNow * 0.015)
    hsv(frac(h + 0.07 * sin(PI2 * h)), 1, v)
  } else {
    // snowglobe: 1D sparks energy buffer read back by pixel index
    var e = energy[floor(index)]
    var b = min(1, e)
    hsv(0.63, clamp(1 - e, 0, 1), b * b)   // deep blue traces, white-hot cores
  }
}

export function beforeRender(delta) {
  tNow += delta / 1000
  if (modeOverride > 0) {
    curMode = modeOverride - 1
    fadeP = 0
    setupMode(curMode, delta)
    return
  }
  curMode = mod(floor(tNow / dwellS), NUM_MODES)
  nextMode = mod(curMode + 1, NUM_MODES)
  // constant-feeling transition window at the tail of each dwell
  var fadeLen = min(2.5, dwellS * 0.25)
  var tm = mod(tNow, dwellS)
  if (tm > dwellS - fadeLen) {
    var u = (tm - (dwellS - fadeLen)) / fadeLen
    fadeP = (1 - cos(u * PI)) / 2         // sinusoidal easing, not linear
  } else {
    fadeP = 0
  }
  setupMode(curMode, delta)
  if (fadeP > 0) setupMode(nextMode, delta)
}

export function render3D(index, r, a, az) {
  // stochastic shimmer dissolve: each pixel independently picks a scene
  var m = curMode
  if (fadeP > 0 && random(1) < fadeP) m = nextMode
  drawMode(m, index, r, frac(a + rotOff + 1))
}

export function render2D(index, r, a) {
  render3D(index, r, a, 0.5)      // equatorial slice
}

export function render(index) {
  render3D(index, index / pixelCount, 0, 0.5)   // degenerate radial fallback
}
