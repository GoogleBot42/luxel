// name: 2D Fireworks Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "2D Fireworks Fade"; original source never consulted.

// A mini playlist engine cycling six red/white/blue sub-effects with a
// stochastic per-pixel dissolve between them: sparkle dust, pulsing plasma
// bands, RWB accent lights, center-out fireworks sparks, warm-white house
// lights, and a flag comet trio. Installation-specific bits (GPIO relay,
// clock-gated hours, hardcoded segment index) are parameterized/dropped.
// The horizontal coordinate is treated as linear position along the strip.

var NMODES = 6
var STRIP = 64            // virtual strip resolution for the particle sim
var BOUNDARY = 0.55       // former hardcoded segment split (now a fraction)

// ---- controls ----
var modeDur = 15          // seconds per mode
var fadeFrac = 0.1        // last tenth of a slot is the crossfade
var houseSpacing = 7      // mode-5 spacing after the boundary
var houseOffset = 0

//# min=3 max=30 step=1 default=15
export function sliderModeDuration(v) { modeDur = 3 + v * 27 }

//# min=0.02 max=0.4 step=0.01 default=0.1
export function sliderCrossfade(v) { fadeFrac = 0.02 + v * 0.38 }

//# min=1 max=12 step=1 default=7
export function sliderHouseSpacing(v) { houseSpacing = 1 + floor(v * 11) }

//# min=0 max=6 step=1 default=0
export function sliderHouseOffset(v) { houseOffset = floor(v * 6) }

// ---- playlist state ----
var clock = 0
var curMode = 0
var fadeProg = 0          // 0 outside the fade window, ramps 0..1 within it

// ---- per-frame animation values shared by the modes ----
var plasmaOsc = 0, plA = 0, plB = 0, plC = 0, plHue = 0
var cometPos = 0

// ---- particle sim (mode 4) ----
var NSPARK = floor(STRIP / 6)
var sEnergy = array(NSPARK)
var sPos = array(NSPARK)
var heat = array(STRIP)
var MAXE = 40

var si
for (si = 0; si < NSPARK; si++) {
  // scatter along the strip, energy decreasing with distance from center,
  // directions pointing away from center (as if already in flight)
  var p = random(STRIP)
  sPos[si] = p
  var half = STRIP / 2
  var dd = (p - half) / half            // -1..1
  var e = MAXE * (1 - abs(dd)) * 0.6 + 4
  sEnergy[si] = (dd >= 0) ? e : -e
}

function respawn(k) {
  var e = MAXE * 0.5 * (1 + (random(1) - 0.5) * 0.66)
  sEnergy[k] = (random(1) < 0.5) ? e : -e
  sPos[k] = STRIP / 2
}

export function beforeRender(delta) {
  var dt = delta / 1000
  clock = mod(clock + dt, modeDur * NMODES)
  curMode = floor(clock / modeDur)
  var local = (clock - curMode * modeDur) / modeDur   // 0..1 within slot
  fadeProg = (local > (1 - fadeFrac)) ? (local - (1 - fadeFrac)) / fadeFrac : 0

  // Mode 2 plasma: three slow sinusoids of different periods + a moving term.
  plasmaOsc = time(0.02) * PI2
  plA = sin(time(0.03) * PI2) * 6
  plB = sin(time(0.045) * PI2) * 4
  plC = sin(time(0.06) * PI2) * 3
  plHue = time(0.5)                       // slow downward creep

  // Mode 6 comet lap ~5 s.
  cometPos = time(0.076)

  // Mode 4 particle sim runs every frame regardless of active mode.
  var hd = clamp(1 - dt * 2.5, 0.5, 0.97)  // capped multiplicative decay
  var i
  for (i = 0; i < STRIP; i++) heat[i] = heat[i] * hd
  for (i = 0; i < NSPARK; i++) {
    sEnergy[i] = sEnergy[i] * (1 - dt * 0.8)          // friction, sign-preserving
    sPos[i] = sPos[i] + sEnergy[i] * dt
    if (abs(sEnergy[i]) < 2 || sPos[i] < 0 || sPos[i] >= STRIP) {
      respawn(i)
    }
    var hi = floor(sPos[i])
    if (hi >= 0 && hi < STRIP) heat[hi] = min(1.2, heat[hi] + abs(sEnergy[i]) / MAXE * dt * 6)
  }
}

