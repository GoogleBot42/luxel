// name: Rainbow
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/rainbow.md,
// rainbow-v2.md, rainbow-fonts.md, regenbogendrogen.md, rainbow-pinwheel.md,
// chasing-rainbows-hsluv.md, ryb-colors.md — reimagined, not a replica.
// The canonical moving rainbow for strips and panels. Shapes: linear at any
// angle, chevron, radial rings, conic pinwheel, mirror fold. Warp bends the
// bands until they fold back and breathe. Wheels: HSV, OKLCH (even brightness)
// or the painter's RYB wheel, each baked once into a 64-entry table. Panels
// paint a static shape canvas through a 48-stop palette rotated each frame;
// strips add up gradientRange spans. No per-pixel pattern code anywhere.

var cycleS = 10
// Seconds for the colours to travel one whole rainbow.
//# min=1 max=120 step=1 default=10
export function sliderCycleSeconds(v) { cycleS = clamp(v, 1, 120) }

var reps = 1
// Rainbows across the strip or panel (whole turns only for the pinwheel).
//# min=0.25 max=8 step=0.25 default=1
export function sliderRepeats(v) { reps = clamp(v, 0.25, 8); dirty = 1 }

var sgn = -1
//# default=0
export function toggleReverse(on) { sgn = on > 0.5 ? 1 : -1 }

// 0 linear, 1 chevron, 2 radial rings, 3 conic pinwheel, 4 mirror fold.
// On a strip, chevron/rings/mirror fold about the middle.
var shape = 0
//# min=0 max=4 step=1 default=0
export function sliderShape(v) { shape = clamp(floor(v), 0, 4); dirty = 1 }

var angle = 0
// Direction of the bands on a panel (and where the pinwheel starts).
//# min=0 max=359 step=1 default=0
export function sliderAngleDegrees(v) { angle = clamp(v, 0, 359); dirty = 1 }

var warp = 0
// Folds the hue ramp so bands squeeze, stretch and double back.
//# min=0 max=100 step=1 default=0
export function sliderWarpPercent(v) { warp = clamp(v, 0, 100) / 100 }

var sat = 1
//# min=0 max=100 step=1 default=100
export function sliderSaturationPercent(v) { sat = clamp(v, 0, 100) / 100; bake() }

// 0 HSV, 1 OKLCH (even brightness), 2 painter's RYB wheel
var wheel = 0
//# min=0 max=2 step=1 default=0
export function sliderWheel(v) { wheel = clamp(floor(v), 0, 2); bake() }

// ---- the wheel, as RGB (palette) and unwrapped HSV (strip spans) tables ----
var TN = 64
var tr = array(65), tg = array(65), tb = array(65)
var th = array(65), ts = array(65), tv = array(65)
var c3 = array(3)

function lin2srgb(c) {
  c = clamp(c, 0, 1)
  return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 0.41667) - 0.055
}
function oklchRGB(L, C, h) {
  var a = C * cos(h * PI2), b = C * sin(h * PI2)
  var l = L + 0.39634 * a + 0.21580 * b
  var m = L - 0.10556 * a - 0.06385 * b
  var s = L - 0.08948 * a - 1.29149 * b
  l = l * l * l; m = m * m * m; s = s * s * s
  c3[0] = lin2srgb(4.07674 * l - 3.30771 * m + 0.23097 * s)
  c3[1] = lin2srgb(-1.26844 * l + 2.60976 * m - 0.34132 * s)
  c3[2] = lin2srgb(-0.00420 * l - 0.70342 * m + 1.70761 * s)
}
// painter's wheel: HSL-style sextants read as R/Y/B paint, mixed over the
// eased RYB cube (white, red, yellow, blue, orange, green, purple, black)
function sext(p, q, t) {
  t = mod(t, 1)
  if (t < 1 / 6) return p + (q - p) * 6 * t
  if (t < 0.5) return q
  if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6
  return p
}
function rybRGB(h) {
  var q = 0.5 + 0.5 * sat, p = 1 - q
  var r = sext(p, q, h + 1 / 3), y = sext(p, q, h), b = sext(p, q, h - 1 / 3)
  r = smoothstep(0, 1, r * r); y = smoothstep(0, 1, y * y); b = smoothstep(0, 1, b * b)
  var ir = 1 - r, iy = 1 - y, ib = 1 - b
  c3[0] = ib + 0.5 * r * iy * b
  c3[1] = ir * ib + 0.5 * r * y * ib + ir * y * b
  c3[2] = ir * iy + r * iy * b
}

function bake() {
  for (var k = 0; k <= TN; k++) {
    var h = k / TN
    if (wheel == 1) oklchRGB(0.75, 0.15 * sat, h)
    else if (wheel == 2) rybRGB(h)
    else hsv2rgb(h, sat, 1, c3)
    tr[k] = c3[0]; tg[k] = c3[1]; tb[k] = c3[2]
    rgb2hsv(c3[0], c3[1], c3[2], c3)
    var hh = c3[0]
    if (k > 0) hh += round(th[k - 1] - hh)
    th[k] = hh; ts[k] = c3[1]; tv[k] = c3[2]
  }
}
bake()

