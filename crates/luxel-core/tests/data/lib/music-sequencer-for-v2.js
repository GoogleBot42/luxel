// name: Opening Act
// Clean-room reimplementation from a prose functional description of the
// community pattern "Music Sequencer for v2"; original source never consulted.
//
// A framework, not a single effect: a beat/tempo/instrument detection engine
// (fed by an optional sound sensor board), a queue/sequencer that plays
// mini-patterns for durations measured in musical beats, and a demo playlist.
// Without a sensor board everything still runs on the internal tempo clock;
// sound-gated features (beat-synced starts, the piano) degrade gracefully.
//
// NOTE: the original nearly exhausts the older hardware's global-variable
// budget (and shipped with a read-the-warning tripwire). This reimplementation
// keeps the state footprint modest but the concern still applies on tiny VMs.

// ---- sensor bindings (engine stubs these with zeros when absent) -------------
export var frequencyData          // 32-band audio spectrum
export var energyAverage          // overall instantaneous loudness
export var maxFrequency           // dominant frequency (Hz)
export var maxFrequencyMagnitude  // its magnitude
export var light                  // used only as a board-presence probe

var sensorPresent = 0

// ---- musical globals ----------------------------------------------------------
var bpm = 122
var beatsPerMeasure = 4
var beatsPerPhrase = 16
var curBeats = 16        // duration of the entry now playing, after Section Length

// ---- controls ------------------------------------------------------------------
var bpmDialSet = 0       // once the dial is touched it outranks the show script
var hueOffset = 0        // added to every theme hue, in turns
var lenScale = 1         // multiplies every entry duration
var punchScale = 1       // scales how hard the note ramps dip the brightness

// Show tempo in BPM. The demo show also asks for a tempo (and can adopt one it
// hears); moving this dial takes ownership so the show cannot stamp on it.
//# min=40 max=200 step=1 default=122
export function sliderTempo(v) {
  bpmDialSet = 1
  bpm = v
}

// Rotates the whole show's palette. The script still steps the theme hue
// through its own sequence; this offsets wherever that sequence sits.
//# min=0 max=360 step=1 default=0
export function sliderThemeHue(v) { hueOffset = v / 360 }

// Stretches or compresses every section of the show. 100% is as authored;
// 25% rips through it, 400% lets each look breathe for four times as long.
//# min=25 max=400 step=5 default=100
export function sliderSectionLength(v) { lenScale = v / 100 }

// How hard the note ramps pump the brightness, relative to as-authored. 0%
// holds every look steady, 100% is the scored dip, 200% slams to black between
// hits.
//# min=0 max=200 step=5 default=100
export function sliderBeatPunch(v) { punchScale = v / 100 }

// current theme hue including the dial offset
function theme() { return themeHue + hueOffset }

// Apply a note ramp `k` (1 at the hit, 0 just before the next) as a brightness
// dip of authored depth `depth`, scaled by the Beat Punch dial.
function punch(k, depth) { return clamp(1 - depth * punchScale * (1 - k), 0, 1) }

// shared scratch for mini-patterns (spare slot keeps interpolation loops safe)
var hueA = array(pixelCount + 1)
var satA = array(pixelCount + 1)
var valA = array(pixelCount + 1)
var themeHue = 0
var direction = 1
var onceLatch = 0
var lastDelta = 16

// timers published to mini-patterns each frame
var totalBeats = 0
var entrySec = 0
var entryBeats = 0
var entryPct = 0
var phrasePct = 0
// countdown ramps (one -> zero; opposite polarity of the engine sawtooth,
// so "hit then decay" effects use them directly as brightness)
var measureRamp = 0, wholeRamp = 0, halfRamp = 0
var quarterRamp = 0, eighthRamp = 0, sixteenthRamp = 0

function noteRamp(lenBeats) {
  return min(1 - frac(totalBeats / lenBeats), 0.9995)
}

// ---- helpers -------------------------------------------------------------------
// perceptual hue warp: smooth S-curve widening the crowded warm region
function hueWarp(h) {
  h = frac(h)
  return frac(h + sin(h * PI2) * 0.06)
}
// nearness kernel: squared (gamma-corrected) bump when |a-b| < halfWidth
function near(a, b, halfWidth) {
  var d = abs(a - b)
  if (d > halfWidth) return 0
  d = 1 - d / halfWidth
  return d * d
}
// exponential per-pixel brightness decay, time constant in seconds
function decayValues(seconds) {
  feedback(valA, exp(-lastDelta / 1000 / seconds))
}
// dominant frequency -> semitone number (log ratio vs middle C); -1 if unsure
function detectNote() {
  if (maxFrequencyMagnitude < 0.01 || maxFrequency < 30) return -1
  return mod(round(12 * log2(maxFrequency / 261.63)), 12)
}

