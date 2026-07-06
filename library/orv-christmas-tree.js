// name: Orv - Christmas Tree
// Clean-room reimplementation from a prose functional description of the
// community pattern "Orv - Christmas Tree"; original source never consulted.

// Static Christmas-tree scene (tree, topper, ground, sky) with warm twinkle
// lights driven by a particle field simulated on a 16x16 virtual canvas.

var W = 16
var CELLS = W * W
var canvas = array(CELLS)          // twinkle intensity per virtual-canvas cell

var numStars = floor(CELLS / 20)   // particle pool ~ 1/20 of cell count
var starVal = array(numStars)      // signed brightness contribution / drift rate
var starPos = array(numStars)      // fractional cell position

export function beforeRender(delta) {
  // fade the whole twinkle field a little each frame
  feedback(canvas, 0.96)

  for (var i = 0; i < numStars; i++) {
    // respawn particles whose value has decayed into a band around zero
    if (abs(starVal[i]) < 0.03) {
      starVal[i] = random(0.7) - 0.35   // symmetric range: negatives dim pixels
      starPos[i] = random(CELLS)
    }
    starVal[i] *= 0.985                 // slow exponential decay toward zero
    // very slow drift proportional to value; wrap at the ends
    starPos[i] += starVal[i] * delta * 0.002
    if (starPos[i] < 0) starPos[i] += CELLS
    if (starPos[i] >= CELLS) starPos[i] -= CELLS
    canvas[floor(starPos[i])] += starVal[i]
  }
}

export function render2D(index, x, y) {
  var cx = x - 0.5   // center the horizontal axis: -0.5 .. +0.5
  var tw = canvas[floor(y * 15.99) * 16 + floor(x * 15.99)]
  var boost = clamp(tw * tw * 8, 0, 1)  // squared + big gain = snappy sparkle

  if (abs(cx) < 0.1 && y > 0.1 && y < 0.33) {
    // tree topper: warm light gold, bright
    hsv(0.13, 0.7, 1)
  } else if (abs(cx) < 0.7 * y && y < 0.85) {
    // inside the tree triangle
    if (index % 10 < 2) {
      // ornaments: short fixed runs of raw indices, slow rainbow by index
      hsv(index / 77, 1, 1)
    } else if (tw > 0.1) {
      // tree lights: warm candle gold
      hsv(0.11, 0.85, boost)
    } else {
      var g = y - cx * 0.3   // slanted garland coordinate
      if ((g > 0.38 && g < 0.43) || (g > 0.6 && g < 0.65)) {
        // garland: pale champagne gold, subdued
        hsv(0.12, 0.3, 0.5)
      } else {
        // foliage: rich green
        hsv(0.34, 1, 0.85)
      }
    }
  } else if (y >= 0.85) {
    // ground: muted brown-orange
    hsv(0.07, 0.8, 0.22)
  } else if (tw > 0.1) {
    // sky star: paler, desaturated gold
    hsv(0.12, 0.4, boost)
  } else {
    // night sky: dusky slate blue
    hsv(0.61, 0.65, 0.32)
  }
}

export function render3D(index, x, y, z) {
  render2D(index, x, y)   // 3D just projects to the 2D drawing
}
