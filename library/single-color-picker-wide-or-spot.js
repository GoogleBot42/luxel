// name: Single Color Picker - wide or spot
// Clean-room reimplementation from a prose functional description of the
// community pattern "Single Color Picker - wide or spot"; original source
// never consulted. A static 1D utility: one picked color, spread wide or
// squeezed into a spot by a sharpness exponent, peaked at a chosen location.

export var pickH = 0.78   // violet-ish default
export var pickS = 1
export var pickV = 1
export var focus = 0.5    // normalized peak location
export var sharpExp = 3   // effective falloff exponent

export function hsvPickerColor(h, s, v) {
  pickH = h
  pickS = s
  pickV = v
}

//# min=0 max=1 step=0.01 default=0.2
export function sliderSharpness(v) {
  // square the slider so the low end gives fine control over wide washes and
  // the top end reaches a very spiky spot (a couple orders of magnitude > 1);
  // below ~1 the term flattens toward a uniform wash.
  sharpExp = 0.1 + v * v * 100
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderLocation(v) {
  focus = v
}

export function render(index) {
  var pos = index / pixelCount
  var d = abs(pos - focus)
  var fall = pow(max(0, 1 - d), sharpExp)
  hsv(pickH, pickS, pickV * fall)
}
