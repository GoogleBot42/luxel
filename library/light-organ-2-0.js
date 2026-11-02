// name: Light Organ
// tags: both, sound
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/light-organ-2-0.md, light-organ-sensor-board.md — reimagined, not a replica.
// The disco light organ: four colour channels (bass, low, mid, high) tiled
// along the strip in bars, each pulsing with its band against that band's own
// floor and peak, so treble flashes as hard as the kick. Beats flash the group
// seams pure white for 110 ms and (Colour 0) rotate which channel sits in which slot.
// On a panel the four channels are columns that glow and fill from the
// bottom with their level, split by white-flashing seams.

var sensDb = 0
//# min=-20 max=20 step=1 default=0
export function sliderSensitivityDb(v) { sensDb = clamp(v, -20, 20); sensK = pow(10, sensDb / 20) }

var decayMs = 300
//# min=50 max=2000 step=10 default=300
export function sliderDecayMs(v) { decayMs = clamp(v, 50, 2000) }

var barPx = 6
//# min=1 max=32 step=1 default=6
export function sliderBarPixels(v) { barPx = clamp(floor(v), 1, 32) }

// 0 rotating slots (hues drift, beats rotate channels), 1 fixed band hues
// (bass red, low orange, mid green, high violet)
var colorMode = 0
//# min=0 max=1 step=1 default=0
export function sliderColorMode(v) { colorMode = clamp(floor(v), 0, 1) }

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

var sensK = 1
var lo = array(5), env = array(4), floorV = array(4), peakV = array(4), hue = array(4)
lo[0] = 0; lo[1] = 4; lo[2] = 10; lo[3] = 20; lo[4] = 32
var rot = 0, phase = 0, flash = 0, sinceRot = 0

export function beforeRender(delta) {
  soundFrame(delta)
  var keep = pow(0.1, delta / decayMs), pkDecay = pow(0.5, delta / 4000), w = min(1, delta / 2000)
  for (var b = 0; b < 4; b++) {
    var v = 0
    for (var i = lo[b]; i < lo[b + 1]; i++) v = max(v, spec[i])
    floorV[b] += (v - floorV[b]) * w
    peakV[b] = max(v, max(0.15, peakV[b] * pkDecay))
    var f = floorV[b] * 0.8, d = clamp((v - f) / (peakV[b] - f + 0.01) * sensK, 0, 1)
    env[b] = max(d * d, env[b] * keep)
  }
  phase = mod(phase + delta / 1000 / 30, 1)
  sinceRot += delta
  if (sndBeat || sinceRot > 4000) {
    if (sndBeat) flash = 110
    rot = (rot + 1) % 4
    phase = mod(phase + 0.09, 1)
    sinceRot = 0
  }
  flash -= delta
  for (b = 0; b < 4; b++) hue[b] = colorMode ? (b == 0 ? 0 : b == 1 ? 0.07 : b == 2 ? 0.33 : 0.76) : phase + b * 0.25
}

function brush(slot, k) {
  var b = colorMode ? slot : (slot + rot) % 4
  hsv(hue[b], 1, env[b] * k)
  return env[b]
}

function strip() {
  var n = pixelCount, bars = ceil(n / barPx)
  for (var i = 0; i < bars; i++) {
    brush(i % 4, 1)
    fillRange(i * barPx, (i + 1) * barPx)
  }
  if (flash > 0) {
    rgb(1, 1, 1)
    for (var g = 0; g < n; g += barPx * 4) fillRange(g, g + max(1, floor(barPx / 3)))
  }
}

function panel() {
  var gw = gridWidth()
  if (gw < 4) gw = 32
  // each column glows at a third of its level and fills from the bottom
  for (var s = 0; s < 4; s++) {
    var e = brush(s, 0.35)
    fillRect(s / 4, -1, (s + 1) / 4, 2)
    brush(s, 1)
    fillRect(s / 4, 1.01 - e, (s + 1) / 4, 2)
  }
  if (flash > 0) rgb(1, 1, 1)
  else rgb(0, 0, 0)
  for (s = 1; s < 4; s++) fillRect(s / 4 - 0.6 / gw, -1, s / 4 + 0.6 / gw, 2)
}

export function renderBulk() {
  if (has2DMap()) panel()
  else strip()
}
