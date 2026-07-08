// name: Emoji Animation #2
// Clean-room reimplementation from a prose functional description of the
// community pattern "Emoji Animation #2"; original source never consulted.
// A cartoon face on a square matrix: two large googly eyes (soft white
// oval rims with a shaded colored iris) blink and glance at random, drawn
// over a procedural pixel-art background (brows, nose, mouth) that flips
// between three expressions a bit faster than once a second.

var GRID = 16

// --- eye geometry (normalized panel units; two eyes at x = 0.25 / 0.75) ---
var maxEyeWidth = 0.36        // full ellipse width
var maxEyeHeight = 0.13       // half-height when fully open
var irisRadius = 0.085
var outlineThick = 0.18
var irisHue = 0.34            // vivid green
var irisSat = 0.9
var asym = 0                  // right eye slightly smaller when 1

// --- animation state ---
var inited = 0
var bgFrame = 0
var bgTimer = 0
var BGFLIP = 900              // ms between expression flips

var blinking = 0
var blinkTimer = 0
var blinkInterval = 1500
var blinkDur = 500
var eyeHeight = 0.13

var moving = 0
var moveTimer = 0
var glanceInterval = 800
var glanceDur = 350
var glanceDir = 1
var maxGlance = 0.05
var irisOffset = 0

var bc = array(3)             // reusable background color scratch

//# min=0 max=1 step=0.01 default=0.34
export function sliderIrisHue(v) { irisHue = v }

//# min=0.2 max=1 step=0.02 default=0.26
export function sliderEyeOpen(v) { maxEyeHeight = v * 0.5 }

function initState() {
  eyeHeight = maxEyeHeight
  blinkInterval = 1000 + random(1000)
  glanceInterval = 300 + random(700)
  inited = 1
}

export function beforeRender(delta) {
  if (!inited) initState()

  // background expression flip
  bgTimer += delta
  if (bgTimer > BGFLIP) { bgTimer = 0; bgFrame = (bgFrame + 1) % 3 }

  // blink: eyelids squeeze shut and reopen on a smooth half-sine
  if (blinking) {
    blinkTimer += delta
    var p = blinkTimer / blinkDur
    if (p >= 1) {
      blinking = 0; blinkTimer = 0; eyeHeight = maxEyeHeight
      blinkInterval = 1000 + random(1000)
    } else {
      eyeHeight = max(0.02, maxEyeHeight * (1 - 0.92 * sin(p * PI)))
    }
  } else {
    eyeHeight = maxEyeHeight
    blinkTimer += delta
    if (blinkTimer > blinkInterval) { blinking = 1; blinkTimer = 0 }
  }

  // glance: iris sweeps out and back on a smooth half-sine
  if (moving) {
    moveTimer += delta
    var g = moveTimer / glanceDur
    if (g >= 1) {
      moving = 0; moveTimer = 0; irisOffset = 0
      glanceInterval = 300 + random(700)
    } else {
      irisOffset = glanceDir * maxGlance * sin(g * PI)
    }
  } else {
    irisOffset = 0
    moveTimer += delta
    if (moveTimer > glanceInterval) {
      moving = 1; moveTimer = 0
      glanceDir = (random(1) < 0.5) ? -1 : 1
    }
  }
}

// Procedural background art. fx,fy are 0..GRID-1 grid cells (fy=0 top).
function bgColor(fx, fy, frame, out) {
  out[0] = 0; out[1] = 0; out[2] = 0
  var red = 0
  if (frame == 0) {
    // horizontal brows, 2x2 nose, magenta W mouth
    if (fy == 2 && ((fx >= 2 && fx <= 6) || (fx >= 9 && fx <= 13))) red = 1
    if ((fx == 7 || fx == 8) && (fy == 8 || fy == 9)) red = 1
    if (fx >= 4 && fx <= 11 && fy == (12 + (fx % 2))) { out[0] = 1; out[2] = 1 }
  } else if (frame == 1) {
    // outward-down brows, nose, yellow smile (flat middle, up at edges)
    if (fx >= 2 && fx <= 5 && fy == (1 + (fx - 2))) red = 1
    if (fx >= 10 && fx <= 13 && fy == (1 + (13 - fx))) red = 1
    if ((fx == 7 || fx == 8) && (fy == 8 || fy == 9)) red = 1
    if (fx >= 4 && fx <= 11) {
      var edge = (fx <= 5 || fx >= 10) ? 1 : 0
      if (fy == (13 - edge)) { out[0] = 1; out[1] = 1 }
    }
  } else {
    // angry chevron brows + red frown arc (edges down, middle up)
    if (fx >= 2 && fx <= 6 && fy == (1 + (6 - fx))) red = 1
    if (fx >= 9 && fx <= 13 && fy == (1 + (fx - 9))) red = 1
    if (fx >= 4 && fx <= 11) {
      var e2 = (fx <= 5 || fx >= 10) ? 1 : 0
      if (fy == (12 + e2)) red = 1
    }
  }
  if (red) { out[0] = 1; out[1] = 0; out[2] = 0 }
}

export function render2D(index, x, y) {
  var eye = (x < 0.5) ? 0 : 1
  var ecx = eye ? 0.75 : 0.25
  var ax = maxEyeWidth * 0.5
  var irisR = irisRadius
  if (eye == 1 && asym) { ax = ax * 0.85; irisR = irisR * 0.85 }

  var px = x - ecx
  var py = y - 0.5
  var ay = eyeHeight

  // 1: iris disc (only when not mid-blink)
  if (!blinking) {
    var dd = hypot(px - irisOffset, py)
    if (dd < irisR) {
      var dn = dd / irisR
      hsv(irisHue, irisSat, 0.25 + 0.75 * dn * dn)
      return
    }
  }

  // 2: eye outline ellipse -- steep power curve => soft white rim
  var exn = px / ax
  var eyn = py / ay
  var ed = sqrt(exn * exn + eyn * eyn)
  if (ed <= 1) {
    var b = pow(min(ed * (1 + outlineThick), 1), 6)
    hsv(0, 0, b)
    return
  }

  // 3: background bitmap texel
  var fx = clamp(floor(x * GRID), 0, GRID - 1)
  var fy = clamp(floor(y * GRID), 0, GRID - 1)
  bgColor(fx, fy, bgFrame, bc)
  rgb(bc[0], bc[1], bc[2])
}