// ---- sound analysis engine ------------------------------------------------------
var SILENCE = 0.02
var volFast = 0, volSlow = 0, volCeil = 0.1, volCeilAge = 0
var vol = 0            // 0..1 smoothed level vs recent ceiling
var volRatio = 0       // instantaneous vs recent average (burst detector)
var loudBurst = 0

var hihatAvg = 0.01, hihatOn = 0, hihatTimer = 0
var clapAvg = 0.01, clapOn = 0, clapTimer = 0
var bassFast = 0, bassSlow = 0, bassCeil = 0.1, bassPrev = 0
var bassOn = 0, beatHit = 0, beatTimer = 0
var DERIV_N = 5
var derivBuf = array(DERIV_N)
var derivI = 0

// instrument one-shot callbacks; mini-patterns may override, reset on advance
var noop = () => 0
var onKick = noop
var onClap = noop
var onHihat = noop

// tempo estimation
var TEMPO_N = 8
var beatIntervals = array(TEMPO_N)
var beatIntI = 0, beatIntCount = 0
var sinceBeatMs = 0
var detectedBpm = 0
var tempoReliable = 0

function processSound(delta) {
  var i
  var debMs = 60000 / bpm * 0.2   // retrigger limit: ~a fifth of a quarter note
  hihatTimer -= delta
  clapTimer -= delta
  beatTimer -= delta
  beatHit = 0

  // volume normalization: scale up, fast EMA, slow-decaying recent ceiling
  var raw = energyAverage * 16
  volFast += (raw - volFast) * 0.2
  volSlow += (raw - volSlow) * 0.01
  if (volFast > volCeil) {
    volCeil = volFast
    volCeilAge = 0
  } else {
    volCeilAge += delta
    if (volCeilAge > 65000 && volCeil > SILENCE) volCeil *= 0.9995
  }
  vol = volFast < SILENCE ? 0 : clamp(volFast / max(volCeil, SILENCE), 0, 1)
  volRatio = volSlow > 0.005 ? volFast / volSlow : 0
  loudBurst = volRatio > 2 && volFast > SILENCE

  // hi-hat: top two spectrum bands, scaled up, spike vs its own slow average
  var hi = (frequencyData[30] + frequencyData[31]) * 32
  hihatAvg += (hi - hihatAvg) * 0.02
  hihatOn = hi > hihatAvg * 2 && hi > 0.01
  if (hihatOn && hihatTimer <= 0) {
    hihatTimer = debMs
    onHihat()
  }

  // claps: a mid-high group of seven bands, triple-average threshold
  var cl = 0
  for (i = 16; i < 23; i++) cl += frequencyData[i]
  cl *= 16
  clapAvg += (cl - clapAvg) * 0.02
  clapOn = cl > clapAvg * 3 && cl > 0.01
  if (clapOn && clapTimer <= 0) {
    clapTimer = debMs
    onClap()
  }

  // bass/kick: lowest bands; beat = rising slope of the fast average
  var ba = (frequencyData[1] + frequencyData[2] + frequencyData[3]) * 16
  bassPrev = bassFast
  bassFast += (ba - bassFast) * 0.3
  bassSlow += (ba - bassSlow) * 0.005
  if (bassFast > bassCeil) bassCeil = bassFast
  else bassCeil *= 0.9995                       // slow AGC decay
  bassOn = bassFast > bassSlow * 2 && bassFast > 0.01
  derivBuf[derivI] = (bassFast - bassPrev) / max(bassCeil, 0.01)
  derivI = (derivI + 1) % DERIV_N
  var slope = arraySum(derivBuf) / DERIV_N
  if (slope > 0.02 && bassFast > SILENCE && beatTimer <= 0) {
    beatTimer = debMs
    beatHit = 1
    onKick()
    recordBeatInterval()
  }
  sinceBeatMs += delta
  if (sinceBeatMs > 5000) beatIntCount = 0     // reset tempo buffer on silence
}