// ---- motion ----
var phase = 0, wph = 0, aW = 0
export function beforeRender(delta) {
  var dt = min(delta, 200) / 1000
  phase = mod(phase + dt / cycleS, 1)
  wph = mod(wph + dt / (cycleS * 2.7), 1)
  aW = warp * 0.3
}

// hue (in turns) at rainbow coordinate x; the warp is periodic in x
function hueAt(x) { return x + sgn * phase + aW * sin(PI2 * (x + wph)) }

// ---- panel: static shape canvas + a palette that carries the motion ----
var K = 48
var pal = array(K * 4)
for (var k = 0; k < K; k++) pal[k * 4] = k / (K - 1)
setPalette(pal)

var cv = array(4096), CW = 0, CH = 0, dirty = 1, lastW = -1, lastH = -1

function buildCanvas(gw, gh) {
  if (gw < 1 || gh < 1) { gw = 32; gh = 32 }
  CW = min(gw, 64); CH = min(gh, 64)
  var m = max(gw, gh), ax = gw / m, ay = gh / m
  var ang = angle / 360 * PI2, co = cos(ang), si = sin(ang)
  var e = abs(co) * ax + abs(si) * ay
  var R = shape == 3 ? max(1, round(reps)) : reps
  for (var cy = 0; cy < CH; cy++) {
    var Y = ((cy + 0.5) / CH - 0.5) * ay
    for (var cx = 0; cx < CW; cx++) {
      var X = ((cx + 0.5) / CW - 0.5) * ax
      var a = (X * co + Y * si) / e, c = a + 0.5
      if (shape == 1) c = a + abs(Y * co - X * si) / e + 0.25
      else if (shape == 2) c = sqrt(X * X + Y * Y) * 1.4142
      else if (shape == 3) c = atan2(Y, X) / PI2 + 0.5 - angle / 360
      else if (shape == 4) c = abs(2 * a)
      cv[cy * CW + cx] = mod(c * R, 1)
    }
  }
  dirty = 0
}

function panel() {
  var gw = gridWidth(), gh = gridHeight()
  resetTransform()
  if (shape == 0 && wheel == 0 && warp == 0) {
    // plain linear HSV: one fillGradient through a rotated frame
    var m = max(max(gw, gh), 1), ax = gw > 0 ? gw / m : 1, ay = gh > 0 ? gh / m : 1
    var ang = angle / 360 * PI2, co = cos(ang), si = sin(ang)
    var e = abs(co) * ax + abs(si) * ay
    translate(-0.5, -0.5)
    scale(ax, ay)
    rotate(-ang)
    scale(1 / e, 1 / e)
    translate(0.5, 0.5)
    fillGradient(sgn * phase, sat, 1, reps + sgn * phase, sat, 1, 1)
    resetTransform()
    return
  }
  if (dirty || gw != lastW || gh != lastH) { buildCanvas(gw, gh); lastW = gw; lastH = gh }
  for (var k = 0; k < K; k++) {
    var u = k / (K - 1), H = hueAt(u)
    var f = mod(H, 1) * TN, i = floor(f)
    f -= i
    var j = k * 4
    pal[j + 1] = tr[i] + (tr[i + 1] - tr[i]) * f
    pal[j + 2] = tg[i] + (tg[i + 1] - tg[i]) * f
    pal[j + 3] = tb[i] + (tb[i + 1] - tb[i]) * f
  }
  // bilinear only when the panel is bigger than the 64-cell canvas
  paintCanvas(cv, CW, CH, 1, gw > CW || gh > CH ? 1 : 0)
}

// ---- strip: gradientRange spans along the index ----
var sh = 0, ss = 0, sv = 0
function sampleHSV(H) {
  var fl = floor(H), f = (H - fl) * TN, i = floor(f)
  f -= i
  sh = fl + th[i] + (th[i + 1] - th[i]) * f
  ss = ts[i] + (ts[i + 1] - ts[i]) * f
  sv = tv[i] + (tv[i + 1] - tv[i]) * f
}

function strip() {
  var n = pixelCount
  var fold = shape == 1 || shape == 2 || shape == 4
  var len = fold ? n / 2 : n
  var R = shape == 3 ? max(1, round(reps)) : reps
  var segs = 1
  if (wheel > 0 || warp > 0) segs = clamp(ceil(R * (32 + 32 * warp)), 4, max(4, floor(len / 3)))
  clear()   // spans add (mode 1), so a shared end pixel sums to full
  sampleHSV(hueAt(0))
  var h0 = sh, s0 = ss, v0 = sv
  for (var j = 1; j <= segs; j++) {
    sampleHSV(hueAt(R * j / segs))
    var p0 = len * (j - 1) / segs, p1 = len * j / segs
    if (fold) {
      gradientRange(len + p0, len + p1, h0, s0, v0, sh, ss, sv, 1)
      gradientRange(len - p0, len - p1, h0, s0, v0, sh, ss, sv, 1)
    } else gradientRange(p0, p1, h0, s0, v0, sh, ss, sv, 1)
    h0 = sh; s0 = ss; v0 = sv
  }
}

export function renderBulk() {
  if (has2DMap()) panel()
  else strip()
}

// A per-pixel fallback for other engines; on Luxel it also keeps a mapless
// strip mapless (renderBulk always wins).
export function render(index) { hsv(index / pixelCount + sgn * phase, sat, 1) }
