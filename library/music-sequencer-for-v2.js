// name: Music Sequencer for v2
// Clean-room reimplementation from a prose functional description of the
// community pattern "Music Sequencer for v2"; original source never consulted.

// A framework for choreographing a playlist of mini-patterns to music at a
// steady tempo, plus a demo playlist:
//   - sound analysis (volume normalization, hi-hat/clap/kick detectors with
//     debounced one-shot callbacks, derivative-based bass beat detection,
//     tempo estimation) — all optional: with no sensor board every input
//     reads zero and the routine free-runs on its internal clock;
//   - a sequencer whose entries run for durations measured in beats, or
//     until a beat / volume spike (skipping slightly ahead to compensate
//     for detection latency), or execute-once commands;
//   - musical countdown ramps (1 -> 0, opposite polarity to the engine
//     sawtooth so "hit then decay" comes free) for measure and whole/half/
//     quarter/eighth/sixteenth notes;
//   - shared per-pixel HSV scratch arrays, a theme hue, a direction flag
//     and a run-once latch, cleared between entries.
// Each mini-pattern installs a "renderer" function of (index, x, y, z); the
// exported render hooks just delegate (1D synthesizes x from the index), so
// every effect is topology-agnostic.
// NOTE: the original nearly exhausted PB v2's global-variable budget (and
// shipped with a read-the-warning compile tripwire). Luxel's budget is
// roomier; the tripwire is intentionally not reproduced.

// ---- sensor bindings (engine stubs these with zeros when absent) -----------
export var frequencyData = array(32)
export var energyAverage = 0
export var maxFrequency = 0
export var maxFrequencyMagnitude = 0
export var accelerometer = array(3)
export var light = 0

var soundPresent = 0         // latches on when any input goes nonzero

// ---- sound analysis state ---------------------------------------------------
var volFast = 0              // fast EMA of scaled energy
var volCeil = 0.01           // loudest recently heard (slow decay)
var ceilAgeSec = 0
var volNorm = 0              // 0..1 smoothed volume vs recent ceiling
var volRatio = 0             // instantaneous vs recent average (burst detect)
var SILENCE = 0.02

var hihatAvg = 0.001
var clapAvg = 0.001
var bassFast = 0
var bassSlow = 0.001
var bassCeil = 0.01
var hihatOn = 0
var clapOn = 0
var bassOn = 0
var lastKickSec = -1
var lastClapSec = -1
var lastHihatSec = -1

var DERIV_N = 5
var derivBuf = array(DERIV_N)
var derivPos = 0
var prevBassFast = 0
var beatHit = 0              // one frame wide
var lastBeatSec = -10

var TEMPO_N = 8
var beatGaps = array(TEMPO_N)
var beatGapPos = 0
var beatGapCount = 0
var detectedBpm = 0
var tempoReliable = 0

// event callbacks mini-patterns may override; reset on every advance
function noop() { }
var onKick = noop
var onClap = noop
var onHihat = noop

