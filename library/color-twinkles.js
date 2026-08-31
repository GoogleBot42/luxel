// name: Color Twinkles
// Curated example (hand-written showcase of the Luxel language/builtins).
// Clean-room reimplementation from a prose description of the community
// pattern "color twinkles" (no source consulted). Stateless, random-free
// twinkles: two incommensurate spatial sines phase-modulate each other
// into a noise-like field; a fourth power plus a hard floor carves it
// into crisp sparks. The same field on a slower clock picks the hues.

// --- controls (defaults reproduce the original constants) ---------------
var twinkleSecs = 10.5   // brightness clock period, seconds (was time(0.16))
var colorSecs = 29       // hue clock period, seconds (was time(0.44))
var spacing = 20.27      // pixels per crest of the spatial field (2*PI/0.31)
var cutoff = 22          // black below this % of peak brightness

//# min=2 max=30 step=0.5 default=10.5
export function sliderTwinkleSeconds(v) { twinkleSecs = max(v, 0.5) }

//# min=2 max=60 step=1 default=29
export function sliderColorSeconds(v) { colorSecs = max(v, 0.5) }

//# min=4 max=40 step=0.01 default=20.27
export function sliderSpacingPixels(v) { spacing = max(v, 1) }

//# min=0 max=60 step=1 default=22
export function sliderCutoffPercent(v) { cutoff = clamp(v, 0, 99) }

// Reference values: every control scales its constant by a ratio that is
// exactly 1 at the control's default, so the untouched pattern is bit-for-bit
// the original.
var REF_SPACING = 20.27
var REF_TWINKLE = 10.5
var REF_COLOR = 29

var k1 = 0.31            // spatial frequency of the carrier
var k2 = 0.171           // ...and of the phase modulator (stays incommensurate)
var floorLevel = 0.22

export function beforeRender(delta) {
  var sp = REF_SPACING / spacing
  k1 = 0.31 * sp
  k2 = 0.171 * sp
  floorLevel = cutoff / 100
  tf = time(0.16 * (twinkleSecs / REF_TWINKLE)) * PI2  // brightness clock
  ts = time(0.44 * (colorSecs / REF_COLOR)) * PI2      // color clock
}

export function render(index) {
  b = (1 + sin(index * k1 + sin(index * k2 + tf) * PI2)) * 0.5
  b = b * b
  b = b * b  // ^4: crush the midtones, keep sparse peaks
  if (b < floorLevel) b = 0  // hard floor: clean black between twinkles
  h = sin(index * k1 + sin(index * k2 + ts) * PI2)
  hsv(h, 1, b * 0.5)
}