// ---- per-mode per-pixel renderers (last color call wins) ----

function mode1(x, y) {          // sparkle dust
  if (random(1) < 0.02) {
    var phase = floor(time(0.05) * 4)     // ~3.3 s cycle, 4 phases
    var white = random(1) < 0.1
    if (phase == 0) {                     // reds
      if (white) hsv(0, 0, 1)
      else hsv(0 + (random(1) - 0.5) * 0.05, 1, 1)
    } else if (phase == 1) {              // blues
      if (white) hsv(0, 0, 1)
      else hsv(0.66 + (random(1) - 0.5) * 0.05, 1, 1)
    } else {                              // white / hold
      hsv(0, 0, 1)
    }
  } else {
    hsv(0, 0, 0)
  }
}

function mode2(x, y) {          // pulsing plasma bands
  var t = triangle(plasmaOsc / PI2 + x * plA + y * plB + plC)
  var bri = t
  bri = bri * bri * bri * bri * bri       // ^5 thins bright bands
  var sat = (bri > 0.6) ? (1 - (bri - 0.6) * 2.5) : 1
  // alternate red / blue families on a repeating cycle
  var hue = (floor(time(0.05)) % 2 == 0) ? 0.02 : 0.66
  hue = hue - plHue * 0.1
  hsv(hue, clamp(sat, 0, 1), clamp(bri, 0, 1))
}

function mode3(x, y) {          // RWB accent lights, every 4th pixel
  var i = floor(x * (STRIP - 0.01))
  var shifted = (x > BOUNDARY) ? i + 1 : i
  if (shifted % 4 == 0) {
    var c = floor(shifted / 4) % 3
    if (c == 0) hsv(0, 1, 1)              // red
    else if (c == 1) hsv(0, 0, 1)         // white
    else hsv(0.66, 1, 1)                  // blue
  } else {
    hsv(0, 0, 0)
  }
}

function mode4(x, y) {          // center-out fireworks sparks
  var i = floor(x * (STRIP - 0.01))
  var h = heat[i]
  var bri = h * h
  var frac1 = i / STRIP
  var hue, sat
  if (frac1 < 0.4) { hue = 0; sat = 1 - h * 0.6 }        // red zone
  else if (frac1 < 0.6) { hue = 0; sat = 1 - h }          // white zone
  else { hue = 0.66; sat = 1 - h * 0.6 }                  // blue zone
  hsv(hue, clamp(sat, 0, 1), clamp(bri, 0, 1))
}

function mode5(x, y) {          // warm-white house lights
  var i = floor(x * (STRIP - 0.01))
  var lit = 0
  if (x <= BOUNDARY) {
    lit = (i % 4 == 0)
  } else {
    lit = ((i + houseOffset) % houseSpacing == 0)
  }
  if (lit) hsv(0.09, 0.35, 1)             // warm incandescent white
  else hsv(0, 0, 0)
}

function mode6(x, y) {          // flag comet trio
  var i = x                                // normalized position
  var lead = cometPos
  var gap = 4.0 / STRIP
  var w = 1.0 / STRIP
  var r = 0, g = 0, b = 0
  if (abs(frac(i - lead + 1)) < w) b = 1                      // blue leader
  if (abs(frac(i - (lead - gap) + 1)) < w) { r += 1; g += 1; b += 1 }   // white
  if (abs(frac(i - (lead - gap * 2) + 1)) < w) r += 1          // red
  rgb(clamp(r, 0, 1), clamp(g, 0, 1), clamp(b, 0, 1))
}

function renderMode(m, x, y) {
  if (m == 0) mode1(x, y)
  else if (m == 1) mode2(x, y)
  else if (m == 2) mode3(x, y)
  else if (m == 3) mode4(x, y)
  else if (m == 4) mode5(x, y)
  else mode6(x, y)
}

export function render2D(index, x, y) {
  var m = curMode
  if (fadeProg > 0) {
    // temporal dither dissolve: more pixels flip to the incoming mode as the
    // eased progress rises
    var eased = fadeProg * fadeProg * (3 - 2 * fadeProg)   // smoothstep S-curve
    if (random(1) < eased) m = (curMode + 1) % NMODES
  }
  renderMode(m, x, y)
}
