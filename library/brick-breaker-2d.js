// name: Brick Breaker
// tags: panel, game, retro
// Clean-room reimplementation from the prose spec docs/pattern-specs/brick-breaker-2d.md — reimagined,
// not a replica. Power-ups, combos and the item-chasing AI: original work from the prose idea spec
// docs/pattern-specs/patternflow/brick-breaker-arcade.md (Patternflow preset, CC BY-SA 4.0 —
// prose-described idea only, no code). An attract-mode brick-breaking game, inspired by the
// classic arcade one, sized from the grid, bricks along the long axis. The AI paddle predicts
// each ball's landing, aims it at a live brick and detours for falling power-ups: multiball
// (cyan), wide paddle (green), fire ball (orange). Tap or drag the preview (or POST /api/events,
// type 1) to take the paddle; the AI is back 4 s after the last touch. renderBulk: a delta-timed
// fade() is the trail, bricks are two fillRects, hits are additive splat bursts.

var spd = 30            // ball speed, grid cells per second
var padCells = 8        // paddle length, grid cells
var dropPct = 15        // share of broken bricks that drop a power-up
var glowMs = 90         // afterglow half-life

//# min=8 max=90 step=1 default=30
export function sliderBallSpeed(v) { spd = clamp(v, 4, 200) }

//# min=3 max=24 step=1 default=8
export function sliderPaddleCells(v) { padCells = clamp(floor(v), 2, 64) }

//# min=0 max=50 step=1 default=15
export function sliderPowerUpsPct(v) { dropPct = clamp(v, 0, 100) }

//# min=20 max=400 step=10 default=90
export function sliderAfterglowMs(v) { glowMs = clamp(v, 10, 2000) }

export var score = 0
export var level = 1

var MAXR = 7, MAXC = 14, MAXB = 6, MAXI = 6, MAXP = 24
var brick = array(MAXR * MAXC)
var bu = array(MAXB), bv = array(MAXB), bdu = array(MAXB), bdv = array(MAXB)
var iu = array(MAXI), iv = array(MAXI), ity = array(MAXI)
var pu = array(MAXP), pv = array(MAXP), pdu = array(MAXP), pdv = array(MAXP), pl = array(MAXP), ph = array(MAXP)
var ev = array(4)
var cS = array(320), cV = array(320)

var gw = -1, gh = -1, turn = 0, A = 32, L = 32, sx = 1, sy = 1, sr = 1
var nc = 6, nr = 5, bw = 5, bh = 2, bu0 = 1, bv0 = 3, padV = 30, thick = 1
var nBalls = 0, nItems = 0, left = 0, serveT = 0.5, wallT = 0
var padU = 16, padTarget = 16, wideT = 0, wideAmt = 0, fireT = 0
var manualU = 16, manualT = 0, combo = 0, quietT = 0, aimC = 0, aimR = 0
var hue0 = 0.97, clock = 0

function layout() {
  gw = gridWidth()
  gh = gridHeight()
  var w = gw > 0 ? gw : 32, h = gw > 0 ? gh : 32
  turn = h > w                 // a tall panel turns the field so the wall runs along its length
  A = turn ? h : w
  L = turn ? w : h
  sx = 1 / max(w - 1, 1)
  sy = 1 / max(h - 1, 1)
  sr = (sx + sy) / 2
  nc = clamp(floor((A - 2) / 5), 4, MAXC)
  bw = min(floor((A - 2) / nc), 20)
  bu0 = floor((A - nc * bw) / 2)
  bh = clamp(round(L / 20), 2, 16)
  nr = clamp(floor(L / 10), 4, MAXR)
  bv0 = max(2, round(L * 0.1))
  thick = max(1, round(L / 48))
  padV = L - 1 - thick
  padU = A / 2
  // one brick sprite (lit face, dark rim on the near/right edges), blitted per brick
  var ia = bw >= 3 ? 1 : 0
  for (var a = 0; a < bw; a++) for (var b = 0; b < bh; b++) {
    var rim = (ia && a == bw - 1) || b == bh - 1
    var i = turn ? a * bh + b : b * bw + a
    cS[i] = rim ? 0.9 : 0.72
    cV[i] = rim ? 0.42 : 1
  }
  rebuild()
}

function rebuild() {
  for (var k = 0; k < nr * nc; k++) brick[k] = 1
  left = nr * nc
  nBalls = 0
  nItems = 0
  serveT = 0.9
  wallT = 0
  fireT = 0
  quietT = 0
  pickAim()
}