function processSound(dSec) {
  var i
  // volume normalization: scale up, fast EMA, slow-decaying ceiling
  var e = energyAverage * 16
  volFast += (e - volFast) * min(dSec * 8, 1)
  if (volFast > volCeil) { volCeil = volFast; ceilAgeSec = 0 }
  else {
    ceilAgeSec += dSec
    if (ceilAgeSec > 70 && volCeil > SILENCE) volCeil *= 1 - dSec * 0.05
  }
  volNorm = volFast < SILENCE ? 0 : clamp(volFast / volCeil, 0, 1)
  var eAvg = max(volFast, 0.001)
  volRatio = e / eAvg

  // instrument detectors: band group vs its own slow average
  var hh = (frequencyData[30] + frequencyData[31]) * 8
  hihatAvg += (hh - hihatAvg) * min(dSec * 0.5, 1)
  hihatOn = hh > hihatAvg * 2 && hh > SILENCE
  var cl = 0
  for (i = 20; i < 27; i++) cl += frequencyData[i]
  clapAvg += (cl - clapAvg) * min(dSec * 0.5, 1)
  clapOn = cl > clapAvg * 3 && cl > SILENCE
  var bass = frequencyData[1] + frequencyData[2] + frequencyData[3]
  bassSlow += (bass - bassSlow) * min(dSec * 0.2, 1)
  bassOn = bass > bassSlow * 2.5 && bass > SILENCE

  // debounced one-shots: at most one per ~fifth of a quarter note
  var minGap = 60 / bpm / 5
  if (bassOn && nowSec - lastKickSec > minGap) { lastKickSec = nowSec; onKick() }
  if (clapOn && nowSec - lastClapSec > minGap) { lastClapSec = nowSec; onClap() }
  if (hihatOn && nowSec - lastHihatSec > minGap) { lastHihatSec = nowSec; onHihat() }

  // derivative-based bass beat: rising slope of the fast average,
  // normalized by a slow-AGC ceiling
  bassFast += (bass - bassFast) * min(dSec * 12, 1)
  if (bassFast > bassCeil) bassCeil = bassFast
  else bassCeil = max(bassCeil * (1 - dSec * 0.02), 0.01)
  derivBuf[derivPos] = (bassFast - prevBassFast) / bassCeil
  derivPos = (derivPos + 1) % DERIV_N
  prevBassFast = bassFast
  var dm = arraySum(derivBuf) / DERIV_N
  beatHit = 0
  if (dm > 0.03 && nowSec - lastBeatSec > 60 / bpm / 5) {
    beatHit = 1
    // tempo estimation from beat-to-beat gaps
    var gap = nowSec - lastBeatSec
    if (gap > 5) { beatGapCount = 0; beatGapPos = 0 }  // stale: reset
    else {
      beatGaps[beatGapPos] = gap
      beatGapPos = (beatGapPos + 1) % TEMPO_N
      beatGapCount = min(beatGapCount + 1, TEMPO_N)
      if (beatGapCount == TEMPO_N) {
        var mean = arraySum(beatGaps) / TEMPO_N
        var varSum = 0
        for (i = 0; i < TEMPO_N; i++) {
          var d = beatGaps[i] - mean
          varSum += d * d
        }
        var sd = sqrt(varSum / TEMPO_N)
        if (mean > 0.05 && sd < mean * 0.1) {
          detectedBpm = floor(60 / mean + 0.5)  // integer tempo
          tempoReliable = 1
        } else tempoReliable = 0
      }
    }
    lastBeatSec = nowSec
  }
}

// ---- sequencer --------------------------------------------------------------
var MAXQ = 64
var qFn = array(MAXQ)
var qBeats = array(MAXQ)
var qMode = array(MAXQ)
var qArg = array(MAXQ)
var qLen = 0
var qPos = 0
var MODE_TIMED = 0, MODE_BEAT = 1, MODE_LOUD = 2, MODE_CMD = 3, MODE_PRED = 4

var bpm = 123
var beatsPerMeasure = 4
var beatsPerPhrase = 32
var nowSec = 0
var entrySec = 0
var entryBeats = 0
var entryPct = 0             // 0..1 through the current entry
var curDur = 1
var beatPhase = 0            // fractional beats, free-running
var phrasePct = 0
// countdown ramps, 1 -> 0 (tops clamped just below 1)
var measureRamp = 0, wholeRamp = 0, halfRamp = 0
var quarterRamp = 0, eighthRamp = 0, sixteenthRamp = 0

// shared scratch for mini-patterns (one spare slot for interpolation loops)
var hueA = array(pixelCount + 1)
var satA = array(pixelCount + 1)
var valA = array(pixelCount + 1)
var themeHue = 0
var direction = 1
var runOnce = 0              // cleared on advance; entry sets it after setup
var requestSkip = 0          // an entry may set this to bail out immediately

