// name: XmasFlies
// Clean-room reimplementation from a prose functional description of the
// community pattern "XmasFlies"; original source never consulted.

// Slowed-down, wrapping fork of the classic "sparks": colored fireflies
// drift along the strip in both directions, each gradually slowing and
// dimming until it stalls and is reborn elsewhere with a fresh random
// speed. Sparks wrap end-to-end. Four color groups (red / green / blue /
// orange-gold) assigned round-robin; the huge brightness multiplier makes
// lit specks slam to full with a very short decaying tail.
// Note: the original only counted the first two groups' energy toward
// brightness (mostly red/green show); this version sums all four squared
// group energies — the spec's "fixed" variant — so all four hues light.

var numSparks = floor(pixelCount / 5) + 1
var sparkV = array(numSparks)
var sparkX = array(numSparks)
// four per-pixel energy buffers, one per spark group. NOT one packed
// `array(pixelCount * 4)`: in 16.16 fixed point that length wraps to 0 at
// 8192 px and the group offsets wrap negative (2026-10-04, 16384-px soak)
var energy0 = array(pixelCount)
var energy1 = array(pixelCount)
var energy2 = array(pixelCount)
var energy3 = array(pixelCount)

var MAXV = .2       // max speed, pixels per scaled time unit
var STALL = .012    // |v| below this = coasted to a stop, respawn
var DRAG = .988     // per-frame velocity multiplier (slow exponential stop)
var DECAY = .9      // per-frame energy buffer decay (short tails)
var BOOST = 120     // brightness multiplier: any lit cell slams to full

// group hues: red, green, blue, orange-gold
var groupHue = array(4)
groupHue[0] = 0
groupHue[1] = .33
groupHue[2] = .66
groupHue[3] = .09

export function beforeRender(delta) {
  var dt = delta * .1   // the fork's ~10x slowdown
  feedback(energy0, DECAY)
  feedback(energy1, DECAY)
  feedback(energy2, DECAY)
  feedback(energy3, DECAY)

  var i
  for (i = 0; i < numSparks; i++) {
    if (abs(sparkV[i]) < STALL) {
      // stalled out: reborn at a random spot, random speed and direction
      sparkV[i] = (random(1) - .5) * MAXV
      sparkX[i] = random(pixelCount)
    }
    sparkV[i] *= DRAG
    sparkX[i] += sparkV[i] * dt
    // wrap, don't bounce
    if (sparkX[i] >= pixelCount) sparkX[i] -= pixelCount
    if (sparkX[i] < 0) sparkX[i] += pixelCount
    // deposit the signed velocity into this spark's group buffer
    var g = i % 4
    var x = floor(sparkX[i])
    if (g == 0) energy0[x] += sparkV[i]
    else if (g == 1) energy1[x] += sparkV[i]
    else if (g == 2) energy2[x] += sparkV[i]
    else energy3[x] += sparkV[i]
  }
}

export function render(index) {
  var e0 = energy0[index]
  var e1 = energy1[index]
  var e2 = energy2[index]
  var e3 = energy3[index]

  // hue of the group with the (signed) maximum energy here
  var h = groupHue[0]
  var best = e0
  if (e1 > best) { best = e1; h = groupHue[1] }
  if (e2 > best) { best = e2; h = groupHue[2] }
  if (e3 > best) { best = e3; h = groupHue[3] }

  var v = (e0 * e0 + e1 * e1 + e2 * e2 + e3 * e3) * BOOST
  hsv(h, 1, min(v, 1))
}
