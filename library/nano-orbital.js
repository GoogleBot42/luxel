// name: Nano Orbital
// Clean-room reimplementation from a prose functional description of the
// community pattern "Nano Orbital"; original source never consulted.

// For modular-panel builds (many identical panels, same LED count each):
// exactly one LED per panel is lit, all panels in lockstep, the dot
// stepping around each panel once per lap of the master clock. Rainbow
// spread across the installation, rotating on the same clock. Edit
// pixelsPerPanel to fit.

var pixelsPerPanel = 12
var panelCount = floor(pixelCount / pixelsPerPanel)
var lit = array(pixelCount)
var lapMs = 0   // sawtooth accumulated in whole milliseconds (drift-free)
var clock = 0

export function beforeRender(delta) {
  // one full lap per second, integrated from delta so it is frame-rate
  // independent and position/hue stay phase-locked to the same sawtooth.
  // Accumulating in ms and dividing fresh keeps 16.16 rounding from drifting
  // the period away from exactly 1.000 s over a long run.
  lapMs = mod(lapMs + delta, 1000)
  clock = lapMs / 1000

  // Step walks every LED of a panel once per clock period.
  var step = floor(clock * pixelsPerPanel)

  // clear last frame's dots. NOTE: arrayReplace(a, 0) writes ONE value at
  // slot 0, it is not a fill; feedback(a, 0) is the in-place zero.
  feedback(lit, 0)
  for (var p = 0; p < panelCount; p++) {
    var i = p * pixelsPerPanel + step
    if (i < pixelCount) lit[i] = 1  // guard non-multiple strip lengths
  }
  // Panels may not tile the strip exactly; leftover tail stays dark.
}

export function render(index) {
  hsv(clock + index / pixelCount, 1, lit[index])
}
