// name: 4th
// Clean-room reimplementation from a prose functional description of the
// community pattern "4th"; original source never consulted.
//
// FORK: deliberately departs from the original per the 2026-09-01 review.
// Spacing moved bands in a background too dim to notice ("seems to do
// nothing"), and the death-crackle piled up faster than it faded, washing the
// strip out to solid white. Same idea -- bunting plus rockets that streak,
// stall and burst -- rebuilt so every dial has a real unit and a visible
// effect, and so the picture stays crisp at any frame rate.
//
// Fourth-of-July strip: red / white / blue bunting drifting slowly along the
// strip, with rockets streaking over it and decelerating as they go. A rocket
// that runs out of speed bursts in a widening pair of sparking fronts -- red
// star, white salute or blue star -- that crackle down over a second or two
// before that rocket relaunches from a fresh spot in a fresh direction.

var maxRockets = 6

var trailHalfLife = 0.07          // seconds; comet streak afterglow

// ---- controls (real units) -------------------------------------------------

var stripeWidth = 5               // pixels per bunting band
var rocketCount = 2               // rockets in the air at once
var flightSeconds = 2.2           // nominal launch-to-burst flight time
var crackleSeconds = 1.4          // how long a burst keeps sparking
var burstSpread = 8               // how far the sparking fronts travel, pixels

//# min=1 max=24 step=1 default=5
export function sliderStripeWidth(v) { stripeWidth = clamp(floor(v), 1, 64) }

//# min=1 max=6 step=1 default=2
export function sliderRockets(v) { rocketCount = clamp(floor(v), 1, maxRockets) }

//# min=0.5 max=8 step=0.1 default=2.2
export function sliderFlightSeconds(v) { flightSeconds = max(0.2, v) }

//# min=0.2 max=5 step=0.1 default=1.4
export function sliderCrackleSeconds(v) { crackleSeconds = max(0.15, v) }

//# min=1 max=30 step=1 default=8
export function sliderBurstSpread(v) { burstSpread = max(1, v) }

// Salute: stall every rocket in the air so they all burst together.
export function triggerSalute() {
  for (var i = 0; i < rocketCount; i++) if (flying[i]) ignite(i)
}

// ---- state -----------------------------------------------------------------

var trail = array(pixelCount)     // warm comet trail
var sparkR = array(pixelCount)    // burst embers, one buffer per channel so a
var sparkG = array(pixelCount)    // red star and a blue star can burn at the
var sparkB = array(pixelCount)    // same time

var pos = array(maxRockets)       // fractional strip position
var vel = array(maxRockets)       // signed pixels per second
var decel = array(maxRockets)     // pixels per second^2
var flying = array(maxRockets)    // 1 = climbing, 0 = bursting or idle
var burstLeft = array(maxRockets) // seconds of crackle remaining
var starR = array(maxRockets)
var starG = array(maxRockets)
var starB = array(maxRockets)

var bunting = 0                   // bunting scroll offset, pixels

// ---- helpers ---------------------------------------------------------------

function spark(p, amp, i) {
  var q = floor(mod(p, pixelCount))
  var r = amp * starR[i]
  var g = amp * starG[i]
  var b = amp * starB[i]
  if (r > sparkR[q]) sparkR[q] = r
  if (g > sparkG[q]) sparkG[q] = g
  if (b > sparkB[q]) sparkB[q] = b
}

function launch(i) {
  // constant deceleration: pick the flight time and the ground it covers, and
  // the launch speed follows (span = v*t/2), so FlightSeconds really is how
  // long the rocket is in the air.
  var t = flightSeconds * (0.75 + random(0.5))
  var span = pixelCount * (0.45 + random(0.4))
  var v = 2 * span / t
  vel[i] = random(1) < 0.5 ? -v : v
  decel[i] = v / t
  pos[i] = random(pixelCount)
  flying[i] = 1
  burstLeft[i] = 0

  // three classic shell colors, so a volley reads red / white / blue
  var roll = random(3)
  if (roll < 1) { starR[i] = 1; starG[i] = 0.12; starB[i] = 0.06 }
  else if (roll < 2) { starR[i] = 1; starG[i] = 1; starB[i] = 1 }
  else { starR[i] = 0.14; starG[i] = 0.3; starB[i] = 1 }
}

