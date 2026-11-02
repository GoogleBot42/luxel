// name: Curl Flow
// tags: panel, ambient
// Curated original (hand-written showcase of the Luxel language/builtins).
// Specks ride a drifting flow field at the panel's own resolution and leave
// fading trails on the persistent frame: fade() plus one anti-aliased
// drawLine per speck, no canvas. Field: curl noise (divergence-free, so the
// specks circulate forever) or a plain noise heading (they braid into
// streams). Colour by speed or by heading, in OKLCH. One curl2 or simplex3
// per speck per frame, so a 128x128 chain costs what a 64x64 panel does.

var MAXN = 300
var px = array(MAXN)
var py = array(MAXN)
var age = array(MAXN)
var life = array(MAXN)
var vel = array(2)
var dt = 0
var drift = 0
var zt = 0
var hueBase = 0
var lineW = 0.02

var count = 150
//# min=20 max=300 step=10 default=150
export function sliderSpecks(v) { count = clamp(floor(v), 1, MAXN) }

// display widths per second for a speck of average speed
var speed = 0.25
//# min=0.05 max=1.5 step=0.05 default=0.25
export function sliderSpeed(v) { speed = clamp(v, 0.01, 3) }

// trail half-life, seconds
var trail = 0.6
//# min=0.1 max=3 step=0.05 default=0.6
export function sliderTrail(v) { trail = clamp(v, 0.05, 5) }

// vortices across the display (the field's spatial period)
var swirls = 2.5
//# min=1 max=6 step=0.5 default=2.5
export function sliderSwirls(v) { swirls = clamp(v, 0.5, 12) }

// 0 curl noise (circulates), 1 noise heading (braided streams)
var field = 0
//# min=0 max=1 step=1 default=0
export function sliderField(v) { field = clamp(floor(v), 0, 1) }

// 0 by speed (violet slow, cyan-white fast), 1 by heading (a hue wheel)
var colour = 0
//# min=0 max=1 step=1 default=0
export function sliderColour(v) { colour = clamp(floor(v), 0, 1) }

// light bleed around the trails, percent
var glow = 0
//# min=0 max=100 step=5 default=0
export function sliderGlowPercent(v) { glow = clamp(v, 0, 100) }

function spawn(i) {
  px[i] = random(1)
  py[i] = random(1)
  life[i] = 2 + random(4)
  age[i] = 0
}

for (var k = 0; k < MAXN; k++) {
  spawn(k)
  age[k] = random(life[k])   // staggered, so respawns never arrive in a wave
}

export function beforeRender(delta) {
  dt = min(delta, 100) * 0.001
  drift += dt * 0.05            // the potential slides: the whole field flows past
  if (drift > 512) drift -= 512
  zt += dt * 0.04               // the heading field morphs instead
  if (zt > 512) zt -= 512
  hueBase = time(1.4)           // ~90 s palette drift
  var gw = gridWidth()
  lineW = gw > 1 ? 1 / (gw - 1) : 0.02
  setGlow(glow * 0.006)
}

export function renderBulk() {
  fade(pow(0.5, dt / trail))
  for (var i = 0; i < count; i++) {
    var x = px[i]
    var y = py[i]
    var vx = 0
    var vy = 0
    var m = 0
    if (field == 0) {
      // curl2's mean vector length is ~2.7 (docs/lang.md): normalise to ~1
      curl2(x * swirls + drift, y * swirls - drift * 0.6, vel, 9)
      vx = vel[0] * 0.37
      vy = vel[1] * 0.37
      m = hypot(vx, vy)
    } else {
      var n = simplex3(x * swirls * 0.5, y * swirls * 0.5, zt, 3)
      var a = n * PI2
      m = 0.5 + abs(n) * 1.2
      vx = cos(a) * m
      vy = sin(a) * m
    }
    var nx = mod(x + vx * speed * dt, 1)
    var ny = mod(y + vy * speed * dt, 1)
    px[i] = nx
    py[i] = ny
    var ag = age[i] + dt
    age[i] = ag
    var b = min(1, min(ag * 3, (life[i] - ag) * 3))  // fade in and out
    if (b <= 0) { spawn(i); continue }
    if (colour == 0) {
      var s = saturate((m - 0.3) * 0.6)
      oklch((0.42 + 0.5 * s) * b, 0.19 - 0.1 * s, hueBase + 0.78 - 0.28 * s)
    } else {
      oklch(0.74 * b, 0.15, atan2(vy, vx) / PI2 + hueBase)
    }
    // a wrap teleports the speck: only its new spot is drawn
    if (abs(nx - x) > 0.5 || abs(ny - y) > 0.5) drawLine(nx, ny, nx, ny, lineW, 2)
    else drawLine(x, y, nx, ny, lineW, 2)
  }
}