function pickAim() {
  var k = floor(random(nr * nc))
  for (var t = 0; t < nr * nc; t++) {
    if (brick[k]) { aimC = k % nc; aimR = floor(k / nc); return }
    k = (k + 1) % (nr * nc)
  }
}

function addBall(u, v, du, dv) {
  if (nBalls >= MAXB) return
  var m = sqrt(du * du + dv * dv)
  bu[nBalls] = u; bv[nBalls] = v; bdu[nBalls] = du / m; bdv[nBalls] = dv / m
  nBalls++
}

function serve() {
  var a = (random(1) - 0.5) * 1.2
  addBall(A / 2 + (random(1) - 0.5) * A * 0.4, bv0 + nr * bh + 2, sin(a), cos(a))
  glow(bu[nBalls - 1], bv[nBalls - 1], A * 0.3, 0.55, 0.35)
}

// Hit light is queued here and splatted (additive) by renderBulk, since the brush
// only exists there. Radius in cells, hue in turns, level 0..1.
var NQ = 10, nPend = 0
var qU = array(NQ), qV = array(NQ), qR = array(NQ), qH = array(NQ), qL = array(NQ)
function glow(u, v, r, hue, lev) {
  if (nPend >= NQ) return
  qU[nPend] = u; qV[nPend] = v; qR[nPend] = r; qH[nPend] = hue; qL[nPend] = lev
  nPend++
}

function spark(u, v, hue, n) {
  for (var k = 0; k < MAXP && n > 0; k++) {
    if (pl[k] > 0) continue
    var a = random(PI2)
    var s = 6 + random(10)
    pu[k] = u; pv[k] = v; pdu[k] = cos(a) * s; pdv[k] = sin(a) * s
    pl[k] = 0.5 + random(0.4); ph[k] = hue
    n--
  }
}

function rowHue(r) { return hue0 + r * 0.105 }

function hitBrick(c, r) {
  brick[r * nc + c] = 0
  left--
  combo++
  quietT = 0
  score = score >= 30000 ? 0 : score + combo
  var cu = bu0 + c * bw + (bw - 1) / 2, cv = bv0 + r * bh + (bh - 1) / 2
  // combos burn hotter and wider: blue -> magenta -> red -> orange
  glow(cu, cv, bw * (0.9 + min(combo, 10) * 0.22), 0.62 - min(combo, 8) * 0.07, 0.25 + min(combo, 8) * 0.08)
  spark(cu, cv, rowHue(r), 3)
  if (nItems < MAXI && random(100) < dropPct) {
    iu[nItems] = cu; iv[nItems] = cv; ity[nItems] = floor(random(3)); nItems++
  }
  if (aimC == c && aimR == r) pickAim()
}

// Mirror-fold an across position into the playable span [1, A - 2].
function fold(u) {
  var R = A - 3
  var x = mod(u - 1, 2 * R)
  return (x > R ? 2 * R - x : x) + 1
}

function stepBall(k, dist, pw) {
  var n = ceil(dist / 0.45)
  var st = dist / n
  for (var s = 0; s < n; s++) {
    var ou = bu[k], ov = bv[k]
    var u = ou + bdu[k] * st, v = ov + bdv[k] * st
    if (u < 1) { u = 2 - u; bdu[k] = abs(bdu[k]) }
    if (u > A - 2) { u = 2 * (A - 2) - u; bdu[k] = -abs(bdu[k]) }
    if (v < 0) { v = -v; bdv[k] = abs(bdv[k]) }
    var c = floor((u - bu0 + 0.5) / bw), r = floor((v - bv0 + 0.5) / bh)
    if (c >= 0 && c < nc && r >= 0 && r < nr && brick[r * nc + c]) {
      hitBrick(c, r)
      if (fireT <= 0) {
        var pr = floor((ov - bv0 + 0.5) / bh)
        if (pr == r) { bdu[k] = -bdu[k]; u = ou } else { bdv[k] = -bdv[k]; v = ov }
      }
    }
    if (bdv[k] > 0 && ov < padV - 1 && v >= padV - 1 && abs(u - padU) <= pw / 2 + 0.6) {
      var off = clamp((u - padU) / (pw / 2 + 0.6), -1, 1)
      var a = off * 1.05 + (random(1) - 0.5) * 0.08
      bdu[k] = sin(a)
      bdv[k] = -cos(a)
      v = padV - 1
      combo = 0
      glow(u, padV, pw * 0.5, 0.55, 0.18)
      pickAim()
    }
    bu[k] = u
    bv[k] = v
  }
}

