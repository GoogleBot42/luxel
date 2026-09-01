// name: Orv - Christmas Tree
// Clean-room reimplementation from a prose functional description of the
// community pattern "Orv - Christmas Tree"; original source never consulted.

// Static Christmas-tree scene (tree, topper, garland, ornaments, ground, sky)
// with warm twinkle lights on the tree and stars in the sky, driven by a small
// pool of drifting, decaying particles writing into a 16x16 virtual canvas.

var GRID = 16
var CANVAS = GRID * GRID          // 16x16 virtual canvas, row-major
var twinkle = array(CANVAS)       // per-canvas-cell twinkle intensity

var MAX_STARS = 32                // particle pool ceiling
var starVal = array(MAX_STARS)    // signed: brightness contribution + drift rate
var starPos = array(MAX_STARS)    // fractional canvas index
// head position + own brightness of each particle, refreshed every frame. The
// sky stars are drawn from THESE rather than from the decaying canvas buffer:
// reading the buffer smeared each star into a multi-cell comet tail, which is
// what made the stars behind the tree look oddly large.
var starX = array(MAX_STARS)      // normalized 0..1
var starY = array(MAX_STARS)
var starBri = array(MAX_STARS)

var BUF_DECAY = 0.97              // per-frame buffer fade (~1 s twinkle tail)
var STAR_DECAY = 0.985            // per-frame particle decay
var RESPAWN_BAND = 0.05           // |value| below this respawns the particle
var THRESH = 0.1                  // buffer value needed to light a tree twinkle
var GAIN = 10                     // post-square brightness boost
var SKY_FLOOR = 0.02              // below this a sky star is not drawn at all

// ---- controls ----------------------------------------------------------------
var numStars = floor(CANVAS / 20) // 12 particles
var starCells = 0.8               // sky-star radius, in canvas cells
var starR2 = starCells * starCells
var driftCells = 2                // cells/second at full particle strength
var ornSpacing = 11               // one ornament pair every N pixels along the tree

// Radius of a sky star in CANVAS CELLS (one cell = 1/16 of the rig). Below ~0.5
// a star is smaller than a pixel and only lights when it passes near a centre.
//# min=0.3 max=3 step=0.1 default=0.8
export function sliderStarSize(v) {
  starCells = v
  starR2 = v * v
}

// Size of the drifting particle pool. These feed BOTH the sky stars and the
// tree's twinkle lights, so turning it down calms the whole scene.
//# min=1 max=32 step=1 default=12
export function sliderSparkleCount(v) { numStars = clamp(floor(v), 1, MAX_STARS) }

// How fast a full-strength particle drifts, in canvas cells per second.
//# min=0 max=8 step=0.1 default=2
export function sliderStarDrift(v) { driftCells = v }

// Ornament spacing: one two-pixel ornament every N pixels of tree.
//# min=4 max=30 step=1 default=11
export function sliderOrnaments(v) { ornSpacing = max(3, floor(v)) }

export function beforeRender(delta) {
  feedback(twinkle, BUF_DECAY)
  var dt = delta / 1000
  for (var i = 0; i < numStars; i++) {
    if (abs(starVal[i]) < RESPAWN_BAND) {
      starVal[i] = random(1) - 0.5          // signed: negatives dim pixels
      starPos[i] = random(CANVAS)
    }
    starVal[i] *= STAR_DECAY
    // |value| tops out at 0.5, so the *2 makes driftCells the full-strength rate
    starPos[i] += starVal[i] * 2 * driftCells * dt
    if (starPos[i] < 0) starPos[i] += CANVAS
    if (starPos[i] >= CANVAS) starPos[i] -= CANVAS
    twinkle[floor(starPos[i])] += starVal[i]
    // point sample of the head: smooth along the row, one cell tall
    starX[i] = (mod(starPos[i], GRID) + 0.5) / GRID
    starY[i] = (floor(starPos[i] / GRID) + 0.5) / GRID
    starBri[i] = starVal[i] > 0 ? saturate(starVal[i] * starVal[i] * GAIN) : 0
  }
}

// Brightest sky star covering (x, y), 0 when none does. Distances are measured
// in canvas cells so the radius stays well inside 16.16 resolution.
function starLight(x, y) {
  var best = 0
  for (var s = 0; s < numStars; s++) {
    var dx = (x - starX[s]) * GRID
    var dy = (y - starY[s]) * GRID
    var dd = dx * dx + dy * dy
    if (dd < starR2) {
      var b = starBri[s] * (1 - dd / starR2)
      if (b > best) best = b
    }
  }
  return best
}

export function render2D(index, x, y) {
  var xc = x - 0.5                 // center the horizontal axis
  var v = twinkle[floor(y * 15.99) * 16 + floor(x * 15.99)]

  if (abs(xc) < 0.1 && y > 0.1 && y < 0.33) {
    hsv(0.13, 0.7, 1)              // tree topper: warm light gold
  } else if (abs(xc) < 0.7 * y && y < 0.85) {
    // on the tree triangle
    if (index % ornSpacing < 2) {
      hsv(index * 0.023, 1, 1)     // ornaments: fixed rainbow speckle by index
    } else if (v > THRESH) {
      hsv(0.12, 0.85, saturate(v * v * GAIN))  // twinkling tree lights
    } else {
      var g = y - 0.25 * xc        // slanted garland bands
      if ((g > 0.38 && g < 0.43) || (g > 0.58 && g < 0.63)) {
        hsv(0.12, 0.3, 0.5)        // pale champagne gold
      } else {
        hsv(0.33, 1, 0.8)          // foliage: rich green
      }
    }
  } else if (y >= 0.85) {
    hsv(0.07, 0.8, 0.25)           // ground: muted brown-orange
  } else {
    var sb = starLight(x, y)       // point-sized sky star: pale gold
    if (sb > SKY_FLOOR) hsv(0.13, 0.4, sb)
    else hsv(0.62, 0.7, 0.35)      // sky: dusky slate blue
  }
}

export function render3D(index, x, y, z) {
  render2D(index, x, y)            // 3D just projects to the 2D scene
}
