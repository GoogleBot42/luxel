// name: Grey Fade
// Curated original (hand-written showcase of the Luxel language/builtins).
// Every grey from black to white and back: the fixture breathes in unison, or
// the fade flows along it as travelling waves. On its own it is a white light
// that breathes; it is built as a SOURCE for scenes — a scene layer's colour
// ramp (`R`) turns each grey into a colour, so a ramp with N stops is a fade
// between N colours (library/scenes/strip-colour-fade.scene). Ping-pong runs
// black → white → black, i.e. a ramp's stops in order and back again; Loop
// runs black → white and starts over, seamless under a ramp whose two ends are
// the same colour. Strip: the wave is straight lines between its corners, so a
// frame is a handful of gradientRange spans. Panel: the same profile loaded as
// a live palette over a fixed axis canvas (across, down or rings).

var fadeS = 10
// Seconds for one whole fade (black to white and back, or one loop).
//# min=1 max=120 step=1 default=10
export function sliderFadeSeconds(v) { fadeS = clamp(v, 1, 120) }

var shape = 0
// 0 ping-pong (black, white, black), 1 loop (black to white, then over again)
//# min=0 max=1 step=1 default=0
export function sliderShape(v) { shape = clamp(floor(v), 0, 1) }

var waves = 1
// Whole fades laid along the fixture at once; 0 = the fixture in unison.
//# min=0 max=4 step=0.25 default=1
export function sliderWaves(v) { waves = clamp(v, 0, 4) }

var axis = 0
// Panel only: 0 across, 1 down, 2 outward rings.
//# min=0 max=2 step=1 default=0
export function sliderAxis(v) { axis = clamp(floor(v), 0, 2) }

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var ph = 0
export function beforeRender(delta) {
  ph = mod(ph + min(delta, 100) / 1000 / fadeS, 1)
}

// grey at fade phase q (any real; one fade per unit)
function grey(q) {
  var p = mod(q, 1)
  if (shape == 1) return p
  return p < 0.5 ? 2 * p : 2 - 2 * p
}

// ---- strip: straight spans between the corners of the wave ----
// Along the strip the phase is q(i) = ph - waves * i / n, so the grey is a
// straight line between the points where 2q is a whole number — a corner of
// the ping-pong, the wrap of the loop. Each span is one gradientRange whose
// end greys are that line's values, taken from the span's own middle so a
// loop's jump lands exactly on the boundary.
function span(pa, pb, qa, qb) {
  var qm = (qa + qb) / 2
  var p = mod(qm, 1)
  var vm = grey(qm), k = 1
  if (shape == 0) k = p < 0.5 ? 2 : -2
  var va = clamp(vm + k * (qa - qm), 0, 1)
  var vb = clamp(vm + k * (qb - qm), 0, 1)
  gradientRange(pa, pb, 0, 0, va, 0, 0, vb, 1)
}

function strip() {
  clear()
  var n = pixelCount
  if (waves <= 0) { hsv(0, 0, grey(ph)); fill(); return }
  var pa = 0, qa = ph, guard = 0
  while (pa < n && guard < 20) {
    var qb = (ceil(qa * 2) - 1) / 2
    var pb = (ph - qb) / waves * n
    if (pb >= n) { pb = n; qb = ph - waves }
    span(pa, pb, qa, qb)
    pa = pb
    qa = qb
    guard++
  }
}

// ---- panel: the profile along u as a live palette over an axis canvas ----
var NS = 64
var pal = array(NS * 4)
setPalette(pal)
var cvs = array(1024)
var cw = 0, ch = 0, builtAxis = -1

function buildCanvas() {
  var gw = gridWidth(), gh = gridHeight()
  if (gw < 1) { gw = 32; gh = 32 }
  if (axis == 0) { cw = min(gw, 64); ch = 1 }
  else if (axis == 1) { cw = 1; ch = min(gh, 64) }
  else { cw = min(gw, 32); ch = min(gh, 32) }
  for (var r = 0; r < ch; r++) {
    for (var c = 0; c < cw; c++) {
      var x = (c + 0.5) / cw, y = (r + 0.5) / ch
      var u = axis == 0 ? x : axis == 1 ? y : sqrt((x - 0.5) * (x - 0.5) + (y - 0.5) * (y - 0.5)) * 1.414
      cvs[r * cw + c] = min(u, 0.999)
    }
  }
  builtAxis = axis
}

function panel() {
  if (waves <= 0) { hsv(0, 0, grey(ph)); fill(); return }
  if (builtAxis != axis) buildCanvas()
  for (var k = 0; k < NS; k++) {
    var u = k / (NS - 1)
    var g = grey(ph - waves * u)
    pal[k * 4] = u
    pal[k * 4 + 1] = g
    pal[k * 4 + 2] = g
    pal[k * 4 + 3] = g
  }
  paintCanvas(cvs, cw, ch, 1, axis == 2 ? 1 : 0)   // the 1-D axes are already pixel-sized
}

export function renderBulk() {
  if (has2DMap()) panel()
  else strip()
}
