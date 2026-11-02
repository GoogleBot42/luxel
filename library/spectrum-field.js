// name: Spectrum Field
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/sound-spectrokalidamandala.md, sound-spectro-kalidastrip.md, sound-spectroblots-pow-fade.md, sound-spectromatrix-agc.md, sound-spectromatrix-render2d.md — reimagined, not a replica.
// Maps the 32 spectrum bands onto a drifting field, so each band owns a ring,
// a plasma ribbon, a noise blob or a strip fold, and flares on its onsets
// (energy over its own running average) with a decaying afterglow. Hue follows
// the band plus a mirrored angular term; overdriven bands whiten (capped). A
// fill controller holds the lit share near the target, starting low. Panel: a
// <=32x32 band canvas, bilinear-filled; strip: the mirrored linear fold.

// 0 rings, 1 plasma, 2 noise blobs, 3 linear fold (strips always fold)
var field = 0
//# min=0 max=3 step=1 default=0
export function sliderField(v) { field = clamp(floor(v), 0, 3); dirty = 1 }

var target = 0.2
//# min=5 max=50 step=1 default=20
export function sliderTargetFillPercent(v) { target = clamp(v, 5, 50) / 100 }

var trailMs = 300
//# min=50 max=2000 step=10 default=300
export function sliderTrailMs(v) { trailMs = clamp(v, 50, 2000) }

var driftS = 10
//# min=2 max=60 step=1 default=10
export function sliderDriftSeconds(v) { driftS = clamp(v, 2, 60) }

// rings: colour petals around the centre; strip: mirrored copies along it
var folds = 2
//# min=1 max=8 step=1 default=2
export function sliderMirrorFolds(v) { folds = clamp(floor(v), 1, 8); dirty = 1 }

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

var avg = array(32), env = array(32), sm = array(33)
var CW = 32, CH = 32, dirty = 1
var umap = array(1024), amap = array(1024), hues = array(1024), sats = array(1024), vals = array(1024)
var wx = array(32), wy = array(32)
var gain = 0.6, fillNow = 0, drift = 0, zoom = 1.5, hb = 0, nz = 0, noiseMs = 0, plasmaT = 0

function setupField() {
  dirty = 0
  var k = 0
  for (var r = 0; r < CH; r++) {
    for (var c = 0; c < CW; c++) {
      var x = c / (CW - 1) - 0.5, y = r / (CH - 1) - 0.5
      if (field == 0) { umap[k] = hypot(x, y); amap[k] = 0.5 * triangle(atan2(y, x) / PI2 * folds) }
      else if (field == 3) { umap[k] = abs(y); amap[k] = 0.3 * triangle(x * folds) }
      else amap[k] = 0
      k++
    }
  }
  noiseMs = 1000
}

export function beforeRender(delta) {
  soundFrame(delta)
  var keep = pow(0.5, delta / trailMs), w = min(1, delta / 1500), i
  for (i = 0; i < 32; i++) {
    var s = spec[i]
    avg[i] += (s - avg[i]) * w
    env[i] = max((s * s * 0.5 + max(0, s - avg[i]) * 3) * gain, env[i] * keep)
  }
  // a 1-2-1 smear across neighbouring bands, so a lone spiking bin reads as a
  // band of colour rather than sub-cell beads on the canvas
  for (i = 0; i < 32; i++) sm[i] = (env[max(i - 1, 0)] + env[i] * 2 + env[min(i + 1, 31)]) / 4
  sm[32] = sm[31]
  // hold the lit share near the target: a relative step, so it settles fast
  gain = clamp(gain * (1 + clamp((target - fillNow) * delta * 0.004, -0.1, 0.1)), 0.25, 8)
  drift = mod(drift + delta / 1000 / driftS, 1)
  hb = mod(hb + delta / 1000 / 40, 1)
  zoom = 1.2 + 0.4 * sin(time(0.6) * PI2)
  if (has2DMap()) {
    var gw = gridWidth(), gh = gridHeight()
    if (gw < 2 || gh < 2) { gw = 32; gh = 32 }
    gw = min(gw, 32); gh = min(gh, 32)
    if (gw != CW || gh != CH) { CW = gw; CH = gh; dirty = 1 }
    if (dirty) setupField()
    if (field == 1) {
      plasmaT = mod(plasmaT + delta / 7000, 20)   // 20 = a whole period of every term
      for (i = 0; i < CW; i++) wx[i] = 0.5 * wave(i * 1.3 / CW + plasmaT)
      for (i = 0; i < CH; i++) wy[i] = 0.5 * wave(i * 0.9 / CH - plasmaT * 0.7 + 0.3 * wave(plasmaT * 0.5))
    } else if (field == 2) {
      noiseMs += delta
      if (noiseMs > 250) {
        noiseMs = 0
        nz = mod(nz + 0.015, 400)                 // z ping-pongs, so it never jumps
        fillNoise3D(umap, CW, CH, 1.6 / CW, 1.6 / CH, 0, 0, 200 * triangle(nz / 400), 7)
      }
    }
  }
}

// band coordinate b (0..31) -> hue / whitening / value; returns the value
var cH = 0, cS = 1, cV = 0
function bandColor(b, extra) {
  var i0 = floor(b), e = sm[i0] + (sm[i0 + 1] - sm[i0]) * (b - i0)
  cH = hb + b * 0.026 + extra
  cS = 1 - min(0.6, max(0, e - 1.1))
  cV = min(e * e, 1)
  return cV
}

function panel() {
  var k = 0, sum = 0, z = zoom, b, i0, e, v
  for (var r = 0; r < CH; r++) {
    var yv = wy[r]
    for (var c = 0; c < CW; c++) {
      if (field == 1) b = triangle((wx[c] + yv) * 0.6 - drift) * 31
      else if (field == 2) b = floor(triangle(umap[k] * 0.4 - drift) * 8 + 0.5) * 3.875
      else b = triangle(umap[k] * z - drift) * 31
      i0 = floor(b); e = sm[i0] + (sm[i0 + 1] - sm[i0]) * (b - i0)
      v = min(e * e, 1)
      hues[k] = hb + b * 0.026 + amap[k]; sats[k] = 1 - min(0.6, max(0, e - 1.1)); vals[k] = v
      sum += v
      k++
    }
  }
  fillNow = sum / k
  fillCanvas(hues, sats, vals, CW, CH, 1)
}

function strip() {
  var n = pixelCount, sum = 0
  for (var i = 0; i < n; i++) {
    var p = i / n
    sum += bandColor(triangle(triangle(p * folds) + drift) * 31, 0.15 * p)
    hsv(cH, cS, cV)
    setPixel(i)
  }
  fillNow = sum / n
}

export function renderBulk() {
  if (has2DMap()) panel()
  else strip()
}
