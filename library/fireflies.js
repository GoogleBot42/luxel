// name: Fireflies
// Clean-room reimplementation from the prose specs docs/pattern-specs/fireflies.md, colourful-fireflies.md, slowflies.md, xmasflies.md — reimagined, not a replica.
// Fireflies on a strip or a panel. Blink: flies hover and flash (fast attack,
// slow decay), and now and then a flash ripples through a neighbourhood in
// sync. Drift: they glide, swell and fade, leaving a cool ghost trail. Coast:
// launched sparks glow brightest while fast, slow to a stop, then respawn.
// Between flashes a blinking fly's lantern idles at a faint glow, so the meadow is
// never fully dark; a sync ripple reaches only part of the swarm.
// One max-blended splat per fly (a sub-pixel splatIndex on a strip) over a fade()d frame.

var MAXF = 96
var fx = array(MAXF), fy = array(MAXF), fvx = array(MAXF), fvy = array(MAXF)
var ph = array(MAXF), rate = array(MAXF)

// 0 blink (hover & flash) · 1 drift (glide, swell, ghost trail) · 2 coast (launch & slow down)
var style = 0
//# min=0 max=2 step=1 default=0
export function sliderStyle(v) { style = clamp(floor(v), 0, 2) }

// 0 warm firefly green-gold · 1 rainbow (a hue per fly) · 2 holiday red/green/blue/gold · 3 picker
var colour = 0
//# min=0 max=3 step=1 default=0
export function sliderColour(v) { colour = clamp(floor(v), 0, 3) }

// Fly colour for Colour 3 (hue and saturation; the picker's brightness is ignored).
var pickH = 0.55, pickS = 0.8
export function hsvPickerFlyColor(h, s, v) { pickH = h; pickS = s }

// Flies per 100 px of strip; a panel counts 8 px per pixel of its side (64x64 = 32 at 4).
var per100 = 4
//# min=1 max=20 step=1 default=4
export function sliderFliesPer100Px(v) { per100 = clamp(v, 1, 20) }

// Blink: seconds between one fly's flashes. Drift: a fly's life. Coast: time to coast to a stop.
var life = 3
//# min=1 max=10 step=0.5 default=3
export function sliderLifetimeSeconds(v) { life = clamp(v, 1, 10) }

// Seconds for a trail to fade to 5 %; 0 = no trail.
var trail = 0.8
//# min=0 max=3 step=0.1 default=0.8
export function sliderTrailSeconds(v) { trail = clamp(v, 0, 3) }

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var n = 0, lastStyle = -1, panel = 0, unit = 0.03, dt = 0, syncIn = 6

function spawn(i) {
  fx[i] = random(1)
  fy[i] = 0.06 + random(0.88)
  var a = random(PI2), sp = 1
  if (style == 1) sp = 3 + random(3)                 // px/s
  if (style == 2) sp = 14 + random(22)
  if (style == 0) sp = 0.8 + random(1.2)
  if (!panel) a = random(1) < 0.5 ? 0 : PI           // strips: left or right
  fvx[i] = cos(a) * sp
  fvy[i] = sin(a) * sp
  rate[i] = 1 / (life * (0.8 + random(0.4)))
  ph[i] = style == 1 ? random(0.3) : random(1)
}

// Blink: a flash that strikes in 0.12 s and dies over ~half a second.
function flash(p) {
  if (p < 0) return 0
  var s = p * life
  if (s < 0.12) return easeOutQuad(s / 0.12)
  return exp((0.12 - s) / min(0.55, life * 0.2))
}

// Synchronous fireflies: flies near an epicentre flash in a ripple outward.
function syncRipple() {
  var k = floor(random(n)), d
  for (var i = 0; i < n; i++) {
    d = panel ? hypot(fx[i] - fx[k], fy[i] - fy[k]) : abs(fx[i] - fx[k])
    if (d < min(unit * 22, 0.3)) ph[i] = -d / unit * 0.035 / life   // 0.035 s per px of distance
  }
}

