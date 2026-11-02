// name: Color Wash
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/1-white-fade.md,
// 2-purple-fade.md, complements-3d.md, gradient-blue-purple-pink.md,
// twocolorhsvmix.md, color-blend.md — reimagined, not a replica.
// One to three colours blended in OKLab into a seamless loop that breathes,
// flows along the fixture, or both — or, in Waves, each colour rides its own
// travelling wave and they mix like light. The loop is cooked into 48 OKLab
// sub-stops only when a colour changes; a frame is a few gradientRange spans
// on a strip and one paintCanvas row on a panel. A scene sky: `P y`.

var hA = 0.07, sA = 1, hB = 0.83, sB = 0.9, hC = 0.55, sC = 1
// The brightness (V) of each picker is ignored; use the global brightness.
export function hsvPickerColorA(h, s, v) { hA = h; sA = s; cook() }
export function hsvPickerColorB(h, s, v) { hB = h; sB = s; cook() }
export function hsvPickerColorC(h, s, v) { hC = h; sC = s; cook() }

var stops = 2
//# min=1 max=3 step=1 default=2
export function sliderStops(v) { stops = clamp(floor(v), 1, 3); cook() }

// 0 the pickers, 1 A + its complement, 2 triad of A, 3 blue-purple-pink
var scheme = 3
//# min=0 max=3 step=1 default=3
export function sliderScheme(v) { scheme = clamp(floor(v), 0, 3); cook() }

// 0 breathe, 1 flow, 2 breathe + flow, 3 waves
var motion = 2
//# min=0 max=3 step=1 default=2
export function sliderMotion(v) { motion = clamp(floor(v), 0, 3) }

var breathS = 8
//# min=2 max=60 step=1 default=8
export function sliderBreathSeconds(v) { breathS = clamp(v, 2, 60) }

// The breath never dips below this share of full brightness.
var floorP = 0.25
//# min=5 max=80 step=1 default=25
export function sliderFloorPercent(v) { floorP = clamp(v, 5, 80) / 100 }

// Seconds for the colours to flow one loop along the fixture.
var flowS = 20
//# min=2 max=120 step=1 default=20
export function sliderFlowSeconds(v) { flowS = clamp(v, 2, 120) }

var reps = 1
//# min=0.5 max=6 step=0.5 default=1
export function sliderRepeats(v) { reps = clamp(v, 0.5, 6) }

// Panels only: 0 along the wiring, 1 across (x), 2 down (y). Strips use the index.
var axis = 2
//# min=0 max=2 step=1 default=2
export function sliderAxis(v) { axis = clamp(floor(v), 0, 2) }

// Waves: wavelengths per second of the first wave (the others run at fixed ratios).
var waveSpd = 0.3
//# min=-2 max=2 step=0.05 default=0.3
export function sliderWaveSpeed(v) { waveSpd = clamp(v, -2, 2) }

// ---- the cooked loop: 48 OKLab sub-stops, as a palette and as HSV ----
var N = 48
var cr = array(3), cg = array(3), cb = array(3), c3 = array(3)
var pal = array((N + 1) * 4)
var subH = array(N + 1), subS = array(N + 1), subV = array(N + 1)

function setCol(i, h, s) { hsv2rgb(h, s, 1, c3); cr[i] = c3[0]; cg[i] = c3[1]; cb[i] = c3[2] }

function cook() {
  if (scheme == 0) { setCol(0, hA, sA); setCol(1, hB, sB); setCol(2, hC, sC) }
  else if (scheme == 1) {
    setCol(0, hA, sA)
    if (stops < 3) setCol(1, hA + 0.5, sA)
    else { setCol(1, hA + 0.44, sA); setCol(2, hA + 0.56, sA) }
  } else if (scheme == 2) { setCol(0, hA, sA); setCol(1, hA + 1 / 3, sA); setCol(2, hA + 2 / 3, sA) }
  else { setCol(0, 0.62, 0.9); setCol(1, 0.9, 0.75); setCol(2, 0.76, 0.9) }
  for (var k = 0; k <= N; k++) {
    var u = k / N * stops, i = min(floor(u), stops - 1), j = (i + 1) % stops
    mixColors(cr[i], cg[i], cb[i], cr[j], cg[j], cb[j], u - i, c3)
    pal[k * 4] = k / N
    pal[k * 4 + 1] = c3[0]; pal[k * 4 + 2] = c3[1]; pal[k * 4 + 3] = c3[2]
    rgb2hsv(c3[0], c3[1], c3[2], c3)
    var h = c3[0]
    if (k > 0) h += round(subH[k - 1] - h)
    subH[k] = h; subS[k] = c3[1]; subV[k] = c3[2]
  }
}
cook()
setPalette(pal)

