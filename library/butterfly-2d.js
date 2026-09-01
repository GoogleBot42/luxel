// name: Butterfly 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Butterfly 2D"; original source never consulted.

// One butterfly drawn explicitly and mirrored about the body axis. Each wing
// pair is a petal lobe of a polar boundary function radiating from the wing
// root — a big swept forewing above, a smaller rounded hindwing below, with a
// waist notch between them — over a dark body spine (abdomen, thorax, head)
// and a thin antenna pair. The wings beat by anisotropic horizontal scaling
// (folded toward vertical, then thrown wide open) and carry a pale margin
// band plus one eyespot per wing in a golden-ratio-spaced companion hue.
// Each cycle the butterfly rises in from below, flutters harder and harder
// and lifts off the top; the next one is born with a fresh hue and a fresh
// resting tilt.

var GOLDEN = 0.61803399    // golden-ratio conjugate hue step

// control-backed tunables
var flapPeak = 3.0         // peak wingbeats per second
var cycleClock = 0.122     // time() clock for one butterfly's arc (~8 s)
var hueBase = 0.0778       // base wing hue (0..1)
var pat = 0.7              // wing-pattern intensity 0..1

// state kept across frames
var flapPhase = 0          // integrates the flap frequency
var open = 1               // 0.62 (folded) .. 1.00 (wings wide)
var lift = 0               // vertical fly-in / fly-away offset
var tilt = 0               // resting tilt of this butterfly
var lastPhase = 0          // cycle sawtooth from the previous frame
var jitter = 0             // per-butterfly hue nudge
var hue = 0

// Peak wingbeats per second at the frantic end of the cycle.
//# min=0.5 max=8 step=0.1 default=3
export function sliderFlapSpeed(v) { flapPeak = clamp(v, 0.2, 12) }

// Seconds one butterfly gets: rise in, flutter, lift away, reborn.
//# min=3 max=40 step=1 default=8
export function sliderCycleSeconds(v) { cycleClock = max(v, 1) / 65.536 }

// Base wing hue in degrees; the eyespots sit a golden-ratio step away from
// it and the margin band is a pale wash of the same hue.
//# min=0 max=359 step=1 default=28
export function sliderWingHue(v) { hueBase = frac(v / 360) }

// How hard the wing markings are pushed: 0 leaves a plain single-hue wing,
// 100 gives full-contrast margin banding and eyespots.
//# min=0 max=100 step=1 default=70
export function sliderPatternPercent(v) { pat = clamp(v, 0, 100) / 100 }

function reseed() {
  tilt = (random(1) - 0.5) * 0.52     // +/- ~15 degrees, stays upright
  jitter = (random(1) - 0.5) * 0.16   // small hue drift per butterfly
}
reseed()

export function beforeRender(delta) {
  var dt = delta / 1000

  // One sawtooth per butterfly: 0 = just born, 1 = gone.
  var p = time(cycleClock)
  if (p < lastPhase) {
    reseed()                          // wrapped -> a brand-new butterfly
  }
  lastPhase = p

  // Flap oscillator: lazy at rest, frantic just before takeoff.
  var freq = flapPeak * (0.25 + 0.75 * p * p)
  flapPhase = frac(flapPhase + freq * dt)
  // eased so the beat snaps shut and dwells open — a folded butterfly is a
  // sliver, and dwelling there would cost the silhouette on a small grid
  var w = wave(flapPhase)
  open = 0.62 + 0.38 * (1 - (1 - w) * (1 - w))

  // Vertical arc: rise in from below, hold center, lift off the top.
  lift = 1.15 * smoothstep(0.88, 1, p) - 1.15 * (1 - smoothstep(0, 0.09, p))

  hue = frac(hueBase + jitter)

  resetTransform()
  translate(-0.5, -0.5)               // origin at display center
  rotate(tilt)
}

// Axis-aligned ellipse field: > 0 inside, 1 at the center, 0 on the boundary.
function ell(u, v, cx, cy, rx, ry) {
  var ex = (u - cx) / rx
  var ey = (v - cy) / ry
  return 1 - sqrt(ex * ex + ey * ey)
}

// One wing petal: a lopsided gaussian bump in the polar boundary radius,
// centered on `dir`, wider on the outboard side than the inboard one.
function lobe(th, dir, amp, wLo, wHi) {
  var d = th - dir
  var w = wLo
  if (d > 0) {
    w = wHi
  }
  var q = d / w
  return amp * exp(-q * q)
}

export function render2D(index, x, y) {
  var u = abs(x)            // mirror about the body axis -> both wings at once
  var v = -y - lift         // + is up; lift slides the whole insect vertically

  // --- body: abdomen, thorax, head. Never scaled by the flap. ---
  var body = max(max(ell(u, v, 0, -0.120, 0.048, 0.190),
                     ell(u, v, 0, 0.115, 0.056, 0.120)),
                 ell(u, v, 0, 0.250, 0.046, 0.050))

  // --- antennae: two thin strokes sweeping up and out from the head ---
  var ant = 0
  var av = v - 0.28
  if (av > 0 && av < 0.15) {
    ant = 1 - abs(u - (0.030 + av * 0.85)) / 0.028
  }

  // --- wings: fold horizontally, stretch a touch when folded ---
  var uu = u / open
  var vv = v / (1 + 0.14 * (1 - open))

  // polar frame at the wing root, widened horizontally so the span reads
  var ex = (uu - 0.015) / 1.55
  var dv = vv - 0.030
  var rr = sqrt(ex * ex + dv * dv)
  var th = atan2(dv, ex)

  var bound = max(lobe(th, 0.70, 0.355, 0.82, 0.50),    // forewing
                  lobe(th, -0.95, 0.300, 0.62, 0.75))   // hindwing
  bound = bound * (1 + 0.035 * sin(th * 11))            // scalloped margin
  bound = max(bound, 0.090)                             // wing roots meet the body

  var wing = bound - rr                 // > 0 inside the wing
  var depth = wing / bound              // 0 at the margin, 1 at the root

  var wa = smoothstep(-0.020, 0.012, wing)
  var ba = smoothstep(-0.020, 0.012, body)

  // --- wing markings ---
  // eyespots: one on each forewing, one on each hindwing
  var spot = clamp(max(1 - dist(uu, vv, 0.252, 0.160) / 0.042,
                       1 - dist(uu, vv, 0.176, -0.118) / 0.036) * 4, 0, 1)

  // Two hues only, a golden-ratio step apart, both near full brightness — at
  // 16x16 a wing is about four pixels across, so anything busier, or any
  // pattern carried by luminance, reads as noise instead of markings.
  var dh = 0                            // hue offset of this plate, in turns
  var sv = 1                            // saturation
  var bv = 0.90                         // brightness
  if (depth < 0.10) {
    sv = 0.30                           // pale margin band
    bv = 1
  }
  if (spot > 0.5) {
    dh = GOLDEN - 1                     // eyespots, one golden step off
    sv = 1
    bv = 1
  }
  dh = dh * pat                         // pat = 0 -> flat single-hue wing
  sv = mix(1, sv, pat)
  bv = mix(0.90, bv, pat)

  // --- composite: wings, then the dark body over them ---
  var outV = bv * wa * (1 - ba) + 0.16 * ba
  var outS = sv * (1 - ba) + 0.25 * ba
  var outH = frac(hue + dh * (1 - ba))

  var aa = clamp(ant, 0, 1) * 0.5
  if (aa > outV) {
    outS = 0.25                         // pale antenna stroke, not a colour blob
    outV = aa
  }

  hsv(outH, clamp(outS, 0, 1), clamp(outV, 0, 1))
}
