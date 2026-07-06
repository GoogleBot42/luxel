// name: 6 Green Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "6 Green Fade"; original source never consulted.

// A single classic green across the whole strip, breathing smoothly
// between off and full brightness. One full up-and-down cycle takes
// about half a minute.

var GREEN_HUE = 1 / 3   // classic pure green
var brightness = 0

export function beforeRender(delta) {
  // time(0.45) -> sawtooth with ~29.5 s period; triangle() folds it into
  // a linear ramp up and back down with no discontinuity.
  brightness = triangle(time(0.45))
}

export function render(index) {
  hsv(GREEN_HUE, 1, brightness)
}
