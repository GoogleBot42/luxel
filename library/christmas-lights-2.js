// name: Christmas Lights
// Clean-room reimplementation from a prose functional description of the
// community pattern "Christmas Lights"; original source never consulted.

// Old-fashioned bulb string: every other pixel dark, lit pixels cycling
// red / green / warm white in a fixed six-pixel layout (red, gap, green,
// gap, white, gap). The whole arrangement hops one pixel toward higher
// indices every few seconds — discrete steps, no fading.

const PERIOD = 6           // pixels per repeat of the bulb layout

var elapsed = 0            // accumulated ms since the last hop
var shift = 0              // current hop offset, 0..5
var stepMs = 5000          // ms per hop (set by the slider)

// Step interval: nearly instant at the bottom up to ~10 s per hop.
//# min=0 max=1 step=0.01 default=0.5
export function sliderTicks(v) {
  stepMs = 100 + v * 9900
}

export function beforeRender(delta) {
  elapsed += delta
  if (elapsed >= stepMs) {
    elapsed -= stepMs      // carry the overshoot so hops stay evenly timed
    shift += 1
    if (shift >= PERIOD) shift = 0
  }
}

export function render(index) {
  // mod() is floored, so the shifted index wraps cleanly even when it goes
  // negative at the start of the strip (fixes the stale-pixel quirk).
  var p = mod(index - shift, PERIOD)
  if (p == 1) {
    rgb(1, 0, 0)           // pure red bulb
  } else if (p == 3) {
    rgb(0, 1, 0)           // pure green bulb
  } else if (p == 5) {
    rgb(1, 1, 1)           // white bulb
  } else {
    rgb(0, 0, 0)           // unlit gap
  }
}
