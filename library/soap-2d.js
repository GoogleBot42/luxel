// name: Soap
// Curated original (hand-written showcase of the Luxel language/builtins).
// Domain warping: two low-frequency noise fields push the coordinates around
// before a third is sampled through them, so the colour smears and swirls like
// oil on a soap film. The warp fields are 16x16 fillNoise3D canvases read back
// bilinearly (canvasGet); the final field is one simplex3 per cell of a canvas
// of at most 32x32, put on the panel with one fillCanvas/paintCanvas.

var flow = 12
var warp = 50
var swirls = 2
var cycle = 15
var palette = 0
var spin = 0

// Seconds for the noise to evolve through one feature.
//# min=2 max=60 step=1 default=12
export function sliderFlowS(v) { flow = clamp(v, 2, 60) }
// How far the warp fields push the coordinates, in percent of the panel.
//# min=0 max=100 step=1 default=50
export function sliderWarpPercent(v) { warp = clamp(v, 0, 100) }
// Noise features across the panel.
//# min=1 max=6 step=0.5 default=2
export function sliderSwirls(v) { swirls = clamp(v, 1, 6) }
// Seconds for the colours to travel once round the wheel.
//# min=3 max=120 step=1 default=15
export function sliderColourCycleS(v) { cycle = clamp(v, 3, 120) }
// 0 marble · 1 iridescent (OKLCH thin-film ramp)
//# min=0 max=1 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 1) }
// Seconds per turn of the whole field; 0 holds it still.
//# min=0 max=120 step=1 default=0
export function sliderRotationS(v) { spin = clamp(v, 0, 120) }

var wx = array(256), wy = array(256)
var hA = array(1024), vA = array(1024)
var pal = array(128), tmp = array(3)
var z = 0, hueT = 0, ang = 0, cw = 32, ch = 32

function okRGB(L, C, h, o) {   // OKLCH -> gamma sRGB, Ottosson's OKLab matrices
  var a = C * cos(h * PI2), b = C * sin(h * PI2)
  var l = L + 0.3963 * a + 0.2158 * b, m = L - 0.1056 * a - 0.0639 * b, s = L - 0.0895 * a - 1.2915 * b
  l = l * l * l; m = m * m * m; s = s * s * s
  o[0] = pow(clamp(4.0767 * l - 3.3077 * m + 0.2310 * s, 0, 1), 0.4167)
  o[1] = pow(clamp(-1.2684 * l + 2.6098 * m - 0.3413 * s, 0, 1), 0.4167)
  o[2] = pow(clamp(-0.0042 * l - 0.7034 * m + 1.7076 * s, 0, 1), 0.4167)
}

setPalette(pal)

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  z = mod(z + dt / flow, 200)
  hueT = mod(hueT + dt / cycle, 1)
  if (spin > 0) ang = mod(ang + dt / spin, 1)
  var s = 1.3 * swirls / 2 / 15
  fillNoise3D(wx, 16, 16, s, s, 0, 0, z, 11)
  fillNoise3D(wy, 16, 16, s, s, 5, 5, z, 12)
  if (palette == 1) {
    for (var k = 0; k < 32; k++) {   // dark veins, then a thin-film sweep of hue
      var p = k / 31
      okRGB(0.12 + 0.68 * sqrt(p), 0.06 + 0.1 * p, hueT + 0.7 * p, tmp)
      pal[k * 4] = p; pal[k * 4 + 1] = tmp[0]; pal[k * 4 + 2] = tmp[1]; pal[k * 4 + 3] = tmp[2]
    }
  }
}

export function renderBulk() {
  var gw = gridWidth(), gh = gridHeight(), k = 1
  if (gw == 0) { gw = 32; gh = 32 }
  while (gw / k > 32 || gh / k > 32) k *= 2
  cw = ceil(gw / k); ch = ceil(gh / k)
  var ca = cos(ang * PI2), sa = sin(ang * PI2), fs = swirls, wk = warp / 100
  var z2 = z * 1.7, i = 0
  for (var r = 0; r < ch; r++) {
    var v = (r + 0.5) / ch
    for (var c = 0; c < cw; c++) {
      var u = (c + 0.5) / cw
      var ax = u - 0.5 + wk * canvasGet(wx, 16, u, v)
      var ay = v - 0.5 + wk * canvasGet(wy, 16, u, v)
      var n = simplex3((ax * ca - ay * sa) * fs, (ax * sa + ay * ca) * fs, z2, 13)
      if (palette == 0) { hA[i] = hueT + n * 0.18; vA[i] = saturate(0.55 + n * 0.7) }
      else hA[i] = saturate(0.5 + n * 0.62)
      i++
    }
  }
  if (palette == 0) fillCanvas(hA, 0.75, vA, cw, ch)
  else paintCanvas(hA, cw, ch)
  setBlur(k > 1 ? 0.5 : 0, k / 2)
}
