// name: Voronoi
// tags: panel, ambient
// Clean-room reimplementation from the prose spec docs/pattern-specs/ice-floes-2d.md
// (Ice style) — reimagined, not a replica; orbs and stained glass are curated originals.
// Drifting seeds split the panel into cells, computed once per frame on a
// canvas (<= 48 wide) with nearest and second-nearest squared distances; the
// F2 - F1 gap draws the borders. Styles: glowing orbs; stained glass (jewel
// panes in dark lead cames, a sunbeam sweeping across, grainy glass); river
// ice (white-blue floes drifting downstream with dark cracks, wrapping round).

var style = 1
//# min=0 max=2 step=1 default=1
export function sliderStyle(v) { style = clamp(floor(v), 0, 2) }

var cells = 10
//# min=4 max=24 step=1 default=10
export function sliderCells(v) { cells = clamp(floor(v), 4, 24) }

// Drift in percent of the panel height per second.
var speedPct = 4
//# min=0 max=20 step=0.5 default=4
export function sliderSpeed(v) { speedPct = clamp(v, 0, 20) }

var MAXS = 24, MAXC = 2304
var bx = array(MAXS), by = array(MAXS), fx = array(MAXS), fy = array(MAXS)
var phx = array(MAXS), phy = array(MAXS), ox = array(MAXS)
var sx = array(MAXS), sy = array(MAXS), jew = array(MAXS)
var cH = array(MAXC), cS = array(MAXC), cV = array(MAXC)
var JEWELS = [0.985, 0.62, 0.37, 0.1, 0.77, 0.5]

// seeds by best-candidate sampling: any first N of them are well spread
for (var i = 0; i < MAXS; i++) {
  var best = -1
  for (var c = 0; c < 10; c++) {
    var cx = random(1), cy = random(1), dmin = 9
    for (var k = 0; k < i; k++) dmin = min(dmin, (cx - bx[k]) * (cx - bx[k]) + (cy - by[k]) * (cy - by[k]))
    if (dmin > best) { best = dmin; bx[i] = cx; by[i] = cy }
  }
  fx[i] = 0.5 + random(1)
  fy[i] = 0.5 + random(1)
  phx[i] = random(1)
  phy[i] = random(1)
  ox[i] = 0
  jew[i] = JEWELS[i % 6]
}

var CW = 32, CH = 32, passes = 0, asp = 1, beam = 0, hueT = 0, nf = 2, fld = 0

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000, sp = speedPct / 100
  var gw = gridWidth(), gh = gridHeight()
  if (gw < 1) { gw = 32; gh = 32 }
  CW = gw
  CH = gh
  passes = 0
  while (CW * CH > MAXC || CW > 48) {
    CW = ceil(CW / 2)
    CH = ceil(CH / 2)
    passes++
  }
  asp = gw / gh
  // interlace: each frame recomputes every nf-th canvas row (~600 cells)
  nf = clamp(ceil(CW * CH / 600), 1, 4)
  beam = mod(beam + dt / 9, 1)
  hueT = mod(hueT + dt / 60, 1)
  var amp = style == 0 ? 0.16 : 0.07
  for (var i = 0; i < cells; i++) {
    phx[i] = mod(phx[i] + fx[i] * sp * dt * 2, 1)
    phy[i] = mod(phy[i] + fy[i] * sp * dt * 2, 1)
    ox[i] = mod(ox[i] + (0.7 + fx[i] * 0.5) * sp * dt, asp)
    if (style == 2) {
      sx[i] = mod(bx[i] * asp + ox[i], asp)
      sy[i] = by[i] + 0.04 * sin(PI2 * phy[i])
    } else {
      sx[i] = (bx[i] + amp * sin(PI2 * phx[i])) * asp
      sy[i] = by[i] + amp * sin(PI2 * phy[i])
    }
  }
}

export function renderBulk() {
  var n = cells, R = 0.62 / sqrt(n), lead = 1.1 / CH, half = asp / 2
  var bpos = beam * 2.6 - 0.8
  var wrap = style == 2
  fld = (fld + 1) % nf
  for (var row = fld; row < CH; row += nf) {
    var py = (row + 0.5) / CH, j = row * CW
    for (var col = 0; col < CW; col++) {
      var px = (col + 0.5) / CW * asp
      var d1 = 99, d2 = 99, b = 0, i, dx, dy, d
      if (wrap) {
        for (i = 0; i < n; i++) {
          dx = abs(px - sx[i])
          if (dx > half) dx = asp - dx
          dy = py - sy[i]
          d = dx * dx + dy * dy
          if (d < d2) { if (d < d1) { d2 = d1; d1 = d; b = i } else d2 = d }
        }
      } else {
        for (i = 0; i < n; i++) {
          dx = px - sx[i]
          dy = py - sy[i]
          d = dx * dx + dy * dy
          if (d < d2) { if (d < d1) { d2 = d1; d1 = d; b = i } else d2 = d }
        }
      }
      var f1 = sqrt(d1), f2 = sqrt(d2), edge = f2 - f1, u = saturate(f1 / R)
      if (style == 0) {
        // glowing orbs that fill their cells, darkening to the borders
        var o = 1 - 2 * f1 / (f1 + f2)
        cH[j] = hueT + b * 0.618
        cS[j] = 1 - 0.55 * o * o * o
        cV[j] = o * (0.35 + 0.65 * o)
      } else if (style == 1) {
        // stained glass: jewel pane, lit by a sweeping sunbeam, grainy
        var sun = 1 - smoothstep(0, 0.38, abs((px / asp) * 0.8 + py * 0.6 - bpos))
        cH[j] = jew[b] + (hash(b) - 0.5) * 0.05
        cS[j] = 0.97 - 0.3 * sun * sun
        cV[j] = edge < lead ? 0.02 : (0.36 + 0.64 * sun) * (0.8 + 0.2 * (1 - u)) * (0.86 + 0.14 * hash2(col, row))
      } else {
        // river ice: white floe cores, blue rims, dark water in the cracks
        cH[j] = 0.53 + 0.09 * u
        cS[j] = edge < lead ? 1 : 0.15 + 0.7 * u
        cV[j] = edge < lead ? 0.22 : 1 - 0.45 * u * u
      }
      j++
    }
  }
  fillCanvas(cH, cS, cV, CW, CH, passes > 0 ? 1 : 0)   // bilinear only when upscaling
}
