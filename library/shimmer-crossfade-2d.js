// name: Shimmer Crossfade 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Shimmer Crossfade 2D"; original source never consulted.

// A 2D demo/framework that cycles through three sub-scenes — a rotating
// white radar line, rainbow plasma rings, and a spinning/zooming rainbow
// checkerboard — dwelling several seconds on each. Transitions are done by
// stochastic dithering: during the crossfade window each pixel randomly
// picks the outgoing or incoming scene every frame, with the incoming
// probability easing from 0 to 1, so scenes dissolve into each other in a
// twinkling shimmer. No color blending; sub-patterns need no rewriting.

var MODES = 3
var DWELL = 5           // seconds per sub-pattern
var XFADE = 0.33        // last third of each slot spends crossfading

var mode = 0            // current sub-pattern index
var fadeP = 0           // eased probability of sampling the incoming scene

// Parallel arrays of function references — the extensible part: to add a
// scene, register its per-frame setup and per-pixel renderer here.
var setups = array(MODES)
var renderers = array(MODES)

// ---- Sub-pattern 1: rotating white line ------------------------------
var lineSlope = 0
var lineNorm = 1

function setupLine() {
  // Full rotation over one dwell period; tangent -> slope, clamped so
  // squaring it can't overflow 16.16 range.
  var angle = time(DWELL / 65.536) * PI
  lineSlope = clamp(tan(angle), -150, 150)
  lineNorm = sqrt(lineSlope * lineSlope + 1)
}

function drawLine(x, y) {
  // Perpendicular distance to a line of that slope through the center.
  var d = abs(lineSlope * (x - 0.5) - (y - 0.5)) / lineNorm
  var v = max(0, 1 - d / 0.2)
  hsv(0, 0, v * v)   // pure white, sharpened falloff
}

// ---- Sub-pattern 2: rainbow plasma rings -----------------------------
var plasmaPh1 = 0
var plasmaPh2 = 0
var plasmaZoom = 1

function setupPlasma() {
  plasmaPh1 = time(0.05) * PI2            // ~3.3 s
  plasmaPh2 = time(0.1) * PI2             // ~6.6 s, half the rate
  plasmaZoom = mix(4, 14, wave(time(0.2)))  // slow ~13 s breathing
}

function drawPlasma(x, y) {
  // Classic plasma field, normalized to 0..1; used as hue AND (cubed,
  // halved) as brightness so dim regions crush to black rings/blobs.
  var f = (sin(x * plasmaZoom + plasmaPh1) + cos(y * plasmaZoom + plasmaPh2)) / 4 + 0.5
  hsv(f, 1, f * f * f / 2)
}

// ---- Sub-pattern 3: rotating checkerboard ----------------------------
function setupChecker() {
  // stateless: everything happens per pixel
}

function drawChecker(x, y) {
  // Full turn every ~8 s about the matrix center.
  var angle = time(8 / 65.536) * PI2
  var c = cos(angle)
  var s = sin(angle)
  var cx = x - 0.5
  var cy = y - 0.5
  // Rotate, then shift back — deliberately unevenly, which nudges the
  // checker pattern off-center (harmless quirk of the original).
  var u = cx * c - cy * s + 0.5
  var v = cx * s + cy * c + 0.6

  // Faster clock drives both the zoom breathing and the color drift.
  var t2 = time(4 / 65.536)
  var blocks = 0.5 + 3 * triangle(t2)

  var parity = mod(floor(u * blocks) + floor(v * blocks), 2)
  // Gentle diagonal rainbow slice, sliding slowly.
  hsv(t2 + (u + v) * 0.15, 1, parity)
}

// ---- Framework -------------------------------------------------------
setups[0] = setupLine
setups[1] = setupPlasma
setups[2] = setupChecker
renderers[0] = drawLine
renderers[1] = drawPlasma
renderers[2] = drawChecker

export function beforeRender(delta) {
  // Master clock ramps 0..MODES over the whole cycle: integer part is the
  // current mode, fractional part is progress within its slot.
  var clock = time(DWELL * MODES / 65.536) * MODES
  mode = floor(clock)
  var slot = clock - mode

  // Progress into the crossfade: 0 for most of the slot, then a linear
  // ramp across the final XFADE fraction — smoothed into an S-curve so
  // the dissolve starts and ends gently.
  var prog = max(0, (slot - (1 - XFADE)) / XFADE)
  fadeP = prog * prog * (3 - 2 * prog)

  // Run every scene's per-frame setup (noted inefficiency, kept simple).
  for (var i = 0; i < MODES; i++) {
    var setup = setups[i]
    setup()
  }
}

export function render2D(index, x, y) {
  // Bernoulli dither: with probability fadeP show the incoming scene.
  var pick = random(1) < fadeP ? 1 : 0
  var m = mod(mode + pick, MODES)
  var draw = renderers[m]
  draw(x, y)
}
