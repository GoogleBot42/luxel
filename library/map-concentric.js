// name: Map - Concentric
// Clean-room reimplementation from a prose functional description of the
// community pattern "Map - Concentric"; original source never consulted.

// A demo of 1D vs 2D renderers: a filled circle in a contrasting hue on a
// mapped panel, or a scrolling rainbow on a plain strip. Both hues drift
// together around the color wheel.

var radius = 0.25          // sensible default so the circle shows before the
var radiusSq = radius * radius  // slider is ever touched

export function sliderRadius(v) {
//# min=0 max=1 step=0.01 default=0.5
  radius = v * 0.45        // even at max, stays inside the unit square
  radiusSq = radius * radius
}

var phase = 0

export function beforeRender(delta) {
  phase = time(0.1)        // one trip around the hue wheel every ~6.5 s
}

export function render(index) {
  // 1D: classic scrolling rainbow
  hsv(phase + index / pixelCount, 1, 1)
}

export function render2D(index, x, y) {
  var dx = x - 0.5
  var dy = y - 0.5
  if (dx * dx + dy * dy < radiusSq) {
    hsv(phase, 1, 0.33)          // inside: base hue, dimmer
  } else {
    hsv(phase + 0.45, 1, 0.75)   // outside: strongly contrasting hue, brighter
  }
}
