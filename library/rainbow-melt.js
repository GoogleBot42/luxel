// name: Rainbow Melt
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/marching-rainbow.md,
// marching-rainbow-buffered.md (and the "rainbow melt" description) — reimagined, not a replica.
// Melt: a mirrored rainbow slowly rotates while its brightness churns in fluid
// folds (a sinusoid nested in itself, two near periods). March: broad bands
// march one way while a fine ripple runs back through them, eased instead of
// clipped. Colours come from an OKLCH-balanced wheel so blue never sags. The
// look is a function of one coordinate: a strip draws it as gradientRange
// spans; a panel unrolls its recent history across the other axis on a small
// canvas (paintCanvas, bilinear), so the melt drips down the panel.

var meltS = 6.5
// Seconds for one churn of the melt (or one march along the fixture).
//# min=1 max=30 step=0.5 default=6.5
export function sliderMeltSeconds(v) { meltS = clamp(v, 1, 30) }

var colorS = 8.5
//# min=1 max=60 step=0.5 default=8.5
export function sliderColorCycleSeconds(v) { colorS = clamp(v, 1, 60) }

var reps = 1
//# min=1 max=8 step=1 default=1
export function sliderRepeats(v) { reps = clamp(floor(v), 1, 8) }

var spread = 1
// Share of the colour wheel one repeat spans.
//# min=0 max=100 step=1 default=100
export function sliderColorSpreadPercent(v) { spread = clamp(v, 0, 100) / 100 }

// 0 melt, 1 march
var motion = 0
//# min=0 max=1 step=1 default=0
export function sliderMotion(v) { motion = clamp(floor(v), 0, 1) }

var ripples = 10
// March: ripple crests along the fixture.
//# min=2 max=30 step=1 default=10
export function sliderRipples(v) { ripples = clamp(floor(v), 2, 30) }

var depth = 0.8
// March: how deeply the ripple carves the bands.
//# min=0 max=100 step=5 default=80
export function sliderRippleDepthPercent(v) { depth = clamp(v, 0, 100) / 100 }

// Panels: 0 across (x), 1 down (y), 2 out from the centre (radius)
var axis = 0
//# min=0 max=2 step=1 default=0
export function sliderAxis(v) { axis = clamp(floor(v), 0, 2) }

// ---- an OKLCH-balanced wheel: RGB for the palette, unwrapped HSV for spans ----
var TN = 64
var tr = array(65), tg = array(65), tb = array(65)
var th = array(65), ts = array(65), tv = array(65)
var c3 = array(3)
function lin2srgb(c) {
  c = clamp(c, 0, 1)
  return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 0.41667) - 0.055
}
for (var k = 0; k <= TN; k++) {
  var a = 0.2 * cos(k / TN * PI2), b = 0.2 * sin(k / TN * PI2)
  var l = 0.7 + 0.39634 * a + 0.21580 * b, m = 0.7 - 0.10556 * a - 0.06385 * b
  var s = 0.7 - 0.08948 * a - 1.29149 * b
  l = l * l * l; m = m * m * m; s = s * s * s
  var r = lin2srgb(4.07674 * l - 3.30771 * m + 0.23097 * s)
  var g = lin2srgb(-1.26844 * l + 2.60976 * m - 0.34132 * s)
  var bl = lin2srgb(-0.00420 * l - 0.70342 * m + 1.70761 * s)
  var mx = max(r, max(g, bl))     // push each hue to full drive
  tr[k] = r / mx; tg[k] = g / mx; tb[k] = bl / mx
  rgb2hsv(tr[k], tg[k], tb[k], c3)
  var hh = c3[0]
  if (k > 0) hh += round(th[k - 1] - hh)
  th[k] = hh; ts[k] = c3[1]; tv[k] = c3[2]
}

// ---- motion ----
var t1 = 0, t2 = 0, t3 = 0
export function beforeRender(delta) {
  var dt = min(delta, 200) / 1000
  t1 = mod(t1 + dt / meltS, 1)
  t2 = mod(t2 + dt / colorS, 1)
  t3 = mod(t3 + 2 * dt / meltS, 1)
}

// the look at coordinate p (0..1), d melt cycles (dh colour turns) in the
// past: sets H and V
var H = 0, V = 0
function look(p, d, dh) {
  var u1 = t1 - d, u2 = t2 - dh
  if (motion == 0) {
    var c = 1 - abs(mod(2 * p * reps, 2) - 1)
    var w = wave(wave(wave(c) + u1) + u1)
    H = c * spread + u2; V = w * w
  } else {
    var q = p * reps, a = wave(u1 + q)
    V = smoothstep(0, 0.55, a - depth * wave(t3 - 2 * d - p * ripples + 0.2))
    H = (wave(a) - q) * spread + u2
  }
}

