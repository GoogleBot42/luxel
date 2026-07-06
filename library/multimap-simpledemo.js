// name: multimap simpledemo
// Clean-room reimplementation from a prose functional description of the
// community pattern "multimap simpledemo"; original source never consulted.

// A minimal "multiple sub-maps" framework: parallel, index-aligned lists of
// region tests and mini-patterns. First region to claim a pixel wins (earlier
// entries are on top); unclaimed pixels are painted black explicitly.
// To extend: bump NUM_REGIONS and append a test + pattern pair.
var NUM_REGIONS = 2
var regionTests = array(NUM_REGIONS)
var regionPatterns = array(NUM_REGIONS)

// Shared out-parameters. Seeded with the pixel's real values before each
// test; a test may overwrite them to re-express the pixel in its region's
// local frame. (Index/count remap hooks exist but are unused in this demo.)
var outIndex = 0
var outCount = 0
var outX = 0
var outY = 0

var clock = 0
var bluePulse = 0

export function beforeRender(delta) {
  clock += delta / 1000
  if (clock > 3600) clock -= 3600
  bluePulse = wave(time(0.023))   // synchronized ~1.5 s pulse
}

// Region 1: disc at panel center, radius ~1/5 of the panel.
// Squared-distance test: no square root needed. Remaps coordinates to be
// center-relative (this demo pattern happens not to use them).
function testCenterDisc(index, x, y) {
  var cx = x - 0.5
  var cy = y - 0.5
  if (cx * cx + cy * cy < 0.04) {   // 0.2 squared
    outX = cx
    outY = cy
    return 1
  }
  return 0
}

// Region 2: the quadrant where both coordinates are below their midpoints.
// No remapping.
function testLowQuadrant(index, x, y) {
  return x < 0.5 && y < 0.5
}

// Pattern 1: blue, whole region pulsing in unison.
function patternBluePulse(index, x, y) {
  hsv(0.667, 1, bluePulse)
}

// Pattern 2: green, pulse period offset per pixel by the coordinate product
// so neighbors drift in and out of phase -> shimmer.
function patternGreenShimmer(index, x, y) {
  hsv(0.333, 1, wave(clock / (1.5 + x * y * 2)))
}

regionTests[0] = testCenterDisc
regionTests[1] = testLowQuadrant
regionPatterns[0] = patternBluePulse
regionPatterns[1] = patternGreenShimmer

export function render2D(index, x, y) {
  var i, test, pat
  for (i = 0; i < NUM_REGIONS; i++) {
    // seed the out-parameters with the pixel's real values
    outIndex = index
    outCount = pixelCount
    outX = x
    outY = y
    test = regionTests[i]
    if (test(index, x, y)) {        // first match wins
      pat = regionPatterns[i]
      pat(outIndex, outX, outY)
      return
    }
  }
  rgb(0, 0, 0)                      // no stale colors on unclaimed pixels
}

// Token 1D fallback: the strip becomes one row (y pinned at 0), so only the
// green shimmer region can match.
export function render(index) {
  render2D(index, index / pixelCount, 0)
}
