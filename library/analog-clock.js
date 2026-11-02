// name: Analog Clock
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/rgbclock-2d.md, 2d-clock-with-hand-color-pickers.md, pixelclock.md — reimagined, not a replica.
// A wall-clock face. Panels: renderBulk draws a rim, ticks and three tapered
// anti-aliased drawLine hands; Style 0 is a classic watch face, 1 RGB additive
// (primary beams that mix to CMY and white, the second hand trailing a comet),
// 2 minimal progress rings with the time in the middle. Strips and LED rings:
// the whole strip is the dial, with ticks and soft hands drawn in index space.

var style = 0           // 0 classic, 1 RGB additive, 2 rings (panels only)
var smooth = 1
var h24 = 0
var reverse = 0
var offset = 0          // ring start, turns
var hrH = 0.09, hrS = 0.7
var mnH = 0.55, mnS = 0.25
var scH = 0.99, scS = 0.95

// 0 classic face, 1 RGB additive beams, 2 progress rings (panels; a strip has one look)
//# min=0 max=2 step=1 default=0
export function sliderStyle(v) { style = clamp(floor(v), 0, 2) }

// On: the second hand sweeps. Off: it ticks, with a little quartz bounce.
//# default=1
export function toggleSmoothSweep(v) { smooth = v }

// The hour hand goes round once a day instead of twice.
//# default=0
export function toggle24HourDial(v) { h24 = v }

//# default=0
export function toggleReverse(v) { reverse = v }

// Where 12 o'clock sits on a strip or LED ring, in degrees along it.
//# min=0 max=359 step=1 default=0
export function sliderRingStartDeg(v) { offset = mod(v, 360) / 360 }

// Hand colours (V is ignored; the RGB style always uses pure primaries).
export function hsvPickerHourHand(h, s, v) { hrH = h; hrS = s }
export function hsvPickerMinuteHand(h, s, v) { mnH = h; mnS = s }
export function hsvPickerSecondHand(h, s, v) { scH = h; scS = s }

// unit circle in 120 steps (index 120 == 0), clockwise from 12 o'clock
var UX = array(121), UY = array(121)
for (var u = 0; u <= 120; u++) { UX[u] = sin(u / 120 * PI2); UY[u] = -cos(u / 120 * PI2) }

var lastSec = -1, secFrac = 0
var aS = 0, aM = 0, aH = 0     // hand angles, turns clockwise from 12
var hh = 0, mm = 0
var gw = 32, gh = 32, kx = 1 / 31, ky = 1 / 31, ku = 1 / 31, cx = 15.5, cy = 15.5, R = 15.5

export function beforeRender(delta) {
  var s = clockSecond()
  if (s != lastSec) { lastSec = s; secFrac = 0 } else secFrac = min(secFrac + delta / 1000, 3000)
  var sp = smooth ? s + secFrac : s - 1 + easeOutBack(clamp(secFrac / 0.18, 0, 1))
  hh = clockHour(); mm = clockMinute()
  aS = sp / 60
  aM = (mm + (s + secFrac) / 60) / 60
  aH = h24 ? (hh + aM) / 24 : (hh % 12 + aM) / 12

  gw = gridWidth(); gh = gridHeight()
  if (gw == 0) { gw = 32; gh = 32 }
  kx = 1 / (gw - 1); ky = 1 / (gh - 1); ku = 1 / (min(gw, gh) - 1)
  cx = (gw - 1) / 2; cy = (gh - 1) / 2
  R = min(gw, gh) / 2 - 0.5
}

// point at angle a (turns) / table step k, radius r (px) -> normalized px2x, px2y
var px2x = 0, px2y = 0
function at(a, r) {
  if (reverse) a = -a
  px2x = (cx + sin(a * PI2) * r) * kx
  px2y = (cy - cos(a * PI2) * r) * ky
}
function atK(k, r) {
  px2x = (cx + (reverse ? -UX[k] : UX[k]) * r) * kx
  px2y = (cy + UY[k] * r) * ky
}