// ---- clocks ----
var bph = 0, fph = 0, br = 1, wp0 = 0, wp1 = 0, wp2 = 0
export function beforeRender(delta) {
  var dt = min(delta, 200) / 1000
  bph = mod(bph + dt / breathS, 1)
  fph = mod(fph + dt / flowS, 1)
  wp0 = mod(wp0 - dt * waveSpd, 1)
  wp1 = mod(wp1 + dt * waveSpd * 0.77, 1)
  wp2 = mod(wp2 - dt * waveSpd * 1.31, 1)
  br = 1
  if (motion == 0 || motion == 2) {
    var e = 1 - easeInOutSine(triangle(bph))   // eased out: lingers near full
    br = floorP + (1 - floorP) * (1 - e * e)
  }
}

// ---- gradient motions ----
var pos = array(128)

function stripLoop() {
  var n = pixelCount, L = n / reps, st = 1
  while (st < 24 && L * st / N < 2) st = st == 4 ? 6 : st == 8 ? 12 : st * 2
  var o = motion == 0 ? 0 : fph * L
  clear()
  for (var b = o - L; b < n; b += L) {
    for (var k = 0; k < N; k += st) {
      var k1 = min(k + st, N)
      gradientRange(b + k * L / N, b + k1 * L / N, subH[k], subS[k], subV[k] * br,
        subH[k1], subS[k1], subV[k1] * br, 1)
    }
  }
}

function panelLoop(gw, gh) {
  var M = axis == 1 ? gw : gh
  M = clamp(M, 2, 128)
  var o = motion == 0 ? 0 : fph
  for (var i = 0; i < M; i++) pos[i] = mod((i + 0.5) / M * reps - o, 1)
  if (axis == 1) paintCanvas(pos, M, 1, br, 0)
  else paintCanvas(pos, 1, M, br, 0)
}

// ---- waves: each colour a travelling wave, mixed additively ----
var wh = array(128), ws = array(128), wv = array(128)

function waveSample(i, M) {
  var p = i / (M - 1) * reps, r = 0, g = 0, b = 0, t = 0
  for (var k = 0; k < stops; k++) {
    var w = wave(k == 0 ? p + wp0 : k == 1 ? p * 1.5 + wp1 : p * 0.75 + wp2)
    w *= w
    r += cr[k] * w; g += cg[k] * w; b += cb[k] * w; t += w
  }
  // overlaps mix their colours instead of piling up to white
  t = max(t, 1)
  rgb2hsv(r / t, g / t, b / t, c3)
  var h = c3[0]
  if (i > 0) h += round(wh[i - 1] - h)
  wh[i] = h; ws[i] = c3[1]; wv[i] = c3[2]
}

function waves(strip, gw, gh) {
  var M = strip ? clamp(floor(pixelCount / 3), 2, 64) : clamp(axis == 1 ? gw : gh, 2, 128)
  for (var i = 0; i < M; i++) waveSample(i, M)
  if (strip) {
    var n = pixelCount
    clear()
    for (var i = 1; i < M; i++)
      gradientRange((i - 1) * n / (M - 1), i * n / (M - 1), wh[i - 1], ws[i - 1], wv[i - 1],
        wh[i], ws[i], wv[i], 1)
  } else if (axis == 1) fillCanvas(wh, ws, wv, M, 1, 0)
  else fillCanvas(wh, ws, wv, 1, M, 0)
}

export function renderBulk() {
  var gw = gridWidth(), gh = gridHeight()
  if (gw < 1 || gh < 1) { gw = 32; gh = 32 }
  var strip = !has2DMap() || axis == 0
  if (motion == 3) waves(strip, gw, gh)
  else if (strip) stripLoop()
  else panelLoop(gw, gh)
}

// Per-pixel fallback for other engines; on Luxel it keeps a mapless strip
// mapless (renderBulk always wins).
export function render(index) { paint(mod(index / pixelCount * reps - fph, 1), br) }
