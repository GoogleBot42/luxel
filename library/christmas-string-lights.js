// name: Christmas string lights
// Clean-room reimplementation from a prose functional description of the
// community pattern "Christmas string lights"; original source never consulted.

// Widely spaced "C9 bulbs" on a dark strand: small bright bulbs a few
// pixels wide with long dark gaps, cycling red / blue / green / orange /
// purple down the strip. Each bulb glows brightest at its center pixel.
// Very rarely a pixel blinks off for one frame (a subtle twinkle).
// Looks best at low global brightness.

var BULB = 3          // bulb width in pixels
var GAP = 15          // dark gap in pixels (several times the bulb width)
var PERIOD = BULB + GAP
var FLOOR = 0.3       // brightness floor at the bulb edges (pre-curve)

// the classic five-color C9 set, in strip order
var hues = array(5)
hues[0] = 0.0         // warm red
hues[1] = 0.66        // medium blue
hues[2] = 0.33        // green
hues[3] = 0.09        // orange-leaning yellow
hues[4] = 0.79        // purple / violet

var twinkleP = 0

export function beforeRender(delta) {
  // scale the per-pixel blink-off chance by frame time so the twinkle
  // rate is frame-rate independent (~0.2% per pixel per 60 fps frame)
  twinkleP = delta * 0.00012
}

export function render(index) {
  var bulb = floor(index / PERIOD)
  var off = index - bulb * PERIOD
  if (off >= BULB) {          // in the gap between bulbs
    rgb(0, 0, 0)
    return
  }

  // rounded filament glow: triangle peak at the bulb center, mapped onto
  // a floor..1 range, then cubed to sharpen the peak
  var v = FLOOR + (1 - FLOOR) * triangle((off + 0.5) / BULB)
  v = v * v * v

  if (random(1) < twinkleP) v = 0   // rare single-frame blink-off

  hsv(hues[mod(bulb, 5)], 1, v)
}
