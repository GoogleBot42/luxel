// name: Rain on Glass
// Curated original (hand-written showcase of the Luxel language/builtins).
// A rainy window at night: out-of-focus city lights glow behind misted glass.
// Beads condense and merge; a heavy drop breaks loose and slides down in
// jerky runs, swallowing the beads in its path and wiping a clear trail
// that slowly fogs over again. Each drop shows its own flipped glint.
// The bokeh backdrop is a canvas (up to 32x32) built once; per frame the fog
// mask mixes it toward a blurred, hazy copy; drops are native-res splats.

var CAP = 32
var bgH = array(1024), bgS = array(1024), bgV = array(1024), blV = array(1024)
var outS = array(1024), outV = array(1024), clr = array(1024)
var CW = 32, CH = 32, gwSeen = -1, U = 1 / 32   // U = one pixel
var ND = 30
var dX = array(ND), dY = array(ND), dR = array(ND), dSt = array(ND), dV = array(ND), dT = array(ND)
var beadAcc = 0

var rain = 8
// New beads condensing on the glass per second.
//# min=1 max=30 step=1 default=8
export function sliderBeadsPerSecond(v) { rain = clamp(v, 1, 30) }

var refog = 6
// Seconds for a wiped trail to mist over again (to about a quarter clear).
//# min=1 max=30 step=0.5 default=6
export function sliderRefogS(v) { refog = clamp(v, 1, 30) }

var fogPct = 70
// How strongly the mist veils the lights.
//# min=0 max=100 step=1 default=70
export function sliderFogPct(v) { fogPct = clamp(v, 0, 100) }

// a defocused light: flat disc, brighter rim, soft edge
function bokeh(cx, cy, r, h, s, v) {
  var x0 = max(floor(cx - r - 1), 0), x1 = min(ceil(cx + r + 1), CW - 1)
  var y0 = max(floor(cy - r - 1), 0), y1 = min(ceil(cy + r + 1), CH - 1)
  for (var y = y0; y <= y1; y++) for (var x = x0; x <= x1; x++) {
    var d = hypot(x + 0.5 - cx, y + 0.5 - cy) / r
    if (d < 1.15) {
      var e = d < 1 ? 0.6 + 0.4 * d * d * d * d : (1.15 - d) / 0.15
      var i = y * CW + x, a = v * e
      if (a > bgV[i] * 0.6) { bgH[i] = h; bgS[i] = s }
      bgV[i] = min(bgV[i] + a, 1)
    }
  }
}

var LH = [0.08, 0.1, 0.52, 0.0, 0.86, 0.33, 0.6, 0.06]
var LS = [0.9, 0.35, 0.8, 0.9, 0.6, 0.8, 0.5, 0.95]

function build() {
  var gw = gridWidth(), gh = gridHeight()
  if (gw == gwSeen) return
  gwSeen = gw
  if (gw == 0) { gw = 32; gh = 32 }
  U = 1 / max(gw, 16)
  var k = ceil(max(gw, gh) / CAP)
  CW = ceil(gw / k)
  CH = ceil(gh / k)
  var i = 0
  for (var y = 0; y < CH; y++) {
    var f = y / max(CH - 1, 1)
    for (var x = 0; x < CW; x++) {
      bgH[i] = 0.64 - f * 0.08
      bgS[i] = 0.85 - f * 0.25
      bgV[i] = 0.09 + f * f * 0.12
      i++
    }
  }
  var sz = CW / 32
  for (var n = 0; n < 18; n++) {
    var c = floor(random(8))
    bokeh(random(CW), CH * (0.15 + random(0.85)), (1.1 + random(2.4)) * sz, LH[c], LS[c], 0.45 + random(0.45))
  }
  arrayMix(blV, bgV, 1)
  blur2D(blV, CW, CH, max(round(sz), 1))
  feedback(clr, 0)
  for (n = 0; n < ND; n++) dR[n] = 0
}

function bead(x, y, r) {
  for (var i = 0; i < ND; i++) if (dR[i] == 0) {
    dX[i] = x
    dY[i] = y
    dR[i] = r
    dSt[i] = 0
    dV[i] = 0
    dT[i] = 0
    return i
  }
  return -1
}

