// name: Candle
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/blue-holiday-candle-2d.md
// — reimagined, not a replica.
// A lamp candle. Panel: a teardrop flame (round base, sharp tip, blue root,
// white-hot core) sheared by a noise-driven sway, a warm halo lighting the
// wall and the wax, and a rare gutter that shrinks the flame before it
// recovers; the flame is evaluated only inside its own bounding box, the halo
// is two splats. Strip: tealights — every group of LEDs is its own candle,
// flickering on low-frequency simplex noise that neighbours share like a draft.

var palette = 0         // 0 warm, 1 icy holiday blue, 2 picked colour
//# min=0 max=2 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 2); buildPal() }

var groupPx = 3         // strip: LEDs per tealight
//# min=1 max=20 step=1 default=3
export function sliderCandlePixels(v) { groupPx = clamp(floor(v), 1, 20) }

var draftPct = 30       // how hard the air moves the flame
//# min=0 max=100 step=1 default=30
export function sliderDraftPercent(v) { draftPct = clamp(v, 0, 100) }

// Flame colour for Palette 2: hue and saturation (the picker's brightness is ignored).
var pickH = 0.85, pickS = 0.8
export function hsvPickerColor(h, s, v) { pickH = h; pickS = s; buildPal() }

// flame ramp: 4 stops at heat 0, 0.4, 0.75, 1; plus root, halo, wax, wall
var PR = array(4), PG = array(4), PB = array(4), tmp = array(3)
var rootR = 0, rootG = 0, rootB = 0, haR = 0, haG = 0, haB = 0
var wxR = 0, wxG = 0, wxB = 0, bgR = 0, bgG = 0, bgB = 0
function stop(i, r, g, b) { PR[i] = r; PG[i] = g; PB[i] = b }
function buildPal() {
  if (palette == 0) {
    stop(0, 0.45, 0.04, 0); stop(1, 1, 0.32, 0.02); stop(2, 1, 0.72, 0.22); stop(3, 1, 0.96, 0.82)
    rootR = 0.08; rootG = 0.16; rootB = 0.9; haR = 1; haG = 0.42; haB = 0.08
    wxR = 0.95; wxG = 0.8; wxB = 0.6; bgR = 0.018; bgG = 0.007; bgB = 0.002
  } else if (palette == 1) {
    stop(0, 0.03, 0.08, 0.55); stop(1, 0.12, 0.42, 1); stop(2, 0.5, 0.82, 1); stop(3, 0.94, 0.98, 1)
    rootR = 0.45; rootG = 0.12; rootB = 1; haR = 0.25; haG = 0.55; haB = 1
    wxR = 0.45; wxG = 0.62; wxB = 1; bgR = 0.006; bgG = 0.004; bgB = 0.02
  } else {
    hsv2rgb(pickH, pickS, 0.45, tmp); stop(0, tmp[0], tmp[1], tmp[2])
    hsv2rgb(pickH, pickS, 1, tmp); stop(1, tmp[0], tmp[1], tmp[2]); haR = tmp[0]; haG = tmp[1]; haB = tmp[2]
    hsv2rgb(pickH, pickS * 0.45, 1, tmp); stop(2, tmp[0], tmp[1], tmp[2])
    stop(3, 1, 0.97, 0.92)
    rootR = 0.1; rootG = 0.2; rootB = 0.9
    hsv2rgb(pickH, pickS * 0.25, 0.9, tmp); wxR = tmp[0]; wxG = tmp[1]; wxB = tmp[2]
    hsv2rgb(pickH, pickS, 0.015, tmp); bgR = tmp[0]; bgG = tmp[1]; bgB = tmp[2]
  }
}
buildPal()

var cr = 0, cg = 0, cb = 0
function ramp(h) {              // cr/cg/cb <- flame colour at heat h (0..1)
  var i = h < 0.4 ? 0 : h < 0.75 ? 1 : 2
  var t = i == 0 ? h / 0.4 : i == 1 ? (h - 0.4) / 0.35 : (h - 0.75) / 0.25
  t = clamp(t, 0, 1)
  cr = mix(PR[i], PR[i + 1], t); cg = mix(PG[i], PG[i + 1], t); cb = mix(PB[i], PB[i + 1], t)
}

// --- flicker (shared by both forms) ---------------------------------------
var t = random(100), gut = 0, sway = 0, lick = 0, hmul = 1, fbri = 1
export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000, dr = draftPct / 100
  t = mod(t + dt, 3000)
  var n1 = simplex2(t * 0.55, 1.7), n2 = simplex2(t * 2.1, 5.1), n3 = simplex2(t * 6.5, 9.3)
  // a rare gutter: the flame ducks and dims, then recovers over ~2 s
  if (random(1) < dt * (0.015 + 0.06 * dr)) gut = 1
  gut = gut * exp(-dt * 1.1)
  sway = (0.1 * dr + 0.012) * n1 + 0.03 * dr * n3 + 0.05 * gut * n3
  lick = 0.04 * (0.3 + dr) * n3
  hmul = 1 + 0.16 * dr * n2 + 0.03 * n3 - 0.45 * gut
  fbri = 0.92 + 0.08 * n2 * (0.4 + dr) - 0.3 * gut
}

// A mapless strip stays a strip: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

