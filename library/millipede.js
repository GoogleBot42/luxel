// name: Millipede
// Clean-room reimplementation from the prose spec docs/pattern-specs/millipede-1d-2d-controls.md
// — reimagined, not a replica.
// A millipede: plated segments that bunch and stretch as a gait wave runs up
// the body, legs rowing in a metachronal wave, a paler head with eyes and two
// flicking feelers. On a panel it wanders on a noise-steered heading and its
// body follows the exact path the head took (a trail of points); each segment
// is a few splats and drawLines, so cost scales with the body, not the panel.
// On a strip it crawls the strip as a loop, shaded over its own span only.

var NT = 96                        // trail points (two per segment pitch)
var trX = array(NT), trY = array(NT)
var head = 0, carry = 0            // newest trail slot, distance past it
var hxp = 0.5, hyp = 0.5, heading = 0.6
var gaitPh = 0, antPh = 0, stripU = 0, nt = random(100)

var segs = 14
//# min=4 max=40 step=1 default=14
export function sliderSegments(v) { segs = clamp(floor(v), 4, 40) }

// Crawl speed in percent of the display (panel width or strip length) per second.
var speedPct = 12
//# min=2 max=50 step=1 default=12
export function sliderSpeed(v) { speedPct = clamp(v, 2, 50) }

// Body colour (hue and saturation; the picker's brightness is ignored).
var hue0 = 0.28, sat0 = 0.9
export function hsvPickerBodyColor(h, s, v) { hue0 = h; sat0 = s }

// How far the hue drifts from head to tail, in degrees.
var shiftDeg = 50
//# min=0 max=180 step=1 default=50
export function sliderHueShift(v) { shiftDeg = clamp(v, 0, 180) }

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

function pitch() { return max(2.3 / max(gridWidth(), 32), 0.06) }

function layTrail() {
  var st = pitch() / 2
  for (var i = 0; i < NT; i++) {
    trX[i] = hxp - cos(heading) * st * (NT - 1 - i)
    trY[i] = hyp - sin(heading) * st * (NT - 1 - i)
  }
  head = NT - 1
  carry = 0
}
layTrail()

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  var spd = speedPct / 100
  gaitPh = mod(gaitPh + (0.4 + spd * 9) * dt, 1)
  antPh = mod(antPh + 0.6 * dt, 1)
  stripU = mod(stripU + spd * dt, 1)
  if (!has2DMap()) return

  // wander: a slow noise turn, plus a pull home near the edges
  nt = mod(nt + dt * 0.3, 1000)
  var turn = simplex2(nt, 7.5) * spd * 6     // turning radius >= ~1/6 panel
  var ex = hxp - 0.5, ey = hyp - 0.5
  var edge = smoothstep(0.12, 0.34, max(abs(ex), abs(ey)))
  var d = mod(atan2(-ey, -ex) - heading + PI, PI2) - PI
  heading = mod(heading + (turn * (1 - edge) + d * edge * spd * 9) * dt, PI2)
  var step = spd * dt
  hxp += cos(heading) * step
  hyp += sin(heading) * step

  // drop a trail point every half pitch
  var st = pitch() / 2
  carry += step
  while (carry >= st) {
    carry -= st
    var nx = hxp - cos(heading) * carry, ny = hyp - sin(heading) * carry
    head = (head + 1) % NT
    trX[head] = nx
    trY[head] = ny
  }
}

// position s (panel units) behind the head, along the trail
var px = 0, py = 0
function along(s) {
  var st = pitch() / 2
  var f = s - carry
  if (f < 0) {
    var k = f / carry
    px = mix(trX[head], hxp, -k)
    py = mix(trY[head], hyp, -k)
    return
  }
  var fi = min(f / st, NT - 2)
  var i = floor(fi), a = mod(head - i, NT), b = mod(head - i - 1, NT)
  px = mix(trX[a], trX[b], fi - i)
  py = mix(trY[a], trY[b], fi - i)
}

