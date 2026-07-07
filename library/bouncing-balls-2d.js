// name: Bouncing Balls 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Bouncing Balls 2D"; original source never consulted.

// One white ball per column, bouncing under real projectile physics (heights
// simulated in unit "meters" against a one-unit ceiling; scaled to rows only
// at render time). Optional rainbow column beneath each ball. A shake of the
// accelerometer relaunches everything; balls whose bounce has died out also
// auto-relaunch, so the pattern stays alive without a sensor board.

var WIDTH = 16
var canvas = array(WIDTH * WIDTH)   // 16x16 virtual grid, row-major
                                    // cell: 0 dark, 1 white ball, 2+h rainbow

// per-column ball state
var v0 = array(WIDTH)      // launch/impact velocity (units/s)
var bt = array(WIDTH)      // seconds since last bounce
var hgt = array(WIDTH)     // current height, 0..1
var delay = array(WIDTH)   // remaining start delay (s)

var gravity = 9.8          // units/s^2 (panel ~= one meter)
var DAMP = 0.9             // velocity kept per bounce
var launchV = sqrt(2 * gravity)   // reaches the one-unit apex exactly
var maxDelay = 0.15        // max random start stagger (s)
var shakeThresh = 1.3
var rainbowOn = 1
var shakeTimer = 2         // debounce accumulator (s), clamped

export var accelerometer = array(3)   // engine feeds this; zeros without a board

function resetAll() {
  launchV = sqrt(2 * gravity)
  for (var c = 0; c < WIDTH; c++) {
    hgt[c] = 0
    bt[c] = 0
    v0[c] = launchV
    delay[c] = random(maxDelay)
  }
}
resetAll()

//# min=0 max=1 step=0.01 default=0.55
export function sliderGravity(v) {
  gravity = 0.5 + 30 * v * v   // gentle .. ~30x; default ~Earth
  resetAll()
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderMotionSensitivity(v) {
  shakeThresh = 0.05 + 5 * v * v   // squared response, never zero
}

//# min=0 max=1 step=1 default=1
export function sliderShowRainbow(v) {
  rainbowOn = v > 0.5
}

//# min=0 max=1 step=0.01 default=0.55
export function sliderRandomness(v) {
  maxDelay = 0.5 * v * v   // up to ~half a second of stagger
}

export function beforeRender(delta) {
  var dt = delta / 1000

  // shake detection: magnitude vs threshold, rate-limited to ~1/s
  shakeTimer = min(shakeTimer + dt, 5)
  var mag = hypot3(accelerometer[0], accelerometer[1], accelerometer[2])
  if (mag > shakeThresh && shakeTimer >= 1) {
    shakeTimer = 0
    resetAll()
  }

  for (var c = 0; c < WIDTH; c++) {
    if (delay[c] > 0) {
      delay[c] -= dt
      continue
    }
    bt[c] += dt
    var t = bt[c]
    var h = v0[c] * t - 0.5 * gravity * t * t
    if (h < 0) {
      // bounce: keep a fraction of the impact velocity
      h = 0
      v0[c] = v0[c] * DAMP
      bt[c] = 0
      // dead ball: relaunch (keeps sensor-less installs moving)
      if (v0[c] < launchV * 0.12) {
        v0[c] = launchV
        delay[c] = random(maxDelay)
      }
    }
    hgt[c] = h
  }

  // paint the virtual canvas: physical row 0 = bottom = canvas row 15
  for (var c = 0; c < WIDTH; c++) {
    var ballRow = clamp(floor(hgt[c] * 15.99), 0, 15)
    var stagger = triangle(c / 15) * 0.5
    for (var r = 0; r < WIDTH; r++) {
      var cell = 0
      if (r == ballRow) {
        cell = 1                                     // the ball: white
      } else if (rainbowOn && r < ballRow) {
        cell = 2 + frac((ballRow - r) / 16 + stagger) // rainbow below it
      }
      canvas[(15 - r) * 16 + c] = cell
    }
  }
}

export function render2D(index, x, y) {
  var v = canvas[floor(y * 15.99) * 16 + floor(x * 15.99)]
  if (v >= 2) {
    hsv(v - 2, 1, 1)
  } else if (v == 1) {
    rgb(1, 1, 1)
  } else {
    rgb(0, 0, 0)
  }
}