// build-the-playlist API
function qAdd(fn, beats, mode, arg) {
  qFn[qLen] = fn; qBeats[qLen] = beats; qMode[qLen] = mode; qArg[qLen] = arg
  qLen += 1
}
function qPlay(fn, beats) { qAdd(fn, beats > 0 ? beats : beatsPerPhrase, MODE_TIMED, 0) }
function qPlayUntilBeat(fn, maxBeats) { qAdd(fn, maxBeats, MODE_BEAT, 0) }
function qPlayUntilLoud(fn, maxBeats) { qAdd(fn, maxBeats, MODE_LOUD, 0) }
function qCmd(fn, arg) { qAdd(fn, 0, MODE_CMD, arg) }
function qBegin() { qPos = 0; entrySec = 0; runOnce = 0 }

// commands
function cmdTempo(a) { bpm = a }
function cmdPhrase(a) { beatsPerPhrase = a }
function cmdTheme(a) { themeHue = a }
function cmdDir(a) { direction = a }
function cmdHueStep(a) { themeHue = mod(themeHue + a, 1) }
function cmdAdoptTempo(a) { bpm = tempoReliable ? detectedBpm : bpm }
function cmdSeedBass(a) { bassCeil = max(bassCeil, a) }

function advanceEntry(skipSec) {
  qPos = (qPos + 1) % qLen             // loop forever at the end
  entrySec = skipSec                   // compensate detection latency
  runOnce = 0
  requestSkip = 0
  onKick = noop
  onClap = noop
  onHihat = noop
}

function sequencerFrame(dSec) {
  var beatSec = 60 / bpm
  entrySec += dSec
  beatPhase += dSec / beatSec                 // fractional beat counter
  if (beatPhase > 16000) beatPhase -= 16000   // stay inside 16.16 range

  // advance the queue (commands run instantly; bounded loop)
  var guard = 0
  while (guard < MAXQ + 4) {
    guard += 1
    var mode = qMode[qPos]
    if (mode == MODE_CMD) {
      var cf = qFn[qPos]
      cf(qArg[qPos])
      advanceEntry(0)
      continue
    }
    var durB = qBeats[qPos]
    var done = entrySec >= durB * beatSec
    var skip = 0
    if (!done && requestSkip) done = 1
    if (!done && mode == MODE_BEAT && beatHit) { done = 1; skip = 0.06 }
    if (!done && mode == MODE_LOUD && soundPresent && volRatio > 2.5 && volFast > SILENCE) { done = 1; skip = 0.06 }
    if (!done && mode == MODE_PRED) {
      var pf = qArg[qPos]
      if (pf()) done = 1
    }
    if (done) { advanceEntry(skip); continue }
    break
  }

  // musical timers for mini-patterns
  curDur = max(qBeats[qPos], 0.001)
  entryBeats = entrySec / beatSec
  entryPct = clamp(entryBeats / curDur, 0, 0.999)
  phrasePct = frac(beatPhase / beatsPerPhrase)
  var w = beatPhase / 4                        // whole note = 4 beats
  wholeRamp = min(1 - frac(w), 0.999)
  measureRamp = min(1 - frac(beatPhase / beatsPerMeasure), 0.999)
  halfRamp = min(1 - frac(w * 2), 0.999)
  quarterRamp = min(1 - frac(w * 4), 0.999)
  eighthRamp = min(1 - frac(w * 8), 0.999)
  sixteenthRamp = min(1 - frac(w * 16), 0.999)

  // run the current entry's per-frame function; it must set `renderer`
  var ef = qFn[qPos]
  ef(0)
}

