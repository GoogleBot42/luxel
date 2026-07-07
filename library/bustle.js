// name: bustle
// Clean-room reimplementation from a prose functional description of the
// community pattern "bustle"; original source never consulted.

// Two-way comet traffic: four pulsers (wide fast magenta / narrow slow
// crimson, each launching from both ends) rasterized into per-pulser
// buffers, combined with a per-channel MAX so crossings occlude instead of
// blowing out to white.

var SLOTS = 10          // pulse pool per pulser

// per-pulser full-strip intensity buffers
var buf0 = array(pixelCount)
var buf1 = array(pixelCount)
var buf2 = array(pixelCount)
var buf3 = array(pixelCount)

// pulse slot state: 4 pulsers x SLOTS, flattened
var alive = array(4 * SLOTS)
var birth = array(4 * SLOTS)
var nextLaunch = array(4)     // scheduled launch time per pulser
var clock = 0                 // shared running clock, seconds

// mean interval + modest ~Gaussian jitter (sum of three uniforms, centered)
function scheduleNext(p, mean) {
  var jitter = (random(1) + random(1) + random(1) - 1.5) * 0.4 * mean
  nextLaunch[p] = clock + max(0.15, mean + jitter)
}

// p: pulser id; buf: its buffer; dir: +1 rightward, -1 leftward;
// crossTime: seconds to cross; mean: mean launch interval; width: pulse
// length as a fraction of the strip
function runPulser(p, buf, dir, crossTime, mean, width) {
  arrayReplace(buf, 0)

  // launch if due and a slot is free
  if (clock >= nextLaunch[p]) {
    var s
    for (s = 0; s < SLOTS; s++) {
      if (!alive[p * SLOTS + s]) {
        alive[p * SLOTS + s] = 1
        birth[p * SLOTS + s] = clock
        break
      }
    }
    scheduleNext(p, mean)
  }

  // advance + rasterize live pulses
  var travel = 1 + 2 * width      // start and finish just off-strip
  var s
  for (s = 0; s < SLOTS; s++) {
    if (!alive[p * SLOTS + s]) continue
    var prog = (clock - birth[p * SLOTS + s]) / crossTime
    if (prog > 1) {               // fully exited the far end
      alive[p * SLOTS + s] = 0
      continue
    }
    // leading-edge position along the strip
    var head, tail
    if (dir > 0) {
      head = -width + prog * travel
      tail = head - width
    } else {
      head = 1 + width - prog * travel
      tail = head + width
    }
    var lo = min(head, tail)
    var hi = max(head, tail)
    var i0 = max(0, ceil(lo * (pixelCount - 1)))
    var i1 = min(pixelCount - 1, floor(hi * (pixelCount - 1)))
    var i
    for (i = i0; i <= i1; i++) {
      var pos = i / (pixelCount - 1)
      // squared ramp rising toward the direction of travel: peak at the
      // leading edge, quadratic tail behind. Same-pulser overlaps add.
      var rel = 1 - abs(head - pos) / width
      if (rel > 0) buf[i] += rel * rel
    }
  }
}

export function beforeRender(delta) {
  clock += delta / 1000
  // wide fast magenta pulses, one from each end
  runPulser(0, buf0, 1, 2.2, 1.5, 0.2)
  runPulser(1, buf1, -1, 2.2, 1.5, 0.2)
  // narrow slower crimson pulses, one from each end
  runPulser(2, buf2, 1, 4.4, 1.0, 0.1)
  runPulser(3, buf3, -1, 4.4, 1.0, 0.1)
}

export function render(index) {
  var wide = max(buf0[index], buf1[index])
  var narrow = max(buf2[index], buf3[index])
  // per-channel max across pulsers; magenta = (1, 0, 0.8), crimson = (1, 0, 0.15)
  var r = clamp(max(wide, narrow), 0, 1)
  var b = clamp(max(wide * 0.8, narrow * 0.15), 0, 1)
  rgb(r * r, 0, b * b)    // output squaring deepens the tails
}
