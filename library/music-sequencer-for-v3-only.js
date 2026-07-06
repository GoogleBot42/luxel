// name: Music Sequencer - for V3 ONLY
// Clean-room reimplementation from a prose functional description of the
// community pattern "Music Sequencer - for V3 ONLY"; original source never
// consulted.

// A music-choreography framework, not a single effect: a queue of
// mini-patterns, each played for a duration in musical beats, driven by a
// beat clock plus audio analysis from the sensor board (volume
// normalization, bass/clap/hi-hat detectors with debounced hooks,
// derivative-based beat detection, tempo estimation). Without audio it
// degrades gracefully: detectors stay quiet, sound-only minis idle or
// skip, clock-driven minis keep time. Content is 1D; the 2D/3D renderers
// forward to the shared 1D renderer.

// ---------------------------------------------------------------- sensors
export var frequencyData = array(32)   // 32-band spectrum
export var energyAverage = 0
export var maxFrequency = 0
export var maxFrequencyMagnitude = 0
export var accelerometer = array(3)
export var light = -1                  // board probe: stays -1 if no board

var sigSeen = 0                        // latched: any real audio observed

// -------------------------------------------------------- analysis state
var vol = 0, emaFast = 0, emaSlow = 0
var maxVol = 0.1, maxHold = 0
var SILENCE = 0.02
var volume01 = 0, loudRatio = 0

var bassNow = 0, bassFast = 0, bassSlow = 0, bassMax = 0.01, bassNorm = 0
var lastBassFast = 0
var DERIV_N = 5
var derivBuf = array(DERIV_N)
var derivIdx = 0

var clapNow = 0, clapAvg = 0.001, clapOn = 0
var hatNow = 0, hatAvg = 0.001, hihatOn = 0

var beatFired = 0                      // this-frame flags
var clapFired = 0, hatFired = 0
var DEBOUNCE_FRAC = 0.2                // min gap between hits, in beats
var beatGap = 9, clapGap = 9, hatGap = 9   // seconds since last hit

// instrument hooks — minis assign lambdas; cleared between entries
var onBeat = 0, onClap = 0, onHihat = 0

// tempo estimation: last 8 beat intervals must agree within ~10%
var ivals = array(8), ivN = 0, lastBeatT = -9
var detectedBpm = 0, tempoReliable = 0

// ------------------------------------------------------------ beat clock
var bpm = 120, beatSec = 0.5
var phraseBeats = 32
var patBeats = 0                       // decimal beat counter, resets/entry
var entryDur = 32
var patProgress = 0, phrasePos = 0     // ramp-ups 0..1
var r1 = 1, r2 = 1, r4 = 1, r8 = 1, r16 = 1   // ramp-DOWNS 1..0 (signature)
var tSec = 0, dt = 0

// ----------------------------------------------------------------- queue
var QMAX = 48
var qFn = array(QMAX), qBeats = array(QMAX), qMode = array(QMAX)
var qArg = array(QMAX), qLen = 0, qPos = 0
var MODE_FIXED = 0, MODE_BEAT = 1, MODE_LOUD = 2, MODE_CMD = 3, MODE_PRED = 4
var setupDone = 0, skipEntry = 0, edgeLatch = 0
var LATENCY_SKIP = 0.08                // beats skipped into the next entry

function add(fn, beats, mode, arg) {
  qFn[qLen] = fn; qBeats[qLen] = beats; qMode[qLen] = mode; qArg[qLen] = arg
  qLen++
}
function cmd(fn, arg) { add(fn, 0, MODE_CMD, arg) }

// -------------------------------------------------------- shared helpers
var N1 = pixelCount + 1                // +1 so interpolation loops are safe
var hueA = array(N1), satA = array(N1), briA = array(N1)
var themeHue = 0.55, dir = 1
var renderFn = 0

