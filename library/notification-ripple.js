// name: Notification Ripple
// Curated original (hand-written showcase of the Luxel language/builtins).
// A home-automation signal light. It rests in a slow OKLCH breathe; an injected
// event's type picks a channel — 1 blue ring burst (doorbell), 2 green sweep (done),
// 3 amber double pulse (message), 4 red cross burst (alert), 0 clear — at the
// event's x/y (MQTT "1" with no x/y lands in the centre). Up to 8 signals play at
// once with easeOutBack/easeOutElastic envelopes, then it eases back; each unread
// channel keeps a breathing dot (top-right / strip end) until a clear event.

var demoPerMin = 2     // phantom signals so it is alive with no automation wired up
//# min=0 max=10 step=0.5 default=2
export function sliderDemoEventsPerMinute(v) { demoPerMin = clamp(v, 0, 10) }

export function triggerTestSignal() { fire(demoCh, 0.5, 0.5, 20); demoCh = demoCh % 4 + 1 }
export function triggerClear() { clearUnread() }

var Q = 8
var aCh = array(Q), aAge = array(Q), aX = array(Q), aY = array(Q)
var unread = array(5), hue = [0, 0.6, 0.36, 0.1, 0.98], dur = [1, 2.4, 2.2, 2.6, 1.8]
var ev = array(4), t = 0, breathe = 0, duck = 0, demoCh = 1, nextDemo = 1.5
for (var i = 0; i < Q; i++) aAge[i] = 99

function clearUnread() { for (var c = 0; c < 5; c++) unread[c] = 0 }

// expire: -1 keeps the dot until a clear, otherwise seconds until it goes
function fire(ch, x, y, expire) {
  var best = 0
  for (var i = 1; i < Q; i++) if (aAge[i] > aAge[best]) best = i
  aCh[best] = ch; aAge[best] = 0; aX[best] = x; aY[best] = y
  if (unread[ch] >= 0) unread[ch] = expire
}

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  t = mod(t + dt, 3000)
  while (readEvent(ev)) {
    var ty = floor(ev[0])
    if (ty == 0) clearUnread()
    else if (ty > 0) {
      var x = ev[1], y = ev[2]
      if (x == 0 && y == 0) { x = 0.5; y = 0.5 }
      fire((ty - 1) % 4 + 1, x, y, -1)
    }
  }
  if (demoPerMin > 0) {
    nextDemo -= dt
    if (nextDemo <= 0) {
      fire(demoCh, 0.25 + random(0.5), 0.25 + random(0.5), 20)
      demoCh = demoCh % 4 + 1
      nextDemo = 60 / demoPerMin * (0.5 + random(1))
    }
  }
  duck = 0
  for (var i = 0; i < Q; i++) {
    aAge[i] = min(aAge[i] + dt, 99)
    var u = aAge[i] / dur[aCh[i]]
    if (u < 1) duck = max(duck, 1 - u * u)
  }
  for (var c = 1; c < 5; c++) if (unread[c] > 0) unread[c] = max(unread[c] - dt, 0)
  breathe = wave(t / 9)
}

// A mapless strip stays a strip: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var panelMode = 0, n = 0, px1 = 0.02
var ah = 0
function ambient() {
  ah = 0.62 + 0.06 * sin(t / 30 * PI2)
  var k = 1 - 0.45 * duck
  oklch((0.3 + 0.05 * breathe) * k, 0.07, ah)
  fill()
  oklch((0.42 + 0.1 * breathe) * k, 0.09, ah + 0.03)
  if (panelMode) splat(0.5, 0.55, 0.85, 2)
  else splatIndex(n / 2, n * 0.45, 2)
}

// one ring of the burst: 20 short capsules round the circle
function ring(x, y, r, w) {
  var px = x + r, py = y
  for (var k = 1; k <= 20; k++) {
    var a = k * PI2 / 20, qx = x + r * cos(a), qy = y + r * sin(a)
    drawLine(px, py, qx, qy, w, 2)
    px = qx; py = qy
  }
}

