// Perceptual color showcase: two drifting hues blended along the strip
// with mixColors (OKLab — no muddy midpoints), an eased breathing dim,
// and setGamma so the low end of the fade stays visually even.
setGamma(2.2)

var a = array(3)
var b = array(3)
var c = array(3)

export function beforeRender(delta) {
  breath = 0.15 + 0.85 * easeInOutCubic(triangle(time(0.1)))
  hsv2rgb(time(0.29), 0.9, 1, a)        // endpoint colors wander slowly,
  hsv2rgb(time(0.37) + 0.35, 0.9, 1, b) // at different rates
}

export function render(index) {
  mixColors(a[0], a[1], a[2], b[0], b[1], b[2], index / (pixelCount - 1), c)
  rgb(c[0] * breath, c[1] * breath, c[2] * breath)
}
