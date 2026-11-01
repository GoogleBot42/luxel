// name: Multisegment Demo
// Clean-room reimplementation from a prose functional description of the
// community pattern "Multisegment Demo"; original source never consulted.
// A framework that splits one 1D strip into up to a dozen independent zones,
// each with its own on/off, color, brightness, effect, size and speed, driven
// either by exported JSON variables or by the web-UI controls (one mode wins).

var MAX_ZONES = 12
var SCRATCH_PER = 3
var EFFECTS = 19

// --- Per-zone records: [on, hue, sat, bright, effect, size, speed] ---
export var z0 = array(7)
export var z1 = array(7)
export var z2 = array(7)
export var z3 = array(7)
export var z4 = array(7)
export var z5 = array(7)
export var z6 = array(7)
export var z7 = array(7)
export var z8 = array(7)
export var z9 = array(7)
export var z10 = array(7)
export var z11 = array(7)
var zones = array(MAX_ZONES)   // references to the above for indexed access

export var activeZones = 4
// Protocol/version: negative => web-UI mode; positive => automation mode.
export var protocolVersion = -1
// Boot handshake: while nonzero the fade-in holds at black.
export var bootHold = 0
// Transition stage: 0 fade-in, 1 steady, 2 fade-out.
export var transitionStage = 0

var scratch = array(MAX_ZONES * SCRATCH_PER)
var zoneStart = array(MAX_ZONES + 1)
var zoneEnabled = array(MAX_ZONES)
var zoneEffBright = array(MAX_ZONES)

var fadeProg = 0        // 0..1 fade progress
var fadeMul = 0         // global brightness multiplier
var rcur = 0            // render-loop zone cursor
var inited = 0

// shared UI latch (mild quirk in the original: not per-zone)
var lastVal = -1

function automationMode() { return protocolVersion > 0 }

function initZones() {
  zones[0] = z0; zones[1] = z1; zones[2] = z2; zones[3] = z3
  zones[4] = z4; zones[5] = z5; zones[6] = z6; zones[7] = z7
  zones[8] = z8; zones[9] = z9; zones[10] = z10; zones[11] = z11
  var per = floor(pixelCount / activeZones)
  if (per < 1) per = 1
  for (var i = 0; i < MAX_ZONES; i++) {
    var zr = zones[i]
    zr[0] = 1                          // on
    zr[1] = i / activeZones            // hue spread around the wheel
    zr[2] = 1                          // saturation
    zr[3] = (i % 2 == 0) ? 1 : 0.35    // alternate full / dim
    zr[4] = 0                          // effect = solid
    zr[5] = per                        // size
    zr[6] = 1                          // speed multiplier (period scale)
  }
}

export function beforeRender(delta) {
  if (!inited) { initZones(); inited = 1 }

  // --- Fade state machine (cubic-eased) ---
  if (bootHold) {
    fadeMul = 0
  } else if (transitionStage == 0) {
    fadeProg += delta / 2000
    if (fadeProg >= 1) { fadeProg = 1; transitionStage = 1 }
    fadeMul = easeInOutCubic(fadeProg)
  } else if (transitionStage == 2) {
    fadeProg -= delta / 1000
    if (fadeProg <= 0) { fadeProg = 0 }
    fadeMul = easeInOutCubic(fadeProg)
  } else {
    fadeMul = 1
  }

  // --- Derived per-frame tables ---
  var acc = 0
  for (var i = 0; i < activeZones; i++) {
    var zr = zones[i]
    zoneStart[i] = acc
    var sz = floor(zr[5])
    var en = (zr[0] != 0) && (sz > 0) && (acc < pixelCount)
    zoneEnabled[i] = en ? 1 : 0
    zoneEffBright[i] = zr[3] * fadeMul
    acc += sz
    if (en) prepEffect(i)
  }
  zoneStart[activeZones] = 999999   // sentinel after the last zone

  rcur = 0
}

// Per-zone effect preparation (advances scratch for stateful effects).
function prepEffect(i) {
  var zr = zones[i]
  var eff = floor(zr[4])
  var b = i * SCRATCH_PER
  if (eff == 1 || eff == 6) {
    // glitter / snow: advance a reseed tick (~4/s glitter, ~3/s snow).
    scratch[b + 0] += 1
  } else if (eff == 10 || eff == 11) {
    // wipe: detect sweep wrap, roll new hue into old, sample fresh hue.
    var sp = zr[6]
    var phase = time(0.008 * sp)
    if (phase < scratch[b + 2]) {
      scratch[b + 0] = scratch[b + 1]        // new -> old
      scratch[b + 1] = frac(time(0.05))      // fresh hue from slow timer
    }
    scratch[b + 2] = phase
  }
}

