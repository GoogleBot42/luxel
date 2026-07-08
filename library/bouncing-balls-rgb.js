// name: bouncing balls - rgb
// Clean-room reimplementation from a prose functional description of the
// community pattern "bouncing balls - rgb"; original source never consulted.
//
// Eight single-pixel balls fall under gravity onto the near end of the
// strip and bounce, each a fixed rainbow hue, each losing a little more
// energy per bounce than the last so they drift out of phase; a per-frame
// RGB accumulation buffer adds overlapping balls toward white, and the
// render pass clears the buffer as it reads it.

var NUM = 8
var GRAV = 0.5            // height-units per second^2 (full drop ~2 s)
var DROP = 1.0           // launch height in normalized strip units
var V0 = sqrt(2 * GRAV * DROP)   // launch speed that just reaches the top
var RELAUNCH = 0.12      // launch-speed threshold that triggers a reset

// MODE: 0 head, 1 tail, 2 both-ends->middle, 3 middle->both-ends
var MODE = 0
var usable = MODE < 2 ? pixelCount : floor((pixelCount + 1) / 2)

var lastStrike = array(NUM)
var vel = array(NUM)
var elast = array(NUM)

// fixed rainbow colors, one per ball
var ballR = array(NUM)
var ballG = array(NUM)
var ballB = array(NUM)

var rBuf = array(usable)
var gBuf = array(usable)
var bBuf = array(usable)

var clock = 0

// color table: red, orange, yellow, green, cyan, blue, purple, magenta
ballR[0] = 1; ballG[0] = 0;   ballB[0] = 0
ballR[1] = 1; ballG[1] = 0.5; ballB[1] = 0
ballR[2] = 1; ballG[2] = 1;   ballB[2] = 0
ballR[3] = 0; ballG[3] = 1;   ballB[3] = 0
ballR[4] = 0; ballG[4] = 1;   ballB[4] = 1
ballR[5] = 0; ballG[5] = 0;   ballB[5] = 1
ballR[6] = 0.5; ballG[6] = 0; ballB[6] = 1
ballR[7] = 1; ballG[7] = 0;   ballB[7] = 0.5

var i = 0
for (i = 0; i < NUM; i = i + 1) {
  elast[i] = 0.99 - i * 0.006      // slight per-ball spread desynchronizes
  vel[i] = V0
  lastStrike[i] = -i * 0.15        // stagger the initial launches
}

export function beforeRender(delta) {
  clock = clock + delta / 1000
  var b = 0
  for (b = 0; b < NUM; b = b + 1) {
    var et = clock - lastStrike[b]
    var h = vel[b] * et - 0.5 * GRAV * et * et
    if (h < 0) {
      h = 0
      vel[b] = vel[b] * elast[b]
      lastStrike[b] = clock
      if (vel[b] < RELAUNCH) vel[b] = V0
    }
    var pos = floor(h * (usable - 1))
    if (pos < 0) pos = 0
    if (pos > usable - 1) pos = usable - 1
    rBuf[pos] = rBuf[pos] + ballR[b]
    gBuf[pos] = gBuf[pos] + ballG[b]
    bBuf[pos] = bBuf[pos] + ballB[b]
  }
}

export function render(index) {
  var src = index
  var last = 1
  if (MODE == 1) {
    src = pixelCount - 1 - index
  } else if (MODE == 2) {
    if (index < usable) { src = index; last = 0 }
    else { src = pixelCount - 1 - index }
  } else if (MODE == 3) {
    if (index < usable) { src = usable - 1 - index; last = 0 }
    else { src = index - usable }
  }
  if (src < 0) src = 0
  if (src > usable - 1) src = usable - 1
  rgb(rBuf[src], gBuf[src], bBuf[src])
  if (last) { rBuf[src] = 0; gBuf[src] = 0; bBuf[src] = 0 }
}