function recordBeatInterval() {
  if (sinceBeatMs < 5000 && sinceBeatMs > 100) {
    beatIntervals[beatIntI] = sinceBeatMs / 1000   // store seconds: keeps the
    beatIntI = (beatIntI + 1) % TEMPO_N            // squared terms in range
    beatIntCount = min(beatIntCount + 1, TEMPO_N)
    if (beatIntCount == TEMPO_N) estimateTempo()
  }
  sinceBeatMs = 0
}

function estimateTempo() {
  var i, d
  var mean = arraySum(beatIntervals) / TEMPO_N
  if (mean < 0.05) return
  var varSum = 0
  for (i = 0; i < TEMPO_N; i++) {
    d = beatIntervals[i] - mean
    varSum += d * d
  }
  var sd = sqrt(varSum / TEMPO_N)
  if (sd / mean < 0.1) {                 // tight spread -> trust it
    detectedBpm = round(60 / mean)       // recorded music has integer tempo
    tempoReliable = 1
  } else {
    tempoReliable = 0
  }
}

// ---- renderer chain --------------------------------------------------------------
// the current entry's per-frame function must assign `renderer`;
// all exported render hooks delegate to it.
var rBlack = (i, x, y, z) => rgb(0, 0, 0)
var renderer = rBlack

// ---- sequencer / queue -------------------------------------------------------------
var PLMAX = 64
var plFn = array(PLMAX)      // per-frame function of each entry
var plBeats = array(PLMAX)   // duration in beats
var plMode = array(PLMAX)    // continue-mode
var plArg = array(PLMAX)     // command argument / custom predicate
var plLen = 0
var plPos = 0
var plForce = 0              // a mini-pattern may set this to bail out early
var LATENCY_SKIP = 0.07      // seconds skipped into next entry on early advance

var M_TIME = 0   // run for the stated duration
var M_BEAT = 1   // until duration expires or a beat is detected
var M_LOUD = 2   // until duration expires or a volume spike
var M_CMD = 3    // execute once with an argument, advance instantly
var M_PRED = 4   // advance when the supplied predicate returns true

function addEntry(fn, beats, mode, arg) {
  if (plLen >= PLMAX) return
  plFn[plLen] = fn
  plBeats[plLen] = beats > 0 ? beats : beatsPerPhrase
  plMode[plLen] = mode
  plArg[plLen] = arg
  plLen += 1
}
function play(fn, beats) { addEntry(fn, beats, M_TIME, 0) }
function playUntilBeat(fn, maxBeats) { addEntry(fn, maxBeats, M_BEAT, 0) }
function playUntilLoud(fn, maxBeats) { addEntry(fn, maxBeats, M_LOUD, 0) }
function playUntil(fn, pred, maxBeats) { addEntry(fn, maxBeats, M_PRED, pred) }
function command(fn, arg) { addEntry(fn, 0, M_CMD, arg) }
function begin() { plPos = 0; startEntry(0) }

// commands
function cmdTempo(a) { if (!bpmDialSet) bpm = a }   // the dial outranks the script
function cmdAdoptTempo(a) { if (tempoReliable) bpm = detectedBpm }  // else keep
function cmdPhrase(a) { beatsPerPhrase = a }
function cmdTheme(a) { themeHue = a }
function cmdThemeStep(a) { themeHue = frac(themeHue + a) }
function cmdDirection(a) { direction = a }
function cmdFlip(a) { direction = -direction }
function cmdBassGain(a) { bassCeil = a }   // pre-seed the bass AGC baseline

function startEntry(skipSec) {
  // execute-once command entries run immediately and advance instantly
  var guard = 0
  while (plLen > 0 && plMode[plPos] == M_CMD && guard <= plLen) {
    var f = plFn[plPos]
    f(plArg[plPos])
    plPos = (plPos + 1) % plLen
    guard += 1
  }
  entrySec = skipSec
  entryBeats = skipSec * bpm / 60
  entryPct = 0
  onceLatch = 0
  onKick = noop
  onClap = noop
  onHihat = noop
  feedback(hueA, 0)   // blank the scratch (arrayReplace is a splat, not a fill)
  feedback(satA, 0)
  feedback(valA, 0)
  renderer = rBlack
}

function advanceEntry(skipSec) {
  plPos = (plPos + 1) % plLen   // hitting the end loops by default
  startEntry(skipSec)
}