// ---- panel: the strip's own history, unrolled across the second axis ----
// Rows further from the edge show the look further in the past, so the melt
// drips down the panel (or sideways, or spirals out with the radius axis).
var pal = array(65 * 4)
for (var k = 0; k <= TN; k++) {
  pal[k * 4] = k / TN; pal[k * 4 + 1] = tr[k]; pal[k * 4 + 2] = tg[k]; pal[k * 4 + 3] = tb[k]
}
setPalette(pal)
var DLY = 0.6          // melt cycles of history across the panel
var ci = array(1024), cvv = array(1024), rr = array(1024), aa = array(1024)
var fA = array(48), fH = array(48)
var CW = 0, CH = 0, lastW = -1, lastH = -1, lastAx = -1

function panel() {
  var gw = gridWidth(), gh = gridHeight()
  if (gw < 1 || gh < 1) { gw = 32; gh = 32 }
  if (gw != lastW || gh != lastH || axis != lastAx) {
    lastW = gw; lastH = gh; lastAx = axis
    var m = max(gw, gh)
    if (axis == 2) {
      CW = min(gw, 28); CH = min(gh, 28)
      for (var y = 0; y < CH; y++) for (var x = 0; x < CW; x++) {
        var X = ((x + 0.5) / CW - 0.5) * gw / m, Y = ((y + 0.5) / CH - 0.5) * gh / m
        rr[y * CW + x] = sqrt(X * X + Y * Y) * 0.7071
        aa[y * CW + x] = atan2(Y, X) / PI2 + 0.5
      }
    } else if (axis == 0) { CW = min(gw, 48); CH = min(gh, 20) }
    else { CW = min(gw, 20); CH = min(gh, 48) }
  }
  var i = 0, NF = axis == 0 ? CW : CH, hr = meltS / colorS
  if (motion == 0 && axis < 2) {
    // melt along x or y: the per-coordinate half of the fold is shared
    for (var j = 0; j < NF; j++) {
      var c = 1 - abs(mod(2 * (j + 0.5) / NF * reps, 2) - 1)
      fA[j] = wave(c); fH[j] = c * spread + t2
    }
    for (var y = 0; y < CH; y++) for (var x = 0; x < CW; x++) {
      var j = axis == 0 ? x : y, d = axis == 0 ? y / CH * DLY : x / CW * DLY
      var u = t1 - d, w = wave(wave(fA[j] + u) + u)
      ci[i] = mod(fH[j] - d * hr, 1); cvv[i] = w * w
      i++
    }
  } else {
    for (var y = 0; y < CH; y++) for (var x = 0; x < CW; x++) {
      if (axis == 0) look((x + 0.5) / CW, y / CH * DLY, y / CH * DLY * hr)
      else if (axis == 1) look((y + 0.5) / CH, x / CW * DLY, x / CW * DLY * hr)
      else look(rr[i], aa[i], aa[i])   // whole turns round the circle: no seam
      ci[i] = mod(H, 1); cvv[i] = V
      i++
    }
  }
  paintCanvas(ci, CW, CH, cvv, 1)
}

// ---- strip: additive gradientRange spans between samples ----
var sh = 0, ss = 0, sv = 0
function sample(p) {
  look(p, 0, 0)
  var fl = floor(H), f = (H - fl) * TN, i = floor(f)
  f -= i
  sh = fl + th[i] + (th[i + 1] - th[i]) * f
  ss = ts[i] + (ts[i + 1] - ts[i]) * f
  sv = (tv[i] + (tv[i + 1] - tv[i]) * f) * V
}

function strip() {
  var n = pixelCount, M = clamp(floor(n / 3), 2, 96)
  clear()
  sample(0)
  var h0 = sh, s0 = ss, v0 = sv
  for (var i = 1; i < M; i++) {
    sample(i / (M - 1))
    gradientRange((i - 1) * n / (M - 1), i * n / (M - 1), h0, s0, v0, sh, ss, sv, 1)
    h0 = sh; s0 = ss; v0 = sv
  }
}

export function renderBulk() {
  if (has2DMap()) panel()
  else strip()
}

// Per-pixel fallback for other engines; on Luxel it keeps a mapless strip
// mapless (renderBulk always wins).
export function render(index) { look(index / pixelCount, 0, 0); hsv(H, 1, V) }
