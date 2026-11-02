// name: Saber
// tags: both, ambient
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/saberdeploy-tutorial.md
// — reimagined, not a replica.
// An energy-blade prop. The blade ignites from the hilt with an eased surge, hums
// (slow simplex flicker plus faint light crawling along it), carries a white-hot
// core and a brighter tip, and retracts. Any injected event while lit is a clash:
// a white flare at a random point on the blade. Strip: the blade runs along the
// strip from the hilt at index 0. Panel: it stands up the centre column, glowing.

var igniteS = 0.8
//# min=0.2 max=3 step=0.1 default=0.8
export function sliderIgniteSeconds(v) { igniteS = clamp(v, 0.2, 3) }

var auto = 1           // ignite, hold, retract, rest, repeat
//# default=1
export function toggleAutoCycle(on) { auto = on ? 1 : 0; tState = 0 }

export function triggerIgniteRetract() { toggle() }

// Blade colour. The picker's brightness is ignored: use the global brightness.
var bh = 0.6, bs = 1
export function hsvPickerBlade(h, s, v) { bh = h; bs = s }

var corePct = 60       // width / whiteness of the hot core
//# min=0 max=100 step=1 default=60
export function sliderCorePercent(v) { corePct = clamp(v, 0, 100) }

var humPct = 30        // depth of the hum flicker
//# min=0 max=100 step=1 default=30
export function sliderHumPercent(v) { humPct = clamp(v, 0, 100) }

var phase = 1          // 0 off, 1 igniting, 2 lit, 3 retracting
var ext = 0, u = 0, tState = 0, t = 0, clashT = 9, clashP = 0.5, clashDone = 0
var hum = 1, crawl1 = 0, crawl2 = 0
var ev = array(4)

function toggle() {
  phase = phase == 1 || phase == 2 ? 3 : 1   // u carries over: a reversal starts where the blade is
  tState = 0
}
function clash() { clashT = 0; clashP = 0.25 + random(0.7) }

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  t = mod(t + dt, 3000)
  while (readEvent(ev)) { if (phase == 1 || phase == 2) clash(); else toggle() }
  tState += dt
  clashT += dt
  if (phase == 1) {
    u = min(u + dt / igniteS, 1)
    ext = easeOutCubic(u)
    if (u >= 1) { phase = 2; tState = 0; clashDone = 0 }
  } else if (phase == 3) {
    u = max(u - dt / (igniteS * 0.8), 0)
    ext = easeOutCubic(u)                    // the same curve backwards: it snaps home at the end
    if (u <= 0) { phase = 0; ext = 0; tState = 0 }
  }
  if (auto) {
    if (phase == 2 && !clashDone && tState > 3.5) { clash(); clashDone = 1 }
    if (phase == 2 && tState > 7) toggle()
    else if (phase == 0 && tState > 0.8) toggle()
  }
  var hd = humPct / 100
  hum = 1 - hd * (0.22 + 0.18 * simplex2(t * 1.3, 3.7) + 0.08 * simplex2(t * 7, 9.1))
  crawl1 = mod(t * 0.37, 1)
  crawl2 = mod(0.5 - t * 0.23, 1)
}

// A mapless strip stays a strip: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var lit = 0
function flare() { return clashT < 0.35 ? 1 - clashT / 0.35 : 0 }

