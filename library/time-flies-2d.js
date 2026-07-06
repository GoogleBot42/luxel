// name: Time Flies 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Time Flies 2D"; original source never consulted.

// Half a dozen rainbow-spread flies wander a black display. One shared
// additive-wave noise function, sampled at a private per-fly phase, drives
// both speed random-walks and steering; turn rate grows with distance from
// center, so flies loosely congregate mid-display without walls. A fast
// global sawtooth added to the dot radius reads as flapping wings. The 1D
// renderer is a deliberate no-op.

const NUM = 6

var fx = array(NUM)   // position
var fy = array(NUM)
var fh = array(NUM)   // heading, in turns
var fs = array(NUM)   // speed per movement tick
var fp = array(NUM)   // private phase into the shared noise

var i
for (i = 0; i < NUM; i++) {
  fx[i] = random(1)
  fy[i] = random(1)
  fh[i] = random(1)
  fs[i] = 0.01 + random(0.02)
  fp[i] = random(1)
}

var rad = 0.05
var moveAcc = 0
var steerAcc = 0

// jagged-but-smooth deterministic noise: triangle + three incommensurate
// sine humps, spanning roughly -1..1
function wnoise(p) {
  var v = triangle(p) - 0.5
  v += 0.4 * (wave(p * 2 + 0.13) - 0.5)
  v += 0.35 * (wave(p * 2.2 + 0.41) - 0.5)
  v += 0.25 * (wave(p * 5 + 0.77) - 0.5)
  return v * 1.1
}

export function beforeRender(delta) {
  rad = 0.045 + 0.03 * time(0.005)   // wing flap: ~3 size pulses / s
  var adv = 0.003 + 0.02 * time(0.5) // noise traversal rate, ~33 s cycle

  steerAcc += delta
  var doSteer = 0
  if (steerAcc >= 240) {             // steer only every few move ticks
    steerAcc = 0
    doSteer = 1
  }

  moveAcc += delta
  if (moveAcc >= 30) {               // movement tick ~33 / s
    moveAcc = 0
    var j
    for (j = 0; j < NUM; j++) {
      fp[j] = frac(fp[j] + adv)
      var n = wnoise(fp[j])
      // speed random-walks between its bounds
      fs[j] = clamp(fs[j] + n * 0.02, 0.002, 0.05)
      fx[j] = clamp(fx[j] - cos(fh[j] * PI2) * fs[j], 0, 1)
      fy[j] = clamp(fy[j] - sin(fh[j] * PI2) * fs[j], 0, 1)
      if (doSteer) {
        // soft containment: the farther out, the harder the turn
        var d = hypot(fx[j] - 0.5, fy[j] - 0.5)
        fh[j] = mod(fh[j] + n * 0.12 * (0.4 + d * 4), 1)
      }
    }
  }
}

export function render2D(index, x, y) {
  var j
  for (j = 0; j < NUM; j++) {
    var dx = abs(x - fx[j])
    var dy = abs(y - fy[j])
    if (dx + dy > rad * 1.5) continue   // cheap Manhattan pre-reject
    var d = hypot(dx, dy)
    if (d < rad) {
      var b = max(0, 1 - d * 8)         // zero at ~ an eighth of display
      b = b * b * b                     // sharp bright-centered dot
      hsv(j / NUM, 1, b)                // even rainbow spread by index
      return                            // first fly wins overlaps
    }
  }
  rgb(0, 0, 0)
}

export function render(index) {
  rgb(0, 0, 0)   // intentionally blank on unmapped 1D strips
}
