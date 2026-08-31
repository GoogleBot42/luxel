// name: _Fairies
// Clean-room reimplementation from a prose functional description of the
// community pattern "_Fairies"; original source never consulted.

// A dense field of stationary twinkles: many colored points (picked hue,
// magenta-pink default) fade out over a few seconds and instantly respawn
// at a new random position at full brightness, plus a much smaller crew of
// short-lived pure white sparkles glinting on top. Nothing travels — pure
// birth-fade-rebirth. Populations scale with pixel count so density looks
// the same on any strip.

var nFairies = max(1, floor(pixelCount / 2))
var nSparks = max(1, floor(pixelCount / 16))
var total = nFairies + nSparks

// per-spark state; fairies occupy 0..nFairies-1, white sparks come after
// so on a shared pixel the white spark's write wins
var pos = array(total)
var life = array(total)
var lifetime = array(total)

// per-pixel buffers — never cleared: a respawned spark abandons its old
// pixel with a near-zero residual that just stays until reused (harmless,
// avoids a clear pass)
var briB = array(pixelCount)
var satB = array(pixelCount)

var hue = 0.9   // magenta-pink default
export function hsvPickerPrimaryColor(h, s, v) {
  hue = h        // only the hue is used; saturation forced full
}

var BASE_LIFE = 3000   // ms: nominal fairy lifetime at speed 1

// Twinkle rate multiplier — HIGHER IS FASTER. Applied to the per-frame life
// decrement rather than to the rolled lifetimes, so a slider move takes
// effect on the whole field immediately instead of trickling in over the
// next respawn cycle (which is why the old 0..1 slider looked inert).
// 1 = the nominal ~3 s fade, 6 = a frantic 0.5 s sparkle, 0.2 = ~15 s.
var speed = 1
//# min=0.2 max=6 step=0.1 default=1
export function sliderSpeed(v) {
  speed = clamp(v, 0.05, 20)
}

function rollLifetime(i) {
  if (i < nFairies) return BASE_LIFE * (0.8 + random(0.4))  // base ±1/5
  return BASE_LIFE * (0.2 + random(0.2))                    // 1/5..2/5 of base
}

// initialize with staggered ages so the field doesn't pulse in unison
var ii = 0
for (ii = 0; ii < total; ii++) {
  pos[ii] = floor(random(pixelCount))
  life[ii] = random(1)
  lifetime[ii] = rollLifetime(ii)
}

export function beforeRender(delta) {
  var i = 0
  for (i = 0; i < total; i++) {
    // life hits zero after lifetime/speed ms, frame-rate independently
    life[i] -= delta * speed / lifetime[i]
    if (life[i] <= 0) {
      pos[i] = floor(random(pixelCount))
      life[i] = 1
      lifetime[i] = rollLifetime(i)
    }
    var p = pos[i]
    briB[p] = life[i]
    satB[p] = i < nFairies ? 1 : 0   // fairy = colored, spark = white
  }
}

export function render(index) {
  var b = briB[index]
  // squared brightness: gentle ease-out, twinkles linger dim before dying
  hsv(hue, satB[index], b * b)
}
