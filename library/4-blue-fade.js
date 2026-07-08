// name: 4 Blue Fade
// Clean-room reimplementation from a prose functional description of the
// community pattern "4 Blue Fade"; original source never consulted.

// The whole strip is one fixed indigo-leaning blue at full saturation,
// breathing up and down on a slow sine. One full bright-dim-bright cycle
// takes ~30 s (time(0.46) => 0.46 * 65.536 s ~= 30 s). The pixel index is
// ignored; no per-pixel variation, no randomness, no controls. (The
// original's unused faster clock is deliberately omitted.)

const HUE = 0.7   // indigo-leaning blue, in hue turns

var bri = 0

export function beforeRender(delta) {
  bri = wave(time(0.46))   // 0..1 sinusoidal, ~30 s period
}

export function render(index) {
  hsv(HUE, 1, bri)
}
