// name: tree setup pattern
// Clean-room reimplementation from a prose functional description of the
// community pattern "tree setup pattern"; original source never consulted.

// Static diagnostic for a 3D-mapped cone/tree: four vertical colored
// stripes, one per compass quadrant around the vertical axis (red / green /
// teal / purple), separated by dark gaps. Height is ignored — stripes are
// purely angular. Only the slider changes anything.

var SECTORS = 4
var thresh = 0.5   // stripe on/off threshold (higher = narrower stripes)

//# min=0 max=1 step=0.01 default=0.5
export function sliderStripyness(v) { thresh = v }

// One-time coordinate setup: center the map at the origin, then pre-rotate
// an eighth of a turn about the vertical axis so a quadrant *seam* sits at
// the back (stripe centers land at left/front/right/back).
translate3D(-0.5, -0.5, -0.5)
rotateZ(PI / 4)

export function render3D(index, x, y, z) {
  // Azimuth around the vertical axis, normalized to 0..1.
  var a = mod(atan2(y, x) / PI2, 1)

  // Quantize into four equal sectors; the sector picks a fixed hue.
  var sector = floor(a * SECTORS)

  // Triangle wave with one peak per sector, peaking mid-stripe and
  // falling to zero at sector boundaries.
  var stripe = triangle(a * SECTORS)

  // Hard threshold: fully on above the slider, fully off below.
  hsv(sector / SECTORS, 1, stripe > thresh ? 1 : 0)
}

export function render2D(index, x, y) {
  render3D(index, x, y, 0)
}
