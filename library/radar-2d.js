// name: Radar
// Clean-room reimplementation from the prose spec docs/pattern-specs/angle-and-radius-from-coordinates.md
// (its hue-by-radius idea) — reimagined, not a replica. A PPI scope: the persistent frame
// IS the phosphor. Each frame fades it (delta-timed), sweeps the beam as sub-stepped
// centre→rim drawLines, and paints range-ring beads, sea clutter and drifting contacts
// only as the beam passes over them, so everything glows and decays like a real CRT.
// The bezel is 32 cached segments redrawn on top. Aspect-corrected on non-square panels.

const NC = 12, NB = 32
var sweepS = 4, persistS = 3, contacts = 5, colour = 0, hueByRadius = 0
var pickH = 0.55, pickS = 0.8
var ang = 0, dA = 0, fadeAcc = 0, fadeK = 1, sx = 1, sy = 1, R = 0.46
var cx = array(NC), cy = array(NC), vx = array(NC), vy = array(NC), ca = array(NC)
var bzx = array(NB), bzy = array(NB)

//# min=1 max=20 step=0.5 default=4
export function sliderSweepSeconds(v) { sweepS = clamp(v, 1, 20) }
//# min=0.5 max=10 step=0.5 default=3
export function sliderPersistenceSeconds(v) { persistS = clamp(v, 0.5, 10) }
//# min=0 max=12 step=1 default=5
export function sliderContacts(v) { contacts = clamp(floor(v), 0, NC) }
//# min=0 max=2 step=1 default=0
export function sliderColour(v) { colour = clamp(floor(v), 0, 2) }
//# default=0
export function toggleHueByRadius(on) { hueByRadius = on > 0.5 ? 1 : 0 }
// Colour 2 uses this hue and saturation; its V is ignored (use the global brightness).
export function hsvPickerScopeColour(h, s, v) { pickH = h; pickS = s }

function spawn(i, anywhere) {
  var a = random(PI2), r = anywhere ? 0.1 + random(0.85) : 1
  cx[i] = cos(a) * r; cy[i] = sin(a) * r                // unit-scope coordinates
  var h = a + PI + random(1.6) - 0.8, sp = 0.015 + random(0.035)
  vx[i] = cos(h) * sp; vy[i] = sin(h) * sp
}
for (var i = 0; i < NC; i++) spawn(i, 1)
for (var k = 0; k < NB; k++) { bzx[k] = cos(k * PI2 / NB); bzy[k] = sin(k * PI2 / NB) }

export function beforeRender(delta) {
  var dt = min(delta, 100) * 0.001
  dA = dt * PI2 / sweepS
  for (var i = 0; i < contacts; i++) {
    cx[i] += vx[i] * dt; cy[i] += vy[i] * dt
    if (cx[i] * cx[i] + cy[i] * cy[i] > 1.05) spawn(i, 0)
    ca[i] = mod(atan2(cy[i], cx[i]), PI2)
  }
  var tau = persistS * 333                // ms: 5 % of the glow left after persistS
  fadeAcc += delta
  fadeK = 1
  if (fadeAcc >= tau / 40) { fadeK = exp(-fadeAcc / tau); fadeAcc = 0 }
}

function tint(r, v) {                     // phosphor colour at unit radius r, value v
  var h = colour == 0 ? 0.34 : (colour == 1 ? 0.085 : pickH)
  var s = colour == 2 ? pickS : 0.9
  if (hueByRadius) { h = h + r * 0.75; s = 0.85 }
  hsv(h, s, v)
}

export function renderBulk() {
  if (fadeK < 1) fade(fadeK)
  var gw = gridWidth(), gh = gridHeight()
  if (gw > 0 && gh > 0) { sx = min(1, gh / gw); sy = min(1, gw / gh) } else gw = 32
  var rx = R * sx, ry = R * sy, lw = 1.6 / gw
  var steps = clamp(ceil(dA * R * gw / 1.2), 1, 12)
  for (var s = 1; s <= steps; s++) {
    var a = ang + dA * s / steps
    var c = cos(a), n = sin(a)
    if (hueByRadius) {
      for (var q = 0; q < 4; q++) {
        tint(q / 4 + 0.12, 0.62)
        drawLine(0.5 + c * rx * q / 4, 0.5 + n * ry * q / 4, 0.5 + c * rx * (q + 1) / 4, 0.5 + n * ry * (q + 1) / 4, lw, 2)
      }
    } else {
      tint(0.5, 0.6)
      drawLine(0.5, 0.5, 0.5 + c * rx, 0.5 + n * ry, lw, 2)
    }
    tint(0.33, 0.8)                       // range-ring beads at 1/3 and 2/3
    splat(0.5 + c * rx * 0.333, 0.5 + n * ry * 0.333, lw * 1.2, 2)
    tint(0.66, 0.8)
    splat(0.5 + c * rx * 0.666, 0.5 + n * ry * 0.666, lw * 1.2, 2)
    var cr = random(1)                    // sea clutter, densest near the centre
    cr = cr * cr * 0.45
    tint(cr, 0.2 + random(0.5))
    splat(0.5 + c * rx * cr, 0.5 + n * ry * cr, lw * 1.3, 2)
  }
  for (var i = 0; i < contacts; i++) {    // contacts flare only as the beam paints them
    if (mod(ca[i] - ang, PI2) < dA) {
      var px = 0.5 + cx[i] * rx, py = 0.5 + cy[i] * ry
      tint(hypot(cx[i], cy[i]), 1)
      splat(px, py, lw * 3.2, 2)
      hsv(colour == 1 ? 0.12 : 0.3, 0.3, 1)
      splat(px, py, lw * 1.6, 2)
    }
  }
  ang = mod(ang + dA, PI2)
  tint(1, 0.3)                            // bezel, on top of the decay every frame
  for (var k = 0; k < NB; k++) {
    var k2 = (k + 1) % NB
    drawLine(0.5 + bzx[k] * (rx + lw), 0.5 + bzy[k] * (ry + lw), 0.5 + bzx[k2] * (rx + lw), 0.5 + bzy[k2] * (ry + lw), lw * 0.8, 2)
  }
}
