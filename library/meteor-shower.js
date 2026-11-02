// name: Meteor Shower
// tags: both, ambient, nature
// Clean-room reimplementation from the prose specs docs/pattern-specs/meteor-shower.md,
// rainbow-comet.md, pew-pew-pew.md, christmaspewpew.md, snake.md — reimagined, not a replica.
// Streaks with white-hot heads and tails that deepen in colour as they fade: a
// shower of single meteors, comets crossing both ways, short laser bolts at
// mixed speeds over a faint underglow, or comets bouncing end to end with a
// tail that folds round the turn. Tails are drawn fresh every frame from the
// head's path (gradientRange spans + a splatIndex head on a strip, drawLine
// segments on a panel at the chosen angle), so there are no buffers at all.

var MAXN = 20
var ep = array(MAXN), ev = array(MAXN), ed = array(MAXN), eh = array(MAXN)
var ea = array(MAXN), eq = array(MAXN), en = array(MAXN), ey = array(MAXN), ek = array(MAXN)

var style = 0
// 0 meteors, 1 comets crossing both ways, 2 laser bolts, 3 bouncing comets
//# min=0 max=3 step=1 default=0
export function sliderStyle(v) { style = clamp(floor(v), 0, 3); reset() }

var count = 4
//# min=1 max=20 step=1 default=4
export function sliderCount(v) { count = clamp(floor(v), 1, MAXN) }

var speed = 0.4
// Strip lengths (panel widths) per second.
//# min=0.05 max=3 step=0.05 default=0.4
export function sliderSpeed(v) { speed = clamp(v, 0.05, 3) }

var tailS = 1
// Tail length, in seconds of travel.
//# min=0.1 max=4 step=0.1 default=1
export function sliderTailSeconds(v) { tailS = clamp(v, 0.1, 4) }

var gapS = 1.2
// Average seconds between launches (meteors and bolts; a panel launches a little faster).
//# min=0 max=5 step=0.1 default=1.2
export function sliderGapSeconds(v) { gapS = clamp(v, 0, 5) }

var pal = 0
// 0 rainbow, 1 warm, 2 neon, 3 holiday red and green, 4 the picker's colour
//# min=0 max=4 step=1 default=0
export function sliderPalette(v) { pal = clamp(floor(v), 0, 4) }

var pickH = 0.6, pickS = 1
// Colour for palette 4 (hue only; the picker's brightness is ignored).
export function hsvPickerColor(h, s, v) { pickH = h; pickS = s }

var flash = 0
// A burst of light where each meteor or bolt launches.
//# default=0
export function toggleLaunchFlash(v) { flash = v }

var angle = 30
// Panel only: direction of travel, degrees clockwise from rightward.
//# min=0 max=359 step=1 default=30
export function sliderAngle(v) { angle = mod(v, 360) }

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var hueClock = 0, nextT = 0, launches = 0, two = 0, dt = 0
var dx = 1, dy = 0

function reset() {
  for (var i = 0; i < MAXN; i++) { ea[i] = -1; ep[i] = 0; ek[i] = 0 }
  nextT = 0
}
reset()

function pickHue() {
  launches = mod(launches + 1, 1000)
  if (pal == 1) return 0.98 + random(0.13)
  if (pal == 2) return 0.72 + random(0.26)
  if (pal == 3) return launches % 5 == 1 || launches % 5 == 3 ? 0 : 0.36
  if (pal == 4) return pickH - 0.03 + random(0.06)
  return hueClock + random(0.3)
}

// colour of a tail at u (0 head .. 1 tail end) for entity i -> ch, cs, cv
var ch = 0, cs = 0, cv = 0
function tailCol(i, u) {
  var shift = pal == 0 ? 0.14 : pal == 3 ? 0 : 0.05
  var s0 = style == 2 ? 0.55 : pal == 3 ? 0.15 : 0.3
  var s1 = pal == 4 ? pickS : 1
  ch = eh[i] - shift * u
  cs = s0 + (s1 - s0) * min(1, u * 3)
  cv = (1 - u) * (1 - (two ? 0.2 : 0.6) * u)
}

