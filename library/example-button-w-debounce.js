// name: Example - Button w/ debounce
// Clean-room reimplementation from a prose functional description of the
// community pattern "Example - Button w/ debounce"; original source never consulted.
//
// Tutorial pattern: reads a push-button on a digital pin (internal pull-up,
// button wired to ground) with "integrate disagreement" software debouncing.
// Each accepted press advances a mode counter; the whole strip shows one
// solid hue per mode. Edit buttonPin / numModes to taste.

var buttonPin = 26      // digital input pin, button to ground
var numModes = 3        // hues spaced evenly around the wheel
var debounceMs = 30     // raw reading must disagree this long to count

var lastState = HIGH    // last accepted (debounced) state; idles high
var disagreeMs = 0      // accumulated milliseconds of disagreement
var mode = 0

pinMode(buttonPin, INPUT_PULLUP)

export function beforeRender(delta) {
  var raw = digitalRead(buttonPin)
  if (raw == lastState) {
    // agreement resets the timer, so short glitches never flip the state
    disagreeMs = 0
  } else {
    disagreeMs += delta
    if (disagreeMs > debounceMs) {
      lastState = raw
      disagreeMs = 0
      // advance exactly once per accepted press (high -> low edge)
      if (lastState == LOW) mode = (mode + 1) % numModes
    }
  }
}

export function render(index) {
  hsv(mode / numModes, 1, 1)
}