// ---- helpers ----------------------------------------------------------------
// perceptual hue warp: smooth S-curve widening the crowded warm region
function hueWarp(h) {
  h = frac(h)
  return frac(h + 0.045 * sin(h * PI2))
}
// gamma-corrected proximity bump: bright when a and b are within halfWidth
function near(a, b, halfWidth) {
  var d = 1 - abs(a - b) / halfWidth
  if (d < 0) return 0
  return d * d
}
// exponential per-pixel brightness decay, time constant in seconds
function decayValues(dSec, tau) {
  var f = max(1 - dSec / tau, 0)
  feedback(valA, f)
}
// dominant frequency -> semitone number vs middle C (flute-solo registers)
function detectedSemitone() {
  if (maxFrequency < 30) return -1
  return floor(12 * log2(maxFrequency / 261.63) + 0.5)
}

// ---- mini-pattern renderers (named globals: no per-frame lambda churn) ------
var renderer = drawOff

function drawOff(i, x, y, z) { rgb(0, 0, 0) }
function patOff(a) { renderer = drawOff }

// theme-colored bar filling over the entry, pulsing on each quarter note
function drawProgress(i, x, y, z) {
  var p = direction > 0 ? x : 1 - x
  var lit = p <= entryPct
  hsv(themeHue, 1, lit * (0.3 + 0.7 * quarterRamp))
}
function patProgress(a) { renderer = drawProgress }

// soft pulse of theme color sweeping the strip once per beat
function drawSweep(i, x, y, z) {
  var pos = frac(beatPhase)
  if (direction < 0) pos = 1 - pos
  hsv(themeHue, 1, near(x, pos, 0.15))
}
function patSweep(a) { renderer = drawSweep }

// whole strip breathing on the quarter note, triangle profile, hue tilt
function drawQuarters(i, x, y, z) {
  hsv(themeHue + x * 0.12, 1, quarterRamp * triangle(x / 2))
}
function patQuarters(a) { renderer = drawQuarters }

// eight segments; the current eighth-note's segment strobes
function drawEighths(i, x, y, z) {
  var seg = floor(x * 7.99)
  var cur = floor(mod(beatPhase * 2, 8))
  var h = themeHue + measureRamp * 0.2 + seg * 0.03
  hsv(h, 0.8 + 0.2 * measureRamp, (seg == cur) * eighthRamp)
}
function patEighths(a) { renderer = drawEighths }

// fake 1D oscilloscope: a dot rings around center, frequency and amplitude
// decaying within each half note like a plucked bass string
function drawBassHit(i, x, y, z) {
  var amp = 0.42 * halfRamp * halfRamp
  var ph = (1 - halfRamp) * (14 + 10 * halfRamp)
  var pos = 0.5 + amp * sin(ph)
  var v = near(x, pos, 0.09)
  hsv(themeHue + entryPct * 0.3 + v * 0.12, 1, v)
}
function patBassHit(a) { renderer = drawBassHit }

// colors emanating from center and withdrawing sharply every half note
var surgeSmooth = 0
function drawSurge(i, x, y, z) {
  var d = abs(x - 0.5) * 2
  var v = triangle(x / 2) * wave(d * 2 - surgeSmooth * 2 + phrasePct * 3)
  hsv(themeHue + entryPct + d * 0.3, 1, v * v)
}
function patSurge(a) {
  surgeSmooth += (halfRamp - surgeSmooth) * 0.2
  renderer = drawSurge
}

