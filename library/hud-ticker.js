// name: HUD Ticker
// tags: both, ambient
// Original work from the prose idea spec docs/pattern-specs/patternflow/hud-ticker-rows.md (Patternflow preset, CC BY-SA 4.0 — prose-described idea only, no code).
// A sci-fi control wall: lanes scroll in alternating directions at a few
// speeds, each chopped into segments that carry a live widget — filling
// progress bars, caution chevrons, blinking data dots, an oscilloscope trace,
// counters, a level meter. Amber and cyan on black. On a strip it is one
// lane of 1-D widgets. A few rects/lines per visible segment, no per-pixel work.

var lane = 8
var seg = 16
var speed = 12

//# min=4 max=16 step=1 default=8
export function sliderLanePx(v) { lane = clamp(floor(v), 4, 16) }

//# min=8 max=32 step=1 default=16
export function sliderSegmentPx(v) { seg = clamp(floor(v), 8, 32) }

// Base scroll speed in pixels per second.
//# min=2 max=40 step=1 default=12
export function sliderSpeedPxS(v) { speed = clamp(v, 2, 40) }

var scroll = 0, t = 0
var gw = 32, gh = 32, kx = 1 / 31, ky = 1 / 31

export function beforeRender(delta) {
  scroll = mod(scroll + speed * delta / 1000, 30000)
  t = mod(t + delta / 1000, 3600)
}

function rect(x0, y0, x1, y1) { fillRect((x0 - 0.5) * kx, (y0 - 0.5) * ky, (x1 + 0.5) * kx, (y1 + 0.5) * ky) }
function line(x0, y0, x1, y1) { drawLine(x0 * kx, y0 * ky, x1 * kx, y1 * ky, 0.9 * kx, 2) }
function amber(l) { oklch(l, 0.16, 0.19) }
function cyan(l) { oklch(l, 0.12, 0.56) }

function tier(l) { return l % 4 == 0 ? 1 : l % 4 == 1 ? 0.55 : l % 4 == 2 ? 1.4 : 0.8 }

// one widget in the box x0..x1, y0..y1 (pixels); h = its hash, s = segment id
function widget(kind, x0, y0, x1, y1, h, s, dir) {
  var w = x1 - x0, mid = (y0 + y1) / 2, i, x
  if (kind == 0) {                                   // progress bar
    var p = easeInOutCubic(frac(t * (0.25 + 0.3 * h) + h))
    amber(0.3)
    rect(x0, mid - 1, x1, mid + 1)
    amber(p > 0.97 ? 1 : 0.85)
    rect(x0, mid - 1, x0 + w * p, mid + 1)
  } else if (kind == 1) {                            // caution chevrons
    var hh = min(2.5, (y1 - y0) / 2)
    for (x = x0 + 1; x + 2 <= x1; x += 4) {
      amber(0.45 + 0.45 * triangle(frac(t * 1.5 - dir * x / 24)))
      line(x + (dir > 0 ? 0 : 2), mid - hh, x + (dir > 0 ? 2 : 0), mid)
      line(x + (dir > 0 ? 2 : 0), mid, x + (dir > 0 ? 0 : 2), mid + hh)
    }
  } else if (kind == 2) {                            // blinking data dots
    var tick = floor(t * 4)
    for (var y = y0 + 1; y <= y1 - 1; y += 3) for (x = x0 + 1; x <= x1 - 1; x += 3) {
      var b = hash2(s * 97 + x * 7 + y, tick)
      if (b > 0.45) { cyan(0.45 + 0.5 * b); rect(x, y, x, y) }
    }
  } else if (kind == 3) {                            // oscilloscope trace
    var amp = (y1 - y0) / 2 - 0.5
    cyan(0.85)
    var py = mid + amp * sin(t * 5 + h * 9)
    for (x = x0 + 2; x <= x1; x += 2) {
      var ny = mid + amp * sin(t * 5 + h * 9 + (x - x0) * 0.55) * (0.6 + 0.4 * sin(t * 1.3 + h * 5))
      line(x - 2, py, x, ny)
      py = ny
    }
  } else if (kind == 4 && y1 - y0 >= 5 && gridWidth() > 0) {   // counter
    if (h > 0.5) amber(0.9)
    else cyan(0.9)
    drawNumber(floor(hash2(s, floor(t * (1 + 2 * h))) * 1000), round(x0), round(mid - 2.5), min(3, floor((w + 2) / 4)), 0)
  } else {                                           // level meter
    for (x = x0; x <= x1; x += 2) {
      var lv = 0.5 + 0.5 * sin(t * (3 + h * 4) + x * 1.7) * sin(t * 2.1 + x * 0.6)
      var top = y1 - (y1 - y0) * lv
      cyan(0.5 + 0.4 * lv)
      rect(x, top, x, y1)
    }
  }
}

export function renderBulk() {
  clear()
  if (!has2DMap()) { renderStrip(); return }
  gw = gridWidth()
  gh = gridHeight()
  if (gw == 0) { gw = 32; gh = 32 }
  kx = 1 / max(gw - 1, 1)
  ky = 1 / max(gh - 1, 1)
  var nl = ceil(gh / lane)
  for (var l = 0; l < nl; l++) {
    var dir = l % 2 ? -1 : 1
    var off = dir * scroll * tier(l)
    var y0 = l * lane
    for (var s = floor(-off / seg) - 1; s * seg + off < gw; s++) {
      var x0 = s * seg + off
      var h = hash2(l * 17 + 3, s)
      oklch(0.28, 0.03, 0.6)                       // dim divider tick
      rect(x0, y0 + 1, x0, y0 + lane - 2)
      widget(floor(h * 5.999), x0 + 2, y0 + 1, x0 + seg - 2, y0 + lane - 2, frac(h * 7), s * 13 + l, dir)
    }
  }
}

// the strip: one lane of 1-D widgets
function renderStrip() {
  var n = pixelCount
  for (var s = floor(-scroll / seg) - 1; s * seg + scroll < n; s++) {
    var x0 = floor(s * seg + scroll)
    var h = hash2(3, s)
    var kind = floor(h * 4.999)
    var h2 = frac(h * 7)
    oklch(0.35, 0.03, 0.6)
    setPixel(x0)
    var a = x0 + 2, b = x0 + seg - 1
    if (kind == 0) {
      var p = easeInOutCubic(frac(t * (0.25 + 0.3 * h2) + h2))
      amber(0.25)
      fillRange(a, b)
      amber(0.85)
      fillRange(a, a + (b - a) * p)
    } else if (kind == 1) {
      for (var i = a; i < b - 1; i += 4) {
        amber(0.4 + 0.5 * triangle(frac(t * 1.5 - i / 24)))
        fillRange(i, i + 2)
      }
    } else if (kind == 2) {
      for (var j = a; j < b; j += 2) {
        var bl = hash2(s * 97 + j, floor(t * 4))
        if (bl > 0.5) { cyan(0.4 + 0.5 * bl); setPixel(j) }
      }
    } else if (kind == 3) {
      for (var k = a; k < b; k++) {
        cyan(0.2 + 0.7 * wave(t * 0.8 + (k - a) * 0.11 + h2))
        setPixel(k)
      }
    } else {
      var lv = 0.5 + 0.5 * sin(t * (3 + h2 * 4)) * sin(t * 2.1 + h2 * 6)
      cyan(0.85)
      fillRange(a, a + (b - a) * lv)
    }
  }
}

// declares the strip form (no default grid on a mapless strip); renderBulk wins
export function render(index) { hsv(0.1, 1, mod(index, seg) < seg / 2 ? 0.5 : 0) }
