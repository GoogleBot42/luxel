// name: Nano Orbital
// Clean-room reimplementation from a prose functional description of the
// community pattern "Nano Orbital"; original source never consulted.

// For modular-panel builds (many identical panels, same LED count each):
// exactly one LED is lit per panel, at the same relative position on every
// panel, so a dot orbits every panel in lockstep. The dot steps discretely
// through the panel over roughly a minute; colors are a full-saturation
// rainbow spread across the whole installation, rotating with the same
// clock. Edit PIXELS_PER_PANEL to match your hardware.

var PIXELS_PER_PANEL = 12
// Derive panel count from the strip (generalized from the original, which
// hardcoded both and relied on panelCount == pixelsPerPanel by luck).
var panels = floor(pixelCount / PIXELS_PER_PANEL)

var lit = array(pixelCount)   // this frame's on/off mask
var t                         // master clock, ~1 minute period

export function beforeRender(delta) {
  t = time(0.9)               // ~59 s sawtooth
  var step = floor(t * PIXELS_PER_PANEL)   // walk every LED of a panel

  arrayReplace(lit, 0)
  for (var p = 0; p < panels; p++) {
    var i = p * PIXELS_PER_PANEL + step
    if (i < pixelCount) lit[i] = 1         // guard partial last panel
  }
}

export function render(index) {
  // Rainbow spread over the installation + slow rotation; all-or-nothing
  // brightness from the mask (discrete steps, faithful to the original).
  hsv(t + index / pixelCount, 1, lit[index])
}
