// name: Bouncing Balls RGB 2D
// Curated original for the Luxel library: the panel reading of "bouncing balls
// - rgb". The strip version drops single-pixel rainbow balls onto one end and
// lets an accumulation buffer add the overlaps toward white; here the same
// balls get a second axis. Each falls under gravity, bounces off the floor
// keeping a little less speed than the ball before it (so they never stay in
// step), drifts sideways off the walls, and is drawn as a soft glow whose
// overlaps add — two arcs crossing flash a secondary, three flash white. Balls
// whose bounce has died out relaunch at full height, so the panel never
// settles.
//
// Every control carries a //# directive, so the UI sends REAL units (balls,
// panel-heights per second squared, the fraction of speed kept per bounce,
// panel widths, panel widths per second) and the handlers use them as-is.

var MAXBALLS = 8

var NUM = 6            // live balls
var GRAV = 2.0         // panel-heights per second^2
var BOUNCE = 0.95      // speed kept per bounce, ball 0 (others keep less)
var RADIUS = 0.18      // glow radius in panel widths
var DRIFT = 0.35       // sideways speed in panel widths per second
var RELAUNCH = 0.12    // relaunch below this fraction of the launch speed
var V0 = sqrt(2 * GRAV)   // launch speed that just reaches the ceiling

var bx = array(MAXBALLS)
var by = array(MAXBALLS)
var bvy = array(MAXBALLS)
var bdir = array(MAXBALLS)   // sideways direction/factor, -1.4 .. 1.4
var elast = array(MAXBALLS)

// the strip version's fixed rainbow table, one color per ball
var ballR = array(MAXBALLS)
var ballG = array(MAXBALLS)
var ballB = array(MAXBALLS)
ballR[0] = 1;    ballG[0] = 0;    ballB[0] = 0
ballR[1] = 1;    ballG[1] = 0.5;  ballB[1] = 0
ballR[2] = 1;    ballG[2] = 1;    ballB[2] = 0
ballR[3] = 0;    ballG[3] = 1;    ballB[3] = 0
ballR[4] = 0;    ballG[4] = 1;    ballB[4] = 1
ballR[5] = 0;    ballG[5] = 0;    ballB[5] = 1
ballR[6] = 0.5;  ballG[6] = 0;    ballB[6] = 1
ballR[7] = 1;    ballG[7] = 0;    ballB[7] = 0.5

var i = 0
for (i = 0; i < MAXBALLS; i = i + 1) {
  bx[i] = (i + 0.5) / MAXBALLS
  by[i] = 0.15 + random(0.8)          // staggered starting heights
  bvy[i] = V0 * (0.3 + random(0.7))
  bdir[i] = random(2.8) - 1.4
  elast[i] = BOUNCE - i * 0.01        // per-ball spread desynchronizes
}

// Whole balls, in balls.
//# min=1 max=8 step=1 default=6
export function sliderBallCount(v) {
  NUM = clamp(floor(v), 1, MAXBALLS)
}

// Gravity in panel-heights per second squared: 2 gives a ~2 s bounce cycle.
//# min=0.2 max=8 step=0.1 default=2
export function sliderGravity(v) {
  GRAV = max(0.1, v)
  V0 = sqrt(2 * GRAV)
}

// Fraction of the impact speed kept by ball 0 on each bounce; later balls
// keep a little less each, which is what pulls them out of phase.
//# min=0.8 max=0.99 step=0.01 default=0.95
export function sliderBounciness(v) {
  BOUNCE = clamp(v, 0.5, 0.999)
  for (var b = 0; b < MAXBALLS; b = b + 1) {
    elast[b] = max(0.2, BOUNCE - b * 0.01)
  }
}

// Glow radius as a fraction of the panel width.
//# min=0.04 max=0.4 step=0.01 default=0.18
export function sliderBallSize(v) {
  RADIUS = max(0.02, v)
}

// Sideways travel in panel widths per second (0 drops them straight down).
//# min=0 max=1.5 step=0.05 default=0.35
export function sliderDrift(v) {
  DRIFT = max(0, v)
}

export function beforeRender(delta) {
  var dt = delta / 1000
  var b = 0
  for (b = 0; b < NUM; b = b + 1) {
    // vertical: constant acceleration, inelastic floor
    bvy[b] = bvy[b] - GRAV * dt
    by[b] = by[b] + bvy[b] * dt
    if (by[b] < 0) {
      by[b] = 0
      bvy[b] = -bvy[b] * elast[b]
      if (bvy[b] < V0 * RELAUNCH) bvy[b] = V0
    }
    if (by[b] > 1) {          // clipped by the ceiling on a gravity change
      by[b] = 1
      if (bvy[b] > 0) bvy[b] = 0
    }
    // sideways: reflective walls
    bx[b] = bx[b] + bdir[b] * DRIFT * dt
    if (bx[b] < 0) { bx[b] = -bx[b]; bdir[b] = -bdir[b] }
    if (bx[b] > 1) { bx[b] = 2 - bx[b]; bdir[b] = -bdir[b] }
  }
}

export function render2D(index, x, y) {
  var r = 0
  var g = 0
  var bl = 0
  for (var b = 0; b < NUM; b = b + 1) {
    var dx = abs(x - bx[b])
    if (dx > RADIUS) continue          // cheap box rejection before the hypot
    var dy = abs(y - by[b])
    if (dy > RADIUS) continue
    var d = hypot(dx, dy)
    if (d >= RADIUS) continue
    var q = d / RADIUS
    var s = 1 - q * q * q              // full core, fast drop at the rim
    r = r + ballR[b] * s
    g = g + ballG[b] * s
    bl = bl + ballB[b] * s
  }
  rgb(r, g, bl)   // overlaps sum; the engine clamps anything over full
}

// 1D fallback: the strip is the horizontal line through mid-height, so the
// balls flash past it on the way up and the way down.
export function render(index) {
  render2D(index, index / pixelCount, 0.5)
}
