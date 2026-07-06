// name: Dire Spider 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Dire Spider 2D"; original source never consulted.

// A glowing orange spider crawls across a 2D matrix (~10 s per pass),
// re-entering from a random direction after each pass, legs scissoring and
// body swaying, while ridged-noise green "poison mist" swirls around it.
//
// The two tricks that carry the pattern:
//  1. Symmetry — only two leg segments are defined; pixel coordinates are
//     mirrored into the first quadrant (abs of both axes) before testing,
//     so each drawn leg appears four times: eight legs from two.
//  2. All motion lives in the coordinate transform — recenter, rotate by
//     the crawl heading, translate along the crawl axis. The spider is
//     always drawn at the origin; the world moves under it.

var legHalfWidth = 0.14        // set by slider
var bgLevel = 0.02             // set by slider

// Two stored leg segments (unit direction + length), defined in the first
// quadrant. Gait rotation is applied to these each frame.
var LEG1_ANG = 0.45            // radians from +x axis
var LEG2_ANG = 1.05
var LEG1_LEN = 0.48
var LEG2_LEN = 0.52
var l1x, l1y, l2x, l2y         // rotated unit directions, per frame

var ABD_X = 0.10               // abdomen center offset along +x
var ABD_R = 0.11               // abdomen radius

var MIST_ANG_DENSITY = 6       // angular noise density (wrap matches it)
setPerlinWrap(MIST_ANG_DENSITY, 0, 0)

var tSec = 0                   // wall clock, seconds, wrapped hourly
var crawl, lastCrawl = 9
var heading = 0                // crawl direction, re-rolled per pass
var mistScroll, mistMorph

export function sliderLegWidth(v) {
  //# min=0 max=1 step=0.05 default=0.4
  legHalfWidth = 0.1 + v * 0.1          // ~a tenth to a fifth of display
}

export function sliderBackgroundLevel(v) {
  //# min=0 max=1 step=0.05 default=0.05
  bgLevel = v * 0.4                      // black up to a dim green haze
}

export function beforeRender(delta) {
  tSec += delta / 1000
  if (tSec > 3600) tSec -= 3600

  // Crawl: sawtooth, ~10 s per pass, spanning ±1.5 display-widths so the
  // spider is fully off-screen at both ends.
  crawl = (frac(tSec / 10) - 0.5) * 3
  if (crawl < lastCrawl) heading = random(PI2)   // wrapped: new entry angle
  lastCrawl = crawl

  // Leg gait: small oscillation, a couple of cycles per second. One leg
  // rotates positively, the other negatively and slightly less.
  var gait = 0.12 * sin(tSec * PI2 * 2)
  var a1 = LEG1_ANG + gait
  var a2 = LEG2_ANG - gait * 0.8
  l1x = cos(a1); l1y = sin(a1)
  l2x = cos(a2); l2y = sin(a2)

  // Mist clocks: slow morph (several seconds), faster radial scroll.
  mistMorph = time(0.12) * 4             // shape morph, ~8 s lap
  mistScroll = time(0.05) * 3            // radial drift, ~3 s lap

  // Motion via transform: recenter, rotate to heading, slide along the
  // crawl axis with a touch of the gait as lateral body sway.
  resetTransform()
  translate(-0.5, -0.5)
  rotate(heading)
  translate(crawl, gait * 0.3)
}

export function render2D(index, x, y) {
  // --- polar-ish quantities (coordinates are spider-centric) ---
  var r = hypot(x, y)
  var rSkew = r - 0.18 * x               // front legs read longer than back
  var mx = abs(x), my = abs(y)           // quadrant mirror: 2 legs -> 8

  // --- mist: ridged noise in cylindrical space, seamless in angle ---
  var ang = atan2(y, x) / PI2 + 0.5
  var m = perlinRidge(ang * MIST_ANG_DENSITY, r * 3 - mistScroll,
                      mistMorph, 2, 0.5, 1, 3)
  m = m * m; m = m * m                   // ^4: sharpen into wisps
  m = m * clamp(1 - r / 0.75, 0, 1)      // hug the spider, die at the edge

  // --- legs: distance-to-segment test on the two rotated definitions ---
  var leg = 0
  var along = mx * l1x + my * l1y
  if (along > 0 && rSkew < LEG1_LEN) {
    var perp = abs(mx * l1y - my * l1x)
    leg = clamp(1 - perp / legHalfWidth, 0, 1)
  }
  along = mx * l2x + my * l2y
  if (along > 0 && rSkew < LEG2_LEN) {
    var perp2 = abs(mx * l2y - my * l2x)
    var b2 = clamp(1 - perp2 / legHalfWidth, 0, 1)
    if (b2 > leg) leg = b2
  }
  leg = smoothstep(0.45, 1, leg)         // soft edges

  // --- abdomen: filled disc, saturates quickly to full ---
  var dAbd = hypot(x - ABD_X, y)
  var abd = clamp((ABD_R - dAbd) * 6, 0, 1)

  var body = max(leg, abd)

  if (m >= body) {
    // Toxic mist: green nudged by intensity, hot cores go whitish-green.
    // The background floor keeps this a dim green haze when raised.
    hsv(0.33 + m * 0.05, 1 - m * 0.35, max(m, bgLevel))
  } else {
    // Spider: deep red-orange at the abdomen, warming toward amber along
    // the legs with distance from the abdomen.
    hsv(0.015 + dAbd * 0.07, 1, body)
  }
}