export function beforeRender(delta) {
  if (gridWidth() != gw || gridHeight() != gh) layout()
  var dt = min(delta, 50) / 1000
  clock += dt
  fadeK = pow(0.5, min(delta, 200) / glowMs)
  while (readEvent(ev)) {
    if (ev[0] == 1) { manualU = (turn ? ev[2] : ev[1]) * (A - 1); manualT = 4 }
  }
  manualT = max(0, manualT - dt)
  wideT = max(0, wideT - dt)
  fireT = max(0, fireT - dt)
  wideAmt += ((wideT > 0 ? 1 : 0) - wideAmt) * min(1, dt * 6)
  var pw = padCells * (1 + 0.7 * easeOutBack(clamp(wideAmt, 0, 1)))
  pw = min(pw, A - 3)
  wallT += dt
  quietT += dt

  // balls
  var k = 0
  while (k < nBalls) {
    stepBall(k, spd * dt, pw)
    if (bv[k] > L + 1) {               // lost: swap-remove
      nBalls--
      bu[k] = bu[nBalls]; bv[k] = bv[nBalls]; bdu[k] = bdu[nBalls]; bdv[k] = bdv[nBalls]
    } else k++
  }
  if (nBalls == 0) {
    serveT -= dt
    if (serveT <= 0) { serve(); serveT = 0.7; combo = 0 }
  }
  if (left <= 0 || quietT > 20) {      // cleared (or stuck): next level
    level = level >= 30000 ? 1 : level + 1
    hue0 = mod(hue0 + 0.29, 1)
    glow(A / 2, bv0 + nr * bh / 2, A * 0.9, hue0 + 0.1, 1)
    rebuild()
  }

  // items fall toward the paddle
  var iSpd = max(6, spd * 0.35)
  var bestT = 999, bestU = padU
  k = 0
  while (k < nItems) {
    iv[k] += iSpd * dt
    var gone = 0
    if (iv[k] >= padV - 0.5 && iv[k] < padV + 1 && abs(iu[k] - padU) <= pw / 2 + 1) {
      var t = ity[k]
      if (t == 0) {
        var nb = nBalls
        for (var j = 0; j < nb; j++) {
          addBall(bu[j], bv[j], bdu[j] + 0.5, bdv[j])
          addBall(bu[j], bv[j], bdu[j] - 0.5, bdv[j])
        }
      } else if (t == 1) wideT = 10
      else fireT = 7
      glow(iu[k], padV, A * 0.35, t == 0 ? 0.55 : t == 1 ? 0.39 : 0.15, 0.6)
      gone = 1
    } else if (iv[k] > L + 1) gone = 1
    if (gone) {
      nItems--
      iu[k] = iu[nItems]; iv[k] = iv[nItems]; ity[k] = ity[nItems]
    } else {
      var ta = (padV - iv[k]) / iSpd
      if (ta > 0 && ta < bestT) { bestT = ta; bestU = iu[k] }
      k++
    }
  }

  // AI: earliest descending ball -> its landing, aimed at a live brick
  var ballT = 999, ballU = A / 2
  for (k = 0; k < nBalls; k++) {
    if (bdv[k] <= 0.05) continue
    var tt = (padV - 1 - bv[k]) / (bdv[k] * spd)
    if (tt >= 0 && tt < ballT) { ballT = tt; ballU = fold(bu[k] + bdu[k] * spd * tt) }
  }
  var pmax = spd * 1.3
  if (manualT > 0) {
    padTarget = manualU
    pmax = spd * 6
  } else if (ballT < 999) {
    var tu = bu0 + aimC * bw + bw / 2, tv = bv0 + aimR * bh + bh / 2
    var off = clamp(atan2(tu - ballU, padV - 1 - tv) / 1.05, -0.8, 0.8)
    padTarget = ballU - off * (pw / 2 + 0.6)
    // detour for an item that lands well before the ball and is reachable in time
    if (bestT < ballT - 0.25 && abs(bestU - padU) < pmax * bestT &&
        abs(ballU - bestU) < pmax * (ballT - bestT)) padTarget = bestU
  } else if (bestT < 999) {
    padTarget = bestU
  } else padTarget = A / 2
  var mv = clamp(padTarget - padU, -pmax * dt, pmax * dt)
  padU = clamp(padU + mv, 1 + pw / 2 - 0.5, A - 2 - pw / 2 + 0.5)

  // debris
  for (k = 0; k < MAXP; k++) {
    if (pl[k] <= 0) continue
    pl[k] -= dt
    pu[k] += pdu[k] * dt
    pv[k] += pdv[k] * dt
    pdv[k] += 30 * dt
  }
  curPw = pw
}
var curPw = 8, fadeK = 0.8

