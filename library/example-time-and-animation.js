// name: Example: time and animation
// Clean-room reimplementation from a prose functional description of the
// community pattern "Example: time and animation"; original source never consulted.
//
// A monochrome demo reel: an array of tiny lambdas, each turning
// (position, time) into brightness with a different motion style.
// The active mode advances a little more often than once per second.

var MODE_HOLD_MS = 620      // each mode holds for a bit over half a second
var REPEATS = 4             // spatial value spans 0..4 across the strip

var numModes = 14
var modes = array(numModes)

// t1 and t2 are two free-running sawtooth phases with periods that are
// deliberately not simple multiples of each other (about 5:3), so the
// interference modes beat against each other.
var t1 = 0
var t2 = 0

// 1. drift left: position plus time, wrapped
modes[0] = (p, t) => mod(p + t, 1)
// 2. drift the other way: position minus time, wrapped
modes[1] = (p, t) => mod(p - t, 1)
// 3. linear bounce: position offset by a triangle of time
modes[2] = (p, t) => mod(p + triangle(t), 1)
// 4. eased bounce: position offset by a sine-shaped wave of time
modes[3] = (p, t) => mod(p + wave(t), 1)
// 5. hard-edged chaser: 50% duty square of position-plus-time
modes[4] = (p, t) => square(p + t, 0.5)
// 6. irregular accelerating bounce: triangle of (triangle of time, times time)
modes[5] = (p, t) => mod(p + triangle(triangle(t) * t), 1)
// 7. warbly drift: wave-of-a-wave of time
modes[6] = (p, t) => mod(p + wave(wave(t)), 1)
// 8. bouncing hard-edged blocks
modes[7] = (p, t) => square(triangle(wave(t)) + p, 0.5)
// 9. beating interference of the two unrelated time phases
modes[8] = (p, t) => wave(p + t) * wave(p + t2)
// 10. rich wave texture
modes[9] = (p, t) => wave(wave(p + t) + wave(p - t2) + p - t)
// 11. stretchy: a bit of position feeds the inner wave too
modes[10] = (p, t) => wave(p + wave(wave(t) + p * 0.2))
// 12. zoomed and blended
modes[11] = (p, t) => wave((p - 0.5) * (1 + wave(t))) * wave(t2 + p)
// 13. kinetic, can overshoot full brightness (clipped by hsv)
modes[12] = (p, t) => 2 * triangle(p + wave(t)) - wave(p * 3 + wave(t2))
// 14. glitchy conveyor belt
modes[13] = (p, t) => abs(triangle(p - triangle(t2)) - wave(p * 2 + triangle(t)))

var accum = 0
var mode = 0

export function beforeRender(delta) {
  accum += delta
  if (accum > MODE_HOLD_MS) {
    accum -= MODE_HOLD_MS
    mode = (mode + 1) % numModes
  }
  t1 = time(0.05)   // ~3.3 s period
  t2 = time(0.03)   // ~2.0 s period (roughly 5:3 against t1)
  // mode = 8        // uncomment to pin one mode while studying it
}

export function render(index) {
  var p = REPEATS * index / pixelCount
  var f = modes[mode]
  var v = f(p, t1)
  hsv(0, 0, v)   // white only: brightness is the whole show
}
