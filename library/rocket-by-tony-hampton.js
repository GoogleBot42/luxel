// name: Rocket by Tony Hampton
// Clean-room reimplementation from a prose functional description of the
// community pattern "Rocket by Tony Hampton"; original source never consulted.
// A generative rocket-launch physics sim on a 1D strip (reads no sensors): a
// bright body accelerates up from the ground, boosts, and streaks off the top,
// spewing an additive exhaust of white-hot sparks that cool through the exhaust
// color and flicker into cooler hues as they fizzle. Trails linger via decay.

var N = pixelCount
var rBuf = array(N)
var gBuf = array(N)
var bBuf = array(N)

var POOL = max(1, floor(N / 6))
var sPos = array(POOL)   // fractional pixel index
var sEng = array(POOL)   // energy
var sHue = array(POOL)   // per-spark hue (multicolor mode)

// rocket kinematics state
var pos = 0
var vel = 0
var elapsed = 0

// --- controls (functions receive 0..1; //# is UI metadata) ------------
var flightTime = 6       // seconds for an unboosted full-strip flight
//# min=0 max=1 step=0.01 default=0.26
export function sliderFlightTime(v) { flightTime = 1 + v * 19 }

var rocketSize = 3       // body length in pixels
//# min=0 max=1 step=0.01 default=0.1
export function sliderRocketSize(v) { rocketSize = max(1, round(v * 20)) }

var boostDelay = 2       // seconds before boost kicks in
//# min=0 max=1 step=0.01 default=0.33
export function sliderBoostDelay(v) { boostDelay = 0.5 + v * 4.5 }

var boostMult = 20       // acceleration multiplier after the delay
//# min=0 max=1 step=0.01 default=0.19
export function sliderBoostMultiplier(v) { boostMult = 1 + v * 99 }

var multicolor = 0
//# min=0 max=1 step=1 default=0
export function toggleMulticolorExhaust(v) { multicolor = v > 0.5 }

var exhaustHue = 0.04    // hot orange-red
var exhaustSat = 1
export function hsvPickerExhaust(h, s, v) { exhaustHue = h; exhaustSat = s }

var rocketR = 1          // default white body
var rocketG = 1
var rocketB = 1
var rc = array(3)
export function hsvPickerRocketBody(h, s, v) {
  hsv2rgb(h, s, v, rc)
  rocketR = rc[0]; rocketG = rc[1]; rocketB = rc[2]
}

// --- helpers ----------------------------------------------------------
var col = array(3)
function deposit(idx, h, s, v) {
  if (idx < 0 || idx >= N) return
  hsv2rgb(h, s, v, col)
  rBuf[idx] += col[0]
  gBuf[idx] += col[1]
  bBuf[idx] += col[2]
}

export function beforeRender(delta) {
  var dt = delta / 1000

  // 1. cooling: frame-rate-independent decay, always < 1
  var decay = pow(0.9, delta / 16.6)
  if (decay > 0.99) decay = 0.99
  feedback(rBuf, decay)
  feedback(gBuf, decay)
  feedback(bBuf, decay)

  // 2. rocket kinematics: base accel so an unboosted flight ~ flightTime
  var accel = 2 * N / (flightTime * flightTime)   // pixels/s^2
  elapsed += dt
  if (elapsed > boostDelay) accel *= boostMult
  vel += accel * dt
  pos += vel * dt
  if (pos > N) { pos = 0; vel = 0; elapsed = 0 }

  // 3. active spark count scales with rocket size
  var active = max(1, floor(POOL * rocketSize / 20))
  if (active > POOL) active = POOL

  var friction = 3 / N
  var i
  for (i = 0; i < active; i++) {
    // 4. dead spark: pop-and-sparkle burst, then respawn at the nozzle
    if (sEng[i] <= 0) {
      var j
      for (j = 0; j < 3; j++) {
        var nb = floor(random(active))
        sEng[nb] = random(0.3)
        sPos[nb] = pos + (random(4) - 2)
        sHue[nb] = random(1)
      }
      sEng[i] = 1 + random(0.33)
      sPos[i] = pos
      if (multicolor) sHue[i] = random(1)
    }

    sEng[i] = max(0, sEng[i] - friction)           // friction, floored at 0
    sPos[i] -= sEng[i] * sEng[i] * 40 * dt          // move down ~ energy^2
    if (sPos[i] < 0 || sPos[i] >= N) { sPos[i] = pos; sEng[i] = 0 }

    var e = sEng[i]
    var bright = e * e
    var hue = multicolor ? sHue[i] : exhaustHue
    if (bright < 0.5) hue = hue + random(0.15)      // end-of-life cool flicker
    var sat = clamp(exhaustSat * (1.2 - bright), 0, 1)
    deposit(floor(sPos[i]), hue, sat, clamp(bright, 0, 1))
  }

  // 5. rocket body: bright block added at the current position
  var b
  for (b = 0; b < rocketSize; b++) {
    var pidx = floor(pos) + b
    if (pidx >= 0 && pidx < N) {
      rBuf[pidx] += rocketR
      gBuf[pidx] += rocketG
      bBuf[pidx] += rocketB
    }
  }
}

export function render(index) {
  rgb(clamp(rBuf[index], 0, 1), clamp(gBuf[index], 0, 1), clamp(bBuf[index], 0, 1))
}
