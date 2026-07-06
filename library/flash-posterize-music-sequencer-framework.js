// name: Flash Posterize + Music Sequencer framework
// Clean-room reimplementation from a prose functional description of the
// community pattern "Flash Posterize + Music Sequencer framework";
// original source never consulted.

// Two things in one file:
//   A) a reusable music-sequencing framework: a declarative queue of
//      sub-patterns with beat-locked timers, sensor-board sound
//      processing (volume normalization, bass/clap/hi-hat detectors,
//      derivative-based beat detection, tempo estimation), and shared
//      helpers for sub-patterns;
//   B) one demo effect, "flash posterize": a drifting rainbow gradient
//      that flips between smooth and stained-glass (posterized segments
//      with dark separators) on every detected kick drum.
// Without a sensor board (or in silence) the detectors never fire and it
// simply shows the smooth drifting gradient.
//
// Divergence from the original noted in its description: continue modes
// are explicit enums (predicates use M_PRED) rather than duck-typed, and
// the framework is kept modular instead of replicating the original
// platform's global-count ceiling.

// ===== sensor bindings (engine overwrites; zeros when absent) =====
export var frequencyData = array(32)   // 32-band spectrum
export var energyAverage = 0           // overall sound energy
export var maxFrequency = 0            // dominant frequency (Hz)
export var maxFrequencyMagnitude = 0
export var accelerometer = array(3)
export var light = -1                  // impossible sentinel: any change
var boardSeen = 0                      //  from -1 marks a board present

// ===== shared per-pixel scratch (one per pixel + spare for interpolation)
var shHue = array(pixelCount + 1)
var shSat = array(pixelCount + 1)
var shVal = array(pixelCount + 1)

// ===== musical timers =====
var bpm = 120                 // default standard dance tempo
var phraseBeats = 32          // phrase length in beats (measure = 4)
var totalSec = 0              // global seconds clock
var entrySec = 0              // seconds inside the current queue entry
var entryDurSec = 0
var entryFrac = 0             // 0->1 over the current entry
var phraseFrac = 0            // 0->1 over the current phrase
var beatCount = 0             // running fractional beat counter
// countdown ramps, 1 -> 0 (inverted phase)
var measureT = 1, wholeT = 1, halfT = 1, quarterT = 1, eighthT = 1, sixteenthT = 1

function updateTimers(delta) {
  var ds = delta / 1000
  totalSec += ds
  entrySec += ds
  beatCount += ds * bpm / 60
  if (entryDurSec > 0) entryFrac = clamp(entrySec / entryDurSec, 0, 1)
  phraseFrac = mod(beatCount, phraseBeats) / phraseBeats
  measureT = 1 - mod(beatCount, 4) / 4
  wholeT = measureT                          // whole note = a 4/4 measure
  halfT = 1 - mod(beatCount, 2) / 2
  quarterT = 1 - frac(beatCount)
  eighthT = 1 - frac(beatCount * 2)
  sixteenthT = 1 - frac(beatCount * 4)
}

// ===== sound processing (needs a sensor board) =====
// volume normalization
var volInst = 0, volFast = 0, maxVol = 0.02, vol = 0, volRatio = 0

// instrument band detectors: current on-state + debounced one-shot hooks
var bassOn = 0, clapOn = 0, hatOn = 0
var bassAvg = 0, clapAvg = 0, hatAvg = 0
var bassLastFire = -9, clapLastFire = -9, hatLastFire = -9

// derivative beat detector
var bassFast = 0, bassPrevFast = 0, bassMax = 0.05
var dRing = array(5)
var dIdx = 0
var beatFlag = 0              // set for one frame on each detected beat
var volSpikeFlag = 0

// tempo estimation
var ivRing = array(8)
var ivIdx = 0, ivCount = 0, lastBeatSec = -99
export var tempoEst = 0, tempoReliable = 0

// sub-pattern instrument hooks (reset to no-ops between entries)
function noop() { }
var onBeat = noop
var onClap = noop
var onHat = noop

