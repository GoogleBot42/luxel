// name: Snake 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Snake 2D"; original source never consulted.

// A glowing white-hot head roams a matrix leaving a smoothly cooling heat
// trail whose hue slowly circles the wheel. It slithers in S-curves, curves
// away from the borders (harder in corners), occasionally flips its curl on
// a whim, hurries near walls and loafs near centre, and bounces like a
// billiard ball as a hard backstop. Trail length looks constant regardless
// of speed: heat decay is scaled by the speed ratio. Simulated on a 16x16
// virtual heat canvas; render2D samples it from normalized map coordinates.

const W = 16
const H = 16
var heat = array(W * H)

var hx = 8, hy = 8          // continuous head position, pixel units
var bearing = 0
var turnSign = 1
var curl = 0                // accumulated turn since last flip
var globalHue = 0
var started = 0

const BASE_TURN = 0.09      // gentle slither turn, rad per nominal frame
const EDGE_TURN = 0.22      // stronger corrective turn near a wall
const ZONE = 3              // border zone thickness
const RADIUS = 2.5          // head heat radius
const NOMINAL = 0.16        // nominal speed, px per nominal frame

var speed = 0.5             // slider

export function sliderSpeed(v) {
  //# min=0 max=1 step=0.01 default=0.5
  speed = v
}

// signed angle difference wrapped into [-PI, PI]
function angDiff(a, b) {
  var d = a - b
  while (d > PI) d -= PI2
  while (d < -PI) d += PI2
  return d
}

export function beforeRender(delta) {
  if (!started) { bearing = random(PI2); started = 1 }
  var step = delta / 16.667

  globalHue = time(0.1)                 // ~6.5 s full wheel
  var wanderClock = time(0.5)           // ~33 s speed mood swing (unused feel)

  // occasional mood flip (~ once every few seconds)
  if (random(1) < delta / 3000) turnSign = -turnSign

  // --- edge avoidance ---
  var inZone = (hx < ZONE) || (hx > W - ZONE) || (hy < ZONE) || (hy > H - ZONE)
  var turn
  if (inZone) {
    // steer toward panel centre -> curves away from the wall/corner
    var toCenter = atan2((H / 2) - hy, (W / 2) - hx)
    var diff = angDiff(toCenter, bearing)
    if (abs(diff) < 0.25) turn = 0             // already heading in: go straight
    else turn = sign(diff) * EDGE_TURN
    curl = 0
  } else {
    turn = turnSign * BASE_TURN
    // slither: flip curl direction after more than half a turn
    curl += abs(turn) * step
    if (curl > PI) { turnSign = -turnSign; curl = 0 }
  }

  // --- speed: wander (product of incommensurate triangle waves) * centre factor ---
  var wander = (triangle(time(0.31)) + 0.35)
             * (triangle(time(0.47)) + 0.35)
             * (triangle(time(0.71)) + 0.35)
  var distC = dist(hx, hy, W / 2, H / 2)
  var centerFactor = 0.5 + distC / 11          // ~0.5 centre .. ~1.5 corner
  var spd = (NOMINAL * (0.3 + speed * 1.4)) * wander * centerFactor

  // --- turn: scaled by speed and delta so curvature per distance is stable ---
  bearing += turn * step * (spd / NOMINAL)
  while (bearing > PI2) bearing -= PI2
  while (bearing < 0) bearing += PI2

  // --- move ---
  hx += cos(bearing) * spd * step
  hy += sin(bearing) * spd * step

  // --- billiard bounce backstop ---
  if (hx < 0.5) { hx = 0.5; bearing = PI - bearing }
  if (hx > W - 0.5) { hx = W - 0.5; bearing = PI - bearing }
  if (hy < 0.5) { hy = 0.5; bearing = -bearing }
  if (hy > H - 0.5) { hy = H - 0.5; bearing = -bearing }

  // --- heat update: speed-compensated decay + head deposit ---
  var ratio = spd / NOMINAL
  var dcy = pow(0.86, ratio * step)            // faster travel -> faster decay
  var lox = max(0, floor(hx - RADIUS)), hix = min(W - 1, ceil(hx + RADIUS))
  var loy = max(0, floor(hy - RADIUS)), hiy = min(H - 1, ceil(hy + RADIUS))
  for (var cy = 0; cy < H; cy++) {
    for (var cx = 0; cx < W; cx++) {
      var i = cy * W + cx
      var v = heat[i] * dcy
      if (cx >= lox && cx <= hix && cy >= loy && cy <= hiy) {
        var d = dist(cx + 0.5, cy + 0.5, hx, hy)
        if (d < RADIUS) {
          var closeness = 1 - d / RADIUS
          var c2 = closeness * closeness
          var add = c2 * c2 * c2 * (0.6 + ratio)   // tight hot core
          v = min(v + add, 1)
        }
      }
      heat[i] = v
    }
  }
}

export function render2D(index, x, y) {
  var h = heat[floor(y * 15.99) * W + floor(x * 15.99)]
  var h2 = h * h
  var h4 = h2 * h2
  var bright = h2
  var satr = 1 - h4 * h4 * h        // only near-max heat whitens
  hsv(globalHue + h * 0.08, satr, bright)
}