function signal(i) {
  var ch = aCh[i], u = aAge[i] / dur[ch], h = hue[ch], x = aX[i], y = aY[i], p = x * n
  if (u >= 1) return
  if (ch == 1) {                                          // ring burst, two rings
    for (var k = 0; k < 2; k++) {
      var uk = (u - 0.2 * k) / (1 - 0.2 * k)
      if (uk <= 0) continue
      var r = easeOutCubic(uk), a = 1 - uk
      hsv(h, 0.75, a)
      if (panelMode) ring(x, y, 0.06 + 0.8 * r, max(0.03 + 0.05 * uk, 1.2 * px1))
      else { var rr = (2 + 5 * uk) * max(1, n / 150); splatIndex(p - r * n * 0.5, rr, 1); splatIndex(p + r * n * 0.5, rr, 1) }
    }
  } else if (ch == 2) {                                   // sweep with a fading wake
    var hx = -0.1 + 1.25 * easeInOutCubic(u), a = 1 - u * u
    if (panelMode) {
      for (var k = 5; k >= 0; k--) {
        hsv(h, 0.8, a * (k == 0 ? 1 : 0.45 * (1 - k / 6)))
        drawLine(hx - k * 0.05, -0.1, hx - k * 0.05, 1.1, k == 0 ? 0.07 : 0.05, 1)
      }
    } else {
      gradientRange((hx - 0.3) * n, hx * n, h, 0.8, 0, h, 0.8, a * 0.7, 1)
      hsv(h, 0.4, a)
      splatIndex(hx * n, 3, 1)
    }
  } else if (ch == 3) {                                   // gentle double pulse
    for (var k = 0; k < 2; k++) {
      var uk = (u - 0.38 * k) / 0.6
      if (uk <= 0 || uk >= 1) continue
      var a = sin(uk * PI) * (k ? 0.75 : 1), s = easeOutBack(uk)
      hsv(h, 0.85, a)
      if (panelMode) splat(0.5, 0.5, 0.25 + 0.6 * s, 1)
      else splatIndex(n / 2, max(3, n * (0.15 + 0.4 * s)), 1)
    }
  } else {                                                // cross burst: comets along row and column
    var r = 1.1 * easeOutCubic(u), a = 1 - u
    if (panelMode) {
      var w = max(0.035, 1.3 * px1)
      hsv(h, 0.85, a * 0.35)
      drawLine(x - r * 0.6, y, x + r * 0.6, y, w * 0.7, 1)
      drawLine(x, y - r * 0.6, x, y + r * 0.6, w * 0.7, 1)
      hsv(h, 0.6, a)
      drawLine(x + r * 0.6, y, x + r, y, w, 1); drawLine(x - r * 0.6, y, x - r, y, w, 1)
      drawLine(x, y + r * 0.6, x, y + r, w, 1); drawLine(x, y - r * 0.6, x, y - r, w, 1)
    } else {
      var d = r * n * 0.5
      gradientRange(p, p + d, h, 0.85, 0.15 * a, h, 0.7, a, 1)
      gradientRange(p - d, p, h, 0.7, a, h, 0.85, 0.15 * a, 1)
    }
  }
  if (ch != 3 && u < 0.4) {                               // an elastic flash where it landed
    hsv(h, 0.3, 1 - u / 0.4)
    var e = easeOutElastic(u / 0.4)
    if (panelMode) splat(x, y, max(0.04 + 0.08 * e, 2 * px1), 1)
    else splatIndex(p, 1 + 3 * e, 1)
  }
}

function dots() {
  var j = 0, dw = max(1, round(n / 75))
  for (var c = 1; c < 5; c++) {
    if (unread[c] == 0) continue
    var v = 0.35 + 0.65 * wave(t / 2.6 + j * 0.25)
    if (unread[c] > 0) v = v * min(unread[c] / 2, 1)       // demo dots fade away
    hsv(hue[c], 0.8, v)
    if (panelMode) {
      var r = max(0.035, 1.6 * px1)
      splat(1 - r * 1.3 - j * r * 2.8, r * 1.3, r * 1.3, 2)
    } else fillRangeAA(n - (2 * j + 2) * dw, n - (2 * j + 1) * dw, 2)
    j++
  }
}

export function renderBulk() {
  panelMode = has2DMap()
  n = pixelCount
  px1 = 1 / max(gridWidth(), 16)
  ambient()
  for (var i = 0; i < Q; i++) if (aAge[i] < 9) signal(i)
  dots()
}