function dpos(p) { return dir > 0 ? p : 1 - p }
function near(a, b, hw) {              // 1 when coincident, 0 at half-width
  var d = clamp(1 - abs(a - b) / hw, 0, 1)
  return d * d                         // squared: gamma-ish soft edge
}
function hueWarp(h) { return wave(h / 2 - 0.25) }  // perceptual respread
function decayBri(sec) {               // fade briA toward 0, ~sec to dark
  feedback(briA, max(0, 1 - 3 * dt / max(sec, 0.05)))
}
var polarOut = array(2)
function polarAdd(h1, b1, h2, b2) {    // mix two hue/level pairs as vectors
  var x = b1 * cos(h1 * PI2) + b2 * cos(h2 * PI2)
  var y = b1 * sin(h1 * PI2) + b2 * sin(h2 * PI2)
  polarOut[0] = atan2(y, x) / PI2
  polarOut[1] = hypot(x, y)
}
function risingEdge(v) {               // run-once edge trigger
  var e = v && !edgeLatch
  edgeLatch = v != 0
  return e
}
function noteNumber() {                // semitones above A1 (55 Hz)
  if (maxFrequency < 30) return -1
  return 12 * log2(maxFrequency / 55)
}
var gainI = 1                          // PI auto-gain for brightness
function autoGain(fill, target) {
  gainI = clamp(gainI + (target - fill) * dt * 2, 0.2, 8)
  return gainI
}

function resetShared() {               // between-entries reset
  arrayReplace(hueA, 0); arrayReplace(satA, 0); arrayReplace(briA, 0)
  onBeat = 0; onClap = 0; onHihat = 0
  edgeLatch = 0; setupDone = 0; skipEntry = 0
}

// ---------------------------------------------------------- queue engine
function nextEntry(skip) {
  qPos = qPos + 1
  if (qPos >= qLen) qPos = 0           // loop forever
  patBeats = skip
  resetShared()
}

function runQueue() {
  if (qLen == 0) return
  var guard = 0
  while (qMode[qPos] == MODE_CMD && guard < QMAX) {
    var cf = qFn[qPos]
    cf(qArg[qPos])                     // one-shot command, advance at once
    nextEntry(0)
    guard++
  }
  entryDur = qBeats[qPos] > 0 ? qBeats[qPos] : phraseBeats
  var adv = patBeats >= entryDur || skipEntry
  var early = 0
  if (qMode[qPos] == MODE_BEAT && beatFired) { adv = 1; early = 1 }
  if (qMode[qPos] == MODE_LOUD && loudRatio > 2.5 && vol > SILENCE) {
    adv = 1; early = 1
  }
  if (qMode[qPos] == MODE_PRED && qArg[qPos]) {
    var pf = qArg[qPos]
    if (pf()) adv = 1
  }
  if (adv) {
    nextEntry(early * LATENCY_SKIP)    // early: skip in, land on the beat
    entryDur = qBeats[qPos] > 0 ? qBeats[qPos] : phraseBeats
  }
  var sf = qFn[qPos]                   // per-frame setup; assigns renderFn
  if (sf) sf()
}

// -------------------------------------------------------------- commands
function cSetTempo(v) { bpm = v; beatSec = 60 / bpm }
function cSetPhrase(v) { phraseBeats = v }
function cSetTheme(v) { themeHue = v }
function cSetDir(v) { dir = v }
function cSeedBass(v) { bassMax = v }  // pre-seed AGC so first beats detect

