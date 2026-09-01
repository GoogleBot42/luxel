// name: millipede 1d/2d controls
// Clean-room reimplementation from a prose functional description of the
// community pattern "millipede 1d/2d controls"; original source never
// consulted.
// DELIBERATELY DEPARTS FROM THE ORIGINAL (2026-09-01 review): Jeremy forked
// this one — fidelity to the corpus pattern is waived and the brief is "make
// the creature clearly better". The original's abstract rainbow pinwheel is
// replaced by an actual millipede: a segmented body with a head and feelers,
// a peristaltic crawling gait and a metachronal leg wave.

// A millipede crawls a closed loop. The loop is the strip in 1D and a ring
// around the panel centre in 2D, so the same creature reads on both rigs.
// Its body is a train of plated segments that bunch and stretch as a gait
// wave travels up it; legs shimmer in a metachronal wave (each segment a
// little behind the one ahead) and, on a 2D panel, stick out sideways and
// visibly stroke. Two feelers sweep ahead of a paler, brighter head.

var segCount = 14          // body segments
var crawl = 0.16           // laps per second
var hueBase = 0.28         // body hue, 0..1
var bodyFrac = 0.55        // fraction of the loop the body covers

//# min=3 max=40 step=1 default=14
export function sliderSegments(v) { segCount = max(1, floor(v)) }

// Laps per second: at 0.16 a lap takes about six seconds.
//# min=0 max=0.6 step=0.01 default=0.16
export function sliderCrawlSpeed(v) { crawl = v }

//# min=0 max=360 step=1 default=100
export function sliderHue(v) { hueBase = v / 360 }

//# min=15 max=95 step=1 default=55
export function sliderBodyLength(v) { bodyFrac = v / 100 }

const RING = 0.32          // radius of the 2D crawl ring (map units)
const HALFW = 0.055        // body half-width in map units (2D)
const LEGREACH = 2.1       // leg reach, in body half-widths
const ANTFRAC = 0.13       // feeler reach as a fraction of the body length
const STRIDES = 3          // gait waves riding the body at once
const LEGLAG = 0.45        // metachronal phase lag per segment

var headU = 0              // head position along the loop, 0..1
var gaitPh = 0             // gait / leg-wave phase, 0..1
var antPh = 0              // feeler sweep phase, 0..1
var ringPh = 0             // slow wobble of the 2D crawl ring
var use2D = 0              // 1 while render2D is driving the shader

export function beforeRender(delta) {
  var dt = delta / 1000
  headU = mod(headU + crawl * dt, 1)
  // The gait is locked to the crawl — walk faster, stride faster — but never
  // stops dead, so a parked millipede still shuffles instead of freezing.
  gaitPh = mod(gaitPh + (0.35 + crawl * 13) * dt, 1)
  antPh = mod(antPh + 0.55 * dt, 1)
  ringPh = mod(ringPh + 0.06 * dt, 1)
}

// Body shading. t01 runs 0 at the head to 1 at the tail; slat is the signed
// lateral offset from the centreline in body half-widths (always 0 in 1D,
// where the leg wave shows up as a travelling shimmer instead of as spikes).
function drawBody(t01, slat) {
  var lat = abs(slat)

  // Peristalsis: the segment ruler itself stretches and bunches as the gait
  // wave travels from tail to head. This is what makes the crawl articulated
  // rather than a rigid band sliding along.
  var seg = t01 * segCount + sin((t01 * STRIDES + gaitPh) * PI2) * 0.45
  var f = mod(seg, 1)
  var plate = pow(sin(f * PI), 0.55)        // dark groove at every joint
  var legs = wave(seg * LEGLAG - gaitPh)    // metachronal leg wave, 0..1

  var headMix = saturate(1 - t01 / 0.12)    // 1 on the head, 0 past it
  var taper = (1 - 0.42 * t01 * t01) * saturate((1 - t01) * 7)   // tail thins to a point
  var halfW = taper * (1 + 0.6 * headMix)

  var body = saturate((halfW - lat) * 3.4)
  // Legs stroke: they reach further on the power half of the wave.
  var reach = halfW + LEGREACH * (0.3 + 0.7 * legs) * (1 - headMix)
  var stripe = pow(abs(sin(seg * PI)), 2.5)
  var leg = stripe * saturate((reach - lat) * 2.4) * saturate((lat - halfW * 0.4) * 2.5)

  var v = body * (0.3 + 0.7 * plate) * (0.55 + 0.45 * legs)
  v = max(v, leg * 0.7)
  v = max(v, body * headMix * (0.65 + 0.35 * plate))

  // A pair of eye specks rides the head capsule (2D only — a strip has no
  // width to put them side by side, so there the head glint stands in).
  var eye = saturate(1 - abs(t01 - 0.045) * 30) *
            saturate(1 - abs(lat - 0.6) * 3) * use2D
  var glint = headMix * saturate(1 - t01 / 0.025) * saturate(1.2 - lat)
  var hot = max(eye, glint)

  var h = hueBase + t01 * 0.15 + legs * 0.02 - headMix * 0.05
  var s = saturate(1 - 0.35 * headMix - 0.6 * hot)
  hsv(h, s, saturate(v * (0.7 + 0.4 * taper) + hot * 0.55))
}

// Two feelers sweeping ahead of the head. `an` is 0 at the head and 1 at the
// far end of their reach.
function drawAntennae(an, slat) {
  var s1 = sin(antPh * PI2)
  var s2 = sin(antPh * PI2 + 2.1)
  var r1 = 0.5 + 0.5 * abs(s1)              // how far each feeler is extended
  var r2 = 0.5 + 0.5 * abs(s2)
  var l1 = s1 * 1.3 * an * use2D            // sideways sweep (2D only)
  var l2 = -s2 * 1.3 * an * use2D
  // each feeler is a thin shaft from the head out to its tip, brightest at
  // the tip so the sweep reads as a flick
  var v1 = saturate(1 - abs(slat - l1) * 2.4) * max(saturate((r1 - an) * 4) * 0.4,
                                                    saturate(1 - abs(an - r1) * 6))
  var v2 = saturate(1 - abs(slat - l2) * 2.4) * max(saturate((r2 - an) * 4) * 0.4,
                                                    saturate(1 - abs(an - r2) * 6))
  var v = max(v1, v2) * 0.9
  hsv(hueBase + 0.08, 0.5, saturate(v))
}

// u is the position along the crawl loop (0..1); slat the signed lateral
// offset in body half-widths.
function creature(u, slat) {
  // signed distance behind the head, in laps — negative means ahead of it
  var d = mod(headU - u + 0.5, 1) - 0.5
  var antLen = bodyFrac * ANTFRAC
  if (d >= 0 && d <= bodyFrac) drawBody(d / bodyFrac, slat)
  else if (d < 0 && d >= -antLen) drawAntennae(-d / antLen, slat)
  else hsv(0, 0, 0)
}

export function render(index) {
  use2D = 0
  creature(index / pixelCount, 0)
}

export function render2D(index, x, y) {
  use2D = 1
  var dx = x - 0.5
  var dy = y - 0.5
  var a = mod(atan2(dy, dx) / PI2, 1)
  // the crawl ring breathes slightly, so the path never looks stencilled
  var ring = RING + 0.02 * sin((a * 3 + ringPh) * PI2)
  creature(a, (hypot(dx, dy) - ring) / HALFW)
}