// --- strip: tealights -------------------------------------------------------
function tealights() {
  var dr = 0.25 + draftPct / 100 * 0.75, n = ceil(pixelCount / groupPx)
  for (var j = 0; j < n; j++) {
    var f = 0.78 + 0.3 * dr * simplex2(t * 0.9, j * 0.21) + 0.22 * dr * simplex2(t * 4.1, j * 1.7 + 40)
    f = clamp(f, 0.2, 1.05)
    ramp(0.2 + 0.6 * f)
    var a = j * groupPx, b = min(a + groupPx, pixelCount)
    if (groupPx >= 3) {
      rgb(cr * f * 0.3, cg * f * 0.22, cb * f * 0.2)        // the glass rim
      fillRange(a, b)
      rgb(cr * f, cg * f, cb * f)
      fillRange(a + floor((groupPx - 1) / 2), a + ceil((groupPx + 1) / 2))
    } else {
      rgb(cr * f, cg * f, cb * f)
      fillRange(a, b)
    }
  }
}

// --- panel: one candle ------------------------------------------------------
var gw = 0, gh = 0
function haloAt(x, y, hx, hy, r, k) { var d = 1 - hypot(x - hx, y - hy) / r; return d > 0 ? d * k : 0 }

function panel() {
  gw = gridWidth()
  gh = gridHeight()
  var pw = 1 / max(gw - 1, 1), ph = 1 / max(gh - 1, 1)
  var asp = min(gw, gh) / max(gw, 1)                          // x widths stay square
  var cx = 0.5, yT = 0.62, bw = max(0.15 * asp, 2 * pw)      // wax top, half width
  var yB = yT - max(0.025, ph), Hf = 0.36 * hmul, R = max(0.075 * asp, 1.6 * pw)
  var sw = sway * asp, lk = lick * asp
  var fx = cx + sw * 0.35, fy = yB - Hf * 0.38               // halo centre
  var k1 = 0.26 * fbri, k2 = 0.38 * fbri

  rgb(bgR, bgG, bgB)
  fill()
  if (palette == 1) {                                         // a few stars for the holiday look
    for (var s = 0; s < 7; s++) {
      var v = 0.25 * triangle(t * (0.3 + hash(s) * 0.4) + hash(s + 9)), sx = hash(s * 3.7 + 1), sy = hash(s * 5.3 + 2) * 0.55
      rgb(v, v * 0.95, v * 0.8)
      setPixel(gridIndex(round(sx * (gw - 1)), round(sy * (gh - 1))))
    }
  }
  rgb(haR * k1, haG * k1, haB * k1)
  splat(fx, fy, 0.6, 1)
  rgb(haR * k2, haG * k2, haB * k2)
  splat(fx, fy, 0.22, 1)

  // the wax body, lit through from the flame: bands fading down from the rim
  var bands = min(12, max(round((1 - yT) / ph), 1)), bh = (1 - yT) / bands
  for (var i = 0; i < bands; i++) {
    var lit = 0.08 + 0.5 * fbri * (1 - i / bands) * (1 - i / bands)
    rgb(mix(wxR, haR, 0.3) * lit, mix(wxG, haG, 0.3) * lit, mix(wxB, haB, 0.3) * lit)
    fillRect(cx - bw, yT + i * bh, cx + bw, yT + (i + 1) * bh)
  }
  rgb(wxR * 0.7 * fbri, wxG * 0.7 * fbri, wxB * 0.7 * fbri)   // the rim
  fillRect(cx - bw, yT, cx + bw, yT + ph * 0.5)

  // the flame, only inside its bounding box, added onto wall + halo
  var reachX = R * 1.15 + abs(sw) + abs(lk)
  var c0 = max(floor((cx - reachX) / pw), 0), c1 = min(ceil((cx + reachX) / pw), gw - 1)
  var r0 = max(floor((yB - Hf) / ph), 0), r1 = min(floor((yT - 0.001) / ph), gh - 1)
  var soft = 0.9 / (R / pw)
  for (var r = r0; r <= r1; r++) {
    var y = r * ph, q = (yB - y) / Hf
    if (q < -0.07 || q > 1) continue
    var qq = q + 0.07, w = R * sqrt(qq) * (1 - q) * 2.4
    var xo = cx + sw * q * q + lk * q * q * q
    var root = clamp(1 - q / 0.22, 0, 1)
    for (var c = c0; c <= c1; c++) {
      var x = c * pw, m = abs(x - xo) / w
      if (m >= 1) continue
      var I = clamp((1 - m) / soft, 0, 1) * fbri
      ramp((1.08 - m * 0.75) * (1 - q * 0.5))
      var ra = root * clamp(m * 1.8 - 0.15, 0, 0.85)
      var h = haloAt(x, y, fx, fy, 0.6, k1) + haloAt(x, y, fx, fy, 0.22, k2)
      rgb(bgR + haR * h + mix(cr, rootR, ra) * I, bgG + haG * h + mix(cg, rootG, ra) * I, bgB + haB * h + mix(cb, rootB, ra) * I)
      setPixel(gridIndex(c, r))
    }
  }
  // the wick, with a glowing ember at its tip
  var wc = round(cx / pw), wr = round(yT / ph) - 1
  rgb(0.04, 0.02, 0.01)
  setPixel(gridIndex(wc, wr))
  rgb(0.5 * fbri, 0.12 * fbri, 0.02)
  setPixel(gridIndex(wc, wr - 1))
}

export function renderBulk() {
  if (has2DMap() && gridWidth() > 0) panel()
  else tealights()
}