// -------------------------------------------------------- sound analysis
function analyze(deltaSec) {
  var fSum = arraySum(frequencyData)
  if (fSum > 0.0005 || energyAverage > 0.0005) sigSeen = 1

  // volume normalization: scaled energy vs. EMAs and a peak-hold max
  vol = energyAverage * 10
  emaFast += (vol - emaFast) * min(1, deltaSec)          // ~1 s
  emaSlow += (vol - emaSlow) * min(1, deltaSec / 5)      // ~5 s
  if (vol > maxVol) { maxVol = vol; maxHold = 0 }
  else {
    maxHold += deltaSec
    if (maxHold > 70) maxVol *= 1 - 0.05 * deltaSec      // longer than a bridge
  }
  volume01 = emaFast > SILENCE ? clamp(emaFast / max(maxVol, 0.01), 0, 1) : 0
  loudRatio = vol / max(emaSlow, 0.005)

  // bass / kick: lowest bins, fast+slow EMA, self-decaying max (AGC)
  bassNow = frequencyData[0] + frequencyData[1] + frequencyData[2]
  bassFast += (bassNow - bassFast) * min(1, deltaSec / 0.06)
  bassSlow += (bassNow - bassSlow) * min(1, deltaSec / 2)
  bassMax = max(bassMax * (1 - 0.02 * deltaSec), 0.005)
  if (bassFast > bassMax) bassMax = bassFast
  bassNorm = clamp(bassFast / bassMax, 0, 1)

  // rising-derivative beat detection (not absolute level)
  derivBuf[derivIdx] = (bassFast - lastBassFast) / bassMax
  derivIdx = (derivIdx + 1) % DERIV_N
  lastBassFast = bassFast
  var dm = arraySum(derivBuf) / DERIV_N
  beatFired = 0
  beatGap += deltaSec
  if (dm > 0.03 && bassNow > 0.001 && beatGap > DEBOUNCE_FRAC * beatSec) {
    beatFired = 1
    // tempo: 8 consecutive intervals agreeing within ~10% -> integer BPM
    var iv = beatGap
    beatGap = 0
    if (iv > 3) ivN = 0                // long silence: restart collection
    else {
      ivals[ivN % 8] = iv
      ivN++
      if (ivN >= 8) {
        var mean = arraySum(ivals) / 8
        var v2 = 0
        for (var k = 0; k < 8; k++) {
          var e = ivals[k] - mean
          v2 += e * e
        }
        var rel = sqrt(v2 / 8) / max(mean, 0.01)
        if (rel < 0.1) {
          detectedBpm = floor(60 / mean + 0.5)   // integer: released music
          tempoReliable = 1
        }
      }
    }
    if (onBeat) onBeat()
  }

  // claps/snare: upper-middle bins vs. slow average
  clapNow = frequencyData[18] + frequencyData[19] + frequencyData[20] +
            frequencyData[21] + frequencyData[22]
  clapAvg += (clapNow - clapAvg) * min(1, deltaSec / 3)
  clapOn = clapNow > clapAvg * 2.2 && clapNow > 0.001
  clapFired = 0
  clapGap += deltaSec
  if (clapOn && clapGap > DEBOUNCE_FRAC * beatSec) {
    clapFired = 1; clapGap = 0
    if (onClap) onClap()
  }

  // hi-hat: top bins, scaled up (raw readings there are tiny)
  hatNow = (frequencyData[30] + frequencyData[31]) * 32
  hatAvg += (hatNow - hatAvg) * min(1, deltaSec / 3)
  hihatOn = hatNow > hatAvg * 2 && hatNow > 0.001
  hatFired = 0
  hatGap += deltaSec
  if (hihatOn && hatGap > DEBOUNCE_FRAC * beatSec) {
    hatFired = 1; hatGap = 0
    if (onHihat) onHihat()
  }
}

// ---------------------------------------------------------- mini-patterns
// Each is a per-frame setup fn queued via add(); it must assign renderFn.

function fnBlack(i, p) { hsv(0, 0, 0) }
function pOff() { renderFn = fnBlack }

function fnProgress(i, p) {
  hsv(themeHue, 1, (dpos(p) < progFill) * (0.35 + 0.65 * r4 * r4))
}
function pProgress() {                 // fill over the queued duration
  progFill = patProgress
  renderFn = fnProgress
}
function pMeasure() {                  // fill over one measure, repeating
  progFill = 1 - r1
  renderFn = fnProgress
}

function fnSweep(i, p) {
  hsv(themeHue, 1, near(dpos(p), 1 - r4, 0.08))
}
function pSweep() { renderFn = fnSweep }   // soft dot, once per beat

function fnQuarters(i, p) {
  var ridge = triangle(p / 2 + 0.25)   // whole-strip triangular ridge
  hsv(themeHue + p * 0.08, 1, ridge * r4 * r4 * r4)
}
function pQuarters() { renderFn = fnQuarters }

function fnEighths(i, p) {
  var seg = floor(dpos(p) * 7.99)
  var cur = floor(frac(patBeats / 4) * 8)
  hsv(themeHue + (1 - r1) * 0.25, 0.75 + 0.25 * r1, (seg == cur) * r8)
}
function pEighths() { renderFn = fnEighths }

function fnStrobe(i, p) { hsv(0, 0, r16 > 0.8) }
function pStrobe() { renderFn = fnStrobe }  // photosensitivity warning!

function fnSurge(i, p) {               // emanates from center every 2 beats
  var c = abs(p - 0.5) * 2
  var reach = 1 - r2 * r2              // smoothed half-note ramp, folded
  hsv(themeHue + patProgress * 0.2 + c * 0.05, 1,
      clamp((reach - c) * 3, 0, 1) * (0.3 + 0.7 * r2))
}
function pSurge() { renderFn = fnSurge }