function processSound(delta) {
  // --- volume: raw energy scaled ~an order of magnitude up, tracked by
  // a fast EMA plus a slowly-decaying "loudest recently heard" maximum
  // that survives quiet bridges (half-life over a minute)
  volInst = energyAverage * 10
  volFast += (volInst - volFast) * clamp(delta / 60, 0, 1)
  maxVol = max(maxVol * pow(0.5, delta / 90000), volFast)
  if (maxVol > 0.04) vol = clamp(volFast / maxVol, 0, 1)   // silence floor
  else vol = 0
  volRatio = volFast > 0.002 ? volInst / volFast : 0
  volSpikeFlag = volRatio > 1.8 && volInst > 0.05

  var debounce = 60 / bpm / 4    // a fraction of a quarter note:
                                 // sixteenth retriggers stay possible
  var k = clamp(delta / 2000, 0, 1)   // slow moving-average rate

  // --- bass / kick: few lowest bins
  var bassE = frequencyData[0] + frequencyData[1] + frequencyData[2]
  bassAvg += (bassE - bassAvg) * k
  bassOn = bassE > bassAvg * 2 && bassE > 0.01

  // --- claps / snare: upper-middle band
  var clapE = frequencyData[18] + frequencyData[19] + frequencyData[20] + frequencyData[21]
  clapAvg += (clapE - clapAvg) * k
  clapOn = clapE > clapAvg * 2.5 && clapE > 0.005
  if (clapOn && totalSec - clapLastFire > debounce) {
    clapLastFire = totalSec
    onClap()
  }

  // --- hi-hat: top bins, boosted (raw values there are tiny)
  var hatE = (frequencyData[30] + frequencyData[31]) * 16
  hatAvg += (hatE - hatAvg) * k
  hatOn = hatE > hatAvg * 2.5 && hatE > 0.005
  if (hatOn && totalSec - hatLastFire > debounce) {
    hatLastFire = totalSec
    onHat()
  }

  // --- beat detection: first derivative of a fast bass average,
  // normalized by a slowly-decaying recent maximum (auto gain), averaged
  // over a short ring buffer; a beat = averaged derivative says bass is
  // rising past a midpoint threshold
  bassFast += (bassE - bassFast) * clamp(delta / 40, 0, 1)
  bassMax = max(bassMax * pow(0.5, delta / 60000), bassFast)
  dRing[dIdx] = (bassFast - bassPrevFast) / max(bassMax, 0.01)
  dIdx = mod(dIdx + 1, 5)
  bassPrevFast = bassFast

  beatFlag = 0
  if (arraySum(dRing) / 5 > 0.05 && bassE > 0.01
      && totalSec - bassLastFire > debounce) {
    bassLastFire = totalSec
    beatFlag = 1
    onBeat()
    trackTempo()
  }
}

function trackTempo() {
  // intervals between the last 8 beats; reset after a long silence
  if (totalSec - lastBeatSec > 4) { ivCount = 0; ivIdx = 0 }
  else {
    ivRing[ivIdx] = totalSec - lastBeatSec
    ivIdx = mod(ivIdx + 1, 8)
    ivCount = min(ivCount + 1, 8)
  }
  lastBeatSec = totalSec
  tempoReliable = 0
  if (ivCount == 8) {
    var mean = arraySum(ivRing) / 8
    var varsum = 0
    var i
    for (i = 0; i < 8; i++) varsum += (ivRing[i] - mean) * (ivRing[i] - mean)
    var sd = sqrt(varsum / 8)
    if (mean > 0.05 && sd / mean < 0.1) {   // tight spread only
      tempoEst = round(60 / mean)
      tempoReliable = 1
    }
  }
}

// pitch helper: dominant frequency -> semitones relative to A440
function pitchSemitones(freqHz) {
  if (freqHz <= 0) return 0
  return log2(freqHz / 440) * 12
}

