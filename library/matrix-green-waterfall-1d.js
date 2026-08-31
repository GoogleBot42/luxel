// name: Matrix Green Waterfall 1D
// Curated original for the Luxel library: the strip reading of the digital-rain
// look. Where the 2D pattern gives every column its own falling ramp, a strip
// has only one column, so the rain becomes a handful of independent drops
// chasing each other along it — each a near-white head trailing a green tail
// that dims with distance, with the occasional glyph-like flicker in the tail.
// Drops respawn above the strip with a fresh speed and gap, so they never
// settle into a fixed rhythm.
//
// Every control carries a //# directive, so the UI sends REAL units (drops,
// pixels per second, pixels of trail, a hue) and the handlers use them as-is.

var MAXDROPS = 8

var dropCount = 4       // live drops
var fallSpeed = 18      // pixels per second, before each drop's own factor
var trailLen = 12       // trail length in pixels, head included
var hue = 0.33          // matrix green

var pos = array(MAXDROPS)   // head position in pixels; negative = still above
var rate = array(MAXDROPS)  // per-drop speed factor, ~0.6 .. 1.4

// brightness accumulator: 0..1 is tail, >= 2 marks a head pixel
var buf = array(pixelCount)

function respawn(d) {
  rate[d] = 0.6 + random(0.8)
  pos[d] = -random(pixelCount * 0.9) - 1
}

var d = 0
for (d = 0; d < MAXDROPS; d = d + 1) respawn(d)

// Whole drops, in drops.
//# min=1 max=8 step=1 default=4
export function sliderDropCount(v) {
  dropCount = clamp(floor(v), 1, MAXDROPS)
}

// Fall rate in pixels per second (each drop scales it by its own factor).
//# min=2 max=60 step=1 default=18
export function sliderSpeed(v) {
  fallSpeed = max(0.5, v)
}

// Trail length in pixels, counting the head.
//# min=2 max=40 step=1 default=12
export function sliderTrailLength(v) {
  trailLen = clamp(floor(v), 2, 40)
}

// Hue of the rain: 0.33 is the classic matrix green.
//# min=0 max=1 step=0.01 default=0.33
export function sliderHue(v) {
  hue = mod(v, 1)
}

export function beforeRender(delta) {
  var dt = delta / 1000
  var i = 0
  for (i = 0; i < pixelCount; i = i + 1) buf[i] = 0

  var k = 0
  for (k = 0; k < dropCount; k = k + 1) {
    pos[k] = pos[k] + fallSpeed * rate[k] * dt
    // gone once the whole tail has cleared the far end
    if (pos[k] - trailLen > pixelCount) respawn(k)

    var head = floor(pos[k])
    var t = 0
    for (t = 0; t < trailLen; t = t + 1) {
      var p = head - t
      if (p < 0) continue
      if (p >= pixelCount) continue
      var v = 0
      if (t == 0) {
        v = 2                      // head marker: rendered hot and whitened
      } else {
        v = 1 - t / trailLen
        v = v * v                  // dark tail, bright near the head
        // glyph shimmer: a tail pixel occasionally drops out for a frame
        if (random(1) < 0.08) v = v * (0.2 + random(0.5))
      }
      if (v > buf[p]) buf[p] = v
    }
  }
}

export function render(index) {
  var v = buf[index]
  if (v >= 2) {
    hsv(hue, 0.2, 1)      // head: bleached almost to white
  } else {
    hsv(hue, 1, v)        // tail: fully saturated, fading out
  }
}
