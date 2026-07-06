// name: _Fairies
// Clean-room reimplementation from a prose functional description of the
// community pattern "_Fairies"; original source never consulted.

// A dense field of stationary twinkles: many colored points fade out over
// a few seconds, each instantly reborn at a new random position; a smaller
// population of short-lived pure white sparkles glints on top. Nothing
// travels — pure birth-fade-rebirth.

var numFairies = floor(pixelCount / 2)      // colored, slow
var numSparks = floor(pixelCount / 16)      // white, several times shorter
var count = numFairies + numSparks

// Per-spark state; white sparks live at the end of the arrays so they are
// written last each frame and win pixel collisions outright
var positions = array(count)
var lives = array(count)      // 1 -> 0 over the spark's lifetime
var lifetimes = array(count)  // ms
var isSpark = array(count)    // type flag, fixed at startup

// Per-pixel buffers — never cleared: a respawned spark abandons a
// near-zero residual at its old pixel (invisibly dim, harmless)
var vals = array(pixelCount)
var sats = array(pixelCount)

var hue = 0.9          // magenta-pink default
var baseLife = 3000    // ms

export function hsvPickerPrimaryColor(h, s, v) { hue = h }

//# min=0 max=1 step=0.01 default=0.4
export function sliderSpeed(v) {
  // ~1-to-5 range of base lifetime: well under 2 s up to many seconds
  baseLife = 1400 + v * 5600
}

function rollLifetime(spark) {
  // fairies: base +/- a fifth; white sparks: a fifth to two fifths of base
  if (spark) return baseLife * (0.2 + random(0.2))
  return baseLife * (0.8 + random(0.4))
}

var i
for (i = 0; i < count; i++) {
  isSpark[i] = i >= numFairies
  positions[i] = floor(random(pixelCount))
  lives[i] = random(1)              // stagger initial phases
  lifetimes[i] = rollLifetime(isSpark[i])
}

export function beforeRender(delta) {
  for (i = 0; i < count; i++) {
    // frame-rate independent: life hits 0 after exactly its lifetime
    lives[i] -= delta / lifetimes[i]
    if (lives[i] <= 0) {
      positions[i] = floor(random(pixelCount))
      lives[i] = 1
      lifetimes[i] = rollLifetime(isSpark[i])
    }
    var p = positions[i]
    vals[p] = lives[i]
    sats[p] = isSpark[i] ? 0 : 1    // white glints vs colored fairies
  }
}

export function render(index) {
  var v = vals[index]
  // squared brightness: gentle ease-out, twinkles linger dim before dying
  hsv(hue, sats[index], v * v)
}
