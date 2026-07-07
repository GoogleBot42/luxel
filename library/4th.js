// name: 4th
// Clean-room reimplementation from a prose functional description of the
// community pattern "4th"; original source never consulted.

// Fourth-of-July strip: a dim red/white/blue block-striped background,
// with two bright orange rockets streaking around under friction. When a
// rocket coasts to a stop it dies in a widening burst of stochastic white
// crackle, then relaunches from a random spot in a random direction.

const numRockets = 2

const trailDecay = 0.9      // fast comet-trail fade (~10% loss / frame)
const crackleDecay = 0.98   // slow glitter fade (~2% loss / frame)

const launchMin = 0.4       // minimum launch energy (pixels per scaled ms)
const launchSpread = 0.25   // uniform extra energy on top of the minimum
const flyThresh = 0.1       // above this magnitude: still flying
const deadThresh = 0.005    // below this magnitude: relaunch
const burstSpread = 5       // max crackle offset in pixels while dying

var trail = array(pixelCount)
var crackle = array(pixelCount)
var energy = array(numRockets)   // signed velocity/energy
var pos = array(numRockets)      // fractional strip position

// friction per scaled-ms, inversely proportional to strip length (halved
// again since each flight covers about half the strip)
var friction = 0.5 / pixelCount / 2

var spacing = 3

//# min=0 max=1 step=0.05 default=0.4
export function sliderSpacing(v) {
  spacing = 1 + floor(v * 5)   // integer block-width multiplier, 1..6
}

export function beforeRender(delta) {
  var dt = delta / 10
  feedback(trail, trailDecay)
  feedback(crackle, crackleDecay)

  for (var i = 0; i < numRockets; i++) {
    var e = energy[i]
    var mag = abs(e)

    if (mag < deadThresh) {
      // relaunch: fresh energy, random direction, random position
      mag = launchMin + random(launchSpread)
      e = random(1) < 0.5 ? -mag : mag
      pos[i] = random(pixelCount)
    }

    // friction: constant loss per unit time, sign preserved
    mag = abs(e) - friction * dt
    if (mag < 0) mag = 0
    e = e < 0 ? -mag : mag
    energy[i] = e

    // advance and wrap
    pos[i] = mod(pos[i] + e * dt, pixelCount)

    var p = floor(pos[i])
    if (mag > flyThresh) {
      trail[p] = 1
    } else if (mag > deadThresh) {
      // dying: two mirrored sparking points spreading outward
      var off = (1 - mag / flyThresh) * burstSpread
      crackle[floor(mod(pos[i] + off, pixelCount))] = random(2)
      crackle[floor(mod(pos[i] - off, pixelCount))] = random(2)
    }
  }
}

export function render(index) {
  // block-striped background: dim red / dim gray / dim blue
  var block = floor(index / (spacing * 3)) % 3
  var r = 0
  var g = 0
  var b = 0
  if (block == 0) {
    r = 0.2
  } else if (block == 1) {
    r = 0.08
    g = 0.08
    b = 0.08
  } else {
    b = 0.2
  }

  // rocket trails read as warm orange over the background
  var t = trail[index]
  r += t
  g += t / 3

  // stochastic crackle: buffer value vs fresh random draw each frame
  if (crackle[index] > random(1)) {
    rgb(1, 1, 1)
  } else {
    rgb(min(r, 1), min(g, 1), min(b, 1))
  }
}