// piano: three octaves of keyboard; detected note lights its key.
// Skipped automatically when no sensor board is present.
var KEYS = 36
var natural = array(12)
natural[0] = 1; natural[1] = 0; natural[2] = 1; natural[3] = 0
natural[4] = 1; natural[5] = 1; natural[6] = 0; natural[7] = 1
natural[8] = 0; natural[9] = 1; natural[10] = 0; natural[11] = 1
var pianoFlash = 0
function pianoHit() { pianoFlash = 1 }
function drawPiano(i, x, y, z) {
  var env = min(entryBeats / 2, 1) * min((curDur - entryBeats) / 2, 1)
  env = clamp(env, 0, 1)                 // fade in over first, out over last
  var key = floor(x * (KEYS - 0.01))
  var pc = mod(key, 12)
  var kx = frac(x * KEYS)                // position within the key
  var v = 0
  var h = 0.08
  var s = 0.6
  if (kx < 0.12) {                       // key divider, pulsing with bass
    v = 0.12 + 0.5 * clamp(bassFast / max(bassCeil, 0.01), 0, 1)
    s = 0.2
  } else if (natural[pc]) {
    v = 0.10 + 0.35 * pianoFlash        // faint warm naturals; hit flash
  }
  var lit = valA[i]                     // detected-note overlay, decaying
  if (lit > 0.02) {
    h = hueA[i]
    s = 1
    v = max(v, lit)
  }
  hsv(h, s, v * env)
}
function patPiano(a) {
  if (!soundPresent) { requestSkip = 1; renderer = drawOff; return }
  if (!runOnce) {
    runOnce = 1
    onClap = pianoHit
    onHihat = pianoHit
  }
  pianoFlash = max(pianoFlash - deltaSec * 4, 0)
  decayValues(deltaSec, 0.25)
  var semi = detectedSemitone()
  if (semi >= -12 && semi < 24 && maxFrequencyMagnitude > 0.01) {
    var key = semi + 12                  // three octaves around middle C
    var lo = floor(key / KEYS * pixelCount)
    var hi = floor((key + 1) / KEYS * pixelCount)
    var p
    for (p = lo; p < hi && p < pixelCount; p++) {
      valA[p] = 1
      hueA[p] = mod(key, 12) / 12
    }
  }
  renderer = drawPiano
}

// ---- demo playlist ----------------------------------------------------------
qCmd(cmdTempo, 123)
qCmd(cmdPhrase, 16)
qCmd(cmdTheme, 0)                        // theme: red
qCmd(cmdDir, 1)
qPlay(patProgress, 16)                   // progress bar, forward
qCmd(cmdDir, -1)
qPlay(patProgress, 16)                   // ...and back
qCmd(cmdAdoptTempo, 0)                   // ride the detected tempo, if any
qCmd(cmdDir, 1)
qPlayUntilBeat(patSweep, 4)              // beat-synced sweeps, alternating
qCmd(cmdDir, -1)
qPlayUntilBeat(patSweep, 4)
qCmd(cmdDir, 1)
qPlayUntilBeat(patSweep, 4)
qCmd(cmdDir, -1)
qPlayUntilBeat(patSweep, 4)
qPlay(patQuarters, 16)
qPlay(patEighths, 16)
var sw
for (sw = 0; sw < 8; sw++) {             // fast sweeps stepping the hue wheel
  qCmd(cmdHueStep, 0.38)
  qCmd(cmdDir, sw % 2 ? -1 : 1)
  qPlay(patSweep, 2)
}
qPlay(patBassHit, 16)
qPlay(patOff, 2)                         // two beats of black
qCmd(cmdTheme, 0.6)
qPlay(patSurge, 16)                      // one full phrase of the surge
qPlay(patPiano, 16)                      // skipped without a sensor board
qBegin()

// ---- frame loop -------------------------------------------------------------
var deltaSec = 0
export function beforeRender(delta) {
  deltaSec = delta / 1000
  nowSec += deltaSec
  if (nowSec > 16000) nowSec -= 16000
  if (!soundPresent) {
    if (energyAverage > 0 || maxFrequencyMagnitude > 0 || arraySum(frequencyData) > 0) soundPresent = 1
  }
  beatHit = 0
  if (soundPresent) processSound(deltaSec)
  sequencerFrame(deltaSec)
}

// render hooks delegate to the installed renderer; 1D synthesizes x
export function render(index) {
  var f = renderer
  f(index, pixelCount > 1 ? index / (pixelCount - 1) : 0, 0, 0)
}
export function render2D(index, x, y) {
  var f = renderer
  f(index, x, y, 0)
}
export function render3D(index, x, y, z) {
  var f = renderer
  f(index, x, y, z)
}