function merge(a) {
  for (var j = 0; j < ND; j++) if (j != a && dR[j] > 0) {
    var rr = dR[a] + dR[j]
    var ex = dX[j] - dX[a], ey = dY[j] - dY[a]
    if (ex * ex + ey * ey < rr * rr * 0.6) {
      var b = dR[j] > dR[a] && dSt[a] == 0 ? j : a, o = b == a ? j : a
      dR[b] = min(sqrt(dR[b] * dR[b] + dR[o] * dR[o]), U * 3.2)
      dR[o] = 0
    }
  }
}

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  build()
  feedback(clr, pow(0.25, dt / refog))
  beadAcc += rain * dt
  while (beadAcc >= 1) {
    beadAcc -= 1
    var i = bead(random(1), random(1), U * (0.5 + random(0.9)))
    if (i >= 0) merge(i)
  }
  for (i = 0; i < ND; i++) if (dR[i] > 0) {
    if (dSt[i] == 0) {
      dR[i] += dt * U * 0.2                    // condensation
      if (dR[i] > U * 1.8) { dSt[i] = 1; dV[i] = 0 }
      continue
    }
    // stick/slip: runs at up to ~a third of the panel a second, random halts
    if (dT[i] > 0) { dT[i] -= dt; continue }
    dV[i] = min(dV[i] + dt * 1.2, 0.18 + dR[i] / U * 0.05)
    if (random(1) < dt * 1.6) { dT[i] = 0.1 + random(0.5); dV[i] = 0 }
    var oy = dY[i]
    dY[i] += dV[i] * dt
    dX[i] += (random(1) - 0.5) * dt * 0.08
    // wipe the mist along the path
    for (var s = 0; s < 3; s++) {
      var yy = mix(oy, dY[i], s / 2)
      canvasSet(clr, CW, dX[i], yy, 1)
      canvasSet(clr, CW, dX[i] - dR[i] * 0.8, yy, 1)
      canvasSet(clr, CW, dX[i] + dR[i] * 0.8, yy, 1)
    }
    // shed a little residue bead now and then
    if (random(1) < dt * 2 && dR[i] > U * 1.3) {
      dR[i] *= 0.95
      bead(dX[i], oy - dR[i], dR[i] * 0.35)
    }
    merge(i)
    if (dY[i] - dR[i] > 1.02) dR[i] = 0
  }
  // mist: blurred, hazy, desaturated copy where the glass is fogged
  var fg = fogPct / 100, i2 = 0, n = CW * CH
  for (i2 = 0; i2 < n; i2++) {
    var c = clr[i2], fv = blV[i2] * (1 - 0.5 * fg) + 0.08 * fg
    outV[i2] = fv + (bgV[i2] - fv) * c
    outS[i2] = bgS[i2] * (1 - fg * 0.5 * (1 - c))
  }
}

export function renderBulk() {
  setBlur(gridWidth() > CW ? 0.25 : 0)
  fillCanvas(bgH, outS, outV, CW, CH)
  var px = U
  for (var i = 0; i < ND; i++) if (dR[i] > 0) {
    var x = dX[i], y = dY[i], r = dR[i]
    // each drop is a tiny lens: the lights behind it, flipped and brightened
    var j = floor(clamp(1 - y + (x - 0.5) * 0.3, 0, 0.999) * CH) * CW + floor(clamp(1 - x, 0, 0.999) * CW)
    if (r < px * 1.5) {
      hsv(bgH[j], bgS[j] * 0.5, 0.18 + bgV[j] * 0.5)
      splat(x, y, px * 1.4, 2)
      continue
    }
    hsv(bgH[j], bgS[j] * 0.7, max(bgV[j] * 2.2, 0.45))
    splat(x, y + r * 0.15, r * 1.3, 0)
    hsv(0.6, 0.1, 0.8)
    splat(x - r * 0.3, y - r * 0.35, px * 1.2, 1)
  }
}
