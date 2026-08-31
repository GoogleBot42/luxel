// name: novas
// Clean-room reimplementation from a prose functional description of the
// community pattern "novas"; original source never consulted.
// Two structurally identical pulse generators, differing only in tint, bloom
// soft embers at random spots: each pops on white-hot, expands and dims over
// several seconds, fading into its tint. Generators merge by brightest-wins.

var NG = 2       // two generators (crimson/pink, orange-red)
var SLOTS = 6    // concurrent pulses per generator

var alive = array(NG * SLOTS)
var birth = array(NG * SLOTS)
var posn  = array(NG * SLOTS)   // normalized 0..1 position
var life  = array(NG * SLOTS)   // seconds
var nextSpawn = array(NG)

var tmp  = array(pixelCount)    // scratch intensity buffer, one generator
var rBuf = array(pixelCount)
var gBuf = array(pixelCount)
var bBuf = array(pixelCount)

var clock = 0

// Tunables — top-level values are the constants the port shipped with, so an
// untouched pattern renders exactly as before.
var spawnSecs = 1.5     // mean seconds between blooms, per generator
var bloomSecs = 4       // shortest bloom lifetime, seconds (jitter adds 50%)
var spreadPct = 33      // how fast a bloom widens, % of the strip per second

// Per-generator tints (channel weights the bloom fades toward as it dims).
var trA = 1, tgA = 0,   tbA = 0.3    // crimson / hot pink
var trB = 1, tgB = 0.3, tbB = 0      // orange-red

// hue+saturation -> channel weight (V is deliberately ignored: Luxel has a
// global brightness and the bloom supplies its own envelope)
function tintR(h, s) { return 1 - s + s * clamp(abs(6 * h - 3) - 1, 0, 1) }
function tintG(h, s) { return 1 - s + s * clamp(2 - abs(6 * h - 2), 0, 1) }
function tintB(h, s) { return 1 - s + s * clamp(2 - abs(6 * h - 4), 0, 1) }

// Average seconds between blooms from each of the two generators.
//# min=0.2 max=6 step=0.1 default=1.5
export function sliderSpawnSeconds(v) { spawnSecs = max(v, 0.1) }

// How long a bloom lives before it is gone, in seconds (each bloom picks a
// random lifetime between this and half again as long).
//# min=1 max=12 step=0.5 default=4
export function sliderBloomSeconds(v) { bloomSecs = max(v, 0.5) }

// How fast a bloom widens, as a percentage of the strip per second.
//# min=0 max=100 step=1 default=33
export function sliderSpreadPercentPerSecond(v) { spreadPct = clamp(v, 0, 200) }

// Tint of the first generator's blooms (hue + saturation; cores stay white).
export function hsvPickerColorA(h, s, v) { trA = tintR(h, s); tgA = tintG(h, s); tbA = tintB(h, s) }

// Tint of the second generator's blooms.
export function hsvPickerColorB(h, s, v) { trB = tintR(h, s); tgB = tintG(h, s); tbB = tintB(h, s) }

export function beforeRender(delta) {
  var dt = delta / 1000
  clock += dt

  var i
  for (i = 0; i < pixelCount; i++) { rBuf[i] = 0; gBuf[i] = 0; bBuf[i] = 0 }

  var g
  for (g = 0; g < NG; g++) {
    for (i = 0; i < pixelCount; i++) tmp[i] = 0

    // spawn a pulse when the clock passes this generator's next-spawn time
    if (clock >= nextSpawn[g]) {
      var s
      for (s = 0; s < SLOTS; s++) {
        var k = g * SLOTS + s
        if (alive[k] < 0.5) {
          alive[k] = 1
          posn[k] = random(1)
          life[k] = bloomSecs + random(bloomSecs / 2)
          birth[k] = clock
          break
        }
      }
      // schedule next spawn ~1.5 s out, bell-curve jitter (summed uniforms)
      var jit = (random(1) + random(1) + random(1)) / 3 - 0.5
      nextSpawn[g] = clock + spawnSecs + jit * (0.8 * spawnSecs / 1.5)
    }

    // accumulate live pulses into this generator's intensity buffer
    var s2
    for (s2 = 0; s2 < SLOTS; s2++) {
      var k2 = g * SLOTS + s2
      if (alive[k2] < 0.5) continue
      var age = clock - birth[k2]
      var frac = age / life[k2]
      if (frac >= 1) { alive[k2] = 0; continue }

      var temporal = (1 - frac) * (1 - frac)   // pops on full, quadratic decay
      var width = 0.1 + (spreadPct / 100) * age   // widens with age (normalized)
      var lo = posn[k2] - width / 2
      var loI = floor(lo * pixelCount)
      var hiI = ceil((posn[k2] + width / 2) * pixelCount)
      if (loI < 0) loI = 0
      if (hiI > pixelCount) hiI = pixelCount

      var px
      for (px = loI; px < hiI; px++) {
        var u = (px / pixelCount - lo) / width   // 0..1 across the hump
        if (u < 0 || u > 1) continue
        tmp[px] += temporal * sin(u * PI)        // half-sine spatial envelope
      }
    }

    // tint -> white lerp by intensity, then max-merge into the RGB buffers
    var tr, tg, tb
    if (g == 0) { tr = trA; tg = tgA; tb = tbA }   // crimson / hot pink
    else        { tr = trB; tg = tgB; tb = tbB }   // orange-red
    for (i = 0; i < pixelCount; i++) {
      var it = tmp[i]
      if (it <= 0) continue
      var w = clamp(it, 0, 1)
      var cr = it * (tr + (1 - tr) * w)
      var cg = it * (tg + (1 - tg) * w)
      var cb = it * (tb + (1 - tb) * w)
      if (cr > rBuf[i]) rBuf[i] = cr
      if (cg > gBuf[i]) gBuf[i] = cg
      if (cb > bBuf[i]) bBuf[i] = cb
    }
  }
}

export function render(index) {
  var r = clamp(rBuf[index], 0, 1)
  var g = clamp(gBuf[index], 0, 1)
  var b = clamp(bBuf[index], 0, 1)
  rgb(r * r, g * g, b * b)   // square for gamma
}
