// name: Nano Orbital
// Clean-room reimplementation from a prose functional description of the
// community pattern "Nano Orbital"; original source never consulted.

// For modular-panel builds (many identical panels, same LED count each):
// exactly one LED per panel is lit, all panels in lockstep, the dot
// stepping around each panel once per ~minute. Rainbow spread across the
// installation, rotating on the same clock. Edit pixelsPerPanel to fit.

var pixelsPerPanel = 12
var panelCount = floor(pixelCount / pixelsPerPanel)
var lit = array(pixelCount)
var clock

export function beforeRender(delta) {
  clock = time(0.9)  // ~59 s master lap

  // Step walks every LED of a panel once per clock period.
  var step = floor(clock * pixelsPerPanel)

  arrayReplace(lit, 0)
  for (var p = 0; p < panelCount; p++) {
    var i = p * pixelsPerPanel + step
    if (i < pixelCount) lit[i] = 1  // guard non-multiple strip lengths
  }
  // Panels may not tile the strip exactly; leftover tail stays dark.
}

export function render(index) {
  hsv(clock + index / pixelCount, 1, lit[index])
}
