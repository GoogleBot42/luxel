// name: 1D Aurora Borealis
// Clean-room reimplementation from a prose functional description of the
// community pattern "1D Aurora Borealis"; original source never consulted.

// Soft aurora blobs drift along the strip over black, fading in over their
// lives and out again, occasionally reversing direction. Overlapping blobs
// alpha-composite in RGB. Northern-lights palette: greens/turquoise/pink/purple.

var POOL = 10
var age = array(POOL)      // ms
var life = array(POOL)     // ms; starts 0 so every wave respawns on frame 1
var opac = array(POOL)     // base opacity
var hw = array(POOL)       // half-width, strip fraction
var ctr = array(POOL)      // center, 0..1
var dir = array(POOL)      // +1 / -1
var spd = array(POOL)      // strip fractions per second
var colR = array(POOL)
var colG = array(POOL)
var colB = array(POOL)

// palette: deep grass green, chartreuse, turquoise, rose pink, violet
var palR = array(5)
var palG = array(5)
var palB = array(5)
palR[0] = 0.00; palG[0] = 0.50; palB[0] = 0.05
palR[1] = 0.35; palG[1] = 0.90; palB[1] = 0.05
palR[2] = 0.00; palG[2] = 0.80; palB[2] = 0.55
palR[3] = 0.95; palG[3] = 0.30; palB[3] = 0.45
palR[4] = 0.55; palG[4] = 0.25; palB[4] = 0.85

// three weighting presets, 5 weights each (row-major)
var wts = array(15)
wts[0] = 0.2;  wts[1] = 0.2;  wts[2] = 0.2;  wts[3] = 0.2;  wts[4] = 0.2   // even
wts[5] = 0.06; wts[6] = 0.06; wts[7] = 0.08; wts[8] = 0.4;  wts[9] = 0.4   // pink/purple
wts[10] = 0.3; wts[11] = 0.3; wts[12] = 0.2; wts[13] = 0.1; wts[14] = 0.1  // greens

var speedMul = 3      // 1..5
var maxHW = 0.3       // max spawn half-width
var preset = 2        // default: greens favored
var nWaves = 5        // active waves (takes effect immediately)

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) { speedMul = 1 + v * 4 }

//# min=0 max=1 step=0.01 default=0.5
export function sliderWidth(v) { maxHW = 0.1 + v * 0.4 }

//# min=0 max=1 step=0.5 default=1
export function sliderPalette(v) { preset = floor(v * 2.99) }

//# min=0 max=1 step=0.111 default=0.44
export function sliderNumberOfWaves(v) { nWaves = 1 + floor(v * 9.99) }

function respawn(i) {
  age[i] = 0
  life[i] = 15000 + random(15000)          // 15..30 s
  opac[i] = 0.5 + random(0.5)
  hw[i] = 0.1 + random(max(0.001, maxHW - 0.1))
  ctr[i] = random(1)
  dir[i] = random(1) < 0.5 ? -1 : 1
  spd[i] = speedMul * (0.5 + random(0.5)) * 0.02   // many seconds to cross
  // weighted palette draw for the current preset
  var r = random(1)
  var acc = 0
  var k = 0
  var pick = 4
  for (k = 0; k < 5; k++) {
    acc += wts[preset * 5 + k]
    if (r < acc) { pick = k; break }
  }
  colR[i] = palR[pick]
  colG[i] = palG[pick]
  colB[i] = palB[pick]
}

export function beforeRender(delta) {
  for (var i = 0; i < nWaves; i++) {
    if (random(1) < 0.02) dir[i] = -dir[i]         // occasional waver
    ctr[i] += spd[i] * dir[i] * delta / 1000       // delta-scaled drift
    age[i] += delta
    if (age[i] >= life[i] || ctr[i] + hw[i] < 0 || ctr[i] - hw[i] > 1) {
      respawn(i)
    }
  }
}

export function render(index) {
  var p = index / pixelCount
  var r = 0, g = 0, b = 0
  for (var i = 0; i < nWaves; i++) {
    var d = abs(p - ctr[i])
    if (d > hw[i]) continue
    // base opacity * feathered dome * age envelope (sqrt skews peak early)
    var a = opac[i] * (1 - sqrt(d / hw[i])) * triangle(sqrt(age[i] / life[i]))
    r += (colR[i] - r) * a          // alpha-over compositing
    g += (colG[i] - g) * a
    b += (colB[i] - b) * a
  }
  rgb(r, g, b)
}
