// name: Example: time and animation
// Clean-room reimplementation from a prose functional description of the
// community pattern "Example: time and animation"; original source never consulted.

// A monochrome demo reel of motion techniques. Fourteen "modes" — each a tiny
// lambda mapping (spatial position, time phase) -> brightness — are stored in
// an array and dispatched by index. A timer advances the active mode every
// ~0.6 s, forever. Two free-running sawtooth phases with a ~5:3 period ratio
// drive the interference/beat modes. White only: hue and saturation stay 0.

var HOLD_MS = 600            // how long each mode is shown
var ZOOM = 4                 // spatial repeats along the strip

var modeTimer = 0
var mode = 0
var t1 = 0                   // primary phase, ~3.3 s period
var t2 = 0                   // secondary phase, ~2 s period (5:3 vs t1)

var NUM_MODES = 14
var modes = array(NUM_MODES)

// 1: drift one way                     2: drift the other way
modes[0] = (p, t) => mod(p + t, 1)
modes[1] = (p, t) => mod(p - t, 1)
// 3: linear bounce                     4: eased (sine) bounce
modes[2] = (p, t) => triangle(p + triangle(t))
modes[3] = (p, t) => triangle(p + wave(t))
// 5: hard-edged moving chaser
modes[4] = (p, t) => square(p + t, 0.5)
// 6: irregular, accelerating bounce
modes[5] = (p, t) => triangle(p + triangle(triangle(t) * t))
// 7: warbly drift (wave of a wave)
modes[6] = (p, t) => triangle(p + wave(wave(t)))
// 8: bouncing hard-edged blocks
modes[7] = (p, t) => square(triangle(wave(t)) + p, 0.5)
// 9: beating interference of the two unrelated phases
modes[8] = (p, t) => wave(p + t) * wave(p + t2)
// 10: rich wave texture
modes[9] = (p, t) => wave(wave(p + t) + wave(p - t2) + p - t)
// 11: stretchy — a bit of position feeds the inner wave too
modes[10] = (p, t) => wave(p + wave(wave(t) + p * 0.2))
// 12: zoomed and blended
modes[11] = (p, t) => wave((p - 0.5) * (1 + wave(t))) * wave(t2 + p)
// 13: kinetic combination — can overshoot 1, clipped by hsv()
modes[12] = (p, t) => 2 * triangle(p + wave(t)) - wave(p * 3 + wave(t2))
// 14: glitchy conveyor belt from an absolute difference
modes[13] = (p, t) => abs(triangle(p - triangle(t2)) - wave(p * 2 + triangle(t)))

export function beforeRender(delta) {
  t1 = time(0.05)            // ~3.28 s sawtooth
  t2 = time(0.03)            // ~1.97 s sawtooth — deliberately not a multiple
  modeTimer += delta
  if (modeTimer > HOLD_MS) {
    modeTimer -= HOLD_MS
    mode = (mode + 1) % NUM_MODES
  }
  // mode = 8   // uncomment to pin a single mode while studying it
}

export function render(index) {
  var p = ZOOM * index / pixelCount
  var f = modes[mode]
  var v = f(p, t1)
  hsv(0, 0, v)               // white; brightness carries the whole show
}
