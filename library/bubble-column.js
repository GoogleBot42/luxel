// name: Bubble Column
// Clean-room reimplementation from a prose functional description of the
// community pattern "Bubble Column"; original source never consulted.

// A vertical tube of dimly glowing fluid with bright pale bubbles rising
// through it. A noise-driven "valve" gates bubble release, so they arrive
// in organic bursts and lulls. Bubbles accelerate as they rise (buoyancy).

const NUM_BUBBLES = 10
const RADIUS = 3        // bubble glow radius, in pixels
const FLUID = 0.06      // dim fluid brightness / bubble threshold
const ACCEL = 6         // upward acceleration, px/s^2

var baseSpeed = pixelCount / 4  // ~4 s to traverse the strip before accel

var pos = array(NUM_BUBBLES)    // pixel-unit position along the strip
var vel = array(NUM_BUBBLES)    // px/s
var buf = array(pixelCount)     // per-pixel bubble brightness
var clock = 0                   // seconds, wrapped after ~1 hour

var fluidHue = 0.66             // deep blue default
var valveOpen = 0.5

// start every bubble parked past the top: plain fluid, bubbles trickle in
var i
for (i = 0; i < NUM_BUBBLES; i++) pos[i] = pixelCount + RADIUS + 1

export function hsvPickerFluidHue(h, s, v) {
  fluidHue = h
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderBubbleValve(v) {
  valveOpen = v
}

export function beforeRender(delta) {
  var dt = delta / 1000
  clock += dt
  if (clock > 3600) clock -= 3600

  // valve: smooth noise over two time-derived axes, rescaled to 0..1
  var valve = (perlin(clock * 0.21, clock * 0.07, 0, 5.3) + 1) * 0.5

  // accumulate bubble glow per pixel: sharp (linear falloff)^4 peaks.
  // The early-out once past the fluid level clips overlapping bubbles
  // against each other — the deliberate "fizzy" merge look.
  var p, b, v, d, c
  for (p = 0; p < pixelCount; p++) {
    v = 0
    for (b = 0; b < NUM_BUBBLES; b++) {
      d = abs(p - pos[b])
      if (d < RADIUS) {
        c = 1 - d / RADIUS
        v += c * c * c * c
        if (v > FLUID) break
      }
    }
    buf[p] = v
  }

  // move bubbles; recycle through the valve once fully past the top
  for (b = 0; b < NUM_BUBBLES; b++) {
    pos[b] += vel[b] * dt
    vel[b] += ACCEL * dt
    if (pos[b] > pixelCount + RADIUS) {
      // park just off the end (glow already decayed off-screen)
      pos[b] = pixelCount + RADIUS + 1
      vel[b] = 0
      if (valve > 1 - valveOpen) {
        // reinject at the bottom: base speed +/- ~75% uniform spread
        pos[b] = -RADIUS
        vel[b] = baseSpeed * (0.25 + random(1.5))
      }
    }
  }
}

export function render(index) {
  // fluid swirl: gentle hue wander along the strip, drifting over tens of s
  var wobble = 0.08 * perlin(index * 12 / pixelCount, clock * 0.04, 0, 8.8)
  var h = fluidHue + wobble
  var v = buf[index]
  if (v <= FLUID) {
    hsv(h, 1, FLUID)             // dim saturated fluid
  } else {
    hsv(h, 0.5, min(v, 1))       // bright, milky bubble
  }
}