export function render(index) {
  // Advance the zone cursor on ascending pixel order (sentinel bounds it).
  while (index >= zoneStart[rcur + 1]) rcur += 1

  if (rcur >= activeZones || !zoneEnabled[rcur]) { hsv(0, 0, 0); return }

  var zr = zones[rcur]
  var off = index - zoneStart[rcur]
  var size = floor(zr[5])
  if (size < 1) size = 1
  var h = zr[1]
  var s = zr[2]
  var br = zoneEffBright[rcur]
  var sp = zr[6]
  var eff = floor(zr[4])
  var b = rcur * SCRATCH_PER
  var n = off / size          // 0..1 within zone

  if (eff == 0) {                                   // Solid
    hsv(h, s, br)
  } else if (eff == 1) {                            // Glitter
    var g = hash2(off, scratch[b + 0])
    var gb = g * g * g
    hsv(h, gb > 0.6 ? 0.7 : s, gb * br)
  } else if (eff == 2) {                            // Rainbow bounce
    var shift = triangle(time(0.05 * sp))
    hsv(h + n + shift, s, br)
  } else if (eff == 3) {                            // Mini scanner
    var w = size / 5; if (w < 3) w = 3
    var center = triangle(time(0.03 * sp)) * size
    var d = 1 - abs(off - center) / w
    if (d < 0) d = 0
    hsv(h, s, d * d * br)
  } else if (eff == 4) {                            // Breathe
    var p = 0.1 + wave(time(0.03 * sp)) * 0.9
    hsv(h, s, p * br)
  } else if (eff == 5) {                            // Slow color
    hsv(time(0.08 * sp), s, br)
  } else if (eff == 6) {                            // Snow
    var r = hash2(off, scratch[b + 0])
    if (r > 0.96) hsv(0, 0, br)
    else hsv(h, s, br)
  } else if (eff == 7) {                            // Chaser up
    var v7 = (1 + sin((off * 0.3) - time(0.05 * sp) * PI2)) / 2
    hsv(h, s, v7 * br)
  } else if (eff == 8) {                            // Chaser down
    var v8 = (1 + sin((off * 0.3) + time(0.05 * sp) * PI2)) / 2
    hsv(h, s, v8 * br)
  } else if (eff == 9) {                            // Strobe
    hsv(h, s, square(time(0.004 * sp), 0.25) * br)
  } else if (eff == 10) {                           // Wipe up
    var edge = time(0.008 * sp) * size
    hsv(off < edge ? scratch[b + 1] : scratch[b + 0], s, br)
  } else if (eff == 11) {                           // Wipe down
    var edgeD = (1 - time(0.008 * sp)) * size
    hsv(off > edgeD ? scratch[b + 1] : scratch[b + 0], s, br)
  } else if (eff == 12) {                           // Springy theater chase
    var spacing = 2 + triangle(time(0.06 * sp)) * 8
    var march = frac((off - time(0.03 * sp) * spacing) / spacing)
    hsv(h, s, (march < 0.2 ? 1 : 0) * br)
  } else if (eff == 13) {                           // Color twinkles
    var t1 = time(0.11 * sp), t2 = time(0.07 * sp)
    var hh = sin(off / 9 + t1 * PI2)
    var vv = sin(off / 5 + t2 * PI2)
    vv = vv * vv * vv
    if (vv < 0.2) vv = 0
    hsv(h + hh * 0.3, s, vv * br)
  } else if (eff == 14) {                           // Plasma
    var pv = (1 + sin(n * PI2 * 2 - time(0.05 * sp) * PI2)) / 2
    var ss = clamp(1.2 - pv, 0, 1)
    hsv(h + time(0.15 * sp), ss, pv * pv * pv * br)
  } else if (eff == 15) {                           // Ripples
    var a = abs(triangle(n * 10 - time(0.05 * sp)))
    var c = abs(triangle(n * 6 + time(0.07 * sp)))
    var e = abs(triangle(n * 3 + wave(time(0.09 * sp))))
    var avg = (a + c + e) / 3
    avg = avg * avg
    hsv(h, clamp(1.2 - avg, 0, 1), avg * br)
  } else if (eff == 16) {                           // Spin cycle
    var freq = 2 + wave(time(0.1 * sp)) * 6
    var band = frac(index / size * freq + time(0.05 * sp))
    var dot = triangle(band)
    hsv(h + frac(time(0.12 * sp)) * 0.5, s, dot * dot * dot * br)
  } else if (eff == 17) {                           // Rainbow up
    hsv(h + n - time(0.02 * sp) * 3, s, br)
  } else {                                          // Rainbow down (18)
    hsv(h + n + time(0.02 * sp) * 3, s, br)
  }
}

// ---------------- Web UI controls (no-ops in automation mode) -------------
var activeZone = 0

//# min=0 max=1 step=0.01 default=0
export function sliderActiveZone(v) {
  if (automationMode()) return
  activeZone = floor(v * (activeZones - 0.001))
}

//# min=0 max=1 step=1 default=1
export function sliderZoneOnOff(v) {
  if (automationMode()) return
  var nv = floor(v + 0.5)
  if (nv != lastVal) { zones[activeZone][0] = nv; lastVal = nv }
}

//# min=0 max=1 step=0.01 default=0
export function sliderEffect(v) {
  if (automationMode()) return
  zones[activeZone][4] = floor(v * (EFFECTS - 0.001))
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) {
  if (automationMode()) return
  // Inverted: right = faster (smaller period multiplier), with a floor.
  zones[activeZone][6] = clamp(2 - v * 2, 0.05, 2)
}

//# min=0 max=1 step=0.01 default=0.25
export function sliderZoneSize(v) {
  if (automationMode()) return
  if (activeZone == activeZones - 1) return   // last zone absorbs remainder
  zones[activeZone][5] = floor(v * pixelCount)
}

export function hsvPickerColor(h, s, v) {
  if (automationMode()) return
  var zr = zones[activeZone]
  zr[1] = h; zr[2] = s; zr[3] = v
}

//# min=0 max=1 step=0.01 default=0.25
export function sliderZoneCount(v) {
  if (automationMode()) return
  activeZones = clamp(floor(v * MAX_ZONES) + 1, 1, MAX_ZONES)
  initZones()
}

//# min=0 max=1 step=1 default=1
export function sliderEnableWebUI(v) {
  // On => web-UI mode (negative sentinel); off => automation mode.
  protocolVersion = (v >= 0.5) ? -1 : 1
}
