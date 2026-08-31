// name: block reflections
// Clean-room reimplementation from a prose functional description of the
// community pattern "block reflections"; original source never consulted.

// Mirror-symmetric bands folding out from the strip midpoint: a signed
// center-distance ramp is zoomed by slow breathing waves, folded into
// repeating blocks with a signed modulo, and the per-block hue feeds back
// into the brightness formula so blocks blink through dark seams.

var foldDepth = 10      // peak zoom: how many times the band ramp folds
var blockTurns = 0.33   // block size as a fraction of the hue circle
var blockWobble = 0.15  // peak-to-peak wobble of the block size (scales with it)
var timeScl = 1         // 1 / speed: multiplies every timer's period
var mirror = 1          // fold about the strip center

//# min=1 max=30 step=1 default=10
export function sliderFoldDepth(v) { foldDepth = max(0.5, v) }

// Block size as a percentage of the color wheel (33% is a third of the wheel:
// three hue slices per fold).
//# min=5 max=50 step=1 default=33
export function sliderBlockSize(v) {
  blockTurns = max(1, v) / 100
  blockWobble = 0.15 * (blockTurns / 0.33)   // keep the wobble proportional
}

// Overall animation rate; 1x is the pattern's native pace.
//# min=0.1 max=4 step=0.1 default=1
export function sliderSpeed(v) { timeScl = 1 / max(0.05, v) }

//# min=0 max=1 step=1 default=1
export function toggleMirror(on) { mirror = on > 0.5 }

export function beforeRender(delta) {
  hueBase = sin(time(0.06 * timeScl) * PI2)    // ~3.9 s: drifting shared base hue
  linPhase = time(0.06 * timeScl)              // same speed, linear: size wobble + brightness bias
  zoomTri = triangle(time(0.5 * timeScl))      // ~33 s: dominant zoom (bands multiply/merge)
  zoomWobble = sin(time(0.15 * timeScl) * PI2) // ~10 s: secondary zoom wobble
}

export function render(index) {
  // Signed distance from the strip midpoint, about -0.5 .. +0.5. Unmirrored,
  // the ramp runs one way along the whole strip instead of folding.
  var d = mirror ? (index - pixelCount / 2) / pixelCount : index / pixelCount

  // Zoom: triangle sweep up to the fold depth plus a sinusoidal wobble of
  // half that reach. Sets how many bands fit on the strip.
  var zoom = zoomTri * foldDepth + zoomWobble * foldDepth * 0.5

  // Block size hovers around a third of the hue circle by default, wobbling
  // by a modest fraction. The signed modulo folds the zoomed ramp into
  // repeating sawtooth blocks; the two halves fold in opposite
  // directions, giving the mirror effect for free.
  var blockSize = blockTurns + (triangle(linPhase) - 0.5) * blockWobble
  var block = (d * zoom) % blockSize

  // All blocks share the drifting base color; each block spans a slice
  // of adjacent hues (hue wraps naturally through both signs).
  var h = hueBase + block

  // Brightness couples back to the per-block hue, so it breaks up
  // per-block; the wrap of frac() makes hard travelling seams and the
  // squaring deepens the dark phase.
  var v = frac(abs(h) + abs(blockSize) + linPhase)
  v = v * v

  hsv(h, 1, v)
}