function fnDancing(i, p) {
  hsv(danceHue, 1, near(p, dancePos, danceW))
}
function pDancing() {                  // layered-oscillation bright dot
  var wander = 0.5 + 0.27 * sin(tSec * 0.6)
  var wiggle = r4 * r4 * r4 * 0.12 * sin(tSec * 25)
  var jitter = (square(patBeats * 2, 0.5) - 0.5) * 0.12 * (1 - patProgress)
  var bassT = bassNorm * 0.18 * patProgress * sin(tSec * 5)
  dancePos = clamp(wander + wiggle + jitter + bassT, 0.02, 0.98)
  danceW = 0.035 + bassNorm * 0.08     // width breathes with the bass
  danceHue = themeHue + (patProgress > 0.5) * 0.5   // complement at halfway
  renderFn = fnDancing
}

function fnScope(i, p) {
  hsv(scopeHue, 1, near(p, scopePos, 0.05))
}
function pBassScope() {                // decaying bass "waveform", 1/2-note
  var ph = 1 - r2                      // 0..1 across two beats
  var amp = r2 * r2 * r2 * 0.38       // cubic amplitude decay
  scopePos = 0.5 + amp * sin(ph * PI2 * (7 - 4 * ph))  // downward chirp
  scopeHue = themeHue + floor((1 - r1) * 4) * 0.12     // hue steps per beat
  renderFn = fnScope
}

// paint fizzle — texture brush; beats start random sputtering strokes,
// claps extend them, everything decays (faster when loud)
var fizzPos = 0, fizzDir = 1, fizzHue = 0
function fizzSputter(n) {
  for (var k = 0; k < n; k++) {
    var idx = floor(fizzPos * (pixelCount - 1)) + k * fizzDir
    if (idx < 0 || idx >= pixelCount) break
    if (random(1) < 0.8) {             // ~4/5 of alternating pixels
      briA[idx] = 0.25 + random(0.75)  // sparkly random brightnesses
      hueA[idx] = fizzHue
    }
  }
}
function pFizzle() {
  if (!setupDone) {
    setupDone = 1
    onBeat = () => {
      fizzPos = random(1)
      fizzDir = random(1) < 0.15 ? -1 : 1          // occasionally reversed
      fizzHue = time(0.3) + (random(1) < 0.5) * 0.07
      fizzSputter(floor(4 + random(0.25) * pixelCount))
    }
    onClap = () => { fizzSputter(6) }  // claps extend the stroke
  }
  feedback(briA, max(0, 1 - dt * (0.8 + volume01 * 2.5)))
  renderFn = fnFizzle
}
function fnFizzle(i, p) {
  var b = briA[i]
  var shimmer = 0.9 + 0.1 * wave(p * 3 + tSec * 0.4)  // slow spatial ripple
  hsv(hueA[i], 1 - b * 0.45, b * b * shimmer)         // bright grains whiten
}

// build-up: segments multiply as the drop approaches, random bitmask flash
var segMask = array(32), segN = 2, lastTickI = -1
function pBuildUp() {
  var remaining = entryDur - patBeats
  var n = clamp(floor(5 - remaining / 4), 1, 5)
  segN = pow(2, n)
  var subdiv = remaining < 3 ? 4 : (remaining < 6 ? 2 : 1)  // beat/8th/16th
  var tickI = floor(patBeats * subdiv)
  if (tickI != lastTickI || !setupDone) {
    setupDone = 1
    lastTickI = tickI
    var lit = 0
    for (var s = 0; s < segN; s++) {
      segMask[s] = random(1) < 0.5
      lit += segMask[s]
    }
    if (lit == 0) segMask[floor(random(segN))] = 1   // at least one lit
  }
  renderFn = fnBuildUp
}
function fnBuildUp(i, p) {
  var remaining = entryDur - patBeats
  hsv(themeHue + phrasePos, remaining < 2 ? 0 : 1,   // white for the drop
      segMask[floor(dpos(p) * (segN - 0.01))] * (0.4 + 0.6 * r8))
}

