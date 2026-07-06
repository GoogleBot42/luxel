// name: 1D Aurora Borealis
// Clean-room reimplementation from a prose functional description of the
// community pattern "1D Aurora Borealis"; original source never consulted.

// Soft aurora blobs drift along the strip over black, fading in and out over
// tens of seconds, occasionally reversing direction. Overlaps blend with
// alpha-over compositing in RGB. Palette: greens, turquoise, pink, purple.

var POOL = 10

// wave record pool (parallel arrays); zeroed => everything is "dead" at
// startup and respawns randomly on the first frame
var wAge = array(POOL)     // ms
var wLife = array(POOL)    // ms
var wOpac = array(POOL)    // base opacity
var wHW = array(POOL)      // half-width, normalized
var wCtr = array(POOL)     // center, normalized
var wDir = array(POOL)     // +1 / -1
var wSpd = array(POOL)     // normalized units per ms
var wR = array(POOL)
var wG = array(POOL)
var wB = array(POOL)

// northern-lights palette: deep grass green, chartreuse, green turquoise,
// warm rose pink, soft violet purple
var palR = array(5), palG = array(5), palB = array(5)
palR[0] = 0.00; palG[0] = 0.60; palB[0] = 0.10
palR[1] = 0.45; palG[1] = 1.00; palB[1] = 0.00
palR[2] = 0.00; palG[2] = 0.90; palB[2] = 0.55
palR[3] = 1.00; palG[3] = 0.30; palB[3] = 0.50
palR[4] = 0.55; palG[4] = 0.30; palB[4] = 0.90

// slider-backed settings (affect waves as they respawn, except wave count)
var speedVal = 2      // 1..5
var maxHW = 0.35      // max half-width for new waves
var palPreset = 2     // 0 = equal, 1 = pink/purple heavy, 2 = greens (default)
var activeN = 6       // active waves

//# min=0 max=1 step=0.05 default=0.25
export function sliderSpeed(v) { speedVal = 1 + clamp(v, 0, 1) * 4 }

//# min=0 max=1 step=0.05 default=0.6
export function sliderWidth(v) { maxHW = 0.1 + clamp(v, 0, 1) * 0.4 }

//# min=0 max=1 step=0.5 default=1
export function sliderPalette(v) { palPreset = floor(clamp(v, 0, 1) * 2.99) }

//# min=0 max=1 step=0.111 default=0.555
export function sliderNumberOfWaves(v) {
  activeN = 1 + floor(clamp(v, 0, 1) * (POOL - 1) + 0.5)
}

// weighted random palette draw per the selected preset
function pickColor(i) {
  var w0 = 1, w1 = 1, w2 = 1, w3 = 1, w4 = 1
  if (palPreset == 1) { w3 = 4; w4 = 4 }             // pink/purple favored
  if (palPreset == 2) { w0 = 4; w1 = 4; w2 = 3 }     // greens favored
  var r = random(w0 + w1 + w2 + w3 + w4)
  var c = 4
  if (r < w0) c = 0
  else if (r < w0 + w1) c = 1
  else if (r < w0 + w1 + w2) c = 2
  else if (r < w0 + w1 + w2 + w3) c = 3
  wR[i] = palR[c]; wG[i] = palG[c]; wB[i] = palB[c]
}

function respawn(i) {
  wLife[i] = 15000 + random(15000)           // 15..30 s
  wAge[i] = 0
  pickColor(i)
  wOpac[i] = 0.5 + random(0.5)
  wHW[i] = 0.1 + random(max(maxHW - 0.1, 0.01))
  wCtr[i] = random(1)
  wDir[i] = random(1) < 0.5 ? -1 : 1
  // tuned so a blob takes many seconds to cross the strip
  // (scaled by delta below, i.e. frame-rate independent)
  wSpd[i] = speedVal * (0.5 + random(0.5)) * 0.00002
}

export function beforeRender(delta) {
  for (var i = 0; i < activeN; i++) {
    if (random(1) < 0.02) wDir[i] = -wDir[i]   // occasional waver
    wCtr[i] += wSpd[i] * wDir[i] * delta
    wAge[i] += delta
    var dead = wAge[i] >= wLife[i]
    if (wCtr[i] + wHW[i] < 0) dead = 1         // drifted fully off an end
    if (wCtr[i] - wHW[i] > 1) dead = 1
    if (dead) respawn(i)
  }
}

export function render(index) {
  var p = index / pixelCount
  var r = 0, g = 0, b = 0
  for (var i = 0; i < activeN; i++) {
    var d = abs(p - wCtr[i])
    if (d > wHW[i]) continue
    // radial dome with feathered shoulders * skewed age triangle
    var a = wOpac[i]
      * (1 - sqrt(d / wHW[i]))
      * triangle(sqrt(wAge[i] / wLife[i]))
    // alpha-over composite
    r = r * (1 - a) + wR[i] * a
    g = g * (1 - a) + wG[i] * a
    b = b * (1 - a) + wB[i] * a
  }
  rgb(r, g, b)
}
