// name: Bubble Column
// Clean-room reimplementation from a prose functional description of the
// community pattern "Bubble Column"; original source never consulted.

const NUM_BUBBLES = 10
const RADIUS = 3            // bubble glow radius, in pixels
const FLUID = 0.07          // dim fluid brightness / bubble threshold

var pos = array(NUM_BUBBLES)   // position in pixel units along the strip
var vel = array(NUM_BUBBLES)   // pixels per second
var buf = array(pixelCount)    // per-pixel bubble brightness buffer
var clock = 0
var baseHue = 0.66             // deep blue fluid by default
var valveEase = 0.5
var baseSpeed = pixelCount / 3 // a few seconds to traverse the strip

// start every bubble beyond the top: display opens as plain fluid and
// bubbles trickle in through the normal reinjection path
var i
for (i = 0; i < NUM_BUBBLES; i++) {
  pos[i] = pixelCount + RADIUS + 1
  vel[i] = 0
}

export function hsvPickerFluidHue(h, s, v) {
  baseHue = h
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderBubbleValve(v) {
  valveEase = v
}

export function beforeRender(delta) {
  var dt = delta / 1000
  clock += dt
  if (clock > 3600) clock -= 3600

  // hidden valve: smooth noise from two time coordinates advancing at
  // different rates, rescaled to 0..1 -> organic bursts and lulls
  var valve = (perlin(clock * 0.21, clock * 0.13, 0, 3.7) + 1) * 0.5

  // accumulate bubble glow per pixel: sharp quartic falloff, with an early
  // exit once past the fluid level (source of the fizzy overlap look)
  var p, b, acc, t
  for (p = 0; p < pixelCount; p++) {
    acc = 0
    for (b = 0; b < NUM_BUBBLES; b++) {
      t = abs(p - pos[b])
      if (t < RADIUS) {
        t = 1 - t / RADIUS
        acc += t * t * t * t
        if (acc > FLUID) break
      }
    }
    buf[p] = acc
  }

  // rise, accelerate, recycle through the valve
  var threshold = 0.85 - valveEase * 0.7   // higher slider = easier to meet
  for (b = 0; b < NUM_BUBBLES; b++) {
    pos[b] += vel[b] * dt
    vel[b] += 6 * dt                       // buoyancy: speed up while rising
    if (pos[b] > pixelCount + RADIUS) {    // fully off the top (glow decayed)
      if (valve > threshold) {
        pos[b] = -RADIUS
        vel[b] = baseSpeed * (0.25 + random(1.5))  // base +/- 3/4 base
      } else {
        pos[b] = pixelCount + RADIUS + 1   // wait off-screen for the valve
        vel[b] = 0
      }
    }
  }
}

export function render(index) {
  // gentle fluid swirl: ~a dozen noise cells along the strip, drifting slowly
  var swirl = perlin(index / pixelCount * 12, clock * 0.04, 0, 7.3) * 0.06
  var v = buf[index]
  if (v <= FLUID) {
    hsv(baseHue + swirl, 1, FLUID)             // dim, fully saturated fluid
  } else {
    hsv(baseHue + swirl, 0.5, min(v, 1))       // bright, milky bubble
  }
}
