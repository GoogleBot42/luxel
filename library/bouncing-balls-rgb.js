// name: bouncing balls - rgb
// Clean-room reimplementation from a prose functional description of the
// community pattern "bouncing balls - rgb"; original source never consulted.
//
// Single-pixel balls fall under gravity onto the near end of the strip and
// bounce, each a fixed rainbow hue, each losing a little more energy per
// bounce than the last so they drift out of phase; a per-frame RGB
// accumulation buffer adds overlapping balls toward white.
//
// Every control carries a //# directive, so the UI sends REAL units (balls,
// strip-heights per second squared, the fraction of speed kept per bounce,
// a mode index) and the handlers use them as-is. The defaults reproduce the
// original render exactly.

var MAXBALLS = 16

var NUM = 8              // live ball count, 1..MAXBALLS
var GRAV = 0.5           // strip-heights per second^2 (full drop ~2 s)
var DROP = 1.0           // launch height in normalized strip units
var V0 = sqrt(2 * GRAV * DROP)   // launch speed that just reaches the top
var BOUNCE = 0.99        // speed kept per bounce, ball 0 (others lose more)
var RELAUNCH = 0.12      // relaunch below this fraction of the launch speed

// MODE: 0 head, 1 tail, 2 both-ends->middle, 3 middle->both-ends
var MODE = 0
var usable = pixelCount

var lastStrike = array(MAXBALLS)
var vel = array(MAXBALLS)
var elast = array(MAXBALLS)

// fixed rainbow colors, one per ball
var ballR = array(MAXBALLS)
var ballG = array(MAXBALLS)
var ballB = array(MAXBALLS)

// accumulation buffers, sized for the whole strip so every mode fits
var rBuf = array(pixelCount)
var gBuf = array(pixelCount)
var bBuf = array(pixelCount)

var clock = 0

// color table: red, orange, yellow, green, cyan, blue, purple, magenta ...
ballR[0] = 1;    ballG[0] = 0;    ballB[0] = 0
ballR[1] = 1;    ballG[1] = 0.5;  ballB[1] = 0
ballR[2] = 1;    ballG[2] = 1;    ballB[2] = 0
ballR[3] = 0;    ballG[3] = 1;    ballB[3] = 0
ballR[4] = 0;    ballG[4] = 1;    ballB[4] = 1
ballR[5] = 0;    ballG[5] = 0;    ballB[5] = 1
ballR[6] = 0.5;  ballG[6] = 0;    ballB[6] = 1
ballR[7] = 1;    ballG[7] = 0;    ballB[7] = 0.5
// ... and the in-between shades, only reached past eight balls
ballR[8] = 1;    ballG[8] = 0.25; ballB[8] = 0
ballR[9] = 0.5;  ballG[9] = 1;    ballB[9] = 0
ballR[10] = 0;   ballG[10] = 1;   ballB[10] = 0.5
ballR[11] = 0;   ballG[11] = 0.5; ballB[11] = 1
ballR[12] = 0.25; ballG[12] = 0;  ballB[12] = 1
ballR[13] = 1;   ballG[13] = 0;   ballB[13] = 1
ballR[14] = 1;   ballG[14] = 0.75; ballB[14] = 0.75
ballR[15] = 0.75; ballG[15] = 1;  ballB[15] = 0.75

var i = 0
for (i = 0; i < MAXBALLS; i = i + 1) {
  elast[i] = BOUNCE - i * 0.006    // slight per-ball spread desynchronizes
  vel[i] = V0
  lastStrike[i] = -i * 0.15        // stagger the initial launches
}

// Whole balls, in balls.
//# min=1 max=16 step=1 default=8
export function sliderBallCount(v) {
  NUM = clamp(floor(v), 1, MAXBALLS)
}

// Gravity in strip-heights per second squared: 0.5 drops a ball the length
// of the strip in about two seconds.
//# min=0.1 max=4 step=0.05 default=0.5
export function sliderGravity(v) {
  GRAV = max(0.05, v)
  V0 = sqrt(2 * GRAV * DROP)
}

// Fraction of the impact speed kept by ball 0 on each bounce; later balls
// keep a little less each, which is what pulls them out of phase.
//# min=0.8 max=0.99 step=0.01 default=0.99
export function sliderBounciness(v) {
  BOUNCE = clamp(v, 0.5, 0.999)
  for (var b = 0; b < MAXBALLS; b = b + 1) {
    elast[b] = max(0.2, BOUNCE - b * 0.006)
  }
}

// Which end the balls fall toward: 0 near end, 1 far end, 2 both ends into
// the middle, 3 middle out to both ends.
//# min=0 max=3 step=1 default=0
export function sliderDirection(v) {
  MODE = clamp(floor(v), 0, 3)
  usable = MODE < 2 ? pixelCount : floor((pixelCount + 1) / 2)
}

// Add one ball's colour at a strip pixel. Out of range is a no-op, which is
// what makes the odd pixel count of a middle-out fold fall out for free.
function emit(p, cr, cg, cb) {
  if (p < 0 || p >= pixelCount) return
  rBuf[p] = rBuf[p] + cr
  gBuf[p] = gBuf[p] + cg
  bBuf[p] = bBuf[p] + cb
}

export function beforeRender(delta) {
  clock = clock + delta / 1000
  // clear the accumulator (a native pass over the whole strip, not a
  // bytecode loop), then drop every live ball into it
  feedback(rBuf, 0); feedback(gBuf, 0); feedback(bBuf, 0)
  var b = 0
  for (b = 0; b < NUM; b = b + 1) {
    var et = clock - lastStrike[b]
    var h = vel[b] * et - 0.5 * GRAV * et * et
    if (h < 0) {
      h = 0
      vel[b] = vel[b] * elast[b]
      lastStrike[b] = clock
      if (vel[b] < V0 * RELAUNCH) vel[b] = V0
    }
    var pos = floor(h * (usable - 1))
    if (pos < 0) pos = 0
    if (pos > usable - 1) pos = usable - 1
    // The direction modes used to be an index remap applied when each pixel
    // read the buffer back. A whole-frame fill is indexed by pixel and cannot
    // remap, so the fold happens here instead: a ball is deposited straight at
    // the strip pixel (or the two mirrored pixels) the old remap made it show
    // up at. Same arithmetic, run NUM times a frame instead of pixelCount.
    var cr = ballR[b], cg = ballG[b], cb = ballB[b]
    if (MODE == 0) {
      emit(pos, cr, cg, cb)                 // head: identity
    } else if (MODE == 1) {
      emit(pixelCount - 1 - pos, cr, cg, cb)  // tail: reversed
    } else if (MODE == 2) {
      emit(pos, cr, cg, cb)                 // both ends into the middle
      var m = pixelCount - 1 - pos
      if (m >= usable) emit(m, cr, cg, cb)  // the centre pixel is not doubled
    } else {
      emit(usable - 1 - pos, cr, cg, cb)    // middle out to both ends
      emit(pos + usable, cr, cg, cb)
    }
  }
}

// With the direction fold moved to the deposit, the accumulator is already
// in strip order and the whole read-out is one `fillRGB`. Index space, so a
// bare strip stays a bare strip and nothing asks for a map.
export function renderFrame() {
  fillRGB(rBuf, gBuf, bBuf)
}
