// name: Spring Colors
// Clean-room reimplementation from a prose functional description of the
// community pattern "Spring Colors"; original source never consulted.

// A gentle, unsynchronized twinkle field: every pixel independently glows in
// one of four palette hues, fades to black over several seconds, then
// instantly relights in a (possibly different) palette hue at a random
// starting brightness. No waves, no motion — a calm shimmering mosaic.

var bri = array(pixelCount)   // per-pixel brightness (all start at 0 -> respawn on frame 1)
var hues = array(pixelCount)  // per-pixel hue

// warm spring-bloom palette: red, red-orange, orange, golden yellow (rare accent)
var h1 = 0.0
var h2 = 0.03
var h3 = 0.08
var h4 = 0.14

var accum = 0
var TICK = 40            // housekeeping runs every ~40 ms
var BASE_FADE = 0.008    // per tick: full brightness reaches black in ~5 s
var fade = BASE_FADE     // BASE_FADE * speed

// Speed multiplier: 1 = the original ~5 s fade, 5 = a ~1 s frantic shimmer,
// 0.2 = a ~25 s slow bloom.
//# min=0.2 max=5 step=0.05 default=1
export function sliderSpeed(v) {
  fade = BASE_FADE * clamp(v, 0.05, 20)
}

export function hsvPickerPrimary(h, s, v)    { h1 = h }   // only hue is used
export function hsvPickerSecondary(h, s, v)  { h2 = h }
export function hsvPickerTertiary(h, s, v)   { h3 = h }
export function hsvPickerQuaternary(h, s, v) { h4 = h }   // rare accent color

export function beforeRender(delta) {
  accum += delta
  while (accum >= TICK) {
    accum -= TICK
    var i
    for (i = 0; i < pixelCount; i++) {
      // fixed fade rate per tick (clean version of the original's
      // frame-rate-entangled decrement)
      bri[i] -= fade
      if (bri[i] <= 0) {
        // weighted palette pick: ~30 / 30 / 37 / 3 percent
        var r = random(1)
        if (r < 0.3)       hues[i] = h1
        else if (r < 0.6)  hues[i] = h2
        else if (r < 0.97) hues[i] = h3
        else               hues[i] = h4
        // respawn anywhere from off to full — keeps it organic, not blinky
        bri[i] = random(1)
      }
    }
  }
}

export function render(index) {
  var v = bri[index]
  hsv(hues[index], 1, v * v)   // squared for a perceptually smooth fade
}
