// name: Coronal Mass Ejection
// tags: panel, nature
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/coronal-mass-ejection.md, sunrise-2d.md — reimagined, not a replica.
// A white-hot star throws ragged plasma tongues over black; now and then the limb
// tears loose as a prominence, and flare sparks arc off the surface and fall back.
// Arms folds the corona into a k-fold pinwheel (Mirror makes it a flower); Rise On
// Start lifts the sun up from the bottom edge first.
// The corona is built once per frame on a 64-angle x 24-radius polar canvas, read
// bilinearly per panel cell through a packed (angle, radius) table and painted with
// one paintCanvas through a blackbody palette; the sparks are splats.

var NA = 64, NR = 24, RMAX = 1.6  // polar canvas: angles, radii, radius span
var NA1 = NA + 1                  // one wrap column so bilinear needs no modulo
var CAP = 64                      // panel canvas cap per axis
var MAXE = 4                      // simultaneous prominences
var NS = 24                       // flare sparks

var eruptRate = 14                // prominences per minute
var intensity = 50                // how much of the corona lights up, percent
var baseHue = 18                  // plasma hue, degrees
var hueCycle = 0                  // seconds per hue revolution, 0 = solar, fixed
var speed = 1                     // churn rate, x realtime
var coreSize = 18                 // core radius, percent of the half-width
var arms = 1                      // rotational symmetry, 1 = a natural star
var twist = 48                    // lean of the tongues, degrees per half-width
var mirror = 0
var riseOnStart = 0

//# min=0 max=60 step=1 default=14
export function sliderEruptions(v) { eruptRate = clamp(v, 0, 60) }
//# min=0 max=100 step=1 default=50
export function sliderIntensity(v) { intensity = clamp(v, 0, 100) }
//# min=0 max=360 step=1 default=18
export function sliderHue(v) { baseHue = clamp(v, 0, 360) }
//# min=0 max=120 step=1 default=0
export function sliderHueCycleSeconds(v) { hueCycle = clamp(v, 0, 120) }
//# min=0.1 max=4 step=0.1 default=1
export function sliderSpeed(v) { speed = clamp(v, 0.1, 4) }
//# min=4 max=50 step=1 default=18
export function sliderCoreSize(v) { coreSize = clamp(v, 4, 50) }
//# min=1 max=12 step=1 default=1
export function sliderArms(v) { arms = clamp(floor(v), 1, 12); tabDirty = 1 }
//# min=-180 max=180 step=1 default=48
export function sliderTwistDegrees(v) { twist = clamp(v, -180, 180); tabDirty = 1 }
export function toggleMirror(v) { mirror = v; tabDirty = 1 }
// Turning it on replays the sunrise straight away.
export function toggleRiseOnStart(v) { if (v && !riseOnStart) rise = 0; riseOnStart = v }
export function triggerErupt() { spawnErupt() }

var pol = array(NA1 * NR)         // polar value canvas, 0..1
var tab = array(CAP * CAP)        // per panel cell: sector x*4096 + radius y, packed
var cv = array(CAP * CAP)         // per panel cell value, painted
var eAng = array(MAXE), eRad = array(MAXE), eVel = array(MAXE)
var eWid = array(MAXE), eLife = array(MAXE)
var sx = array(NS), sy = array(NS), svx = array(NS), svy = array(NS), sLife = array(NS)
var pal = array(20)
setPalette(pal)
var rgbTmp = array(3)

var CW = 0, CH = 0, GW = 0, GH = 0
var unit = 1, cyU = 0             // half-width in canvas cells, sun centre y (units)
var rise = 1                      // 0 -> 1 sunrise progress; 1 = risen
var tabDirty = 1
var churn = 0, shape = 0, huePhase = 0, eruptTimer = 0.6, sparkTimer = 0
var cut = 0.67, coreR = 0.18, glow = 1

function spawnErupt() {
  for (var i = 0; i < MAXE; i++) if (eLife[i] <= 0) {
    eAng[i] = random(1); eRad[i] = coreR
    eVel[i] = 0.3 + random(0.3); eWid[i] = 0.05 + random(0.05)
    eLife[i] = 1
    return
  }
}

