// name: Police Lights
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/policelights.md
// — reimagined, not a replica.
// An emergency light bar: left colour / right colour (or alternating blocks) running
// real sequences — alternate, quad flash, a rotator sweep, and takedown white with
// the ends still alternating. Flashes have a fast attack and a short tail. Every
// colour is capped at 4 flashes/s and the two sides never swap faster than 3 Hz.
// Strip: ranges along the index. Panel: a light bar across the middle that spills
// its colour onto the wall above and below.

var seq = 0            // 0 alternate, 1 quad flash, 2 rotator sweep, 3 takedown white
//# min=0 max=3 step=1 default=0
export function sliderSequence(v) { seq = clamp(floor(v), 0, 3) }

var split = 0          // 0 halves, 1 alternating blocks
//# min=0 max=1 step=1 default=0
export function sliderSplit(v) { split = clamp(floor(v), 0, 1) }

var blockPx = 8        // block size in pixels (columns on a panel) for Split 1
//# min=4 max=32 step=1 default=8
export function sliderBlockPixels(v) { blockPx = clamp(floor(v), 4, 32) }

// Side colours. The pickers' brightness is ignored: use the global brightness.
var lh = 0, ls = 1, rh = 0.64, rs = 1
export function hsvPickerLeft(h, s, v) { lh = h; ls = s }
export function hsvPickerRight(h, s, v) { rh = h; rs = s }

var t = 0, la = 0, lb = 0, rot = 0, hold = 0, white = 0

// One flash in a slot of length 1: quick attack, hold, short tail.
function flash(u) {
  if (u < 0 || u >= 1) return 0
  return u < 0.08 ? u / 0.08 : u < 0.6 ? 1 : 1 - smoothstep(0.6, 1, u)
}

export function beforeRender(delta) {
  t = mod(t + min(delta, 100) / 1000, 600)
  white = 0
  if (seq == 0 || seq == 3) {                            // 0.9 s cycle: A, then B
    var p = mod(t / 0.9, 1)
    la = flash(p / 0.42)
    lb = flash((p - 0.5) / 0.42)
    white = seq == 3 ? 1 : 0
  } else if (seq == 1) {                                 // four flashes per side, 3.6/s
    var q = mod(t / 2.2, 1) * 2, k = floor(q), f = mod(q * 4, 1)
    var v = q - k < 0.98 ? flash(f / 0.55) : 0
    la = k == 0 ? v : 0
    lb = k == 1 ? v : 0
  } else {                                               // rotating beacons, 1.25 rev/s
    rot = mod(t * 1.25, 1)
  }
}

// A mapless strip stays a strip: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

// lamp(p0, p1, side, level): one module of the bar between fractions p0..p1
var isPanel = 0, n = 0
function lamp(p0, p1, side, v) {
  if (v <= 0.004) return
  var h = side ? rh : lh, s = side ? rs : ls
  if (!isPanel) { hsv(h, s, v); fillRangeAA(p0 * n, p1 * n, 2); return }
  hsv(h, s, v * 0.22)                                    // spill on the wall
  fillRect(p0, 0, p1, 1)
  hsv(h, s, v * 0.45)
  splat((p0 + p1) / 2, 0.5, max(p1 - p0, 0.2), 1)
  hsv(h, s * 0.85, v)                                    // the lens
  fillRect(p0 + 0.01, 0.38, p1 - 0.01, 0.62)
}

function whiteLamp(p0, p1) {
  if (!isPanel) { rgb(1, 1, 1); fillRangeAA(p0 * n, p1 * n); return }
  rgb(0.2, 0.2, 0.2); fillRect(p0, 0, p1, 1)
  rgb(1, 1, 1); fillRect(p0 + 0.01, 0.38, p1 - 0.01, 0.62)
}

// rotator: a beam turning inside each lamp; bright and wide when facing out
function beacon(p0, p1, side, phase) {
  var a = (rot + phase) * PI2, face = sin(a)
  var v = face > 0 ? 0.25 + 0.75 * face * face : 0.25 * (1 + face) * (1 + face)
  var m = (p0 + p1) / 2, w = (p1 - p0) / 2, c = m + w * 0.7 * cos(a), hw = w * (0.25 + 0.35 * max(face, 0))
  lamp(p0, p1, side, v * 0.18)
  lamp(max(c - hw, p0), min(c + hw, p1), side, v)
}

export function renderBulk() {
  isPanel = has2DMap()
  n = pixelCount
  clear()
  if (isPanel) {                                         // the dark housing of the bar
    rgb(0.04, 0.04, 0.045)
    fillRect(0, 0.36, 1, 0.64)
  }
  var len = isPanel ? max(gridWidth(), 1) : max(n, 1)
  var blocks = split ? max(2, round(len / blockPx)) : 2
  var bw = 1 / blocks
  for (var b = 0; b < blocks; b++) {
    var p0 = b * bw, p1 = p0 + bw, side = split ? b % 2 : (b >= 1 ? 1 : 0)
    if (seq == 3 && (p0 >= 0.25 && p1 <= 0.75 || !split)) {
      if (!split) {                                      // halves: the middle half goes white
        lamp(side ? 0.75 : 0, side ? 1 : 0.25, side, side ? lb : la)
        whiteLamp(side ? 0.5 : 0.25, side ? 0.75 : 0.5)
      } else whiteLamp(p0, p1)
    } else if (seq == 2) beacon(p0, p1, side, split ? b / blocks : side * 0.5)
    else lamp(p0, p1, side, side ? lb : la)
  }
}
