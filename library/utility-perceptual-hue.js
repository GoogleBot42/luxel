// name: Utility: Perceptual hue
// Clean-room reimplementation from a prose functional description of the
// community pattern "Utility: Perceptual hue"; original source never consulted.
//
// HSV's hue axis is not perceptually even: greens/blues hog the range while
// reds/oranges get squeezed. This pattern ships two reusable helpers that map
// a *perceptual* hue (0..1) to a corrected hue for hsv(), and demos them by
// cycling a scrolling rainbow through three modes:
//   mode 0: raw linear hue        (greens/blues dominate)
//   mode 1: perceptualHue()       (table remap - the good one)
//   mode 2: waveHue()             (cheap sinusoidal remap - decent)
// The strip blinks dark briefly at each mode change so you can tell them apart.

// --- Remap A: table interpolation ------------------------------------------
// Hand-tuned anchors, chosen by eye for equal *perceived* spacing. Early
// anchors bunch near red so reds/oranges/yellows get stretched; greens and
// blues get compressed. Tune to taste. Last real entry duplicates the wrap
// value; one extra entry guards the interpolation's out-of-range read.
var hueAnchors = array(11)
hueAnchors[0] = 0       // red
hueAnchors[1] = 0.02    // orange, only slightly above red
hueAnchors[2] = 0.085   // yellow, still quite low
hueAnchors[3] = 0.33    // big jump to green
hueAnchors[4] = 0.5     // cyan
hueAnchors[5] = 0.62    // blue
hueAnchors[6] = 0.7     // indigo
hueAnchors[7] = 0.78    // purple
hueAnchors[8] = 0.88    // pink
hueAnchors[9] = 1       // back around to red (wrap)
hueAnchors[10] = 1      // overflow guard
var hueArcs = 9         // gaps between real anchors

// Perceptual hue -> HSV hue via the anchor table. Wrap-safe input.
export function perceptualHue(h) {
  h = h - floor(h)              // wrap into 0..1
  var scaled = h * hueArcs
  var arc = floor(scaled)
  var f = scaled - arc          // progress through this arc
  return hueAnchors[arc] * (1 - f) + hueAnchors[arc + 1] * f
}

// --- Remap B: smooth wave ---------------------------------------------------
// One sinusoidal S-curve arc: shift the input by half, halve it, take wave().
// Cheaper than the table; bright greens still over-represented.
export function waveHue(h) {
  h = h - floor(h)              // wrap into 0..1
  return wave((h - 0.5) / 2)
}

// --- Demo harness ------------------------------------------------------------
// Try: freeze the scroll (scroll = 0) or offset by half a turn (+ 0.5)
// to park the reds where you can inspect them.
var scroll = 0
var demoMode = 0
var blink = 1

export function beforeRender(delta) {
  scroll = time(0.015)          // fast phase: ~1 s per hue revolution
  var thirds = time(0.15) * 3   // slow phase: ~10 s split into three modes
  demoMode = floor(thirds)
  // blackout blink for a beat at the start of each third
  blink = frac(thirds) < 0.15 ? 0 : 1
}

export function render(index) {
  var h = index / pixelCount + scroll
  if (demoMode == 1) h = perceptualHue(h)
  if (demoMode == 2) h = waveHue(h)
  hsv(h, 1, blink)
}