// A burst of sparks from one point on the limb, flung out on looping arcs.
function sparkBurst() {
  var a = random(PI2), n = 3 + floor(random(4))
  for (var i = 0; i < NS && n > 0; i++) if (sLife[i] <= 0) {
    var aa = a + random(0.5) - 0.25, sp = 0.55 + random(0.45), tg = random(1.2) - 0.6
    sx[i] = cos(aa) * coreR; sy[i] = sin(aa) * coreR
    svx[i] = (cos(aa) - sin(aa) * tg) * sp; svy[i] = (sin(aa) + cos(aa) * tg) * sp
    sLife[i] = 1; n--
  }
}

// Each panel cell's place on the polar canvas, packed in one number. The canvas
// holds ONE symmetry sector, so twist, arms and mirror are folded in here and
// every arm count keeps the full angular resolution.
function buildTable() {
  var sc = 1 / unit, ry = (CH * 0.5 - 0.5) + cyU * unit, tw = twist / 360
  for (var r = 0; r < CH; r++) {
    var dy = (r - ry) * sc
    for (var c = 0; c < CW; c++) {
      var dx = (c - CW * 0.5 + 0.5) * sc
      var d = hypot(dx, dy)
      var s = mod((atan2(dy, dx) / PI2 + d * tw) * arms, 1)
      if (mirror) s = 1 - abs(2 * s - 1)
      var rr = min(d, RMAX) * (NR - 1) / RMAX
      tab[r * CW + c] = floor((s * NA + 0.5) / NA1 * 4096) + (rr + 0.5) / NR
    }
  }
  tabDirty = 0
}

function setPal(h, i, pos, s, v) {
  hsv2rgb(h, s, v, rgbTmp)
  pal[i] = pos; pal[i + 1] = rgbTmp[0]; pal[i + 2] = rgbTmp[1]; pal[i + 3] = rgbTmp[2]
}

export function beforeRender(delta) {
  var dt = min(delta, 60) * 0.001, st = dt * speed

  GW = gridWidth(); GH = gridHeight()
  var w = GW > 0 ? min(GW, CAP) : 32, h = GH > 0 ? min(GH, CAP) : 32
  if (w != CW || h != CH) { CW = w; CH = h; unit = min(w, h) * 0.5; tabDirty = 1 }

  // sunrise: the centre climbs from below the bottom edge with an ease-out
  if (rise < 1) {
    rise = min(1, rise + dt / 5)
    cyU = (1 - easeOutCubic(rise)) * (CH * 0.5 + unit * 0.5) / unit
    tabDirty = 1
  } else if (cyU != 0) { cyU = 0; tabDirty = 1 }
  if (tabDirty) buildTable()
  glow = 0.8 + 0.2 * rise

  churn = mod(churn + st * 0.9, 4096)
  shape = mod(shape + st * 0.5, 4096)
  if (hueCycle > 0) huePhase = mod(huePhase + dt / hueCycle, 1)
  var hh = baseHue / 360 + huePhase, warm = 1 - rise   // a rising sun burns redder
  setPal(hh - 0.04, 0, 0, 1, 0)
  setPal(hh - 0.03, 4, 0.2, 1, 0.5)
  setPal(hh + 0.01 - 0.02 * warm, 8, 0.45, 1, 1)
  setPal(hh + 0.07 - 0.04 * warm, 12, 0.72, 0.6 + 0.4 * warm, 1)
  setPal(hh + 0.1 - 0.05 * warm, 16, 1, 0.06 + 0.5 * warm, 1)

  cut = 0.95 - intensity * 0.0055
  coreR = coreSize * 0.01

  if (eruptRate > 0) {
    eruptTimer -= dt
    if (eruptTimer <= 0) { spawnErupt(); eruptTimer = (60 / eruptRate) * (0.55 + random(0.9)) }
  }
  for (var i = 0; i < MAXE; i++) if (eLife[i] > 0) {
    eRad[i] += eVel[i] * st
    eWid[i] += 0.05 * st
    eLife[i] -= st / 1.9
    if (eRad[i] > RMAX) eLife[i] = 0
  }

  // flare sparks: constant-magnitude pull to the centre, so they loop and fall back
  sparkTimer -= st
  if (sparkTimer <= 0) { sparkBurst(); sparkTimer = 0.25 + random(1.1 - intensity * 0.007) }
  for (i = 0; i < NS; i++) if (sLife[i] > 0) {
    var d = max(hypot(sx[i], sy[i]), 0.01), g = 1.6 * st / d
    svx[i] -= sx[i] * g; svy[i] -= sy[i] * g
    sx[i] += svx[i] * st; sy[i] += svy[i] * st
    sLife[i] -= st * 0.45
    if (d < coreR * 0.8 && sLife[i] < 0.9) sLife[i] = 0
  }

  buildPolar()
}