// a hand from radius r0 (negative = tail) to r1, w px wide, tapering to the tip
function hand(a, r0, r1, w, mode) {
  at(a, r0); var x0 = px2x, y0 = px2y
  var rm = r0 + (r1 - r0) * 0.6
  at(a, rm); var xm = px2x, ym = px2y
  at(a, r1)
  drawLine(x0, y0, xm, ym, w * ku, mode)
  drawLine(xm, ym, px2x, px2y, w * 0.6 * ku, mode)
}

// circle of radius r (px) in 120/st chords; lit (value v) up to fraction f with a
// bright head, the rest a dim track; f >= 1 draws it all lit
function ring(r, w, st, h, s, f, v) {
  var i, k, x0, y0
  atK(0, r); x0 = px2x; y0 = px2y
  if (f >= 1) hsv(h, s, v)
  for (i = st; i <= 120; i += st) {
    var t = i / 120
    var on = f >= 1 || t <= f
    if (f < 1) hsv(h, s, v * (on ? 0.3 + 0.7 * t / max(f, 0.001) : 0.14))
    atK(i, r)
    drawLine(x0, y0, px2x, px2y, (on ? w : w * 0.55) * ku, 2)
    x0 = px2x; y0 = px2y
  }
  if (f < 1) {                               // partial chord to the exact value + head
    i = floor(f * 120 / st) * st
    atK(i, r); x0 = px2x; y0 = px2y
    at(f, r)
    hsv(h, s, v)
    drawLine(x0, y0, px2x, px2y, w * ku, 2)
    hsv(h, s * 0.4, v)
    splat(px2x, px2y, w * 1.1 * ku, 2)
  }
}

function classic() {
  var y, i
  clear()
  for (y = 0; y < gh; y++) {                  // navy dial, a soft dome highlight at the top
    var dy = y - cy
    if (abs(dy) > R - 0.6) continue
    var hc = sqrt((R - 0.6) * (R - 0.6) - dy * dy)
    hsv(0.62, 0.75, 0.15 - 0.09 * (dy + R) / (2 * R))
    fillRect((cx - hc) * kx, (y - 0.2) * ky, (cx + hc) * kx, (y + 0.2) * ky)
  }
  ring(R - 0.4, 1.0, R >= 24 ? 3 : 4, 0.6, 0.2, 1, 0.5)   // steel rim
  var n = h24 ? 24 : 12
  for (i = 0; i < n; i++) {                   // hour ticks, the quarters longer
    var big = i % (n / 4) == 0, k = i * 120 / n
    hsv(0.1, 0.12, big ? 0.85 : 0.45)
    atK(k, big ? R * 0.77 : R * 0.84); var x0 = px2x, y0 = px2y
    atK(k, R * 0.93)
    drawLine(x0, y0, px2x, px2y, (big ? 1 : 0.75) * max(1, R * 0.06) * ku, 2)
  }
  if (R >= 24 && !h24) {                      // minute dots on big panels
    hsv(0.1, 0.1, 0.22)
    for (i = 0; i < 60; i++) {
      if (i % 5 == 0) continue
      atK(i * 2, R * 0.92)
      setPixel(gridIndex(round(px2x / kx), round(px2y / ky)))
    }
  }
  hsv(hrH, hrS, 0.95); hand(aH, -R * 0.1, R * 0.52, max(1.3, R * 0.11), 2)
  hsv(mnH, mnS, 0.95); hand(aM, -R * 0.12, R * 0.8, max(1.15, R * 0.08), 2)
  hsv(scH, scS, 1)
  at(aS, -R * 0.2); x0 = px2x; y0 = px2y
  at(aS, R * 0.9)
  drawLine(x0, y0, px2x, px2y, max(0.95, R * 0.035) * ku, 0)
  at(aS, -R * 0.16)
  splat(px2x, px2y, max(1.2, R * 0.07) * ku, 2)
  splat(cx * kx, cy * ky, max(1.4, R * 0.09) * ku, 0)
}