// --- strip ------------------------------------------------------------------
function strip() {
  var n = pixelCount, hp = max(2, round(n * 0.05))
  var L = (n - hp) * ext, tip = hp + L, c = corePct / 100, f = flare()
  clear()
  for (var i = 0; i < hp; i++) {                         // the hilt, ridged metal
    var g = i % 2 ? 0.05 : 0.1
    rgb(g, g, g * 1.15)
    setPixel(i)
  }
  if (ext <= 0) { hsv(0, 0.9, 0.15); setPixel(1); return }
  var v = hum * (1 - 0.15 * f)
  gradientRange(hp, tip, bh, bs * (1 - 0.35 * c), v * 0.9, bh, bs * (1 - 0.75 * c), v, 0)
  var cv = 0.25 * humPct / 100 + 0.08
  hsv(bh, bs * 0.3, cv)
  splatIndex(hp + L * crawl1, max(2, n * 0.03), 1)
  splatIndex(hp + L * crawl2, max(2, n * 0.02), 1)
  hsv(bh, bs * 0.25, phase == 1 ? 1 : 0.8)               // brighter tip, a surge while igniting
  splatIndex(tip - 0.5, phase == 1 ? 3 : 1.6, 2)
  if (f > 0) {
    hsv(bh, 0.1, f)
    splatIndex(hp + (n - hp) * clashP * ext, (2 + 6 * f) * max(1, n / 120), 1)
  }
}

// --- panel ------------------------------------------------------------------
function panel() {
  var gw = max(gridWidth(), 8), cx = 0.5, px1 = 1 / gw
  var yH = 0.8, yT = yH - 0.74 * ext, c = corePct / 100, f = flare()
  clear()
  if (ext > 0) {
    var v = hum * (1 + 0.3 * f)
    hsv(bh, bs, 0.32 * v)                                 // light on the wall
    drawLine(cx, yH, cx, yT, max(0.3, 6 * px1), 1)
    hsv(bh, bs, 0.9 * v)                                  // the blade
    drawLine(cx, yH, cx, yT, max(0.075, 2.4 * px1), 1)
    hsv(bh, bs * (1 - c), 0.5 + 0.5 * c)                  // the white-hot core
    drawLine(cx, yH, cx, yT, max(0.012 + 0.03 * c, 1.1 * px1), 1)
    hsv(bh, 0.3, 0.12 + 0.2 * humPct / 100)               // hum crawling along it
    splat(cx, yH - (yH - yT) * crawl1, 0.07, 1)
    splat(cx, yH - (yH - yT) * crawl2, 0.05, 1)
    hsv(bh, 0.3, phase == 1 ? 1 : 0.6)                    // the tip
    splat(cx, yT, phase == 1 ? 0.09 : 0.05, 1)
    if (f > 0) {
      var y = yH - (yH - yT) * clashP, r = 0.06 + 0.16 * (1 - f)
      hsv(bh, 0.15, f)
      splat(cx, y, 0.06 + 0.12 * f, 1)
      hsv(0.12, 0.5, f * 0.8)                             // sparks
      drawLine(cx - r, y - r * 0.6, cx - r * 0.4, y - r * 0.25, 0.02, 1)
      drawLine(cx + r, y - r * 0.7, cx + r * 0.4, y - r * 0.3, 0.02, 1)
      drawLine(cx - r * 0.9, y + r * 0.5, cx - r * 0.35, y + r * 0.2, 0.02, 1)
      drawLine(cx + r * 0.8, y + r * 0.6, cx + r * 0.3, y + r * 0.25, 0.02, 1)
    }
  }
  var hw = max(0.045, 1.5 * px1)                          // the hilt
  rgb(0.2, 0.2, 0.23)
  fillRect(cx - hw, yH, cx + hw, 0.98)
  rgb(0.05, 0.05, 0.06)
  for (var k = 0; k < 3; k++) fillRect(cx - hw, 0.86 + k * 0.035, cx + hw, 0.865 + k * 0.035)
  rgb(0.35, 0.35, 0.38)                                   // emitter collar
  fillRect(cx - hw * 1.3, yH, cx + hw * 1.3, yH + 0.025)
  if (ext > 0) hsv(bh, bs, 0.8)
  else hsv(0, 0.9, 0.5)                                   // the activation stud
  fillRect(cx + hw * 0.2, 0.92, cx + hw, 0.94)
}

export function renderBulk() {
  if (has2DMap()) panel()
  else strip()
}
