// name: M5Stack Hex panels
// Clean-room reimplementation from a prose functional description of the
// community pattern "M5Stack Hex panels"; original source never consulted.
// A six-mode 3D demo for hex tiles (37 px/tile, z selects the tile). One
// slider picks the mode; shared slow clocks and a gamma-shaped proximity
// helper drive nearly every mode.

var TILE = 37           // pixels per hex tile
var MAXR = 0.7071       // max normalized radius from center (fix: derive from map)

var mode = 0
var fast = 0, stretch = 0, slow = 0

// Soft, gamma-shaped bump: full when a==b, linear falloff to 0 at half-width.
function proximity(a, b, hw) {
  if (hw == 0) hw = 0.125
  var v = 1 - abs(a - b) / hw
  if (v < 0) v = 0
  return v * v
}

//# min=0 max=1 step=0.01 default=0
export function sliderMode(v) {
  // Six equal bins; slight compression keeps max position in the last bin.
  mode = floor(v * 5.999)
}

export function beforeRender(delta) {
  fast = time(0.045)                 // ~3 s
  stretch = fast * 1.2 - 0.1         // overshoots below 0 and above 1
  slow = time(0.1)                   // ~6.5 s, hue drift

  resetTransform()
  translate(-0.5, -0.5)              // origin to center of mapped area
  if (mode == 1) rotate(time(0.06) * PI2)   // rotating-line mode only
}

export function render3D(index, x, y, z) {
  var ang = atan2(y, x) / PI2 + 0.5  // 0..1 around the circle
  var rad = hypot(x, y) / MAXR       // 0..1 radius

  var h = 0, s = 1, v = 0

  if (mode == 0) {
    // Radiating rainbow rings.
    h = rad * 0.5 + slow * 0.5
    v = proximity(rad, stretch * 1.1, 0.25)
  } else if (mode == 1) {
    // Rotating rainbow bar (frame already spinning).
    h = rad * 0.3 + slow
    var diag = ((x + y) * 0.5) + 0.5
    v = proximity(diag, stretch, 0.15)
  } else if (mode == 2) {
    // Radar wedge on a ring.
    var ring = proximity(rad, stretch, 0.25)
    var target = frac(stretch * 4)
    var wedge = proximity(ang, target, 0.1)
    v = ring * wedge
    h = slow
  } else if (mode == 3) {
    // Warm panel-by-panel wipe: offset x by z to lay tiles on a line.
    var line = (x + z * 4)
    var span = frac((line + 4) * 0.125)   // fold onto a virtual line
    v = proximity(span, fast, 0.2)
    v = v * v * v                          // punchy hot core
    h = 0.06                               // warm amber
    s = 1
  } else if (mode == 4) {
    // Blue iris with wandering dark pupil.
    var px = cos(fast * PI2) * 0.25
    var py = sin(fast * PI2) * 0.25
    var hole = proximity(x, px, 0.4) * proximity(y, py, 0.4)
    v = 1 - hole
    h = 0.6
    s = 0.7
  } else {
    // Hardcoded status / strobe demo (three 4-px clusters per tile).
    var m = index % TILE
    h = 0.95            // bright crimson/pink field
    s = 1
    v = 1
    if (m >= 0 && m < 4) {              // cluster A: dim red, 50% duty
      h = 0; s = 1
      v = square(fast, 0.5) ? 0.4 : 0
    } else if (m >= 8 && m < 12) {     // cluster B: dim blue, ~80% duty
      h = 0.6; s = 1
      v = square(fast + 0.3, 0.8) ? 0.4 : 0
    } else if (m >= 16 && m < 20) {    // cluster C: dark
      v = 0
    }
  }

  hsv(h, s, v)
}
