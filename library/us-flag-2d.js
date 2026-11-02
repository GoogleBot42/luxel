// name: Waving Flag
// tags: both, holiday
// Curated original (hand-written showcase of the Luxel language/builtins);
// the Pride flag mode also carries the idea of the prose spec docs/pattern-specs/rainbow-flag.md.
// A flag waving as cloth: US (13 stripes, canton, a staggered star lattice
// sized to the panel), Pride, Progress Pride, Trans, or a custom 2-3 stripe
// flag. A sine travels from the hoist to the fly end, displacing and shading
// the cloth, both growing with the square of the distance from the pole.
// Drawn a column at a time from per-column tables: stripe runs, one blended
// row per stripe edge, and no per-pixel trig or stripe search. On a
// strip the flag lies along it (hoist at pixel 0), stripes scrolling gently:
// each stripe is a few gradientRanges (the cloth's shading lerped across them)
// meeting at sub-pixel edges found from a coarse sampling of the sway.

var MAXS = 13, MAXC = 256
var sr = array(MAXS), sg = array(MAXS), sb = array(MAXS)   // stripe colours
var kr = array(MAXS), kg = array(MAXS), kb = array(MAXS)   // ...under the US canton
var dispA = array(MAXC), lumA = array(MAXC)                 // per-column cloth

var flag = 0            // 0 US, 1 Pride, 2 Progress, 3 Trans, 4 custom
//# min=0 max=4 step=1 default=0
export function sliderFlag(v) { flag = clamp(floor(v), 0, 4); build() }

var waveSecs = 6        // seconds for one wave to cross the flag
//# min=1 max=30 step=0.5 default=6
export function sliderWaveSeconds(v) { waveSecs = clamp(v, 1, 30) }

var ripple = 4          // fly-end displacement, percent of the panel height
//# min=0 max=15 step=0.5 default=4
export function sliderRipplePercent(v) { ripple = clamp(v, 0, 15) }

var swayPx = 3          // strip only: how far the fly end slides, in pixels
//# min=0 max=12 step=0.5 default=3
export function sliderSwayPixels(v) { swayPx = clamp(v, 0, 12) }

var twinkle = 20        // star shimmer depth, percent
//# min=0 max=100 step=1 default=20
export function sliderStarTwinklePercent(v) { twinkle = clamp(v, 0, 100) }

var customN = 3         // custom flag: number of stripes
//# min=2 max=3 step=1 default=3
export function sliderCustomStripes(v) { customN = clamp(floor(v), 2, 3); build() }

// Custom flag stripe colours, top to bottom (the pickers' brightness is used).
var c1h = 0, c1s = 1, c1v = 1, c2h = 0, c2s = 0, c2v = 1, c3h = 0.36, c3s = 1, c3v = 0.8
export function hsvPickerStripe1(h, s, v) { c1h = h; c1s = s; c1v = v; build() }
export function hsvPickerStripe2(h, s, v) { c2h = h; c2s = s; c2v = v; build() }
export function hsvPickerStripe3(h, s, v) { c3h = h; c3s = s; c3v = v; build() }

// Progress chevron, hoist outward: white, pink, light blue, brown, black.
var chR = [1, 1, 0.3, 0.35, 0], chG = [1, 0.38, 0.7, 0.15, 0], chB = [1, 0.6, 1, 0.04, 0]
var CH0 = 0.2, CHW = 0.06   // white triangle reach and band width, panel heights

var n = 13, tmp = array(3)
var hh = array(MAXS), hs = array(MAXS), hv = array(MAXS)   // stripe colours as HSV
function put(k, r, g, b) { sr[k] = r; sg[k] = g; sb[k] = b }
function putHSV(k, h, s, v) { hsv2rgb(h, s, v, tmp); put(k, tmp[0], tmp[1], tmp[2]) }
function build() {
  if (flag == 0) { n = 13; for (var k = 0; k < 13; k++) put(k, 1, k % 2 ? 1 : 0.05, k % 2 ? 1 : 0.1) }
  else if (flag <= 2) {
    n = 6; put(0, 1, 0.02, 0.02); put(1, 1, 0.33, 0); put(2, 1, 0.8, 0)
    put(3, 0, 0.75, 0.12); put(4, 0.05, 0.2, 1); put(5, 0.5, 0.04, 0.75)
  } else if (flag == 3) {
    n = 5; put(0, 0.3, 0.7, 1); put(1, 1, 0.38, 0.6); put(2, 1, 1, 1); put(3, 1, 0.38, 0.6); put(4, 0.3, 0.7, 1)
  } else { n = customN; putHSV(0, c1h, c1s, c1v); putHSV(1, c2h, c2s, c2v); putHSV(2, c3h, c3s, c3v) }
  for (var i = 0; i < n; i++) {
    rgb2hsv(sr[i], sg[i], sb[i], tmp)
    hh[i] = tmp[0]; hs[i] = tmp[1]; hv[i] = tmp[2]
  }
  for (var i = 0; i < MAXS; i++) {
    var blue = flag == 0 && i < 7
    kr[i] = blue ? 0.06 : sr[i]; kg[i] = blue ? 0.08 : sg[i]; kb[i] = blue ? 1 : sb[i]
  }
}
build()

