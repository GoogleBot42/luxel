// name: Crawling Spider 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Crawling Spider 2D"; original source never consulted.

// A red-orange spider crawls straight across the display (~10 s per pass),
// legs scissoring and body swaying, re-entering from a new random direction
// each pass. Only two legs are ever defined: mirroring the pixel into the
// first quadrant (abs of both coordinates) replicates them into eight.
// Requires a mapped 2D display.

var PERIOD = 10      // seconds per crossing (incl. off-screen dead time)
var SWEEP = 3        // travel spans ~three display-widths
var ABD_R = 0.1      // abdomen radius
var LEG_W = 0.03     // leg line half-width
var BASE_HUE = 0.02  // warm red-orange

// two leg definitions: angle from the body x axis + length
// (one shorter/steeper, one longer/shallower); mirroring makes 8 legs
var LEG_A_ANG = 1.1, LEG_A_LEN = 0.3
var LEG_B_ANG = 0.45, LEG_B_LEN = 0.42

var t = 0            // wall-clock accumulator, seconds
var crawlAngle = 0
var lastPos = -10

// per-frame rotated leg unit vectors
var aX = 0, aY = 1, bX = 1, bY = 0

export function beforeRender(delta) {
  t += delta / 1000
  if (t > 3600) t -= 3600  // wrap after a long period

  // linear sweep from well off one edge to well off the other
  var p = mod(t, PERIOD) / PERIOD
  var pos = (p - 0.5) * SWEEP

  // new pass: pick a fresh travel direction over the full circle
  if (pos < lastPos) crawlAngle = random(PI2)
  lastPos = pos

  // fast small leg-swing oscillation, a few swings per second
  var swing = 0.1 * sin(t * PI2 * 3)

  // counter-rotate the two legs slightly asymmetrically: scissoring gait
  var a1 = LEG_A_ANG + swing
  var a2 = LEG_B_ANG - swing * 0.85
  aX = cos(a1)
  aY = sin(a1)
  bX = cos(a2)
  bY = sin(a2)

  // spider-centered frame: center origin, rotate to the travel axis, then
  // slide along it; perpendicular offset tied to the swing = body sway
  resetTransform()
  translate(-0.5, -0.5)
  rotate(crawlAngle)
  translate(-pos, -swing * 0.5)
}

export function render2D(index, x, y) {
  // radial distance skewed by x: front legs reach longer than back ones
  var r = hypot(x, y) - 0.15 * x
  // distance from the abdomen center, slightly ahead of the origin
  var dAbd = hypot(x - 0.06, y)

  // mirror into the first quadrant: two legs become eight
  var ax = abs(x)
  var ay = abs(y)

  var leg = 0
  // leg A: half-line from the origin toward (aX, aY), length LEG_A_LEN
  if (r <= LEG_A_LEN && ax * aX + ay * aY > 0) {
    leg = 1 - abs(ax * aY - ay * aX) / LEG_W
  }
  // leg B
  if (r <= LEG_B_LEN && ax * bX + ay * bY > 0) {
    leg = max(leg, 1 - abs(ax * bY - ay * bX) / LEG_W)
  }
  // clip away the dim fringe so legs render as crisp thin lines
  leg = smoothstep(0.45, 0.9, leg)

  // abdomen: linear falloff inside its radius, maxed with the legs
  var v = max(leg, clamp((ABD_R - dAbd) / ABD_R, 0, 1))

  // hue shades from red-orange at the body toward amber at the leg tips
  var h = dAbd < ABD_R ? BASE_HUE : BASE_HUE + dAbd * 0.5
  hsv(h, 1, v)
}
