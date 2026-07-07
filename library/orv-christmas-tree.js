// name: Orv - Christmas Tree
// Clean-room reimplementation from a prose functional description of the
// community pattern "Orv - Christmas Tree"; original source never consulted.

// Static Christmas-tree scene (tree, topper, garland, ornaments, ground, sky)
// with warm twinkle lights on the tree and stars in the sky, driven by a small
// pool of drifting, decaying particles writing into a 16x16 virtual canvas.

var GRID = 16
var CANVAS = GRID * GRID          // 16x16 virtual canvas, row-major
var twinkle = array(CANVAS)       // per-canvas-cell twinkle intensity

var NUM_STARS = floor(CANVAS / 20)
var starVal = array(NUM_STARS)    // signed: brightness contribution + drift rate
var starPos = array(NUM_STARS)    // fractional canvas index

var BUF_DECAY = 0.97              // per-frame buffer fade (~1 s twinkle tail)
var STAR_DECAY = 0.985            // per-frame particle decay
var RESPAWN_BAND = 0.05           // |value| below this respawns the particle
var THRESH = 0.1                  // buffer value needed to light a twinkle/star
var GAIN = 10                     // post-square brightness boost

export function beforeRender(delta) {
  feedback(twinkle, BUF_DECAY)
  for (var i = 0; i < NUM_STARS; i++) {
    if (abs(starVal[i]) < RESPAWN_BAND) {
      starVal[i] = random(1) - 0.5          // signed: negatives dim pixels
      starPos[i] = random(CANVAS)
    }
    starVal[i] *= STAR_DECAY
    starPos[i] += starVal[i] * delta * 0.004  // very slow drift
    if (starPos[i] < 0) starPos[i] += CANVAS
    if (starPos[i] >= CANVAS) starPos[i] -= CANVAS
    twinkle[floor(starPos[i])] += starVal[i]
  }
}

export function render2D(index, x, y) {
  var xc = x - 0.5                 // center the horizontal axis
  var v = twinkle[floor(y * 15.99) * 16 + floor(x * 15.99)]

  if (abs(xc) < 0.1 && y > 0.1 && y < 0.33) {
    hsv(0.13, 0.7, 1)              // tree topper: warm light gold
  } else if (abs(xc) < 0.7 * y && y < 0.85) {
    // on the tree triangle
    if (index % 11 < 2) {
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
  } else if (v > THRESH) {
    hsv(0.13, 0.4, saturate(v * v * GAIN))  // sky star: pale gold
  } else {
    hsv(0.62, 0.7, 0.35)           // sky: dusky slate blue
  }
}

export function render3D(index, x, y, z) {
  render2D(index, x, y)            // 3D just projects to the 2D scene
}
