// name: Spectrum Analyzer
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/sound-spectrum-analyser.md, sound-music-spectrum-visualizer.md, audio-volume-meter.md — reimagined, not a replica.
// The bar-graph analyser with falling peak caps, or (Mode 1) a loudness VU.
// Panel: bars rise from the bottom; the VU is a square that grows from the
// centre. Strip: each band is a segment that fills along the strip; the VU
// grows out from the middle. Colour is laid down once and masked with black
// rects, so a frame is a handful of fills at any pixel count.

// 0 bands, 1 level (VU meter)
var mode = 0
//# min=0 max=1 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 1) }

var bands = 16
//# min=4 max=32 step=1 default=16
export function sliderBands(v) { bands = clamp(floor(v), 4, 32) }

var holdMs = 1000
//# min=0 max=3 step=0.1 default=1
export function sliderPeakHoldSeconds(v) { holdMs = clamp(v, 0, 3) * 1000 }

// 0 scrolling rainbow, 1 green-to-red by height, 2 the picker's colour
var colorMode = 0
//# min=0 max=2 step=1 default=0
export function sliderColorMode(v) { colorMode = clamp(floor(v), 0, 2) }

// Colour mode 2. The picker's V is ignored: brightness is the device's setting.
var pickH = 0.55, pickS = 0.9
export function hsvPickerBarColor(h, s, v) { pickH = h; pickS = s }

// Mapless strips stay strips: this export suppresses the default grid.
export function render(index) { hsv(0, 0, 0) }

// ---- sound front-end (the same block in every library sound pattern) ----
// spec[0..31]: the bins, treble-tilted and auto-gained so recent PEAKS read 1.
// sndLevel: loudness 0..1 against its own peak; sndBeat: 1 on a bass-onset
// frame; sndBpm: tempo estimate. No sensor, or 1.5 s of silence, crossfades
// to a built-in 120 BPM demo groove, so the pattern never sits dark.
export var frequencyData = array(32)
var spec = array(32)
var sndPeak = 0.3, sndLvlPk = 1, sndLevel = 0, sndBeat = 0, sndBpm = 120
var sndAvg = 0, sndPrev = 0, sndSince = 0, sndQuiet = 9999, sndDemo = 1, sndClock = 0
function soundFrame(delta) {
  var i, v, raw = 0, pk = 0, sum = 0
  for (i = 0; i < 32; i++) raw = max(raw, frequencyData[i])
  sndQuiet = raw > 0.01 ? 0 : min(sndQuiet + delta, 9999)
  sndDemo = clamp(sndDemo + (sndQuiet > 1500 ? delta / 1000 : -delta / 150), 0, 1)
  sndClock = mod(sndClock + delta / 500, 64)
  var ph = frac(sndClock), kick = pow(1 - ph, 4), hat = pow(1 - frac(sndClock * 2), 6)
  var snare = floor(sndClock) % 2 ? pow(1 - ph, 2) * 0.6 : 0
  var lead = 14 + floor(8 * wave(sndClock / 16))
  for (i = 0; i < 32; i++) {
    v = 0.06 * wave(i * 0.13 - sndClock * 0.02)
    v += i < 6 ? kick * (1 - i / 7) : (i < 18 ? snare * wave(i * 0.37) : hat * 0.35 * wave(i * 0.29))
    if (i == lead || i == lead + 5) v += 0.4 - ph * 0.2
    v = mix(frequencyData[i], v * 0.5, sndDemo) * (1 + i / 16)
    spec[i] = v
    pk = max(pk, v)
    sum += v
  }
  sndPeak = max(pk, max(0.04, sndPeak * pow(0.5, delta / 2500)))
  sndLvlPk = max(sum, max(0.3, sndLvlPk * pow(0.5, delta / 4000)))
  sndLevel = sum / sndLvlPk
  arrayScale(spec, 1 / sndPeak)
  var bass = (spec[0] + spec[1] + spec[2] + spec[3]) / 4
  sndAvg += (bass - sndAvg) * min(1, delta / 300)
  sndSince += delta
  sndBeat = 0
  if (bass > sndAvg * 1.25 + 0.1 && bass > sndPrev && sndSince > 250) {
    v = sndSince
    while (v > 860) v /= 2
    if (sndSince < 2000) sndBpm = mix(sndBpm, 60 / (max(v, 330) / 1000), 0.3)
    sndSince = 0
    sndBeat = 1
  }
  sndPrev = bass
}
// ---- end sound front-end ----

var lvl = array(33), pk = array(33), pkT = array(33)   // slot 32 = the VU level
var scroll = 0, GW = 32, GH = 32

function track(i, v, delta) {
  lvl[i] = v > lvl[i] ? v : max(v, lvl[i] - delta / 350)
  if (lvl[i] >= pk[i]) { pk[i] = lvl[i]; pkT[i] = holdMs }
  else if (pkT[i] > 0) pkT[i] -= delta
  else pk[i] = max(lvl[i], pk[i] - delta / 900)
}

