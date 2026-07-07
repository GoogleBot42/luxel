// name: multimap simpledemo
// Clean-room reimplementation from a prose functional description of the
// community pattern "multimap simpledemo"; original source never consulted.

// Framework demo: partition one mapped display into named regions, each
// running its own mini-pattern. Region tests and patterns live in parallel,
// index-aligned lists (append your own to grow it). First match wins, so
// earlier regions layer on top; unmatched pixels are painted black.

const NUM_REGIONS = 2
var regionTests = array(NUM_REGIONS)
var regionPatterns = array(NUM_REGIONS)

// shared out-parameters: a region test sets `matched`, and may overwrite the
// remapped index / count / coordinates to re-express the pixel in its own
// local frame before its pattern runs
var matched = 0
var rIndex = 0
var rCount = 0
var rX = 0
var rY = 0

var t1 = 0
var t2 = 0

// --- region 1: disc at panel center, radius ~1/5 of the panel -------------
const DISC_R2 = 0.2 * 0.2  // squared radius: circle test without a sqrt

function testDisc(index, x, y) {
  var cx = x - 0.5
  var cy = y - 0.5
  if (cx * cx + cy * cy < DISC_R2) {
    matched = 1
    rX = cx // pass center-relative coordinates through
    rY = cy
  }
}

function patternDisc(index, x, y) {
  // synchronized blue pulse, ~1.3 s period
  hsv(0.667, 1, wave(t1))
}

// --- region 2: the quadrant where both coordinates are below midpoint -----
function testQuadrant(index, x, y) {
  if (x < 0.5 && y < 0.5) matched = 1 // no remapping
}

function patternQuadrant(index, x, y) {
  // green pulse whose rate varies with the product of the coordinates:
  // nearby pixels drift in and out of phase — a gentle shimmer
  hsv(0.333, 1, wave(t2 * (8 + rX * rY * 16)))
}

regionTests[0] = testDisc
regionPatterns[0] = patternDisc
regionTests[1] = testQuadrant
regionPatterns[1] = patternQuadrant

export function beforeRender(delta) {
  t1 = time(0.02)
  t2 = time(0.3)
}

export function render2D(index, x, y) {
  var i, test, pat
  for (i = 0; i < NUM_REGIONS; i++) {
    // seed the out-parameters with the pixel's real values
    matched = 0
    rIndex = index
    rCount = pixelCount
    rX = x
    rY = y
    test = regionTests[i]
    test(rIndex, rX, rY)
    if (matched) {
      pat = regionPatterns[i]
      pat(rIndex, rX, rY)
      return // first match wins
    }
  }
  rgb(0, 0, 0) // explicit black so stale colors never linger
}

// token 1D fallback: strip position as x, y pinned to zero
export function render(index) {
  render2D(index, index / pixelCount, 0)
}