// ---- frame loop -------------------------------------------------------------------
export function beforeRender(delta) {
  lastDelta = delta
  var dtSec = delta / 1000

  // presence probe: the light channel (and any energy) sits at zero-stub
  // when no sensor board is attached; all sound processing is gated on it
  sensorPresent = energyAverage > 0.001 || light > 0.001
  if (sensorPresent) processSound(delta)
  else beatHit = 0

  var dBeats = dtSec * bpm / 60
  totalBeats = mod(totalBeats + dBeats, 1024)   // 1024 divides all note grids
  entrySec += dtSec
  entryBeats += dBeats

  phrasePct = frac(totalBeats / beatsPerPhrase)
  measureRamp = noteRamp(beatsPerMeasure)
  wholeRamp = noteRamp(4)
  halfRamp = noteRamp(2)
  quarterRamp = noteRamp(1)
  eighthRamp = noteRamp(0.5)
  sixteenthRamp = noteRamp(0.25)

  if (plLen == 0) {
    renderer = rBlack
    return
  }

  var dur = plBeats[plPos] * lenScale   // Section Length stretches the whole show
  entryPct = clamp(entryBeats / dur, 0, 1)

  // continue-mode checks
  var adv = 0
  var skip = 0
  if (entryBeats >= dur) adv = 1
  var m = plMode[plPos]
  if (m == M_BEAT && beatHit) { adv = 1; skip = LATENCY_SKIP }
  if (m == M_LOUD && loudBurst) { adv = 1; skip = LATENCY_SKIP }
  if (m == M_PRED) {
    var pred = plArg[plPos]
    if (pred()) adv = 1
  }
  if (plForce) { adv = 1; plForce = 0 }
  if (adv) advanceEntry(skip)

  curBeats = plBeats[plPos] * lenScale   // duration of whatever is playing now
  var fn = plFn[plPos]
  fn()
}

export function render(index) {
  var f = renderer
  f(index, index / pixelCount, 0, 0)
}
export function render2D(index, x, y) {
  var f = renderer
  f(index, x, y, 0)
}
export function render3D(index, x, y, z) {
  var f = renderer
  f(index, x, y, z)
}

// ---- demo mini-patterns --------------------------------------------------------------
function pOff() { renderer = rBlack }

// theme-colored bar filling over the entry, pulsing on the quarter note
var rProgress = (i, x, y, z) => {
  var p = direction > 0 ? x : 1 - x
  var v = p <= entryPct ? punch(quarterRamp * quarterRamp, 0.65) : 0
  hsv(theme(), 1, v)
}
function pProgress() { renderer = rProgress }

// soft theme-colored pulse sweeping the strip once per beat
var rSweep = (i, x, y, z) => {
  var pos = frac(entryBeats)
  if (direction < 0) pos = 1 - pos
  hsv(theme(), 1, near(x, pos, 0.15))
}
function pSweep() { renderer = rSweep }

// single sweep across the whole (possibly sub-beat) entry
var rSweepOnce = (i, x, y, z) => {
  var pos = direction > 0 ? entryPct : 1 - entryPct
  hsv(theme(), 1, near(x, pos, 0.15))
}
function pSweepOnce() { renderer = rSweepOnce }

// whole strip breathing to the quarter note, triangle profile, slight hue tilt
var rQuarters = (i, x, y, z) => {
  hsv(theme() + x * 0.08, 1, triangle(x) * punch(quarterRamp * quarterRamp, 1))
}
function pQuarters() { renderer = rQuarters }

// eight segments; the current eighth-note's segment strobes down the strip
var rEighths = (i, x, y, z) => {
  var seg = floor(frac(totalBeats / beatsPerMeasure) * 8)
  var here = floor(min(x, 0.999) * 8)
  var v = here == seg ? punch(eighthRamp * eighthRamp, 1) : 0
  hsv(theme() + measureRamp * 0.2, 0.7 + 0.3 * measureRamp, v)
}
function pEighths() { renderer = rEighths }

// fake 1D oscilloscope: a plucked-bass dot settling within each half note
var rBassHit = (i, x, y, z) => {
  var settle = halfRamp                      // 1 at the hit, decays to 0
  var ph = 1 - halfRamp
  var dot = 0.5 + sin(ph * PI2 * (2 + 4 * settle)) * 0.42 * settle
  var v = near(x, dot, 0.09)
  hsv(hueWarp(theme() + entryPct * 0.3 + v * 0.15), 1, v)
}
function pBassHit() { renderer = rBassHit }