export function beforeRender(delta) {
  soundFrame(delta)
  scroll = mod(scroll + delta / 9000, 1)
  for (var b = 0; b < bands; b++) {
    var lo = floor(b * 32 / bands), hi = max(lo + 1, floor((b + 1) * 32 / bands)), v = 0
    for (var i = lo; i < hi; i++) v = max(v, spec[i])
    track(b, v, delta)
  }
  track(32, sndLevel, delta)
}

// brush for a bar/ring at fraction f along the colour axis
function tint(f, v) {
  if (colorMode == 0) hsv(scroll + f * 0.8, 1, v)
  else if (colorMode == 1) hsv(0.36 - 0.36 * f, 1, v)
  else hsv(pickH, pickS, v)
}

// grid cells [c0, c1) x [r0, r1) with the brush (cell c sits at c / (GW - 1))
function cells(c0, c1, r0, r1) {
  if (c1 > c0 && r1 > r0)
    fillRect((c0 - 0.45) / (GW - 1), (r0 - 0.45) / (GH - 1), (c1 - 0.55) / (GW - 1), (r1 - 0.55) / (GH - 1))
}

function panelBars() {
  if (colorMode == 0) fillGradient(scroll, 1, 1, scroll + 0.8, 1, 1, 1)
  else if (colorMode == 1) fillGradient(0, 1, 1, 0.36, 1, 1, 2)
  else { hsv(pickH, pickS, 1); fill() }
  var n = min(bands, GW), b, c0, c1, g
  rgb(0, 0, 0)
  for (b = 0; b < n; b++) {
    c0 = floor(b * GW / n); c1 = floor((b + 1) * GW / n)
    cells(c0, c1, 0, GH - round(lvl[b] * GH))
    if (c1 - c0 >= 3) cells(c1 - 1, c1, 0, GH)
  }
  hsv(0, 0, 1)
  for (b = 0; b < n; b++) {
    var r = round(pk[b] * GH)
    if (r < 1) continue
    c0 = floor(b * GW / n); c1 = floor((b + 1) * GW / n); g = c1 - c0 >= 3 ? 1 : 0
    cells(c0, c1 - g, GH - r, GH - r + 1)
  }
}

// concentric rings, lit from the centre out to the level, peak = white frame
function box(s) { fillRect(0.5 - s / 2, 0.5 - s / 2, 0.5 + s / 2, 0.5 + s / 2) }
function panelVU() {
  clear()
  var steps = max(4, floor(min(GW, GH) / 6)), gap = 1.2 / min(GW, GH), P = pk[32], L = lvl[32]
  if (P > 0.05) { hsv(0, 0, 1); box(P + gap); rgb(0, 0, 0); box(P - gap) }
  for (var k = steps; k > 0; k--) {
    var f = k / steps, on = clamp((L - f + 1 / steps) * steps, 0, 1)
    if (on <= 0) continue
    tint(f, on)
    box(f)
    rgb(0, 0, 0)
    box(f - 1 / steps + gap)
  }
}

function stripBars() {
  clear()
  var L = pixelCount / bands, g = L >= 4 ? 1 : 0, len = L - g
  for (var b = 0; b < bands; b++) {
    var s0 = b * L, f = b / bands, v = lvl[b]
    tint(colorMode == 1 ? 0 : f, 0.06)
    fillRangeAA(s0, s0 + len)
    if (colorMode == 1) gradientRange(s0, s0 + v * len, 0.36, 1, 1, 0.36 - 0.36 * v, 1, 1)
    else { tint(f, 1); fillRangeAA(s0, s0 + v * len) }
    var p = s0 + pk[b] * len
    if (p - s0 >= 1) { hsv(0, 0, 1); fillRange(ceil(p) - 1, ceil(p)) }
  }
}

function stripVU() {
  clear()
  var c = pixelCount / 2, e = lvl[32] * c, p = pk[32] * c, h1
  if (colorMode == 1) {
    h1 = 0.36 - 0.36 * lvl[32]
    gradientRange(c, c + e, 0.36, 1, 1, h1, 1, 1)
    gradientRange(c, c - e, 0.36, 1, 1, h1, 1, 1)
  } else {
    var s = colorMode == 0 ? 1 : pickS, h0 = colorMode == 0 ? scroll : pickH
    h1 = colorMode == 0 ? scroll + 0.8 * lvl[32] : pickH
    gradientRange(c, c + e, h0, s, 1, h1, s, 1)
    gradientRange(c, c - e, h0, s, 1, h1, s, 1)
  }
  if (p >= 1) { hsv(0, 0, 1); fillRange(ceil(c + p) - 1, ceil(c + p)); fillRange(floor(c - p), floor(c - p) + 1) }
}

export function renderBulk() {
  if (has2DMap()) {
    GW = gridWidth(); GH = gridHeight()
    if (GW < 2 || GH < 2) { GW = 32; GH = 32 }
    if (mode == 0) panelBars()
    else panelVU()
  } else if (mode == 0) stripBars()
  else stripVU()
}