// --- panel geometry, from the grid ---------------------------------------
var gw = -1, gh = 1
var cantonCols = 6, cantonRows = 9, stepX = 3, stepY = 3, starR = 0, halfX = 1
function layout() {
  gw = gridWidth()
  gh = max(gridHeight(), 1)
  cantonCols = max(round(gw * 0.4), 1)
  cantonRows = max(floor(7 * gh / 13 - 0.5) + 1, 1)
  // aim for the real 6-across x 9-down lattice; a pitch below 2 spreads to 3
  // so a small canton reads as stars, not a chequerboard (Gitea #285)
  stepX = round(cantonCols / 6)
  if (stepX < 2) stepX = 3
  stepY = round(cantonRows / 9)
  if (stepY < 2) stepY = 3
  halfX = floor(stepX / 2)
  starR = floor((min(stepX, stepY) - 2) / 2)
}

var tPhase = 0, scroll = 0, shade = 0.25, starT = 0
export function beforeRender(delta) {
  var dt = min(delta, 200) / 1000
  tPhase = mod(tPhase + dt / waveSecs, 1)
  scroll = mod(scroll + dt / waveSecs, 1000)
  starT = mod(starT + dt * 0.7, 1000)
  shade = min(ripple * 0.07, 0.6)
  if (gridWidth() != gw) layout()
  var cols = clamp(gw, 0, MAXC), amp = ripple / 100 * gh
  for (var c = 0; c < cols; c++) {
    var u = c / max(gw - 1, 1), ph = (u * 1.25 - tPhase) * PI2
    dispA[c] = amp * u * u * sin(ph)
    lumA[c] = 1 - shade * (0.25 + 0.75 * u) * (0.5 - 0.5 * cos(ph))
  }
}

// rows r0..r1 of column c in the current brush (a setPixel walk measured
// cheaper than one fillRect per run at these lengths)
function run(c, r0, r1) {
  for (var r = max(r0, 0); r <= min(r1, gh - 1); r++) setPixel(gridIndex(c, r))
}

function column(c) {
  var d = dispA[c], lum = lumA[c], pp = gh / n
  var cant = flag == 0 && c < cantonCols
  var T0 = cant ? kr : sr, T1 = cant ? kg : sg, T2 = cant ? kb : sb
  var rPrev = -1
  for (var k = 0; k < n; k++) {
    // edge between stripe k and k+1 sits at fractional row b; one row straddles it
    var b = (k + 1) * pp - 0.5 - d, rb = k < n - 1 ? floor(b + 0.5) : gh
    rgb(T0[k] * lum, T1[k] * lum, T2[k] * lum)
    run(c, rPrev + 1, rb - 1)
    if (k < n - 1 && rb >= 0 && rb < gh) {
      var t = clamp(0.5 + rb - b, 0, 1)
      rgb(mix(T0[k], T0[k + 1], t) * lum, mix(T1[k], T1[k + 1], t) * lum, mix(T2[k], T2[k + 1], t) * lum)
      setPixel(gridIndex(c, rb))
    }
    rPrev = max(rb, rPrev)
  }
  if (flag == 2) chevron(c, d, lum, pp)
}

// Progress chevron: s = distance in from the hoist + |distance from centre|,
// in panel heights; the 45-degree band edges are antialiased per pixel.
function chevron(c, d, lum, pp) {
  var X = (c + 0.5) / gh, reach = (CH0 + CHW * 4 - X) * gh + 1
  if (reach <= 0) return
  var rc = gh / 2 - 0.5 - d, kp = gh * 0.7071
  for (var r = max(ceil(rc - reach), 0); r <= min(floor(rc + reach), gh - 1); r++) {
    var s = X + abs(r - rc) / gh
    var bi = s < CH0 ? 0 : min(floor((s - CH0) / CHW) + 1, 5)
    var dOut = (CH0 + CHW * bi - s) * kp, dIn = (s - CH0 - CHW * (bi - 1)) * kp
    var j = bi, t = 0
    if (bi < 5 && dOut < 0.5) { j = bi + 1; t = 0.5 - dOut } else if (bi > 0 && dIn < 0.5) { j = bi - 1; t = 0.5 - dIn }
    if (bi == 5 && j == 5) continue
    var k = clamp(floor((r + 0.5 + d) / pp), 0, n - 1)
    var ar = bi < 5 ? chR[bi] : sr[k], ag = bi < 5 ? chG[bi] : sg[k], ab = bi < 5 ? chB[bi] : sb[k]
    var br = j < 5 ? chR[j] : sr[k], bg = j < 5 ? chG[j] : sg[k], bb = j < 5 ? chB[j] : sb[k]
    rgb(mix(ar, br, t) * lum, mix(ag, bg, t) * lum, mix(ab, bb, t) * lum)
    setPixel(gridIndex(c, r))
  }
}