// panel line for entity i: entry and exit distance along the direction
var sIn = 0, sOut = 0
function chord(q) {
  var cx = 0.5 - dy * q, cy = 0.5 + dx * q
  var ax = -9, bx = 9, ay = -9, by = 9
  if (dx > 0.001) { ax = -cx / dx; bx = (1 - cx) / dx }
  if (dx < -0.001) { ax = (1 - cx) / dx; bx = -cx / dx }
  if (dy > 0.001) { ay = -cy / dy; by = (1 - cy) / dy }
  if (dy < -0.001) { ay = (1 - cy) / dy; by = -cy / dy }
  sIn = max(ax, ay)
  sOut = min(bx, by)
}

function launch(i, spread) {
  ea[i] = 0
  ev[i] = style == 2 ? 0.6 + random(1.6) : style == 1 ? 0.7 + random(0.6) : 0.8 + random(0.45)
  ed[i] = style == 1 && i % 2 ? -1 : 1
  eh[i] = pickHue()
  ek[i] = 0.75 + random(0.5)                // bounce: y rate ratio
  if (style == 3) { ep[i] = i / max(count, 1) + random(0.2); ey[i] = random(1); return }
  if (two) {
    var q = random(1.3) - 0.65
    chord(q)
    if (sIn > sOut) { q *= 0.4; chord(q) }
    if (ed[i] < 0) { var t = sIn; sIn = -sOut; sOut = -t }
    eq[i] = q
    ep[i] = sIn + spread * (sOut - sIn)
    en[i] = sOut
  } else {
    ep[i] = spread * pixelCount
    if (ed[i] < 0) ep[i] = pixelCount - ep[i]
  }
}

export function beforeRender(delta) {
  dt = min(delta, 100) / 1000
  hueClock = mod(hueClock + dt / 30, 1)
  two = has2DMap()
  setGlow(two ? 0.5 : 0)               // a native one-pixel bloom round the streaks
  var a = angle * PI / 180
  dx = cos(a)
  dy = sin(a)
  var unit = two ? 1 : pixelCount
  for (var i = 0; i < MAXN; i++) {
    if (i >= count) { ea[i] = -1; continue }
    if (ea[i] < 0) {
      // comets and bounce keep every slot filled; meteors and bolts queue
      if (style == 1 || style == 3) launch(i, random(1))
      continue
    }
    ea[i] += dt
    var v = speed * ev[i]
    if (style == 3) { ep[i] = mod(ep[i] + v * dt * 0.5, 1); ey[i] = mod(ey[i] + v * ek[i] * dt * 0.5, 1); continue }
    ep[i] += ed[i] * v * unit * dt
    var L = v * unit * tailS * (style == 2 ? 0.35 : 1)
    var gone = two ? ep[i] - L > en[i] : (ed[i] > 0 ? ep[i] - L > pixelCount : ep[i] + L < 0)
    if (gone) {
      if (style == 1) launch(i, 0)
      else ea[i] = -1
    }
  }
  if (style == 0 || style == 2) {
    nextT -= dt
    if (nextT <= 0) {
      for (var j = 0; j < count; j++) {
        if (ea[j] < 0) {
          launch(j, 0)
          nextT = max(0.05, gapS * (0.4 + random(1.2)) * (two ? 0.6 : 1))
          break
        }
      }
    }
  }
}

// ---- strip ----
function tail1D(i, head, L) {
  var d = ed[i], u0 = 0, p0 = head
  for (var k = 1; k <= 3; k++) {
    var u1 = k == 1 ? 0.15 : k == 2 ? 0.45 : 1
    var p1 = head - d * L * u1
    tailCol(i, u0)
    var h0 = ch, s0 = cs, v0 = cv
    tailCol(i, u1)
    gradientRange(p1, p0, ch, cs, cv, h0, s0, v0, 1)
    u0 = u1
    p0 = p1
  }
}

function tw(ph) { return triangle(mod(ph, 1)) }
function tri1(ph) { return tw(ph) * (pixelCount - 1) + 0.5 }

