// name: dimbypixel
// Clean-room reimplementation from a prose functional description of the
// community pattern "dimbypixel"; original source never consulted.

// A "how much of the strip is lit" utility. A single slider sets the fraction
// of the strip that is on; pixels from the start up to that fraction are lit
// chartreuse, the rest are off. Hard cutoff, no gradient.

var litFraction = 1     // default: whole strip lit

//# min=0 max=1 step=0.01 default=1
export function sliderLightsOn(v) {
  litFraction = v
}

export function render(index) {
  var pos = index / pixelCount        // 0..1 along the strip
  var on = litFraction > pos          // 1 if lit, 0 if past the cutoff
  hsv(0.22, 1, on)                    // chartreuse when on, black when off
}