// ocean ("budget Pacifica"): layered slow waves, cubed, volume-scaled
function fnOcean(i, p) {
  var w1 = wave(p * 1.5 + tSec * 0.05)
  var w2 = wave(p * 3.3 - tSec * 0.08 + wave(tSec * 0.011) * 1.5)
  var w3 = wave(p * 7 + tSec * 0.03)
  var b = (w1 + w2 + w3) / 3
  b = b * b * b
  var crest = near(p, frac(tSec * 0.14) * 1.2 - 0.1, 0.04) *
              square(tSec * 0.09, 0.12)              // occasional white pulse
  var late = max(0, patProgress - 0.7) * 0.3         // hue drifts late
  hsv(themeHue + w2 * 0.05 + late, 1 - crest,
      min(1, b * (0.25 + 0.75 * volume01) + crest * 0.8))
}
function pOcean() { renderFn = fnOcean }

// splotch on beat: random soft blob (red/pink; cyan on the hi-freq
// variant) decaying exponentially; hats/claps flick the strip ends
var blobC = 0.5, blobW = 0.08, blobHue = 0.95, blobB = 0
function pSplotch() {
  if (!setupDone) {
    setupDone = 1
    onBeat = () => {
      blobC = random(1)
      blobW = 0.04 + random(0.1)
      blobHue = hihatOn ? 0.5 : 0.93 + random(0.05)
      blobB = 1
    }
  }
  blobB *= max(0, 1 - dt * 2.5)
  renderFn = fnSplotch
}
function fnSplotch(i, p) {
  var b = blobB * near(p, blobC, blobW)
  if (hihatOn && p > 0.92 && i % 3 == 0) { hsv(0.09, 0.5, random(1)); return }
  if (clapOn && p < 0.08 && i % 3 == 0) { hsv(0.09, 0.5, random(1)); return }
  hsv(blobHue, 1, b)
}

// spectrum analyzer: ~20 bins along the strip through the PI auto-gain
var BINS = 20
var binLvl = array(BINS)
function pAnalyzer() {
  if (!sigSeen) { skipEntry = 1 }      // skips itself without a board
  var fill = 0
  for (var b2 = 0; b2 < BINS; b2++) {
    var raw = frequencyData[floor(b2 * 32 / BINS)] * (2 + b2 * 0.5)
    binLvl[b2] += (raw - binLvl[b2]) * min(1, dt / 0.15)
    fill += clamp(binLvl[b2] * gainI, 0, 1)
  }
  autoGain(fill / BINS, 0.15 + volume01 * 0.3)       // target fill ~ volume
  renderFn = fnAnalyzer
}
function fnAnalyzer(i, p) {
  var b = floor(p * (BINS - 0.01))
  var lvl = clamp(binLvl[b] * gainI, 0, 1)
  var peaky = clamp(loudRatio - 1, 0, 1)
  // second half: desaturated marker tracks the dominant frequency
  if (patProgress > 0.5 && maxFrequency > 30) {
    var mp = clamp(log2(maxFrequency / 60) / 8, 0, 1)
    if (abs(p - mp) < 0.02) { hsv(0, 0.15, 0.6); return }
  }
  hsv(hueWarp(p) * 0.6 + peaky * 0.3, 1, lvl * lvl)
}

// elastic: springy particle chain chasing a beat-jumped target
var EN = 5
var epx = array(EN), evx = array(EN)
var elasticTarget = 0.5, lastWholeBeat = -1
function pElastic() {
  if (!setupDone) {
    setupDone = 1
    for (var k = 0; k < EN; k++) { epx[k] = 0.5; evx[k] = 0 }
    onBeat = () => {
      var t2 = random(1)
      if (abs(t2 - elasticTarget) < 0.125) t2 = frac(t2 + 0.5) // jump >= 1/8
      elasticTarget = t2
    }
  }
  if (!sigSeen) {                      // no board: once per beat by clock
    var wb = floor(patBeats)
    if (wb != lastWholeBeat) {
      lastWholeBeat = wb
      elasticTarget = 0.3 + random(0.4)   // wander around the center
    }
  }
  var stiff = 6 + patProgress * 14     // spring constant stiffens over slot
  var d2 = min(dt, 0.05)
  evx[0] += (elasticTarget - epx[0]) * stiff * d2
  for (var k2 = 1; k2 < EN; k2++) {
    var stretch = epx[k2 - 1] - epx[k2]
    stretch -= sign(stretch) * min(abs(stretch), 0.04)   // rest length
    evx[k2] += stretch * stiff * d2
    evx[k2 - 1] -= stretch * stiff * d2 * 0.5
  }
  // trails linger periodically: decay time itself oscillates over beats
  decayBri(0.15 + 0.5 * wave(patBeats / 24))
  for (var k3 = 0; k3 < EN; k3++) {
    evx[k3] *= max(0, 1 - 3 * d2)      // friction
    epx[k3] = clamp(epx[k3] + evx[k3] * d2 * 8, 0, 1)
    var fi = epx[k3] * (pixelCount - 1)
    var i0 = floor(fi)
    var ff = fi - i0
    var hue2 = 0.78 + k3 * volume01 * 0.05  // violet base, volume splay
    if (1 - ff > briA[i0]) { briA[i0] = 1 - ff; hueA[i0] = hue2 }
    if (ff > briA[i0 + 1]) { briA[i0 + 1] = ff; hueA[i0 + 1] = hue2 }
  }
  renderFn = fnScratch
}
function fnScratch(i, p) { hsv(hueA[i], 1, briA[i]) }

