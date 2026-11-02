// name: Sparks
// tags: strip, ambient
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/sparks.md, sparks-center.md — reimagined, not a replica.
// A grinder's shower of sparks along a strip. Each spark leaves its origin
// white-hot, flies fast and slows as it burns down, cooling through orange to a
// deep ember before it winks out, and relaunches at once. Origin picks where the
// shower starts: the first pixel, the centre (both ways), or both ends inward.
// Drawn on the persistent frame: fade() for the trail, fillRangeAA + splatIndex
// per spark, so there are no pixel buffers at any strip length.

var MAX = 64

var count = 24
//# min=1 max=64 step=1 default=24
export function sliderSparks(v) { count = clamp(floor(v), 1, MAX) }

var speedPct = 100
//# min=10 max=300 step=10 default=100
export function sliderSpeed(v) { speedPct = clamp(v, 10, 300) }

// seconds for a trail to fade out behind a spark
var trailS = 0.6
//# min=0.1 max=3 step=0.1 default=0.6
export function sliderTrail(v) { trailS = clamp(v, 0.1, 3) }

// 0 start, 1 centre, 2 both ends
var origin = 0
//# min=0 max=2 step=1 default=0
export function sliderOrigin(v) { origin = clamp(floor(v), 0, 2) }

// the colour a spark cools to (degrees); 7 is a classic ember
var hueDeg = 7
//# min=0 max=360 step=1 default=7
export function sliderEmberHue(v) { hueDeg = clamp(v, 0, 360) }

var pos = array(MAX)
var dir = array(MAX)
var energy = array(MAX)
var reach = array(MAX)      // this spark's speed scale

var LIFE = 2.2              // seconds for a spark to burn down at 100 %
var dt = 0.05

function launch(i) {
  var n = pixelCount
  energy[i] = 1 + random(0.3)
  reach[i] = 0.7 + random(0.6)
  if (origin == 1) { pos[i] = n / 2 + random(2) - 1; dir[i] = random(1) < 0.5 ? -1 : 1 }
  else if (origin == 2 && i % 2) { pos[i] = n - 1 - random(3); dir[i] = -1 }
  else { pos[i] = random(3); dir[i] = 1 }
}

// stagger the opening shower so the strip is busy from the first frame
for (var i = 0; i < MAX; i++) { launch(i); energy[i] = random(1.3) }

export function beforeRender(delta) { dt = min(delta, 80) * 0.001 * speedPct / 100 }

export function renderBulk() {
  fade(pow(0.5, dt * 100 / speedPct * 4.3 / trailS))
  var n = pixelCount
  // a full-strength spark crosses most of its run before it burns out
  var run = origin == 0 ? n : n * 0.5
  var base = run / LIFE * 1.1
  var h = hueDeg / 360
  for (var i = 0; i < count; i++) {
    var e = energy[i]
    if (e <= 0) { launch(i); e = energy[i] }
    e -= dt / LIFE
    var p0 = pos[i]
    var p = p0 + dir[i] * max(0, e) * max(0, e) * base * reach[i] * dt
    if (e <= 0 || p < 0 || p >= n) { energy[i] = 0; continue }
    energy[i] = e
    pos[i] = p
    // hot = white, cooling = orange, dying = deep ember
    var s = clamp(2 - e * 1.6, 0.3, 1)
    var v = clamp(0.2 + e, 0, 1)
    var hh = h + 0.09 * clamp(e, 0, 1)
    if (abs(p - p0) > 1) {
      hsv(hh, min(1, s + 0.4), v * 0.3)
      fillRangeAA(min(p0, p), max(p0, p), 2)
    }
    hsv(hh, s, v)
    splatIndex(p, 1, 2)
  }
}