function ignite(i) {
  vel[i] = 0
  flying[i] = 0
  burstLeft[i] = crackleSeconds
  // muzzle flash is white whatever the shell color is
  var p = floor(mod(pos[i], pixelCount))
  sparkR[p] = 2.2; sparkG[p] = 2.2; sparkB[p] = 2.2
  var l = floor(mod(pos[i] - 1, pixelCount))
  var r = floor(mod(pos[i] + 1, pixelCount))
  sparkR[l] = 1.3; sparkG[l] = 1.3; sparkB[l] = 1.3
  sparkR[r] = 1.3; sparkG[r] = 1.3; sparkB[r] = 1.3
}

// ---- frame -----------------------------------------------------------------

export function beforeRender(delta) {
  var dt = min(delta, 60) * 0.001

  // every decay is per SECOND, so the strip looks the same on a slow rig
  feedback(trail, pow(0.5, dt / trailHalfLife))
  var sparkDecay = pow(0.5, dt / (crackleSeconds * 0.4))
  feedback(sparkR, sparkDecay)
  feedback(sparkG, sparkDecay)
  feedback(sparkB, sparkDecay)

  // bunting creeps along at one band every 2.5 s whatever the band width, so
  // the stripe geometry is legible even standing still
  var period = stripeWidth * 3
  bunting = mod(bunting + dt * stripeWidth * 0.4, period)

  for (var i = 0; i < rocketCount; i++) {
    if (flying[i]) {
      var mag = abs(vel[i]) - decel[i] * dt
      if (mag <= 0) { ignite(i); continue }
      var v = vel[i] < 0 ? -mag : mag
      vel[i] = v

      var p0 = pos[i]
      var p1 = p0 + v * dt
      pos[i] = mod(p1, pixelCount)

      // deposit once per pixel crossed, so a fast rocket draws a continuous
      // streak instead of a dashed line
      var steps = clamp(ceil(abs(p1 - p0)), 1, 48)
      var head = min(1, mag / (pixelCount * 0.4))     // dims as the fuse burns
      var lit = 0.3 + 0.7 * head
      for (var k = 0; k < steps; k++) {
        var q = floor(mod(p0 + (p1 - p0) * ((k + 1) / steps), pixelCount))
        if (lit > trail[q]) trail[q] = lit
      }
      continue
    }

    if (burstLeft[i] <= 0) { launch(i); continue }

    var bl = burstLeft[i] - dt
    burstLeft[i] = bl
    if (bl <= 0) { launch(i); continue }

    // two mirrored fronts walking outward from the burst point, dimming as
    // they widen, plus loose embers scattered inside the shell
    var age = 1 - bl / crackleSeconds
    var off = age * burstSpread
    var amp = (1 - age) * 1.4
    spark(pos[i] + off, amp * (0.6 + random(0.8)), i)
    spark(pos[i] - off, amp * (0.6 + random(0.8)), i)
    // a few stragglers just behind each front, never a filled block
    if (random(1) < 0.5) {
      var s = random(1) < 0.5 ? -1 : 1
      spark(pos[i] + s * off * (0.45 + random(0.5)), amp * random(0.55), i)
    }
  }
}

export function render(index) {
  // bunting: dim red / white / blue blocks, stripeWidth pixels each
  var band = floor((index + bunting) / stripeWidth) % 3
  var r = 0.02
  var g = 0.03
  var b = 0.3
  if (band == 0) { r = 0.26; g = 0.02; b = 0.03 }
  else if (band == 1) { r = 0.13; g = 0.13; b = 0.13 }

  // rocket streaks read as warm orange over the bunting
  var t = trail[index]
  r += t
  g += t * 0.34
  b += t * 0.06

  var sr = sparkR[index]
  var sg = sparkG[index]
  var sb = sparkB[index]
  var amp = max(sr, max(sg, sb))

  // stochastic crackle: the buffer level is a per-frame ignition PROBABILITY
  // rather than a brightness, so a burst sputters and gutters out instead of
  // dissolving smoothly. Firing at full amplitude keeps the star's color pure.
  if (amp > random(1)) {
    var k = 1 / amp
    rgb(saturate(sr * k), saturate(sg * k), saturate(sb * k))
  } else {
    rgb(saturate(r), saturate(g), saturate(b))
  }
}