// ------------------------------------------------------- the demo show
// (a condensed tour in the spirit of the shipped sequence)
cmd(cSetTempo, 120)
cmd(cSetPhrase, 32)
add(pOff, 128, MODE_LOUD)              // hold dark until any sound
cmd(cSeedBass, 0.3)                    // pre-seed AGC for the first beats
cmd(cSetTheme, 0.55)                   // light blue
add(pOcean, 32, MODE_BEAT)             // ocean until a beat is detected
add(pFizzle, 32)
add(pBuildUp, 16)
add(pSurge, 16)
add(pFizzle, 16)
cmd(cSetTheme, 0.78)                   // violet
add(pOcean, 16)
cmd(cSetTheme, 0)                      // red
add(pOff, 16, MODE_BEAT)               // wait on black for the downbeat
add(pProgress, 8)
add(pSweep, 8)
cmd(cSetDir, -1)
add(pSweep, 8)
cmd(cSetDir, 1)
add(pQuarters, 8)
add(pEighths, 8)
add(pBassScope, 8)
add(pOff, 4)
add(pStrobe, 4)
cmd(cSetTheme, 0.33)                   // green
add(pDancing, 16)
add(pBassScope, 8)
add(pElastic, 32)
add(pSplotch, 16)
add(pMeasure, 8)
add(pAnalyzer, 64)                     // long analyzer stint, then loop
// To drive minis by hand, replace the show with:
//   add(pManual, 4)   // and let sliderChooseManualPattern pick

// manual-pattern debugging aid (takes effect only via pManual above)
var manualFns = array(16)
manualFns[0] = pOff;      manualFns[1] = pProgress; manualFns[2] = pSweep
manualFns[3] = pQuarters; manualFns[4] = pEighths;  manualFns[5] = pStrobe
manualFns[6] = pSurge;    manualFns[7] = pDancing;  manualFns[8] = pBassScope
manualFns[9] = pFizzle;   manualFns[10] = pBuildUp; manualFns[11] = pOcean
manualFns[12] = pSplotch; manualFns[13] = pAnalyzer; manualFns[14] = pElastic
manualFns[15] = pMeasure
var manualSel = 0
export function sliderChooseManualPattern(v) {
  //# min=0 max=1 step=0.0667 default=0
  manualSel = floor(v * 15.99)
  resetShared()
}
function pManual() { manualFns[manualSel]() }

// --------------------------------------------------------------- engine
export function beforeRender(delta) {
  dt = delta / 1000
  tSec += dt
  if (tSec > 3600) tSec -= 3600

  analyze(dt)

  patBeats += dt / beatSec
  patProgress = clamp(patBeats / entryDur, 0, 1)
  phrasePos = frac(patBeats / phraseBeats)
  r16 = 1 - frac(patBeats * 4)
  r8 = 1 - frac(patBeats * 2)
  r4 = 1 - frac(patBeats)
  r2 = 1 - frac(patBeats / 2)
  r1 = 1 - frac(patBeats / 4)

  runQueue()
}

export function render(index) {
  if (renderFn) renderFn(index, index / pixelCount)
  else hsv(0, 0, 0)
}
export function render2D(index, x, y) { render(index) }
export function render3D(index, x, y, z) { render(index) }
