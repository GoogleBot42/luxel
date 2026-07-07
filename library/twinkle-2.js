// name: twinkle
// Clean-room reimplementation from a prose functional description of the
// community pattern "twinkle"; original source never consulted.

// Random pixels ignite at full brightness in a random hue drawn from a
// slider-bounded wedge of the wheel, then fade to black. A few new
// ignitions per frame keep the strip alive with scattered twinkles.
// (The original hardcoded a ~250-slot state array; here the slot arrays
// are sized to the actual pixel count.)

var slots = pixelCount
var hues = array(slots)
var vals = array(slots)

var hueLo = 0
var hueHi = 1
var sat = 1
var perFrame = 2   // new ignitions per frame
var decayAmt = .04 // intensity lost per frame

//# min=0 max=1 step=0.01 default=1
export function sliderHighHue(v) { hueHi = v }
//# min=0 max=1 step=0.01 default=0
export function sliderLowHue(v) { hueLo = v }
//# min=0 max=1 step=0.01 default=1
export function sliderSaturation(v) { sat = .5 + v * .5 }
//# min=0 max=1 step=0.01 default=0.25
export function sliderIgnitionRate(v) { perFrame = 1 + floor(v * 5) }
//# min=0 max=1 step=0.01 default=0.3
export function sliderDecaySpeed(v) { decayAmt = .005 + v * .1 }

export function beforeRender(delta) {
  var i
  // fade every slot (frame-based, like the original)
  for (i = 0; i < slots; i++) vals[i] -= decayAmt

  // ignite a handful of fresh twinkles
  var span = hueHi - hueLo
  if (span < 0) span = 0   // crossed bounds collapse to the low bound
  for (i = 0; i < perFrame; i++) {
    var s = floor(random(slots))
    vals[s] = 1
    hues[s] = hueLo + random(span)
  }
}

export function render(index) {
  var i = index % slots
  hsv(hues[i], sat, max(vals[i], 0))
}
