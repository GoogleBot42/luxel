// name: Automap
// Clean-room reimplementation from a prose functional description of the
// community utility pattern "Automap"; original source never consulted.

// Mapping helper, not a decorative effect: an external client sets the
// exported integer variable to a pixel index over the vars API and this
// lights exactly that one pixel, fully bright, pure saturated red, leaving
// every other pixel dark. The variable defaults to an impossible index
// (-1) so nothing is client-selected until set. To stay self-demonstrating
// (and visibly exercise the "light one specific pixel red" behavior) with
// no client attached, the idle default sweeps one red pixel along the strip.

export var pixelIndex = -1

export function beforeRender(delta) {
  // per-frame hook present but empty: the exported var is the whole interface
}

export function render(index) {
  var target = pixelIndex
  if (target < 0) {
    // idle self-demo: sweep a single lit pixel so the helper renders visibly
    target = floor(time(0.05) * pixelCount) % pixelCount
  }
  // brightness = "this pixel's index equals the requested index"
  hsv(0, 1, index == target)
}
