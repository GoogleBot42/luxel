// name: Gears
// Original work from the prose idea spec docs/pattern-specs/patternflow/spinning-crosses.md (Patternflow preset, CC BY-SA 4.0 — prose-described idea only, no code).
// A clockwork wall: a checkerboard of cogs whose teeth really mesh. Every
// gear ticks a quarter turn with an easeOutBack overshoot and rests, each
// neighbour turning the other way. Only two gear images exist (one per spin
// direction): they are drawn into small canvases while the gears move and
// blitted once per cell, so nothing is evaluated per panel pixel.

var MAXP = 38                 // largest image: a 32 px cell plus tooth padding
var tabR = array(MAXP * MAXP) // polar table of one image: radius in px…
var tabA = array(MAXP * MAXP) // …and angle in turns
var s0 = array(MAXP * MAXP), v0 = array(MAXP * MAXP)   // clockwise gear
var s1 = array(MAXP * MAXP), v1 = array(MAXP * MAXP)   // counter-clockwise gear

var cell = 16
var spokes = 4
var tickS = 1.5

// Cell size in panel pixels: one gear per cell.
//# min=8 max=32 step=2 default=16
export function sliderCellPx(v) { cell = clamp(floor(v / 2) * 2, 8, 32); built = -1 }

//# min=3 max=6 step=1 default=4
export function sliderSpokes(v) { spokes = clamp(floor(v), 3, 6); last = -99 }

// Seconds per tick (move + rest).
//# min=0.4 max=4 step=0.1 default=1.5
export function sliderTickS(v) { tickS = clamp(v, 0.4, 4) }

var built = -1, P = 0, pad = 0, N = 12, Rp = 8, hT = 1.4, rimIn = 5, hubR = 2.5, axR = 1, spokeW = 1
var clock = 0, theta = 0, last = -99

function build() {
  built = cell
  pad = ceil(max(1.2, cell * 0.09)) + 1
  P = cell + 2 * pad
  N = max(8, round(PI * cell / 16) * 4)        // teeth: a multiple of 4 so neighbours mesh at any angle
  Rp = cell / 2
  hT = max(1.2, cell * 0.09)
  rimIn = Rp - hT - max(1, cell * 0.07)
  hubR = max(1.5, cell * 0.13)
  axR = max(0.7, cell * 0.05)
  spokeW = max(0.9, cell * 0.06)
  var c = (P - 1) / 2
  for (var i = 0; i < P; i++) for (var j = 0; j < P; j++) {
    var dx = j - c, dy = i - c
    tabR[i * P + j] = hypot(dx, dy)
    tabA[i * P + j] = atan2(dy, dx) / PI2
  }
  last = -99
}

// one gear image at rotation `th` (turns) into s/v
function draw(th, sa, va) {
  var c = (P - 1) / 2
  for (var i = 0; i < P; i++) for (var j = 0; j < P; j++) {
    var k = i * P + j
    var r = tabR[k]
    if (r > Rp + hT + 0.5) { va[k] = 0; continue }
    var a = tabA[k] - th
    var sh = abs(mod(a * N, 1) - 0.5) * 2         // 0 = tooth centre, 1 = gap
    var rOut = Rp + hT * clamp((0.44 - sh) * 5, -1, 1)
    var edge = rOut - r
    if (edge < -0.5) { va[k] = 0; continue }
    var lit = 0.82 - 0.18 * (j + i - 2 * c) / (r + 1)  // light from the top-left
    var v = 1, s = 1
    if (r < rimIn) {
      var w = mod(a * spokes, 1)
      var perp = r * min(w, 1 - w) * PI2 / spokes
      var body = max(clamp(spokeW - perp + 0.5, 0, 1), clamp(hubR - r + 0.5, 0, 1))
      body = max(body, clamp(r - rimIn + 1.5, 0, 1))
      v = 0.07 + 0.93 * body
      if (r < axR) v = 0.05
    }
    if (edge < 1.1) s = clamp(edge * 0.7, 0, 1)    // white-hot outer edge
    sa[k] = s
    va[k] = clamp(edge + 0.5, 0, 1) * v * lit
  }
}

export function beforeRender(delta) {
  clock = mod(clock + delta / 1000, tickS * 4096)
  var f = clock / tickS
  var m = min(frac(f) / min(0.45, 0.6 / tickS), 1)
  theta = (floor(f) + easeOutBack(m)) * 0.25
}

export function renderBulk() {
  var gw = gridWidth(), gh = gridHeight()
  hsv(0.74, 0.7, 0.09)
  fill()
  if (gw == 0) return
  if (built != cell) build()
  if (theta != last) {
    last = theta
    draw(theta, s0, v0)
    draw(0.5 / N - theta, s1, v1)                 // half a tooth out of phase, opposite spin
  }
  var nc = ceil(gw / cell), nr = ceil(gh / cell)
  var ox = floor((gw - nc * cell) / 2), oy = floor((gh - nr * cell) / 2)
  var kx = 1 / max(gw - 1, 1), ky = 1 / max(gh - 1, 1)
  hsv(0.76, 0.55, 0.3)                             // housing rivets at the cell corners
  for (var l = 0; l <= nr; l++) for (var k = 0; k <= nc; k++) {
    fillCircle((ox + k * cell - 0.5) * kx, (oy + l * cell - 0.5) * ky, 1.1 * kx)
  }
  for (var gy = 0; gy < nr; gy++) for (var gx = 0; gx < nc; gx++) {
    var col = ox + gx * cell - pad, row = oy + gy * cell - pad
    if ((gx + gy) % 2 == 0) blit(0.07, s0, v0, P, P, col, row, 2)
    else blit(0.53, s1, v1, P, P, col, row, 2)
  }
}
