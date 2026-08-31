// name: marching rainbow
// Clean-room reimplementation from a prose functional description of the
// community pattern "marching rainbow"; original source never consulted.

// Overlapping rainbow waves march along the strip. Brightness is the
// difference of a slow one-cycle wave and a faster fine-grained wave, so
// bright bands beat against a ripple and roughly half the strip is dark at
// any moment. Hue is the slow traveling wave fed through itself twice more
// (the nesting warps the rainbow nonlinearly), offset by position.

var t1 = 0
var t2 = 0

// Tunables, initialized to the constants the port shipped with so the
// untouched render is unchanged; the controls re-express them in real units.
var i1 = 0.1          // time() interval for the main march (~6.5 s)
var i2 = 0.05         // the fine ripple, always half that period
var ripples = 10      // ripple crests along the strip
var depth = 1         // how deeply the ripple cuts into the bands
var mirror = 0        // 1 = fold the march about the strip's midpoint

// Seconds for the main rainbow band to march the length of the strip once.
// The fine ripple stays locked at half this period.
//# min=1 max=30 step=0.5 default=6.5
export function sliderMarchTime(v) {
  i1 = max(v, 1) / 65.536
  i2 = i1 / 2
}

// How many ripple crests fit along the strip.
//# min=2 max=30 step=1 default=10
export function sliderRippleCount(v) {
  ripples = clamp(floor(v), 2, 30)
}

// How deeply the ripple cuts into the marching bands: 0 = a single smooth
// pulsing rainbow, 1 = the original's half-dark chop.
//# min=0 max=1 step=0.05 default=1
export function sliderRippleDepth(v) {
  depth = clamp(v, 0, 1)
}

// Fold the march about the middle of the strip, so it runs outward from
// the centre to both ends.
//# default=0
export function toggleMirror(v) {
  mirror = v
}

export function beforeRender(delta) {
  t1 = time(i1)     // main cycle, ~6.5 s
  t2 = time(i2)     // fine ripple, twice as fast
}

export function render(index) {
  var p = index / pixelCount
  if (mirror) p = abs(p * 2 - 1)
  var v = wave(t1 + p) - depth * wave(t2 - p * ripples + 0.2)
  if (v < 0) v = 0
  var h = wave(wave(wave(t1 + p)) - p)
  hsv(h, 1, v)
}
