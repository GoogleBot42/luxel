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
          life[k] = 4 + random(2)        // 4..6 s
          birth[k] = clock
          break
        }
      }
      // schedule next spawn ~1.5 s out, bell-curve jitter (summed uniforms)
      var jit = (random(1) + random(1) + random(1)) / 3 - 0.5
      nextSpawn[g] = clock + 1.5 + jit * 0.8
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
      var width = 0.1 + 0.33 * age             // widens with age (normalized)
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
    if (g == 0) { tr = 1; tg = 0; tb = 0.3 }     // crimson / hot pink
    else        { tr = 1; tg = 0.3; tb = 0 }     // orange-red
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