// Inclusive game-cell rectangle (u across, v along) -> fillRect in map space.
function rect(u0, v0, u1, v1) {
  if (turn) fillRect((v0 - 0.45) * sx, (u0 - 0.45) * sy, (v1 + 0.45) * sx, (u1 + 0.45) * sy)
  else fillRect((u0 - 0.45) * sx, (v0 - 0.45) * sy, (u1 + 0.45) * sx, (v1 + 0.45) * sy)
}
function dot(u, v, r, mode) {
  if (turn) splat(v * sx, u * sy, r * sr, mode)
  else splat(u * sx, v * sy, r * sr, mode)
}

// One game cell; cell k spans [k - 0.5, k + 0.5) of a continuous position.
function cell(u, v) {
  if (gw <= 0) rect(u, v, u, v)
  else setPixel(turn ? gridIndex(v, u) : gridIndex(u, v))
}
function plus(u, v, big) {
  var cu = round(u), cv = round(v)
  cell(cu, cv)
  if (big) { cell(cu - 1, cv); cell(cu + 1, cv); cell(cu, cv - 1); cell(cu, cv + 1) }
}

export function renderBulk() {
  fade(fadeK)
  var k, c, r

  // queued hit bursts
  for (k = 0; k < nPend; k++) {
    oklch(0.45 + qL[k] * 0.45, 0.16, qH[k])
    dot(qU[k], qV[k], qR[k], 1)
  }
  nPend = 0

  // side rails
  oklch(0.3, 0.06, 0.78)
  rect(0, 0, 0, L - 1)
  rect(A - 1, 0, A - 1, L - 1)

  // bricks: rows grow in after a rebuild; fire mode makes them shimmer
  for (r = 0; r < nr; r++) {
    if (wallT < r * 0.08) continue
    var v0 = bv0 + r * bh, hue = rowHue(r)
    for (c = 0; c < nc; c++) {
      if (!brick[r * nc + c]) continue
      var u0 = bu0 + c * bw
      var h = fireT > 0 ? hue + 0.06 * wave(clock * 3 + c * 0.13 + r * 0.21) : hue
      if (gw > 0) {
        if (turn) blit(h, cS, cV, bh, bw, v0, u0, 0)
        else blit(h, cS, cV, bw, bh, u0, v0, 0)
      } else {                          // not a grid: plain rectangles
        hsv(h, 0.72, 1)
        rect(u0, v0, u0 + bw - 1, v0 + bh - 1)
      }
    }
  }

  // debris
  for (k = 0; k < MAXP; k++) {
    if (pl[k] <= 0) continue
    hsv(ph[k], 0.55, min(1, 0.4 + pl[k]))
    dot(pu[k], pv[k], 1.1, 1)
  }

  // items: blinking plus signs, multiball cyan / wide green / fire orange
  for (k = 0; k < nItems; k++) {
    var t = ity[k]
    var blink = square(clock * (2.5 + t), 0.5)
    oklch(0.55 + blink * 0.4, 0.17, t == 0 ? 0.55 : t == 1 ? 0.39 : 0.15)
    plus(iu[k], iv[k], 1)
  }

  // paddle: pale bar, pulsing ends while wide, a magenta marker while a human holds it
  var pw = curPw, p0 = padU - pw / 2, p1 = padU + pw / 2
  oklch(0.9, 0.05, 0.6)
  rect(round(p0), padV, round(p1) - 1, padV + thick - 1)
  if (wideAmt > 0.05) {
    oklch(0.7 + 0.25 * wave(clock * 3), 0.17, 0.39)
    rect(round(p0), padV, round(p0), padV + thick - 1)
    rect(round(p1) - 1, padV, round(p1) - 1, padV + thick - 1)
  }
  if (manualT > 0) {
    oklch(0.55 + 0.3 * wave(clock * 2), 0.22, 0.9)
    rect(round(padU) - 1, padV, round(padU), padV + thick - 1)
  }

  // balls: halo, then a white core; fire balls burn orange and bigger
  for (k = 0; k < nBalls; k++) {
    if (fireT > 0) { oklch(0.75, 0.19, 0.12); dot(bu[k], bv[k], 3.2, 1) }
    else { oklch(0.6, 0.08, 0.6); dot(bu[k], bv[k], 2, 1) }
    rgb(1, 1, 0.9)
    plus(bu[k], bv[k], A >= 48 ? 1 : 0)
  }
}
