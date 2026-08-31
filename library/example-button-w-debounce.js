// name: Example - Button w/ debounce
// Clean-room reimplementation from a prose functional description of the
// community pattern "Example - Button w/ debounce"; original source never consulted.
//
// Tutorial pattern: "integrate disagreement" software debouncing of a push
// button. Each accepted press advances a mode counter; the whole strip shows
// one solid hue per mode.
//
// The button is a UI control, not a pin: Luxel's `digitalRead()` reports the
// pin's idle level (HIGH here, since `pinMode` asked for a pull-up) and there
// is no way to drive it from outside the pattern, so a pin read would sit at
// "never pressed" and would let nobody without a soldered-on switch try the
// pattern. Hold "Button" to press, and "Tap" fires
// a press deliberately SHORTER than the debounce window so you can watch the
// debouncer reject it — lower "DebounceMs" and the same tap gets through.
// When Luxel grows real GPIO, the one marked line in beforeRender() becomes
// `digitalRead(buttonPin)` again and nothing else changes.

var buttonPin = 26      // digital input pin the button would use (to ground)
var numModes = 3        // hues spaced evenly around the wheel
var debounceMs = 50     // raw reading must disagree this long to count

var lastState = HIGH    // last accepted (debounced) state; idles high
var disagreeMs = 0      // accumulated milliseconds of disagreement
var mode = 0

var held = HIGH         // virtual line: LOW while the UI button is held
var tapMs = 0           // ms left of a simulated too-short press

pinMode(buttonPin, INPUT_PULLUP)   // the wiring this pattern expects: idle high

//# min=0 max=1 step=1 default=0
export function toggleButton(v) {
  held = v > 0.5 ? LOW : HIGH      // pressed pulls the line to ground
}

// A contact bounce: a press too short to be real. 60% of the debounce window,
// so a working debouncer must ignore it. Frame granularity is the floor here —
// a glitch shorter than one frame's delta still occupies one whole frame.
export function triggerTap() {
  tapMs = debounceMs * 0.6
}

//# min=5 max=250 step=5 default=50
export function sliderDebounceMs(v) {
  debounceMs = floor(v)
}

//# min=2 max=8 step=1 default=3
export function sliderModes(v) {
  numModes = floor(v)
  mode = mode % numModes
}

export function beforeRender(delta) {
  var raw = held                   // hardware: digitalRead(buttonPin)
  if (tapMs > 0) {
    tapMs = tapMs - delta
    raw = LOW
  }
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