export function beforeRender(delta) {
  dt = min(delta, 100) / 1000
  panel = has2DMap()
  var side = panel ? (gridWidth() > 0 ? gridWidth() : 32) : pixelCount
  unit = 1 / max(side, 8)
  var want = clamp(round(panel ? per100 * sqrt(pixelCount) / 8 : per100 * pixelCount / 100), 1, MAXF)
  if (style != lastStyle) { n = 0; lastStyle = style }
  for (var i = n; i < want; i++) {                    // new flies: spread evenly, staggered
    spawn(i)
    fx[i] = frac(i * 0.618 + 0.2)
    fy[i] = 0.06 + frac(i * 0.755 + 0.4) * 0.88
    ph[i] = frac(i * 0.618 + 0.1)                    // phased apart from the start
    if (style == 2) { fvx[i] *= 0.15 + random(0.85); fvy[i] *= 0.15 + random(0.85) }
  }
  n = want
  if (style == 0) {
    syncIn -= dt
    if (syncIn < 0) { syncRipple(); syncIn = 7 + random(9) }
  }
  for (i = 0; i < n; i++) {
    if (style == 0) {                                 // wander: heading turns slowly
      var a = atan2(fvy[i], fvx[i]) + (random(2) - 1) * 2 * dt
      var sp = hypot(fvx[i], fvy[i])
      fvx[i] = cos(a) * sp
      fvy[i] = sin(a) * sp
    }
    if (style == 2) {                                 // coast: speed /10 over a lifetime
      var drag = exp(-dt * 2.3 * rate[i])
      fvx[i] *= drag
      fvy[i] *= drag
      if (hypot(fvx[i], fvy[i]) < 1.5) { spawn(i); ph[i] = 0 }
    }
    fx[i] = mod(fx[i] + fvx[i] * unit * dt, 1)
    if (panel) {
      fy[i] += fvy[i] * unit * dt
      if (fy[i] < 0.03 || fy[i] > 0.97) { fvy[i] = -fvy[i]; fy[i] = clamp(fy[i], 0.03, 0.97) }
    }
    ph[i] += rate[i] * dt
    if (ph[i] >= 1) {
      if (style == 1) spawn(i)
      ph[i] = mod(ph[i], 1)
    }
  }
}

function flyHue(i) {
  if (colour == 1) return hash(i + 0.5)
  if (colour == 2) return (i % 4 == 0) ? 0 : (i % 4 == 1) ? 0.33 : (i % 4 == 2) ? 0.62 : 0.1
  if (colour == 3) return pickH
  return 0.15 + frac(i * 0.37) * 0.06
}

function brightness(i) {
  if (style == 0) return max(flash(ph[i]), 0.15)  // idle glow between flashes
  if (style == 1) { var e = sin(PI * ph[i]); return e * e }
  var e2 = min(hypot(fvx[i], fvy[i]) / 22, 1)
  return e2 * (0.35 + 0.65 * e2)
}

export function renderBulk() {
  if (trail <= 0) clear()
  else fade(exp(-3 * dt / trail))
  var r = unit * (unit > 0.022 ? 2.8 : 2.2), sat0 = colour == 3 ? pickS : colour == 0 ? 0.95 : 1
  for (var i = 0; i < n; i++) {
    var v = brightness(i)
    if (v < 0.03) continue
    var h = flyHue(i), s = sat0 * (1 - 0.45 * v * v)  // a hot flash whitens
    if (style == 1) {                                 // ghost: cooler, a step behind
      var sp = max(hypot(fvx[i], fvy[i]), 0.01)
      var gx = fx[i] - fvx[i] / sp * unit * 1.6, gy = fy[i] - fvy[i] / sp * unit * 1.6
      hsv(h + 0.42, 0.85, v * 0.5)
      if (panel) splat(gx, gy, r, 2)
      else splatIndex(mod(gx, 1) * pixelCount - 0.5, 1.5, 2)   // sub-pixel: glides between LEDs
    }
    hsv(h, s, v)
    // the step this frame, so a fast fly paints a stroke, not dots
    var ox = fx[i] - fvx[i] * unit * dt, oy = fy[i] - fvy[i] * unit * dt
    if (panel) {
      if (style == 2 && abs(ox - fx[i]) < 0.5) drawLine(ox, oy, fx[i], fy[i], r, 2)
      else splat(fx[i], fy[i], r, 2)
    } else {
      var p1 = fx[i] * pixelCount, p0 = mod(ox, 1) * pixelCount
      if (abs(p1 - p0) > 1 && abs(p1 - p0) < pixelCount / 2) fillRangeAA(min(p0, p1), max(p0, p1), 2)
      splatIndex(p1 - 0.5, 1.5, 2)                   // a ~1-px tent: glides between LEDs
    }
  }
  setGlow(panel ? 0.35 : 0.3)
}