// ===== other shared helpers for sub-patterns =====
// proportional-integral controller for automatic brightness gain:
// sub-patterns add rendered brightness into agAccum each pixel; call
// autoGainUpdate once per frame with a target fill (0..1 of full-on).
var agAccum = 0, agIntegral = 0, agGain = 1
function autoGainUpdate(targetFill) {
  var err = targetFill - agAccum / pixelCount
  agIntegral = clamp(agIntegral + err * 0.02, -2, 2)
  agGain = clamp(1 + err * 0.5 + agIntegral, 0.05, 8)
  agAccum = 0
}

// fade the shared value array so a starting value dies away over
// `seconds`, frame-rate compensated
function decayValues(seconds, delta) {
  feedback(shVal, pow(0.01, delta / 1000 / max(seconds, 0.001)))
}

// edge trigger: fire fn once per wrap of any 0..1 sawtooth ramp
var edgePrev = 0, edgeArmed = 0
function edgeTrigger(ramp, fn) {
  if (edgeArmed && ramp < edgePrev) fn()   // ramp wrapped
  edgePrev = ramp
  edgeArmed = 1
}

// ===== dispatch: one active frame-prep + one active per-pixel fn =====
function prepOff(delta) { }
function pixelOff(index, x, y, z) { rgb(0, 0, 0) }
var curPrep = prepOff
var curPixel = pixelOff

// ===== the queue =====
var QCAP = 200
var qFn = array(QCAP)     // frame-prep function (or command function)
var qBeats = array(QCAP)  // duration in beats
var qMode = array(QCAP)   // continue mode
var qArg = array(QCAP)    // command value / predicate function
var qLen = 0, qIdx = 0, qActive = 0
var setupDone = 0
var LATENCY = 0.08        // pre-advance after a detection-cut entry (s)

// continue modes
var M_TIME = 0        // fixed duration, then advance
var M_UNTIL_BEAT = 1  // advance on bass beat, or when duration expires
var M_UNTIL_VOL = 2   // advance on volume spike, or when duration expires
var M_CMD = 3         // run fn(qArg) immediately, advance at once
var M_PRED = 4        // advance when the predicate in qArg returns truthy

// end-of-queue behavior
var END_LOOP = 0, END_BLACK = 1, END_HOLD_LAST = 2
var endMode = END_LOOP

function addEntry(fn, beats, mode) {
  qFn[qLen] = fn
  qBeats[qLen] = beats
  qMode[qLen] = mode
  qLen += 1
}
function addCmd(fn, value) {
  qFn[qLen] = fn
  qBeats[qLen] = 0
  qMode[qLen] = M_CMD
  qArg[qLen] = value
  qLen += 1
}
function addWait(pred, maxBeats) {
  qFn[qLen] = prepOff
  qBeats[qLen] = maxBeats
  qMode[qLen] = M_PRED
  qArg[qLen] = pred
  qLen += 1
}

function enterEntry(preroll) {
  // fresh canvas & hooks for every entry
  arrayReplace(shHue, 0)
  arrayReplace(shSat, 0)
  arrayReplace(shVal, 0)
  setupDone = 0
  edgeArmed = 0
  onBeat = noop
  onClap = noop
  onHat = noop
  entrySec = preroll
  entryFrac = 0
  entryDurSec = qBeats[qIdx] * 60 / bpm

  // execute-once commands run immediately, then the queue advances
  while (qActive && qMode[qIdx] == M_CMD) {
    var cmd = qFn[qIdx]
    cmd(qArg[qIdx])
    stepQueue(0)
  }
}

function stepQueue(preroll) {
  qIdx += 1
  if (qIdx >= qLen) {
    if (endMode == END_LOOP) qIdx = 0
    else if (endMode == END_HOLD_LAST) qIdx = qLen - 1
    else {                       // END_BLACK: hold darkness forever
      qActive = 0
      curPrep = prepOff
      curPixel = pixelOff
      return
    }
  }
  enterEntry(preroll)
}

function startSequence() {
  qIdx = 0
  qActive = qLen > 0
  if (qActive) enterEntry(0)
}

