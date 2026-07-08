// name: Rainbow Flag
// Clean-room reimplementation from a prose functional description of the
// community pattern "Rainbow Flag"; original source never consulted.

// Six equal, fully-saturated, bright stripes running from the start of the
// strip to the end: violet, medium blue, green, yellow, orange, red. This
// is the reverse of the usual flag order, preserved as described. Nothing
// animates and no state is kept. Stripe length adapts to any pixel count.
// The described boundary quirk (strict inequalities leaving edge pixels
// dark) is fixed here with floor(index/stripeLen) into a six-color table.

var hues = array(6)
hues[0] = 0.80   // violet / purple
hues[1] = 0.62   // medium blue
hues[2] = 0.33   // green
hues[3] = 0.16   // yellow
hues[4] = 0.07   // orange
hues[5] = 0.00   // red

export function beforeRender(delta) {
  // static pattern: no per-frame animation or state
}

export function render(index) {
  var stripeLen = pixelCount / 6
  var stripe = clamp(floor(index / stripeLen), 0, 5)
  hsv(hues[stripe], 1, 1)
}
