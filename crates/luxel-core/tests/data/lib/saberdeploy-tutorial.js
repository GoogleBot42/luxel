// name: SaberDeploy Tutorial
// Clean-room reimplementation from a prose functional description of the
// community pattern "SaberDeploy Tutorial"; original source never consulted.

// A "light saber blade" of solid saturated red extends from index 0 toward the
// far end when triggered, then retracts on the next trigger. Pixels are binary
// on/off; the lit region is a contiguous run starting at index zero. Advance is
// scaled by the frame delta so deploy speed is frame-rate independent.
//
// The real pushbutton is an INJECTED EVENT: click/drag the preview, POST
// /api/events, or publish to the MQTT event topic ([type, x, y, value]) — each
// frame that carries at least one event counts as one press. The UI toggle is
// kept as an edge-detected stand-in for driving it by hand.

// Exported state (watchable in a debugging vars view)
export var moving = 1        // is the blade animating (auto-deploys on boot)
export var dir = 1           // +1 extending, -1 retracting
export var lit = 0           // fraction of strip currently lit, 0..1
export var btn = 0           // current (simulated) button state
export var lastBtn = 0       // previous frame's button state

var rate = 1.4               // lit fraction per second (set by the speed slider)
var ev = array(4)            // injected-event scratch: [type, x, y, value]

// Momentary pushbutton stand-in: each off->on transition toggles deploy/retract.
export function toggleDeploy(v) {
  btn = v ? 1 : 0
}

// Speed across roughly an order of magnitude, with a small floor so the blade
// always moves even at the slider minimum.
export function sliderSpeed(v) { //# min=0 max=1 step=0.01 default=0.5
  rate = 0.2 + v * 3.0
}

export function beforeRender(delta) {
  // Real presses: drain the event queue. A burst inside one frame is one
  // press — flipping twice would cancel out, which is never what was meant.
  var press = 0
  while (readEvent(ev)) press = 1

  // Edge-detect the toggle: only the not-pressed -> pressed transition counts.
  // Either source pressing mid-animation reverses the blade in place.
  if (press || (btn && !lastBtn)) {
    dir = -dir
    moving = 1
  }
  lastBtn = btn

  if (moving) {
    lit += dir * rate * (delta / 1000)   // time-scaled advance
    if (lit >= 1) { lit = 1; moving = 0 }
    if (lit <= 0) { lit = 0; moving = 0 }
  }
}

export function render(index) {
  var edge = lit * pixelCount
  if (index < edge) hsv(0, 1, 1)   // full-saturation red, full brightness
  else hsv(0, 0, 0)
}