function runQueue(delta) {
  if (!qActive) return
  var mode = qMode[qIdx]
  var expired = entrySec >= entryDurSec
  if (mode == M_UNTIL_BEAT && beatFlag) stepQueue(LATENCY)
  else if (mode == M_UNTIL_VOL && volSpikeFlag) stepQueue(LATENCY)
  else if (mode == M_PRED) {
    var pred = qArg[qIdx]
    if (pred() || expired) stepQueue(0)
  }
  else if (expired) stepQueue(0)
  if (qActive) {
    var prep = qFn[qIdx]
    curPrep = prep
  }
}

// ===== global frame hook & renderer forwarding =====
export function beforeRender(delta) {
  if (light != -1) boardSeen = 1     // sentinel changed: board present
  if (boardSeen) processSound(delta)
  updateTimers(delta)
  runQueue(delta)
  curPrep(delta)
}

// 1D maps index to a fractional strip position; 2D forces z = 0; both
// funnel into the 3D entry point, which calls the active per-pixel fn
export function render(index) { render3D(index, index / pixelCount, 0, 0) }
export function render2D(index, x, y) { render3D(index, x, y, 0) }
export function render3D(index, x, y, z) { curPixel(index, x, y, z) }

// ===== queue command functions =====
function cmdSetTempo(v) { bpm = v }
function cmdSetPhrase(v) { phraseBeats = v }
function cmdAdoptDetectedTempo(v) { if (tempoReliable) bpm = tempoEst }
function cmdSeedBassGain(v) { bassMax = v }

// ===== Part B: the "flash posterize" demo effect =====
var posterize = 0
var segStart = 0, prevSign = 0
var segParam = 1              // breathes over the phrase

function flipPosterize() { posterize = !posterize }

// full-width rainbow slice sliding along the strip (~6 s loop)
function gradHue(p) { return mod(0.33 + p + time(0.09), 1) }

function prepFlash(delta) {
  if (!setupDone) {
    setupDone = 1
    curPixel = pixelFlash
    posterize = 0
    onBeat = flipPosterize    // every kick flips smooth <-> posterized
  }
  // segment lengths breathe once per phrase, +-~a sixth around base
  segParam = 1 + 0.35 * (triangle(phraseFrac) - 0.5)
  // reset the per-frame segmentation scan
  segStart = 0
  prevSign = 0
}

function pixelFlash(index, x, y, z) {
  // index-based on purpose (the original demo is 1D; a matrix shows
  // wiring-order stripes)
  var pos = index / pixelCount
  var gh = gradHue(pos)

  if (posterize) {
    // stored segment hue at full saturation; brightness forced to zero
    // exactly where the stored hue differs from the neighbor's — thin
    // dark separators on the first pixel of each segment
    var sep = index > 0 && shHue[index] != shHue[index - 1]
    hsv(shHue[index], 1, sep ? 0 : 0.8)
  } else {
    hsv(gh, 0.85, 0.7)
  }

  // precompute NEXT frame's segments inline (one frame of latency):
  // a quasi-random gap function — two products of periodic waves at
  // incommensurate frequencies, offset downward — crosses zero
  // irregularly; each crossing ends a segment. segParam stretches two
  // frequencies, so segments drift and breathe over the phrase.
  var g = wave(pos * 3.7 * segParam) * wave(pos * 1.27)
        + wave(pos * 2.3 * segParam) * wave(pos * 5.11) - 0.55
  var sgn = g >= 0
  if (index == 0) {
    segStart = 0
    prevSign = sgn
  } else if (sgn != prevSign) {
    // fill the finished segment with the gradient sampled at its middle
    var mh = gradHue((segStart + index) / 2 / pixelCount)
    var k
    for (k = segStart; k <= index; k++) shHue[k] = mh
    segStart = index
    prevSign = sgn
  }
}

// ===== shipped sequence =====
// default dance tempo, then the demo effect for several minutes, looped
addCmd(cmdSetTempo, 120)
addCmd(cmdSetPhrase, 32)
// (option: wait in darkness until sound is heard)
// addWait(() => volSpikeFlag, 9999)
addEntry(prepFlash, 512, M_TIME)
startSequence()
