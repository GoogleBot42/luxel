// name: tree setup pattern
// Clean-room reimplementation from a prose functional description of the
// community pattern "tree setup pattern"; original source never consulted.
//
// Static diagnostic for a 3D-mapped cone/tree: four vertical stripes, one
// per compass quadrant (red / green / cyan / purple), separated by dark
// gaps. One slider sets stripe thickness. Height is unused — stripes are
// purely angular.

const SECTORS = 4

var threshold = 0.5

//# min=0 max=1 step=0.01 default=0.5
export function sliderStripyness(v) {
  // low = wide stripes almost touching; high = narrow slivers
  threshold = v
}

function paintQuadrant(x, y) {
  // azimuth around the vertical axis, normalized to 0..1; the eighth-turn
  // offset centers a seam (not a stripe) at the back of the tree
  var az = frac(atan2(y - 0.5, x - 0.5) / PI2 + 0.125 + 1)

  // quantize into four sectors: hues land at red, green, cyan, purple
  var hue = floor(az * SECTORS) / SECTORS

  // triangle wave with four peaks per revolution, peaking mid-sector and
  // reaching zero at sector boundaries
  var tri = triangle(az * SECTORS)

  // hard on/off against the slider — no gradient
  hsv(hue, 1, tri > threshold ? 1 : 0)
}

export function render3D(index, x, y, z) {
  paintQuadrant(x, y)
}

export function render2D(index, x, y) {
  paintQuadrant(x, y)
}

export function render(index) {
  // 1D fallback: unwrap the strip as a full revolution
  var az = index / pixelCount
  var hue = floor(az * SECTORS) / SECTORS
  var tri = triangle(az * SECTORS)
  hsv(hue, 1, tri > threshold ? 1 : 0)
}
