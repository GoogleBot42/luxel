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
// four per-pixel energy buffers, packed group-major into one array
var energy = array(pixelCount * 4)

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
  feedback(energy, DECAY)

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
    energy[(i % 4) * pixelCount + floor(sparkX[i])] += sparkV[i]
  }
}

export function render(index) {
  var e0 = energy[index]
  var e1 = energy[pixelCount + index]
  var e2 = energy[pixelCount * 2 + index]
  var e3 = energy[pixelCount * 3 + index]

  // hue of the group with the (signed) maximum energy here
  var h = groupHue[0]
  var best = e0
  if (e1 > best) { best = e1; h = groupHue[1] }
  if (e2 > best) { best = e2; h = groupHue[2] }
  if (e3 > best) { best = e3; h = groupHue[3] }

  var v = (e0 * e0 + e1 * e1 + e2 * e2 + e3 * e3) * BOOST
  hsv(h, 1, min(v, 1))
}