// psychedelic surge: colors emanate from center and withdraw each half note
var rHalfSurge = (i, x, y, z) => {
  var sm = halfRamp * halfRamp * (3 - 2 * halfRamp)   // smoothed half ramp
  var d = abs(x - 0.5) * 2
  var band = frac(d / (0.2 + sm) + phrasePct * 2)
  var v = triangle(x) * (0.25 + 0.75 * wave(band)) * (0.25 + 0.75 * sm)
  hsv(hueWarp(theme() + entryPct * 0.5 + band * 0.3), 1, v)
}
function pHalfSurge() { renderer = rHalfSurge }

// piano: three octaves of keyboard, pitch-reactive; skipped without a sensor
var KEYS = 36
var keyVal = array(KEYS)
var nat12 = array(12)
nat12[0] = 1; nat12[2] = 1; nat12[4] = 1; nat12[5] = 1
nat12[7] = 1; nat12[9] = 1; nat12[11] = 1
var naturalPulse = 0
var rPiano = (i, x, y, z) => {
  var env = clamp(entryBeats / 2, 0, 1) * clamp((curBeats - entryBeats) / 2, 0, 1)
  var k = floor(min(x, 0.999) * KEYS)
  var s = k % 12
  var v = nat12[s] ? 0.05 + naturalPulse * 0.25 : 0.01
  var h = 0.08                              // faint warm keys
  var sat = 0.6
  if (frac(x * KEYS) < 0.12) {              // key dividers pulse with the bass
    v = 0.03 + clamp(bassFast / max(bassCeil, 0.01), 0, 1) * 0.3
    sat = 0.2
  }
  if (keyVal[k] > 0.02) {                   // detected note lights its key
    v = keyVal[k]
    h = s / 12                              // hue keyed to pitch class
    sat = 1
  }
  hsv(h, sat, v * env)
}
function pPiano() {
  if (!sensorPresent) {                     // skipped without a sensor board
    plForce = 1
    renderer = rBlack
    return
  }
  if (!onceLatch) {
    onceLatch = 1
    feedback(keyVal, 0)   // clear the keyboard (arrayReplace is a splat, not a fill)
    onClap = () => { naturalPulse = 1 }
    onHihat = () => { naturalPulse = 1 }
  }
  naturalPulse *= exp(-lastDelta / 1000 / 0.1)
  feedback(keyVal, exp(-lastDelta / 1000 / 0.15))   // quick exponential fade
  var n = detectNote()
  if (n >= 0) {
    var k
    for (k = n; k < KEYS; k += 12) keyVal[k] = 1
  }
  renderer = rPiano
}

// ---- demo playlist ---------------------------------------------------------------------
command(cmdTempo, 122)
command(cmdPhrase, 16)
command(cmdTheme, 0)               // theme red
play(pProgress, 8)                 // progress bars with direction flips
command(cmdFlip, 0)
play(pProgress, 8)
command(cmdFlip, 0)
playUntilBeat(pSweep, 4)           // beat-synced sweeps, alternating direction
command(cmdFlip, 0)
playUntilBeat(pSweep, 4)
command(cmdFlip, 0)
playUntilBeat(pSweep, 4)
play(pQuarters, 8)
play(pEighths, 8)
// eight fast half-beat sweeps, stepping the theme hue and flipping direction
command(cmdThemeStep, 0.38); command(cmdFlip, 0); play(pSweepOnce, 0.5)
command(cmdThemeStep, 0.38); command(cmdFlip, 0); play(pSweepOnce, 0.5)
command(cmdThemeStep, 0.38); command(cmdFlip, 0); play(pSweepOnce, 0.5)
command(cmdThemeStep, 0.38); command(cmdFlip, 0); play(pSweepOnce, 0.5)
command(cmdThemeStep, 0.38); command(cmdFlip, 0); play(pSweepOnce, 0.5)
command(cmdThemeStep, 0.38); command(cmdFlip, 0); play(pSweepOnce, 0.5)
command(cmdThemeStep, 0.38); command(cmdFlip, 0); play(pSweepOnce, 0.5)
command(cmdThemeStep, 0.38); command(cmdFlip, 0); play(pSweepOnce, 0.5)
play(pBassHit, 4)                  // decaying bass-string oscilloscope
play(pOff, 2)                      // two beats of black
play(pHalfSurge, 16)               // a full phrase of the surge
play(pPiano, 16)                   // pitch-reactive piano (needs a sensor)
begin()                            // rewind to start; end of list loops
