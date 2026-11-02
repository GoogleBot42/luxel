// name: Sound Rays
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/sound-rays.md, sound-rays-frequency-bpm-reactive-1.md — reimagined, not a replica.
// A chart recorder for sound: rays are born at the start of the strip and
// stream along it. Brightness is loudness against its own recent peak (only
// peaks reach full), hue is the spectral centroid (low = red, high = violet),
// and beats stamp white-hot rays. Speed is fixed or follows the detected
// tempo. Strip: a ring of at most 128 samples drawn as gradientRange spans
// with sub-pixel scroll. Panel: one sample per column, a scrolling
// seismograph whose height and brightness are the loudness.

// 0 fixed speed, 1 follow tempo (speed x detected BPM / 120 x Tempo Factor)
var speedMode = 0
//# min=0 max=1 step=1 default=0
export function sliderSpeedMode(v) { speedMode = clamp(floor(v), 0, 1) }

var speed = 40
//# min=5 max=200 step=1 default=40
export function sliderSpeedPixelsPerSecond(v) { speed = clamp(v, 5, 200) }

var tempoK = 1
//# min=0.25 max=4 step=0.25 default=1
export function sliderTempoFactor(v) { tempoK = clamp(v, 0.25, 4) }

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

const CAP = 128
var rh = array(CAP), rs = array(CAP), rv = array(CAP)
for (var q = 0; q < CAP; q++) { rh[q] = 0.6; rs[q] = 1; rv[q] = 0.05 }
var lvlAvg = 0, head = 0, ph = 0, drift = 0, curV = 0, curH = 0, curS = 1, maxV = 0, flash = 0

export function beforeRender(delta) {
  soundFrame(delta)
  var i, w = 0, sw = 0
  for (i = 0; i < 32; i++) { w += spec[i] * i; sw += spec[i] }
  drift = mod(drift + delta / 30000, 1)
  curH = drift + (sw > 0.01 ? w / sw : 8) / 31 * 1.1
  // loudness over its running mean: a steady signal stays lit, music pulses
  lvlAvg += (sndLevel - lvlAvg) * min(1, delta / 1500)
  var f = lvlAvg * 0.75, e = clamp((sndLevel - f) / max(0.05, 1 - f), 0, 1)
  flash = sndBeat ? 1 : flash * pow(0.5, delta / 60)
  curV = max(e * e, flash)
  curS = 1 - flash * 0.6
  // advance the scroll; push the loudest moment of each elapsed cell
  geometry()
  var v = speedMode ? speed * sndBpm / 120 * tempoK : speed
  maxV = max(maxV, curV)
  ph += delta / 1000 * v / L
  while (ph >= 1) {
    ph -= 1
    head = (head + CAP - 1) % CAP
    rh[head] = curH; rs[head] = curS; rv[head] = maxV
    maxV = curV
  }
}

var C = 8, L = 1, GW = 32
function geometry() {
  if (has2DMap()) {
    GW = gridWidth()
    if (GW < 2) GW = 32
    C = min(GW + 1, CAP); L = 1
  } else { C = clamp(floor(pixelCount / 2), 8, CAP); L = pixelCount / (C - 1) }
}

function panel() {
  clear()
  for (var k = -1; k < C - 1; k++) {
    var j = (head + k + CAP) % CAP, x = (k + 1 + ph) / (GW - 1), v = rv[j]
    if (k < 0) { hsv(curH, curS, 0.3 + 0.7 * curV); v = curV }
    else hsv(rh[j] + k * 0.25 / C, rs[j], 0.3 + 0.7 * v)
    var hh = 0.03 + 0.47 * v
    fillRect(x - 0.5 / (GW - 1), 0.5 - hh, x + 0.45 / (GW - 1), 0.5 + hh)
  }
}

export function renderBulk() {
  geometry()
  if (has2DMap()) { panel(); return }
  clear()
  // spans add (mode 1) so the shared end pixels sum to full coverage; hue
  // also slides a little with age, so the stream shifts colour as it travels
  var x0 = 0, h0 = curH, s0 = curS, v0 = curV, ha = 0.25 / C
  for (var k = 0; k < C - 1; k++) {
    var j = (head + k) % CAP, x1 = (k + ph) * L, h1 = rh[j] + (k + 1) * ha
    gradientRange(x0, x1, h0, s0, v0, h1, rs[j], rv[j], 1)
    x0 = x1; h0 = h1; s0 = rs[j]; v0 = rv[j]
  }
}
