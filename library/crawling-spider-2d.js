// name: Crawling Spider 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Crawling Spider 2D"; original source never consulted.

// A red-orange spider crawls straight across the display (~9 s per pass,
// with off-screen dead time), entering from a new random direction each
// pass. Two leg segments are mirrored through both axes into eight legs;
// counter-rotating them by a fast sine gives the scissoring gait, and the
// body sways perpendicular to travel in sympathy. Coordinates are brought
// into the spider's frame manually (inverse rotate + slide) per pixel.

const PERIOD = 9        // seconds per pass
const SWEEP = 3         // travel spans ~three display-widths
const LA_ANG = 1.15     // leg A: steeper, shorter
const LA_LEN = 0.16
const LB_ANG = 0.45     // leg B: shallower, longer
const LB_LEN = 0.22
const LEG_W = 0.025     // leg line half-width
const ABR = 0.09        // abdomen radius
const HUE0 = 0.02       // warm red-orange base hue

var crawlT = random(PERIOD)
var angle = random(PI2)
var cosA = cos(angle)
var sinA = sin(angle)
var crawlPos = 0
var swingPh = random(1)
var sway = 0
var laX = 0
var laY = 0
var lbX = 0
var lbY = 0

export function beforeRender(delta) {
  crawlT += delta / 1000
  if (crawlT >= PERIOD) {
    crawlT = mod(crawlT, PERIOD)
    angle = random(PI2)        // new pass: fresh crawl direction
    cosA = cos(angle)
    sinA = sin(angle)
  }
  crawlPos = (crawlT / PERIOD - 0.5) * SWEEP

  swingPh = frac(swingPh + delta * 0.003)      // ~3 scissor cycles / s
  var legSwing = 0.1 * sin(swingPh * PI2)      // ~±0.1 rad
  sway = legSwing * 0.3                        // body sway rides the gait

  // counter-rotate the two leg definitions for the scissoring walk
  var a = LA_ANG + legSwing
  laX = cos(a)
  laY = sin(a)
  a = LB_ANG - legSwing * 0.85
  lbX = cos(a)
  lbY = sin(a)
}

export function render2D(index, x, y) {
  // into the spider's frame: center, rotate by -angle, slide along travel
  var px = x - 0.5
  var py = y - 0.5
  var rx = px * cosA + py * sinA - crawlPos
  var ry = py * cosA - px * sinA - sway

  var r = hypot(rx, ry) - 0.18 * rx   // skew: front legs reach farther
  var da = hypot(rx - 0.03, ry)       // distance from abdomen center

  // quadrant mirror: two leg definitions become eight legs
  var mx = abs(rx)
  var my = abs(ry)

  var b = 0
  if (r < LA_LEN && mx * laX + my * laY > 0) {
    b = 1 - abs(mx * laY - my * laX) / LEG_W
  }
  if (r < LB_LEN && mx * lbX + my * lbY > 0) {
    b = max(b, 1 - abs(mx * lbY - my * lbX) / LEG_W)
  }
  b = smoothstep(0.4, 0.85, b)        // clip the dim fringe: crisp legs

  b = max(b, (ABR - da) / ABR)        // abdomen: linear falloff disc
  if (b <= 0) {
    rgb(0, 0, 0)
  } else {
    // red-orange body shading toward amber at the leg tips
    var h = da < ABR ? HUE0 : HUE0 + da * 0.5
    hsv(h, 1, b)
  }
}
