// name: Dire Spider 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Dire Spider 2D"; original source never consulted.

// A glowing orange spider crawls across a 2D matrix, legs scissoring and
// body swaying, re-entering from a random direction after each pass, while
// swirling toxic-green mist wisps radiate around it.
//
// Two structural tricks (per the description):
// 1. Only two leg segments exist; pixel coords are mirrored into the first
//    quadrant so each drawn leg appears four times -> eight legs.
// 2. All motion lives in the map transform: recenter, rotate to the crawl
//    heading, translate along the crawl axis. The spider itself is always
//    drawn at the origin.

var t = 0                 // wall clock, seconds (wrapped hourly)
var crawlPeriod = 9       // seconds per pass
var lastPhase = 1
var heading = 0           // crawl direction, radians (re-rolled each pass)

// leg segments, defined once in the first quadrant: endpoint angles/lengths
var legA_ang = 0.5, legA_len = 0.44
var legB_ang = 1.05, legB_len = 0.4
// per-frame rotated leg endpoint unit vectors
var laX, laY, lbX, lbY

var legWidth = 0.14
export function sliderLegWidth(v) {
  //# min=0.1 max=0.2 step=0.01 default=0.14
  legWidth = 0.1 + v * 0.1
}

var bgLevel = 0.02
export function sliderBackgroundLevel(v) {
  //# min=0 max=0.35 step=0.01 default=0.02
  bgLevel = v * 0.35
}

var mistMorph, mistScroll
var ANG_DENSITY = 8
setPerlinWrap(ANG_DENSITY, 0, 0)  // hide the seam at the angle wraparound

export function beforeRender(delta) {
  t = t + delta / 1000
  if (t > 3600) t -= 3600

  // Crawl position: sawtooth spanning ~1.5 display-widths off both edges
  var phase = frac(t / crawlPeriod)
  if (phase < lastPhase) heading = random(PI2)  // wrapped: new entry angle
  lastPhase = phase
  var crawl = -1.5 + 3 * phase

  // Leg gait: ~2 scissor cycles per second, ~0.1 rad amplitude
  var gait = 0.1 * sin(t * PI2 * 2)
  var ca = cos(gait), sa = sin(gait)
  var ax = cos(legA_ang), ay = sin(legA_ang)
  var bx = cos(legB_ang), by = sin(legB_ang)
  laX = ax * ca - ay * sa           // leg A rotated +gait
  laY = ax * sa + ay * ca
  var ca2 = cos(-gait * 0.7), sa2 = sin(-gait * 0.7)
  lbX = bx * ca2 - by * sa2         // leg B rotated -0.7*gait
  lbY = bx * sa2 + by * ca2

  // Mist clocks: slow morph (several s), faster radial outward scroll
  mistMorph = time(0.12) * 4        // ~7.9 s per lap, scaled to noise units
  mistScroll = time(0.05) * 3       // ~3.3 s per lap

  // World moves under the spider: recenter, rotate to heading, slide along
  // the crawl axis; a slice of the gait becomes lateral body sway.
  resetTransform()
  translate(-0.5, -0.5)
  rotate(heading)
  translate(-crawl, gait * 0.3)
}

export function render2D(index, x, y) {
  // spider-centric polar-ish quantities
  var r = hypot(x, y)
  var rSkew = r - 0.12 * x          // front legs read longer than back
  var ang = atan2(y, x) / PI2 + 0.5 // 0..1 turn
  var abX = x - 0.09                // abdomen center offset along x
  var abd = hypot(abX, y)

  // mirrored quadrant for the four-fold leg symmetry
  var mx = abs(x), my = abs(y)

  // Mist: ridged noise in cylindrical space, sharpened, edge-attenuated
  var mist = perlinRidge(ang * ANG_DENSITY, r * 3 - mistScroll, mistMorph, 2, 0.5, 1, 3)
  mist = clamp(mist, 0, 1)
  mist = mist * mist
  mist = mist * mist
  mist = mist * clamp(1 - r / 0.72, 0, 1)

  // Legs: brightness from perpendicular distance to each rotated segment
  var leg = 0
  if (rSkew <= legA_len && mx * laX + my * laY > 0) {
    var d = abs(mx * laY - my * laX)
    leg = max(leg, 1 - d / legWidth)
  }
  if (rSkew <= legB_len && mx * lbX + my * lbY > 0) {
    var d2 = abs(mx * lbY - my * lbX)
    leg = max(leg, 1 - d2 / legWidth)
  }
  leg = smoothstep(0.45, 1, leg)    // soft leg edges

  // Abdomen: filled disc that saturates quickly
  var body = max(leg, clamp((0.14 - abd) * 8, 0, 1))

  var mistV = max(mist, bgLevel)
  if (mistV > body) {
    // toxic green, whitening at the hottest cores
    hsv(0.33 + mist * 0.06, 1 - mist * 0.35, mistV)
  } else {
    // deep red-orange at the abdomen, warming to amber along the legs
    hsv(0.015 + abd * 0.13, 1, body)
  }
}
