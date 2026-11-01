// name: Mystify
// Original work from the prose idea spec docs/pattern-specs/patternflow/normal-map-plexus.md
// (Patternflow preset, CC BY-SA 4.0 — prose-described idea only, no code); mode 0 is the
// classic screensaver idea. Mode 0: polygons whose corners bounce off the walls, each
// leaving a fan of fading echoes as it folds through itself. Mode 1: a plexus of drifting
// nodes, linked when close; a link fades in with proximity and takes its hue from its angle
// (an OKLCH normal-map wheel). renderBulk: delta-timed fade() + a few drawLines per frame.

const MAXN = 18
var mode = 0, shapes = 2, verts = 4, echoS = 1.2, linkPct = 40
var x = array(MAXN), y = array(MAXN), vx = array(MAXN), vy = array(MAXN)
var hueT = 0, fadeAcc = 0, fadeK = 1, T = 0, oldMode = -1

//# min=0 max=1 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 1) }
// Mode 1 uses 1.5 × Shapes × Vertices nodes (up to 18).
//# min=1 max=3 step=1 default=2
export function sliderShapes(v) { shapes = clamp(floor(v), 1, 3) }
//# min=3 max=6 step=1 default=4
export function sliderVertices(v) { verts = clamp(floor(v), 3, 6) }
//# min=0.2 max=4 step=0.1 default=1.2
export function sliderEchoSeconds(v) { echoS = clamp(v, 0.2, 4) }
//# min=10 max=80 step=1 default=40
export function sliderLinkDistancePercent(v) { linkPct = clamp(v, 10, 80) }

for (var i = 0; i < MAXN; i++) {
  x[i] = 0.1 + 0.8 * hash(i + 0.5); y[i] = 0.1 + 0.8 * hash(i + 7.5)
  var a = hash(i + 3.5) * PI2, sp = 0.22 + 0.2 * hash(i + 11.5)
  vx[i] = cos(a) * sp; vy[i] = sin(a) * sp
}

export function beforeRender(delta) {
  var dt = min(delta, 100) * 0.001
  T = mod(T + dt, 3600)
  hueT = mod(hueT + dt * 0.03, 1)
  var k = mode == 0 ? 1 : 0.35                 // plexus nodes drift slower
  for (var i = 0; i < MAXN; i++) {
    x[i] += vx[i] * dt * k; y[i] += vy[i] * dt * k
    if (x[i] < 0.02) { x[i] = 0.04 - x[i]; vx[i] = abs(vx[i]) }
    if (x[i] > 0.98) { x[i] = 1.96 - x[i]; vx[i] = -abs(vx[i]) }
    if (y[i] < 0.02) { y[i] = 0.04 - y[i]; vy[i] = abs(vy[i]) }
    if (y[i] > 0.98) { y[i] = 1.96 - y[i]; vy[i] = -abs(vy[i]) }
  }
  var tau = echoS * (mode == 0 ? 333 : 110)   // ms: 5 % left after the echo time (a third in plexus)
  fadeAcc += delta
  fadeK = 1
  if (fadeAcc >= tau / 40) { fadeK = exp(-fadeAcc / tau); fadeAcc = 0 }
}

// A long capsule as short pieces, so each piece's bounding box stays small.
function seg(x0, y0, x1, y1, w) {
  var n = clamp(ceil(min(abs(x1 - x0), abs(y1 - y0)) / 0.15), 1, 6)
  for (var k = 0; k < n; k++) drawLine(x0 + (x1 - x0) * k / n, y0 + (y1 - y0) * k / n, x0 + (x1 - x0) * (k + 1) / n, y0 + (y1 - y0) * (k + 1) / n, w, 2)
}

export function renderBulk() {
  if (mode != oldMode) { clear(); oldMode = mode }
  if (fadeK < 1) fade(fadeK)
  var gw = gridWidth() > 0 ? gridWidth() : 32
  var lw = 1.4 / gw, n = mode == 0 ? shapes * verts : min(MAXN, floor(shapes * verts * 1.5))
  if (mode == 0) {
    for (var s = 0; s < shapes; s++) {
      oklch(0.8, 0.17, hueT + s * 0.37)
      for (var v = 0; v < verts; v++) {
        var a = s * verts + v, b = s * verts + (v + 1) % verts
        seg(x[a], y[a], x[b], y[b], lw)
      }
    }
    return
  }
  var L = linkPct / 100, links = 0
  for (var i = 0; i < n; i++) {
    for (var j = i + 1; j < n && links < 30; j++) {
      var dx = x[j] - x[i], dy = y[j] - y[i]
      if (abs(dx) < L && abs(dy) < L) {
        var d = hypot(dx, dy)
        if (d < L) {
          var f = 1 - d / L
          oklch(0.2 + 0.65 * f, 0.17, mod(atan2(dy, dx) / PI, 1) + hueT)
          seg(x[i], y[i], x[j], y[j], lw)
          links++
        }
      }
    }
  }
  hsv(0, 0, 1)                                 // nodes: small white stars
  for (var i = 0; i < n; i++) splat(x[i], y[i], lw * 1.5, 2)
}