function panel() {
  var P = pitch(), hw = P * 0.62, n = segs
  var shift = shiftDeg / 360
  var lw = max(0.6 / max(gridWidth(), 32), 0.012)
  // legs, then plates over their roots, tail first so the head sits on top
  for (var k = n - 1; k >= 0; k--) {
    var t01 = k / n
    var s = P * (k + 0.35 * sin((t01 * 3 - gaitPh) * PI2))
    along(s + P * 0.5)
    var bx = px, by = py
    along(max(s - P * 0.5, 0))
    var tx = px - bx, ty = py - by, tl = max(hypot(tx, ty), 0.0001)
    tx /= tl
    ty /= tl
    along(s)
    var cx = px, cy = py
    var taper = (1 - 0.45 * t01 * t01) * (k == 0 ? 1.25 : 1)
    if (k > 0) {
      var sw = 0.55 * sin((gaitPh * 2 - k * 0.45) * PI2)
      var reach = hw * 2.3 * taper
      hsv(hue0 + t01 * shift + 0.05, sat0 * 0.6, 0.55)
      for (var side = -1; side <= 1; side += 2) {
        var lx = -ty * side + tx * sw * 0.6, ly = tx * side + ty * sw * 0.6
        drawLine(cx, cy, cx + lx * reach, cy + ly * reach, lw, 2)
      }
    }
    var lit = k % 2 ? 0.8 : 1
    hsv(hue0 + t01 * shift, sat0, lit)
    splat(cx, cy, hw * 1.7 * taper, 2)
    hsv(hue0 + t01 * shift + 0.04, sat0 * 0.5, lit)
    splat(cx, cy, hw * 0.7 * taper, 2)
  }
  // head: feelers, then two eyes
  along(0)
  var hx = px, hy = py, c = cos(heading), sn = sin(heading)
  hsv(hue0 + 0.08, sat0 * 0.4, 0.8)
  for (var e = -1; e <= 1; e += 2) {
    var a = heading + e * (0.55 + 0.3 * sin((antPh + e * 0.17) * PI2))
    var a2 = a + e * 0.35 * sin((antPh * 2 + e * 0.3) * PI2)
    var mx = hx + cos(a) * P * 1.3, my = hy + sin(a) * P * 1.3
    drawLine(hx + c * hw, hy + sn * hw, mx, my, lw, 2)
    drawLine(mx, my, mx + cos(a2) * P * 1.1, my + sin(a2) * P * 1.1, lw, 2)
  }
  hsv(0, 0, 1)
  var er = max(0.9 / max(gridWidth(), 32), 0.015)
  splat(hx + c * hw * 0.6 - sn * hw * 0.6, hy + sn * hw * 0.6 + c * hw * 0.6, er, 2)
  splat(hx + c * hw * 0.6 + sn * hw * 0.6, hy + sn * hw * 0.6 - c * hw * 0.6, er, 2)
}

// strip: the body is shaded pixel by pixel over its own span only
function strip() {
  var n = pixelCount, len = max(floor(n * 0.5), 8), ant = max(floor(len * 0.12), 2)
  var hp = stripU * n, shift = shiftDeg / 360
  for (var i = -ant; i < len; i++) {
    var idx = mod(floor(hp) - i, n), d = hp - floor(hp) + i
    if (i < 0) {
      // feelers: a flick of light just ahead of the head
      var an = -d / ant, r = 0.55 + 0.45 * abs(sin(antPh * PI2))
      hsv(hue0 + 0.08, sat0 * 0.4, saturate(1 - abs(an - r) * 4) * 0.8)
    } else {
      var t01 = d / len
      var seg = t01 * segs + 0.45 * sin((t01 * 3 - gaitPh) * PI2)
      var plate = sin(frac(seg) * PI)
      var legs = wave(seg * 0.45 - gaitPh * 2)
      var headMix = saturate(1 - t01 * 8)
      var tail = saturate((1 - t01) * 7)
      var v = (0.25 + 0.75 * plate) * (0.6 + 0.4 * legs) * tail
      v = max(v, headMix * (0.7 + 0.3 * plate))
      var glint = headMix * saturate(1 - t01 * 40)
      hsv(hue0 + t01 * shift - headMix * 0.04, sat0 * (1 - 0.4 * headMix - 0.6 * glint), saturate(v + glint * 0.5))
    }
    setPixel(idx)
  }
}

export function renderBulk() {
  clear()
  if (has2DMap()) panel()
  else strip()
}
