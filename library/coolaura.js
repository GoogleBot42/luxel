// name: coolaura
// Clean-room reimplementation from a prose functional description of the
// community pattern "coolaura"; original source never consulted.

// Calm aquatic aura on a 1D strip: soft blue-green pulses appear at random
// places, swell, spread outward, and fade; several are alive at once and
// their light adds where they overlap. The whole strip also breathes in and
// out over several seconds. Slow, ambient, never black (background is a dim
// blue). Behavior reimplemented from prose, not the node-graph structure.

const POOL = 10           // allocation ceiling for simultaneous pulses

// --- controls (defaults reproduce the original constants) ---------------
var maxPulses = 10        // pulses that may be alive at once
var lifetime = 4          // seconds each pulse lives
var spawnBase = 1.5       // seconds between spawns (jittered)
var breathSecs = 8        // period of the whole-strip breath, seconds
var maxWidth = 120        // widest a pulse gets, % of the strip

//# min=1 max=10 step=1 default=10
export function sliderMaxPulses(v) { maxPulses = clamp(floor(v), 1, POOL) }

//# min=1 max=12 step=0.5 default=4
export function sliderPulseSeconds(v) { lifetime = max(v, 0.5) }

//# min=0.2 max=8 step=0.1 default=1.5
export function sliderSpawnSeconds(v) { spawnBase = max(v, 0.1) }

//# min=2 max=30 step=1 default=8
export function sliderBreathSeconds(v) { breathSecs = max(v, 0.5) }

//# min=10 max=300 step=5 default=120
export function sliderMaxWidthPercent(v) { maxWidth = max(v, 5) }

var clock = 0             // wall-clock accumulator, seconds
var nextSpawn = 0

var alive = array(POOL)
var birth = array(POOL)
var center = array(POOL)  // 0..1 fraction of strip

var intensity = array(pixelCount)
var rbuf = array(pixelCount)
var gbuf = array(pixelCount)
var bbuf = array(pixelCount)

export function beforeRender(delta) {
  var dt = delta / 1000
  clock += dt

  // 1. clear intensity buffer
  var j = 0
  for (j = 0; j < pixelCount; j++) intensity[j] = 0

  // 2. spawn: if due and a free slot exists, activate it
  if (clock >= nextSpawn) {
    var slot = -1
    var s = 0
    for (s = 0; s < maxPulses; s++) {
      if (!alive[s]) { slot = s; break }
    }
    if (slot >= 0) {
      alive[slot] = 1
      birth[slot] = clock
      center[slot] = random(1)
    }
    // approx-normal jitter: sum of three uniforms (mean 1.5), centered
    var jitter = (random(1) + random(1) + random(1) - 1.5) * 0.5
    nextSpawn = clock + spawnBase + jitter
  }

  // 3. update every live pulse (iterate all slots, skip dead ones)
  var i = 0
  for (i = 0; i < maxPulses; i++) {
    if (!alive[i]) continue
    var age = clock - birth[i]
    if (age >= lifetime) { alive[i] = 0; continue }

    var na = age / lifetime              // normalized age 0..1
    var tEnv = sin(na * PI)              // half-sine temporal envelope
    // grows from a tenth of the strip to maxWidth% of it; the ratio is
    // exactly 1 at the control's default, so an untouched pattern is unchanged
    var width = 0.1 + na * (1.1 * ((maxWidth - 10) / 110))
    var c = center[i]
    var lo = c - width / 2
    var hi = c + width / 2

    for (j = 0; j < pixelCount; j++) {
      var pos = j / pixelCount
      if (pos < lo || pos > hi) continue
      var profile = sin((pos - lo) / width * PI)   // half-sine spatial bump
      intensity[j] += tEnv * profile
    }
  }

  // 4. global breathing envelope (raised cosine, ~8 s, 0..1)
  var breath = (1 - cos(clock / breathSecs * PI2)) / 2

  // 5. colorize through a two-stop gradient over (breathed) intensity
  for (j = 0; j < pixelCount; j++) {
    var t = clamp(intensity[j] * breath, 0, 1)
    // stop 0: medium blue-cyan   stop 1: green-teal
    rbuf[j] = 0
    gbuf[j] = mix(0.4, 0.9, t)
    bbuf[j] = mix(1.0, 0.3, t)
  }
}

export function render(index) {
  // square each channel: gamma-like curve that deepens darks, softens pulses
  var r = rbuf[index]
  var g = gbuf[index]
  var b = bbuf[index]
  rgb(r * r, g * g, b * b)
}
