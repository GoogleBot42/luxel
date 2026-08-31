// name: ChristmasLights
// Clean-room reimplementation from a prose functional description of the
// community pattern "ChristmasLights"; original source never consulted.
//
// Classic blinking holiday string lights: the strip is cut into fixed
// blocks of consecutive LEDs that cycle through three looks — vivid red,
// vivid green, and a dimmed soft white — repeating down the strip. Every
// couple of seconds all blocks switch at once, each stepping to the next
// look, so the coloring appears to rotate one role at a time. Hard
// synchronized switches, no fading.

var blockSize = 20       // LEDs per block
var blinkInterval = 2000 // ms between switches
var hueA = 0             // first bulb color (red)
var hueB = 1 / 3         // second bulb color (green)

//# min=1 max=40 step=1 default=20
export function sliderBlockSize(v) { blockSize = max(1, floor(v)) }

// Seconds each coloring holds before every block steps to the next look.
//# min=0.2 max=20 step=0.1 default=2
export function sliderBlinkSeconds(v) { blinkInterval = max(0.05, v) * 1000 }

//# min=0 max=360 step=5 default=0
export function sliderColorAHue(v) { hueA = v / 360 }

//# min=0 max=360 step=5 default=120
export function sliderColorBHue(v) { hueB = v / 360 }

// Role enum: 0 = red, 1 = green, 2 = soft white.
// All three initialized up front (the original left two uninitialized
// until the first switch — not worth reproducing).
var roles = array(3)
roles[0] = 0
roles[1] = 1
roles[2] = 2

var accum = 0

export function beforeRender(delta) {
  accum += delta
  if (accum > blinkInterval) {
    accum = 0
    for (var i = 0; i < 3; i++) {
      roles[i] = (roles[i] + 1) % 3
    }
  }
}

export function render(index) {
  var role = roles[floor(index / blockSize) % 3]
  if (role == 0) {
    hsv(hueA, 1, 1)       // vivid red
  } else if (role == 1) {
    hsv(hueB, 1, 1)       // vivid green
  } else {
    hsv(0, 0.15, 0.5)     // soft white at half brightness
  }
}