function bounce1D(i) {
  var r = speed * ev[i] * 0.5, K = 10
  var pa = ep[i]
  for (var k = 0; k < K; k++) {
    var pb = pa - r * tailS / K
    var ua = k / K, ub = (k + 1) / K
    tailCol(i, ua)
    var h0 = ch, s0 = cs, v0 = cv
    tailCol(i, ub)
    var wa = floor(pa * 2), wb = floor(pb * 2)
    if (wa == wb) gradientRange(tri1(pb), tri1(pa), ch, cs, cv, h0, s0, v0, 1)
    else {
      // split at the wall the comet turned on
      var pw = wa / 2, f = (pa - pw) / (pa - pb), uw = ua + (ub - ua) * f
      var wall = tri1(pw)
      tailCol(i, uw)
      gradientRange(wall, tri1(pa), ch, cs, cv, h0, s0, v0, 1)
      var hw = ch, sw = cs, vw = cv
      tailCol(i, ub)
      gradientRange(tri1(pb), wall, ch, cs, cv, hw, sw, vw, 1)
    }
    pa = pb
  }
  tailCol(i, 0)
  hsv(ch, cs * 0.4, 1)
  splatIndex(tri1(ep[i]), 1.8, 2)
}

function strip() {
  var n = pixelCount
  if (style == 2) {
    hsv(pal == 3 ? 0 : pal == 0 ? hueClock : eh[0], 1, 0.035)
    fill()
  }
  for (var i = 0; i < count; i++) {
    if (ea[i] < 0) continue
    if (style == 3) { bounce1D(i); continue }
    var head = ep[i], L = max(3, speed * ev[i] * n * tailS * (style == 2 ? 0.35 : 1))
    tail1D(i, head, L)
    tailCol(i, 0)
    hsv(ch, cs * 0.4, 1)
    if (style == 2) fillRangeAA(head - ed[i] * 3, head, 1)
    else splatIndex(head, 1.8, 1)
    if (flash && ea[i] < 0.3) {
      var f = 1 - ea[i] / 0.3
      hsv(ch, 0.25, f)
      splatIndex(ed[i] > 0 ? 0 : n, 3 + 6 * f, 1)
    }
  }
}

// ---- panel ----
function seg2D(i, ua, ub, xa, ya, xb, yb, w) {
  tailCol(i, (ua + ub) / 2)
  hsv(ch, cs, cv)
  drawLine(xa, ya, xb, yb, w * (1.3 - ua), 2)
}

function panel() {
  var gw = gridWidth()
  if (gw < 1) gw = 32
  var w = 1.6 / gw
  if (style == 2) {
    hsv(pal == 3 ? 0 : pal == 0 ? hueClock : eh[0], 1, 0.035)
    fill()
  }
  for (var i = 0; i < count; i++) {
    if (ea[i] < 0) continue
    var hx, hy, K = 6
    if (style == 3) {
      var m = 2 / gw, sp = 1 - 2 * m, r = speed * ev[i] * 0.5
      var pa = ep[i], qa = ey[i]
      hx = m + sp * tw(pa)
      hy = m + sp * tw(qa)
      K = 10
      for (var k = 0; k < K; k++) {
        var pb = pa - r * tailS / K, qb = qa - r * ek[i] * tailS / K
        seg2D(i, k / K, (k + 1) / K, m + sp * tw(pa), m + sp * tw(qa),
          m + sp * tw(pb), m + sp * tw(qb), w)
        pa = pb
        qa = qb
      }
    } else {
      var d = ed[i], L = speed * ev[i] * tailS * (style == 2 ? 0.35 : 1)
      var cx = 0.5 - dy * eq[i], cy = 0.5 + dx * eq[i]
      var s = ep[i]
      hx = cx + d * dx * s
      hy = cy + d * dy * s
      for (var k = 0; k < K; k++) {
        var sa = s - L * k / K, sb = s - L * (k + 1) / K
        seg2D(i, k / K, (k + 1) / K, cx + d * dx * sa, cy + d * dy * sa, cx + d * dx * sb, cy + d * dy * sb, w)
      }
      if (flash && ea[i] < 0.3) {
        var f = 1 - ea[i] / 0.3
        var s0 = s - speed * ev[i] * ea[i]
        tailCol(i, 0)
        hsv(ch, 0.25, f)
        splat(cx + d * dx * s0, cy + d * dy * s0, (2 + 5 * f) / gw, 1)
      }
    }
    tailCol(i, 0)
    hsv(ch, cs * 0.4, 1)
    if (style == 2) drawLine(hx, hy, hx - ed[i] * dx * 4 / gw, hy - ed[i] * dy * 4 / gw, 1.2 / gw, 2)
    else splat(hx, hy, 2.4 / gw, 2)
  }
}

export function renderBulk() {
  clear()
  if (two) panel()
  else strip()
}
