// name: TV Simulator
// Curated original (hand-written showcase of the Luxel language/builtins).
// The glow of a television through the curtains, for an empty house. A programme
// keeps a colour grade for about Scene seconds; inside it the camera cuts every
// second or few, each shot a two-tone picture with its own level, and the light
// breathes with on-screen motion (simplex in time); now and then a white flash.
// Strip: brightest at the middle, falling off to the ends. Panel: lit from the
// bottom, the picture's two tones split top and bottom.

var sceneS = 8         // mean length of one programme's colour grade
//# min=2 max=60 step=1 default=8
export function sliderSceneSeconds(v) { sceneS = clamp(v, 2, 60) }

var bias = 0           // -100 all warm interiors .. +100 all cool broadcast blue
//# min=-100 max=100 step=1 default=0
export function sliderWarmCoolBiasPercent(v) { bias = clamp(v, -100, 100) }

var sceneT = 0, shotT = 0, gradeH = 0.6, gradeS = 0.4
var h0 = 0.6, s0 = 0.4, v0 = 0.6, h1 = 0.6, s1 = 0.4, v1 = 0.6
var t = random(100), lvl = 1, flashT = 1

function newGrade() {
  var cool = clamp(0.6 + bias / 100 * 0.6, 0, 1)
  if (random(1) < cool) { gradeH = 0.52 + random(0.16); gradeS = 0.2 + random(0.45) }
  else { gradeH = random(0.13); gradeS = 0.25 + random(0.5) }
  sceneT = sceneS * (0.4 + random(1.2))
}

function newShot() {
  var lv = 0.35 + random(0.65)
  h0 = gradeH + random(0.08) - 0.04; s0 = gradeS * (0.6 + random(0.5)); v0 = lv * (0.75 + random(0.25))
  h1 = gradeH + random(0.12) - 0.06; s1 = gradeS * (0.6 + random(0.6)); v1 = lv
  if (random(1) < 0.2) s1 = s1 * 0.3                       // a bright, washed-out shot
  shotT = 0.6 + random(3.5)
}

newGrade()
newShot()

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  t = mod(t + dt, 3000)
  sceneT -= dt
  shotT -= dt
  if (sceneT <= 0) { newGrade(); newShot() }
  else if (shotT <= 0) newShot()
  lvl = 0.84 + 0.12 * simplex2(t * 1.7, 4.2) + 0.05 * simplex2(t * 9, 1.3)
  flashT += dt
  if (random(1) < dt * 0.04) flashT = 0                      // an explosion on screen, ~1 per 25 s
}

// A mapless strip stays a strip: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

export function renderBulk() {
  if (flashT < 0.12) {
    rgb(1, 1, 0.95)
    fill()
    return
  }
  var a = v0 * lvl, b = v1 * lvl
  if (has2DMap()) {                                          // falls off upward from the set
    fillGradient(h0, s0, a * 0.55, h1, s1, b, 2)
    hsv(h1, s1 * 0.7, b * 0.35)
    splat(0.5, 1.05, 0.65, 1)
  } else {
    var m = pixelCount / 2
    gradientRange(0, m, h0, s0, a * 0.7, h1, s1, b)
    gradientRange(m, pixelCount, h1, s1, b, h0, s0, a * 0.7)
  }
}
