// name: Rock sparks
// Clean-room reimplementation from a prose functional description of the
// community pattern "Rock sparks"; original source never consulted.

// A three-mode 2D medley that auto-cycles every few seconds, blending
// between modes with a per-pixel stochastic dither crossfade: (A) confetti
// dust, (B) sweeping searchlights, (C) a spinning/zooming rainbow
// checkerboard. Independently, every frame it drives a digital output pin
// high only during a late-evening window (~9pm-11pm) off a synced real-time
// clock, to switch a relay for house lights. No sound reactivity despite the
// name. Needs a 2D map normalized 0..1; nothing hardcodes pixel count.

const RELAY_PIN = 25

// Mode B controls (defaults chosen to match the described default look).
var cCol = array(3)      // center beam color (strong green)
var eCol = array(3)      // edge beam color (violet-blue)
var beamWidth = 0.30     // angular offset between center and edge beams
var beamFocus = 0.03     // beam tightness / brightness
var beamDrive = 4        // overdrive ceiling
var speedScale = 1       // sweep speed multiplier
var vSweep = 0.5         // 0 = horizontal sweep bias, 1 = vertical

cCol[0] = 0; cCol[1] = 1; cCol[2] = 0
eCol[0] = 0.55; eCol[1] = 0.15; eCol[2] = 0.95

// Per-frame globals.
var mc = 0          // mode clock (0..1 over the whole ~10 s loop)
var phaseB = 0       // searchlight sweep accumulator
var palPhase = 0     // confetti palette phase clock
var bobX = 0
var bobY = 0

// --- Mode A: confetti dust (stateless per-pixel sparkle) ---
function drawA(x, y) {
  var phase = floor(palPhase * 4)          // 4 palette phases over ~4 s
  if (random(1) < 0.1) {                    // ~1-in-10 pixels lit
    var v = random(1)
    v = v * v * v                           // cubed: mostly dim, rare bright
    var hue = 0
    var sat = 1
    if (phase == 0) {
      // reds/pinks clustered at both ends of the hue wheel
      if (random(1) < 0.7) hue = random(0.05)        // deep red
      else hue = 1 - random(0.08)                    // other side of red
    } else if (phase == 1) {
      hue = 0.33 - random(0.05)             // greens, slight downward jitter
    } else if (phase == 2) {
      hue = random(1)                        // full spread, saturation forced
      sat = 1
    } else {
      hue = random(1)
      if (random(1) < 0.14) sat = 0          // ~1-in-7 sparks rendered white
    }
    hsv(hue, sat, v)
  } else {
    rgb(0, 0, 0)
  }
}

// --- Mode B: three sweeping searchlights over a 2D plane ---
function drawB(index, x, y) {
  var px = x - 0.5 + bobX                    // origin shifted to panel center
  var py = y - 0.5 + bobY
  var r = 0
  var g = 0
  var b = 0
  var s = 0
  for (s = 0; s < 3; s++) {
    var srcx = (s - 1) * 0.34                 // sources spaced along one axis
    var dx = px - srcx
    var dy = py
    var dir = sin(phaseB + s * 2.094)         // beam direction oscillates
    var ang = atan2(dy, dx) - dir
    // center beam
    var sc = abs(sin(ang))
    var ic = min(beamDrive, beamFocus / max(sc, 0.001))
    ic = ic * ic
    // offset edge beam
    var se = abs(sin(ang - beamWidth))
    var ie = min(beamDrive, beamFocus / max(se, 0.001))
    ie = ie * ie
    if (ic >= ie) {
      r += cCol[0] * ic; g += cCol[1] * ic; b += cCol[2] * ic
    } else {
      r += eCol[0] * ie; g += eCol[1] * ie; b += eCol[2] * ie
    }
  }
  rgb(clamp(r, 0, 1), clamp(g, 0, 1), clamp(b, 0, 1))
}

// --- Mode C: rotating, zooming rainbow checkerboard ---
function drawC(x, y) {
  var px = x - 0.5
  var py = y - 0.5
  var a = time(0.08) * PI2                    // full revolution every few s
  var ca = cos(a)
  var sa = sin(a)
  var rx = px * ca - py * sa + 0.5
  var ry = px * sa + py * ca + 0.4            // slightly asymmetric re-shift
  var zoom = 1 + triangle(time(0.06)) * 4     // ~1..5 squares across
  var cx = floor(rx * zoom)
  var cy = floor(ry * zoom)
  var on = mod(cx + cy, 2)                      // checkerboard parity gate
  var hue = frac(rx + ry + time(0.06)) * 0.66  // fold: never re-crosses red
  hsv(hue, 1, on > 0.5)
}

export function beforeRender(delta) {
  var dt = delta / 1000
  mc = time(0.15)                              // ~9.8 s full mode loop
  palPhase = time(0.06)                        // ~3.9 s palette cycle
  phaseB += dt * 1.5 * speedScale              // sweep accumulator

  // slow gentle bob: small triangle-wave drift biased by vertical-sweep
  var bob = (triangle(time(0.09)) - 0.5) * 0.15
  bobX = bob * (1 - vSweep)
  bobY = bob * vSweep

  // house lights: relay high only in the ~9pm-11pm window (hour granularity)
  var h = clockHour()
  digitalWrite(RELAY_PIN, (h >= 21 && h < 23) ? HIGH : LOW)
}

export function render2D(index, x, y) {
  var mode = min(floor(mc * 3), 2)
  var localPhase = mc * 3 - mode
  var useMode = mode
  // Crossfade over the last third of a mode's slot. Fixed here to work for
  // every transition (the original derived progress from the clock modulo
  // two while running three modes, so some cuts were hard) and noted.
  if (localPhase > 0.6667) {
    var prog = (localPhase - 0.6667) / 0.3333
    var eased = (1 - cos(prog * PI)) / 2       // sine-shaped ease 0..1
    if (random(1) < eased) useMode = mod(mode + 1, 3)
  }
  if (useMode == 0) drawA(x, y)
  else if (useMode == 1) drawB(index, x, y)
  else drawC(x, y)
}

// --- UI controls (Mode B) ---
var pc = array(3)
export function hsvPickerCenterBeamColor(h, s, v) {
  hsv2rgb(h, s, v, pc)
  cCol[0] = pc[0]; cCol[1] = pc[1]; cCol[2] = pc[2]
}
export function hsvPickerEdgeBeamColor(h, s, v) {
  hsv2rgb(h, s, v, pc)
  eCol[0] = pc[0]; eCol[1] = pc[1]; eCol[2] = pc[2]
}
export function sliderWidth(v) {
  //# min=0 max=1 step=0.01 default=0.35
  beamWidth = 0.02 + v * 0.8
}
export function sliderFocus(v) {
  //# min=0 max=1 step=0.01 default=0.3
  beamFocus = 0.005 + v * 0.1
}
export function sliderDrive(v) {
  //# min=0 max=1 step=0.01 default=0.4
  beamDrive = 1 + v * v * 9                    // unity up to ~10, quadratic
}
export function sliderSpeed(v) {
  //# min=0 max=1 step=0.01 default=0.3
  speedScale = 1 + v * 2                        // ~factor-of-three range
}
export function sliderVerticalSweep(v) {
  //# min=0 max=1 step=0.01 default=0.5
  vSweep = v
}
