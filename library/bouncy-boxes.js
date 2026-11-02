// name: Bouncy Boxes
// tags: panel, ambient
// Clean-room reimplementation from the prose spec docs/pattern-specs/bouncy-boxes.md — reimagined, not a replica.
// Square blocks glide around a cylinder — wrapping left/right, bouncing off
// the top and bottom — and collide like billiard blocks that never slow
// down. Each block is drawn at native resolution as nested fillRect rings,
// so its rainbow runs in concentric squares. Glitch adds hashed sparkles
// and a torn streak. Simulated in a 16-unit-wide space scaled to the panel.

var MAXB = 8
var W = 16, H = 16             // sim space; H follows the panel's aspect
var bx = array(MAXB), by = array(MAXB), bvx = array(MAXB), bvy = array(MAXB)
for (var k = 0; k < MAXB; k++) {
  bx[k] = mod(k * 9.89, W)
  by[k] = mod(k * 5.3 + 1, 12)
  var a = 0.6 + k * 1.37
  bvx[k] = cos(a)
  bvy[k] = sin(a)
}

var nb = 4
var sizePct = 25
var speed = 0.4
var glitch = 0

//# min=1 max=8 step=1 default=4
export function sliderBoxes(v) { nb = clamp(floor(v), 1, MAXB) }

// Block side as a percentage of the panel height.
//# min=10 max=45 step=1 default=25
export function sliderSizePct(v) { sizePct = clamp(v, 10, 45) }

// Panel widths per second.
//# min=0.05 max=1.5 step=0.05 default=0.4
export function sliderSpeedPanelsS(v) { speed = clamp(v, 0.05, 1.5) }

//# default=0
export function toggleGlitch(v) { glitch = v > 0.5 }

var S = 4, hueBase = 0, tick = 0, tickAcc = 0

function wrapDist(d) { return mod(d + W / 2, W) - W / 2 }

function bounceY(k) {
  if (by[k] < 0) { by[k] = -by[k]; bvy[k] = abs(bvy[k]) }
  if (by[k] > H - S) { by[k] = 2 * (H - S) - by[k]; bvy[k] = -abs(bvy[k]) }
  by[k] = clamp(by[k], 0, H - S)
}

export function beforeRender(delta) {
  var gw = gridWidth(), gh = gridHeight()
  H = gw > 0 ? W * gh / gw : W
  S = min(H * sizePct / 100, sqrt(0.45 * W * H / nb))   // never more blocks than fit
  var dt = min(delta, 50) / 1000, i, j, k, tmp
  for (k = 0; k < nb; k++) {
    bx[k] = mod(bx[k] + bvx[k] * dt, W)
    by[k] += bvy[k] * dt
    bounceY(k)
  }
  for (var pass = 0; pass < 3; pass++) for (i = 0; i < nb - 1; i++) for (j = i + 1; j < nb; j++) {
    var dx = wrapDist(bx[j] - bx[i]), dy = by[j] - by[i]
    var ox = S - abs(dx), oy = S - abs(dy)
    if (ox > 0 && oy > 0) {
      if (ox < oy) {                             // separate along x, swap x velocities if closing
        var sx = dx >= 0 ? 1 : -1, px = max(ox * 0.45 - 0.01, 0)
        bx[i] = mod(bx[i] - sx * px, W)
        bx[j] = mod(bx[j] + sx * px, W)
        if ((bvx[j] - bvx[i]) * sx < 0) { tmp = bvx[i]; bvx[i] = bvx[j]; bvx[j] = tmp }
      } else {
        var sy = dy >= 0 ? 1 : -1, py = max(oy * 0.45 - 0.01, 0)
        by[i] -= sy * py
        by[j] += sy * py
        bounceY(i)
        bounceY(j)
        if ((bvy[j] - bvy[i]) * sy < 0) { tmp = bvy[i]; bvy[i] = bvy[j]; bvy[j] = tmp }
      }
    }
  }
  var target = speed * W                         // renormalize: collisions redirect, never slow
  for (k = 0; k < nb; k++) {
    var sp = hypot(bvx[k], bvy[k])
    if (sp < 0.01) { bvx[k] = 0.7; bvy[k] = 0.7; sp = 0.99 }
    bvx[k] *= target / sp
    bvy[k] *= target / sp
  }
  hueBase = time(0.15)
  tickAcc += delta
  if (tickAcc > 60) { tickAcc = 0; tick = (tick + 1) % 997 }
}

var kx = 1, ky = 1
// pixel-edge rectangle [x0, x1) x [y0, y1): pixels whose centres fall inside
function box(x0, y0, x1, y1) { fillRect((x0 - 0.5) * kx, (y0 - 0.5) * ky, (x1 - 0.51) * kx, (y1 - 0.51) * ky) }

function block(k, x, y, side) {
  var rings = clamp(round(side / 2), 1, 10), step = side / 2 / rings
  for (var r = 0; r < rings; r++) {
    hsv(hueBase + k * 0.25 + 0.35 * (1 - r / rings), 1 - 0.25 * r / rings, 1)
    box(x + r * step, y + r * step, x + side - r * step, y + side - r * step)
  }
}

export function renderBulk() {
  var gw = gridWidth(), gh = gridHeight()
  if (gw == 0) { gw = 32; gh = 32 }
  kx = 1 / max(gw - 1, 1)
  ky = 1 / max(gh - 1, 1)
  var u = gw / W, side = S * u, k
  clear()
  for (k = 0; k < nb; k++) {                     // dim halo under every block first
    hsv(hueBase + k * 0.25 + 0.35, 1, 0.22)
    box(bx[k] * u - 1, by[k] * u - 1, bx[k] * u + side + 1, by[k] * u + side + 1)
    if (bx[k] * u + side > gw) box(bx[k] * u - gw - 1, by[k] * u - 1, bx[k] * u + side - gw + 1, by[k] * u + side + 1)
  }
  for (k = 0; k < nb; k++) {
    block(k, bx[k] * u, by[k] * u, side)
    if (bx[k] * u + side > gw) block(k, bx[k] * u - gw, by[k] * u, side)   // wrapped around the cylinder
  }
  if (glitch) {
    for (var i = 0; i < pixelCount / 120; i++) {
      hsv(hash2(i, tick + 500), 0.6, 1)
      setPixel(floor(hash2(i, tick) * pixelCount))
    }
    if (hash(tick) < 0.4) {
      var row = floor(hash2(tick, 7) * gh), c0 = hash2(tick, 11) * gw
      hsv(hash2(tick, 13), 0.3, 1)
      box(c0, row, c0 + gw * 0.2, row + 1)
    }
  }
}