// US stars: a diamond per lattice point, on the undisplaced grid so they
// never crawl between pixels (the canton barely moves anyway)
function stars() {
  for (var cy = 0; cy < cantonRows; cy += stepY) {
    var g = cy / stepY, ox = g % 2 ? halfX : 0
    for (var cx = ox; cx < cantonCols; cx += stepX) {
      var v = (1 - twinkle / 100 * triangle(starT + hash(cx * 31 + cy))) * lumA[cx]
      rgb(v, v, v)
      for (var dy = -starR; dy <= starR; dy++) {
        var w = starR - abs(dy)
        for (var dx = -w; dx <= w; dx++) {
          if (cx + dx < cantonCols && cy + dy < cantonRows) setPixel(gridIndex(cx + dx, cy + dy))
        }
      }
    }
  }
}

// --- strip: the flag along the strip, hoist at pixel 0 -------------------
// Pixel i's centre is position i + 0.5; the cloth at original pixel x has
// shading lum(x) and stripe coordinate pos(x) (both with the travelling wave).
var shA = 0, shB = 0, wph = 0, dph = 0, s1 = 0, ds = 1, sw = 0
function lum(x) { return 1 - (shA + shB * x) * (0.5 - 0.5 * cos(wph + x * dph)) }
function pos(x) { return x * ds + s1 + sw * x * x * sin(wph + x * dph) }

// [a, b) in one colour, cut into pieces of <= 8 px with the shading lerped across
function seg(a, b, h, s, v) {
  var m = ceil((b - a) / 8)
  if (m < 1) return
  var w = (b - a) / m, x = a, l0 = lum(a - 0.5)
  for (var k = 0; k < m; k++) {
    var l1 = lum(x + w - 0.5)
    gradientRange(x, x + w, h, s, v * l0, h, s, v * l1, 1)
    x += w; l0 = l1
  }
}

function strip() {
  var N = pixelCount, last = max(N - 1, 1)
  var head = flag == 0 ? 0.25 : flag == 2 ? 0.15 : 0, hEnd = ceil(head * last)
  shA = shade * 0.25; shB = shade * 0.75 / last
  wph = -tPhase * PI2; dph = 1.5 * PI2 / last
  clear()
  if (flag == 2) {                           // Progress chevron bands, crisp
    for (var b = 0; b < 5; b++) {
      rgb2hsv(chR[b], chG[b], chB[b], tmp)
      seg(ceil(b * hEnd / 5), ceil((b + 1) * hEnd / 5), tmp[0], tmp[1], tmp[2])
    }
  } else if (flag == 0) {                    // the US canton's blue
    rgb2hsv(0.06, 0.08, 1, tmp)
    seg(0, hEnd, tmp[0], tmp[1], tmp[2])
  }
  // stripes: as many as fit at >= 4 px each (US: up to 13), scrolling gently
  var nn = flag == 0 ? 2 : n                  // US: red/white alternate forever
  var vis = flag == 0 ? clamp(floor(N * 0.75 / 4), 3, 13) : n
  var pxPer = (N - hEnd) / vis
  ds = 1 / pxPer
  s1 = -hEnd * ds - scroll
  sw = swayPx * ds / (last * last)
  // walk the stripe coordinate in steps of <= 6 px; each whole-number crossing
  // is a stripe edge, placed by linear interpolation inside its step
  var steps = ceil((N - hEnd) / 6), dx = (N - hEnd) / steps
  var xa = hEnd, pa = pos(xa - 0.5), x0 = hEnd, cur = floor(pa)
  for (var j = 1; j <= steps; j++) {
    var xb = hEnd + j * dx, pb = pos(xb - 0.5)
    for (var m = floor(pa) + 1; m <= pb; m++) {
      var xe = xa + (m - pa) / (pb - pa) * dx
      if (xe > x0) seg(x0, xe, hh[mod(cur, nn)], hs[mod(cur, nn)], hv[mod(cur, nn)])
      x0 = max(x0, xe); cur = m
    }
    xa = xb; pa = pb
  }
  var k = mod(cur, nn)
  seg(x0, N, hh[k], hs[k], hv[k])
  if (flag == 0) {                            // stars: one pixel each, twinkling
    var nStars = clamp(round(N * 0.25 / 6), 2, 10), spp = hEnd / nStars
    for (var q = 0; q < nStars; q++) {
      var c = (q + 0.5) * spp, sv = 1 - twinkle / 100 * triangle(starT + q * 0.37)
      for (var i = ceil(c - 0.51); i <= floor(c + 0.51) && i < hEnd; i++) {
        var l = lum(i)
        rgb(mix(0.06, 1, sv) * l, mix(0.08, 1, sv) * l, l)
        setPixel(i)
      }
    }
  }
}

// A mapless strip stays a strip: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

export function renderBulk() {
  if (!has2DMap() || gw < 1) { strip(); return }
  for (var c = 0; c < min(gw, MAXC); c++) column(c)
  if (flag == 0) stars()
}