function rgbBeams() {
  clear()
  hsv(0, 0, 0.3)                              // twelve dim hour pips
  for (var i = 0; i < 12; i++) { atK(i * 10, R * 0.94); splat(px2x, px2y, max(1, R * 0.05) * ku, 2) }
  rgb(0, 0, 1); hand(aH, 0, R * 0.62, max(2, R * 0.2), 1)
  rgb(0, 1, 0); hand(aM, 0, R * 0.95, max(1.6, R * 0.14), 1)
  for (var g = 6; g >= 0; g--) {              // the second hand's comet tail
    var k = g == 0 ? 1 : 0.4 * (1 - g / 7)
    rgb(k, 0, 0)
    at(aS - g * 0.006, R * 0.15); var x0 = px2x, y0 = px2y
    at(aS - g * 0.006, R * 0.97)
    drawLine(x0, y0, px2x, px2y, max(1.2, R * 0.09) * ku, 1)
  }
  rgb(0.5, 0.5, 0.5); splat(cx * kx, cy * ky, max(1.3, R * 0.08) * ku, 1)
}

function rings() {
  clear()
  var w = max(1.1, R * 0.1)
  ring(R * 0.9, w * 0.8, 3, scH, scS, mod(aS, 1), 1)
  ring(R * 0.68, w, 4, mnH, mnS, mod(aM, 1), 1)
  ring(R * 0.44, w * 1.2, 6, hrH, hrS, mod(aH, 1), 1)
  var room = 2 * (R * 0.44 - w * 1.2)
  font("tiny")
  hsv(0.1, 0.15, 0.8)
  var hd = h24 ? hh : (hh % 12 == 0 ? 12 : hh % 12)
  if (room >= 18) {
    var x = round(cx) - 8, y = round(cy) - 2
    drawNumber(hd, x, y, 2, 0)
    if (mod(lastSec, 2) == 0) drawText(":", x + 6, y)
    drawNumber(mm, x + 10, y, 2, 0)
  } else if (room >= 9) {
    drawNumber(hd, round(cx) - 3, round(cy) - 2, 2, 0)
  }
}

// strip / ring: an anti-aliased span of half-width hw centred at turn a,
// wrapped round the ring's seam
function blob(a, hw, h, s, v) {
  var n = pixelCount
  var c = mod((reverse ? -a : a) + offset, 1) * n
  hsv(h, s, v)
  fillRangeAA(c - hw, c + hw, 0)
  if (c - hw < 0) fillRangeAA(c - hw + n, c + hw + n, 0)
  if (c + hw > n) fillRangeAA(c - hw - n, c + hw - n, 0)
}

function strip() {
  var n = pixelCount
  hsv(0.62, 0.4, 0.03)
  fill()
  for (var k = 0; k < 12; k++) {              // 5-minute ticks, 12 o'clock brightest
    hsv(0.1, 0.15, k == 0 ? 0.6 : 0.16)
    setPixel(floor(mod((reverse ? -k : k) / 12 + offset, 1) * n + 0.5) % n)
  }
  blob(aH, max(1, n / 45), hrH, hrS, 1)
  blob(aM, max(0.75, n / 90), mnH, mnS, 1)
  for (var g = 4; g >= 1; g--) blob(aS - g * 0.6 / n, 0.5, scH, scS, 0.5 - g * 0.1)   // short comet tail
  blob(aS, 0.75, scH, scS, 1)
}

export function renderBulk() {
  if (!has2DMap()) { strip(); return }
  if (style == 1) rgbBeams()
  else if (style == 2) rings()
  else classic()
}

// per-pixel fallback for engines without renderBulk: a dim face
export function render(index) { hsv(0.62, 0.4, 0.03) }
