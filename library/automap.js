// name: Automap
// Clean-room reimplementation from a prose functional description of the
// community utility pattern "Automap"; original source never consulted.

// Mapping helper, not a decorative effect: an external client sets the
// exported integer variable to a pixel index over the vars API and this
// lights exactly that one pixel, fully bright, pure saturated red, leaving
// every other pixel dark. The variable defaults to an impossible index
// (-1) so nothing is lit until a client sets it: undriven, the pattern is
// inert and renders black — no idle animation, no self-demo.

export var pixel = -1

export function beforeRender(delta) {
  // per-frame hook present but empty: the exported var is the whole interface
}

export function render(index) {
  // brightness = "this pixel's index equals the requested index"
  hsv(0, 1, index == pixel)
}
