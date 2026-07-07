// name: Time Flies 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Time Flies 2D"; original source never consulted.

// A handful of rainbow-hued dots wander erratically like insects on a black
// 2D display: speeds random-walk, headings veer via a shared jagged wave
// noise (per-fly phase offsets), and dot radius pulses fast like flapping
// wings. Turn rate grows with distance from center, which keeps flies
// loosely congregated without walls. The 1D renderer is a deliberate no-op.

var NUM = 6          // fly count (source constant)
var MOVE_MS = 30     // position tick
var STEER_MS = 250   // heading tick
var MAX_SPEED = 0.05 // per movement tick
var MIN_SPEED = 0.002
var MAX_TURN = 0.13  // turns, at screen center
var FALL_R = 0.125   // brightness hits zero about an eighth of the display

var fx = array(NUM)
var fy = array(NUM)
var heading = array(NUM) // fraction of a full turn
var speed = array(NUM)
var phase = array(NUM)   // private phase into the shared noise
var hue = array(NUM)

var i
for (i = 0; i < NUM; i++) {
  fx[i] = random(1)
  fy[i] = random(1)
  heading[i] = random(1)
  speed[i] = MIN_SPEED
  phase[i] = random(1)
  hue[i] = i / NUM  // even rainbow spread
}

// Hand-rolled 1D "wave noise": a triangle plus three sines at incommensurate
// frequencies, rescaled to roughly -1..+1. Deterministic but chaotic-looking.
function wnoise(p) {
  var s = triangle(p) * 0.5 + wave(p * 2) * 0.3
        + wave(p * 2.2 + 0.31) * 0.4 + wave(p * 5 + 0.67) * 0.3
  return (s - 0.75) * 1.35
}

var flyR = 0.05   // current pulsing dot radius
var moveT = 0, steerT = 0

export function beforeRender(delta) {
  // slow sawtooth (~33 s) modulates how fast flies traverse the noise,
  // so their temperament evolves over tens of seconds
  var adv = 0.004 + time(0.5) * 0.02
  // fast tiny sawtooth = the wing flap
  flyR = 0.04 + time(0.005) * 0.03

  moveT += delta
  if (moveT >= MOVE_MS) {
    moveT = 0
    for (i = 0; i < NUM; i++) {
      phase[i] = mod(phase[i] + adv, 8)
      // speed random-walks, riding its clamps
      speed[i] = clamp(speed[i] + wnoise(phase[i]) * 0.008, MIN_SPEED, MAX_SPEED)
      fx[i] = clamp(fx[i] - cos(heading[i] * PI2) * speed[i], 0, 1)
      fy[i] = clamp(fy[i] - sin(heading[i] * PI2) * speed[i], 0, 1)
    }
  }

  steerT += delta
  if (steerT >= STEER_MS) {
    steerT = 0
    for (i = 0; i < NUM; i++) {
      // soft containment: the farther from center, the harder the turn
      var d = hypot(fx[i] - 0.5, fy[i] - 0.5)
      heading[i] = mod(heading[i] + wnoise(phase[i]) * MAX_TURN * (1 + d * 4), 1)
    }
  }
}

export function render2D(index, x, y) {
  rgb(0, 0, 0)
  var k, dx, dy, d, v
  for (k = 0; k < NUM; k++) {
    dx = x - fx[k]
    dy = y - fy[k]
    // cheap Manhattan-style pre-test before the true distance
    if (abs(dx) + abs(dy) > flyR * 1.5) continue
    d = hypot(dx, dy)
    if (d < flyR) {
      // sharp bright-centered dot: cubic falloff
      v = max(0, 1 - d / FALL_R)
      hsv(hue[k], 1, v * v * v)
      return  // first fly in list order wins overlaps
    }
  }
}

// deliberate no-op on unmapped strips
export function render(index) {
  rgb(0, 0, 0)
}