// The corona on one sector of the polar canvas: one turbulence read per cell,
// then the white core chewed by the same noise and the prominences as stretched
// blobs, each pass touching only the rows it can reach. Values are squared.
function buildPolar() {
  var L = arms == 1 ? 6 : max(2, round(6 / arms))
  setPerlinWrap(L, 0, 0)
  var dR = RMAX / (NR - 1), du = L / NA
  for (var j = 0; j < NR; j++) {
    var r = j * dR, ry = r * 0.9 - churn, row = j * NA1
    var fall = saturate(1.3 - r * 0.5) * saturate((RMAX - r) * 4)
    for (var i = 0; i < NA; i++) {
      var n = 1 - perlinTurbulence(i * du, ry, shape, 2, 0.5, 3)
      cv[row + i] = n             // scratch: the noise, for the passes below
      var f = smoothstep(cut, cut + 0.22, n) * fall
      pol[row + i] = f * f
    }
  }
  for (j = 0; j * dR < coreR * 2.2; j++) {
    r = j * dR; row = j * NA1
    for (i = 0; i < NA; i++) {
      var ce = coreR * (0.55 + cv[row + i] * 0.9)
      var c = saturate(1 + (ce - r) / (0.35 * ce))
      pol[row + i] = max(pol[row + i], c * c)
    }
  }
  for (var e = 0; e < MAXE; e++) {
    var lf = eLife[e]
    if (lf <= 0) continue
    var iw = 1 / (eWid[e] * arms * (mirror ? 2 : 1) * NA)   // per canvas column
    for (j = max(0, ceil((eRad[e] - 0.45) / dR)); j < NR && j * dR < eRad[e] + 0.2; j++) {
      r = j * dR; row = j * NA1
      var dr = r - eRad[e], w = dr < 0 ? dr * 2.2 : dr * 5
      w = w * w
      for (i = 0; i < NA; i++) {
        var da = abs(i - eAng[e] * NA)
        if (!mirror && da > NA * 0.5) da = NA - da
        var u = da * iw
        if (u >= 1) continue
        var q = u * u + w
        if (q >= 1) continue
        var p = min(1, (1 - q) * (1 - q) * lf * (0.3 + cv[row + i] * 1.5) * 1.8)
        pol[row + i] = max(pol[row + i], p * p)
      }
    }
  }
  for (j = 0; j < NR; j++) pol[j * NA1 + NA] = pol[j * NA1]
}

export function renderBulk() {
  var n = CW * CH
  for (var i = 0; i < n; i++) {
    var p = tab[i]
    cv[i] = canvasGet(pol, NA1, floor(p) * 0.000244140625, frac(p))
  }
  paintCanvas(cv, CW, CH, glow)

  // sparks: a short streak and a hot head, added over the corona
  var ox = 0.5, oy = 0.5 + cyU * unit / CH, fx = unit / CW, fy = unit / CH
  var sr = 1.3 / CW
  for (i = 0; i < NS; i++) {
    var l = sLife[i]
    if (l <= 0) continue
    hsv(baseHue / 360 + huePhase + 0.04, 0.4, l * glow)
    var x = ox + sx[i] * fx, y = oy + sy[i] * fy
    drawLine(x, y, x - svx[i] * fx * 0.08, y - svy[i] * fy * 0.08, sr * 0.8, 1)
    splat(x, y, sr * 1.4, 1)
  }
}
