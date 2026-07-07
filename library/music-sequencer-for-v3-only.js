// name: Music Sequencer - for V3 ONLY
// Clean-room reimplementation from a prose functional description of the
// community pattern "Music Sequencer - for V3 ONLY"; original source never
// consulted.

// A music-choreography framework plus demo mini-patterns and a scripted
// show: a queue of mini-patterns, each played for a duration in musical
// beats, with audio analysis (volume normalization, bass/clap/hi-hat
// detectors, derivative-based beat detection, tempo estimation) driving
// beat-locked timers. Expects the sensor expansion board; without one it
// degrades gracefully (clock-driven behavior, some patterns skip).
// Without any audio input it idles dark / gently — that is by design.

// ---------------------------------------------------------------- sensors
export var frequencyData = array(32)     // 32-band spectrum
export var energyAverage = 0
export var maxFrequency = 0
export var maxFrequencyMagnitude = 0
export var light = -1                    // board probe: stays negative = no board
var boardPresent = 0

// ---------------------------------------------------------------- shared
var N = pixelCount
var hueA = array(N + 1)                  // shared scratch (one extra: safe interp)
var satA = array(N + 1)
var valA = array(N + 1)
var themeHue = 0.6                       // script-settable shared theme
var direction = 0                        // script-settable orientation flag
var setupDone = 0                        // run-once latch per queue entry
var edgeLatch = 0                        // shared edge-trigger latch

// ------------------------------------------------------- volume tracking
var SILENCE = 0.02
var emaE = 0, maxE = 0.05, sinceMax = 0
var volume = 0, loudRatio = 0, soundSpike = 0

// ---------------------------------------------------- instrument detection
var bassFast = 0, bassSlow = 0, bassMax = 0.001, bassNorm = 0, lastNorm = 0
var dBuf = array(5), dPos = 0            // derivative ring buffer
var beatFired = 0, beatOn = 0, sinceBeat = 9
var clapAvg = 0, clapFired = 0, clapOn = 0, sinceClap = 9
var hhAvg = 0, hhFired = 0, hhOn = 0, sinceHh = 9

// ------------------------------------------------------- tempo estimation
var ivl = array(8), ivlPos = 0, ivlCount = 0
var detectedBpm = 0, tempoReliable = 0

// ------------------------------------------------------------- beat clock
var bpm = 120
var phraseBeats = 16
var entrySec = 0                         // time in current entry (seconds!)
var beatSec = 0.5
var beatCount = 0                        // decimal running beat counter
// ramp-DOWN (1 -> 0) note timers — the framework's signature shape
var whole = 0, half = 0, quarter = 0, eighth = 0, sixteenth = 0
var phraseProg = 0, patternProg = 0
var curDur = 16

// ------------------------------------------------------------------ queue
var QMAX = 64
var qPat = array(QMAX), qDur = array(QMAX), qMode = array(QMAX), qArg = array(QMAX)
var qLen = 0, qIndex = 0
// modes: 0 = fixed beats; 1 = also advance on detected beat ("hold for the
// drop"); 2 = also advance on a volume spike (silence -> sound); 3 = one-shot
// command with argument, executes and advances immediately
function enq(pat, dur, mode, arg) {
  qPat[qLen] = pat
  qDur[qLen] = dur
  qMode[qLen] = mode
  qArg[qLen] = arg
  qLen += 1
}

// pattern ids
var OFF = 0, PROGRESS = 1, MPROGRESS = 2, SWEEP = 3, QUARTERS = 4, EIGHTHS = 5
var STROBE = 6, SURGE = 7, DANCER = 8, BSCOPE = 9, FIZZLE = 10, BUILDUP = 11
var OCEAN = 12, SPLOTCH = 13, ANALYZER = 14, ELASTIC = 15
// command ids
var CMD_TEMPO = 100, CMD_PHRASE = 101, CMD_THEME = 102, CMD_FLIP = 103
var CMD_SEEDBASS = 104

// debugging aid: pick a mini-pattern by hand (takes effect only if the
// manual entry below is uncommented into the script)
var manualPattern = 0
export function sliderChooseManualPattern(v) {
  //# min=0 max=1 step=0.0625 default=0
  manualPattern = floor(v * 15.99)
  resetShared()
}
// commented-out tuning aid, as shipped:
// export function sliderHihatThreshold(v) { hhThresh = 1.5 + v * 3 }

// ---------------------------------------------------------------- helpers
function resetShared() {
  arrayReplace(hueA, 0)
  arrayReplace(satA, 1)
  arrayReplace(valA, 0)
  setupDone = 0
  edgeLatch = 0
  beatFired = 0
  clapFired = 0
  hhFired = 0
}

// proximity: 1 when positions coincide, 0 at halfWidth, squared soft edge
function near(a, b, w) {
  var v = 1 - abs(a - b) / w
  return v > 0 ? v * v : 0
}

// perceptually-evened hue redistribution
function warpHue(h) {
  h = frac(frac(h) + 1)
  return h + 0.08 * sin(h * PI2)
}

// directed position of pixel i, honoring the shared direction flag
function posOf(i) {
  var p = i / N
  return direction ? 1 - p : p
}

// exponential-ish fade of the brightness scratch toward black
function decayVals(secToDark, dt) {
  feedback(valA, max(0, 1 - dt / secToDark))
}

// run fn once on a rising edge of v (shared latch, cleared between entries)
function onRisingEdge(v) {
  var fire = v && !edgeLatch
  edgeLatch = v != 0
  return fire
}

// dominant frequency -> semitones above A1 (55 Hz)
function noteNumber() {
  if (maxFrequency < 20) return 0
  return log2(maxFrequency / 55) * 12
}

// --------------------------------------------------------- sound analysis
function analyzeSound(delta) {
  var dt = delta / 1000
  boardPresent = light >= 0

  // volume normalization: EMA vs. a very-slowly-decaying running max
  var e = energyAverage * 10
  emaE += (e - emaE) * min(1, dt)
  if (e > maxE) {
    maxE = e
    sinceMax = 0
  } else {
    sinceMax += dt
    if (sinceMax > 70) maxE = max(0.02, maxE * (1 - dt / 30))
  }
  volume = emaE > SILENCE ? clamp(emaE / maxE, 0, 1) : 0
  loudRatio = emaE > 0.001 ? e / emaE : 0
  soundSpike = loudRatio > 2 && e > SILENCE

  // bass / kick: slow + fast EMAs and a self-decaying max (auto gain)
  var braw = frequencyData[0] + frequencyData[1] + frequencyData[2]
  bassFast += (braw - bassFast) * min(1, dt * 12)
  bassSlow += (braw - bassSlow) * min(1, dt * 0.7)
  bassMax = max(bassMax * (1 - dt / 60), braw)
  bassNorm = bassMax > 0.0001 ? bassFast / bassMax : 0

  // beat = rising derivative of the normalized fast bass average
  dBuf[dPos] = bassNorm - lastNorm
  lastNorm = bassNorm
  dPos = (dPos + 1) % 5
  var dMean = arraySum(dBuf) / 5
  beatOn = dMean > 0.015 && braw > 0.0001
  beatFired = 0
  sinceBeat += dt
  var debounce = beatSec / 5             // 16th-note doubles still retrigger
  if (beatOn && sinceBeat > debounce) {
    beatFired = 1
    // tempo: accept 8 consecutive intervals agreeing within ~10%
    if (sinceBeat < 3) {
      ivl[ivlPos] = sinceBeat
      ivlPos = (ivlPos + 1) % 8
      ivlCount = min(8, ivlCount + 1)
      if (ivlCount == 8) {
        var mean = arraySum(ivl) / 8
        var ss = 0, k
        for (k = 0; k < 8; k++) ss += (ivl[k] - mean) * (ivl[k] - mean)
        var relSd = mean > 0.01 ? sqrt(ss / 8) / mean : 1
        if (relSd < 0.1) {
          detectedBpm = round(60 / mean)  // released music: integer BPM
          tempoReliable = 1
        }
      }
    } else {
      ivlCount = 0                        // long gap: restart collection
    }
    sinceBeat = 0
  }

  // claps / snare: upper-middle bins vs. their slow average
  var craw = frequencyData[16] + frequencyData[17] + frequencyData[18] + frequencyData[19]
  clapAvg += (craw - clapAvg) * min(1, dt * 0.5)
  clapFired = 0
  sinceClap += dt
  if (craw > clapAvg * 2 && craw > 0.001 && sinceClap > debounce) {
    clapFired = 1
    sinceClap = 0
  }
  clapOn = sinceClap < 0.15

  // hi-hat: top-of-spectrum bins, scaled way up
  var hraw = (frequencyData[30] + frequencyData[31]) * 30
  hhAvg += (hraw - hhAvg) * min(1, dt * 0.5)
  hhFired = 0
  sinceHh += dt
  if (hraw > hhAvg * 2 && hraw > 0.01 && sinceHh > debounce) {
    hhFired = 1
    sinceHh = 0
  }
  hhOn = sinceHh < 0.15
}

// ---------------------------------------------------------------- commands
function doCmd(id, arg) {
  if (id == CMD_TEMPO) bpm = arg > 0 ? arg : (tempoReliable ? detectedBpm : bpm)
  if (id == CMD_PHRASE) phraseBeats = max(1, arg)
  if (id == CMD_THEME) themeHue = arg
  if (id == CMD_FLIP) direction = !direction
  if (id == CMD_SEEDBASS) bassMax = max(bassMax, arg)
}

function nextEntry(skipSec) {
  qIndex = (qIndex + 1) % qLen
  entrySec = skipSec                     // skip in: absorbs detection latency
  resetShared()
}

// ============================================================ mini-patterns
// Every mini-pattern fills the shared hue/sat/val scratch arrays each frame;
// the exported renderers just read them (2D/3D forward to the same 1D look).

function fpOff() {
  arrayReplace(valA, 0)
}

function fpProgress(dt, perMeasure) {
  var fill = perMeasure ? frac(beatCount / 4) : patternProg
  var pulse = 0.45 + 0.55 * quarter * quarter    // brightness rides each beat
  for (var i = 0; i < N; i++) {
    var lit = posOf(i) <= fill
    hueA[i] = themeHue
    satA[i] = 1
    valA[i] = lit ? pulse : 0
  }
}

function fpSweep(dt) {
  var dot = frac(beatCount)              // one pass per beat
  if (direction) dot = 1 - dot
  for (var i = 0; i < N; i++) {
    hueA[i] = themeHue
    satA[i] = 1
    valA[i] = near(i / N, dot, 0.08)
  }
}

function fpQuarters(dt) {
  var q2 = quarter * quarter
  for (var i = 0; i < N; i++) {
    var p = i / N
    hueA[i] = themeHue + p * 0.12        // slight hue grade along the strip
    satA[i] = 1
    valA[i] = (1 - abs(p - 0.5) * 2) * q2
  }
}

function fpEighths(dt) {
  decayVals(0.35, dt)
  var mPos = frac(beatCount / 4)
  var e8 = floor(mPos * 8)
  var e2 = eighth * eighth
  for (var i = 0; i < N; i++) {
    var seg = min(7, floor(posOf(i) * 8))
    if (seg == e8) {
      valA[i] = max(valA[i], e2)
      hueA[i] = themeHue + mPos * 0.25   // hue drifts through the measure
      satA[i] = 0.75 + 0.25 * mPos
    }
  }
}

function fpStrobe(dt) {
  var on = frac(beatCount * 4) < 0.25 ? 1 : 0  // leading edge of each 16th
  for (var i = 0; i < N; i++) {
    satA[i] = 0
    valA[i] = on
  }
}

function fpSurge(dt) {
  // colors emanate from center and withdraw sharply every two beats
  var front = 1 - half * half            // smoothed half-note ramp
  for (var i = 0; i < N; i++) {
    var radial = abs(i / N - 0.5) * 2
    var v = clamp(1 - abs(radial - front) * 4, 0, 1)
    if (radial < front) v = max(v, 0.3 * (1 - radial))
    hueA[i] = themeHue + patternProg * 0.35 + radial * 0.08
    satA[i] = 1
    valA[i] = v
  }
}

function fpDancer(dt) {
  var wig = quarter * quarter * quarter
  var jit = (square(frac(beatCount * 2), 0.5) * 2 - 1) * max(0, 1 - patternProg * 2)
  var grow = patternProg * bassNorm
  var dot = 0.5 + 0.27 * sin(entrySec * 0.8)     // slow wander
          + 0.1 * wig * sin(entrySec * 13)       // beat-cubed wiggle
          + 0.07 * jit                           // 8th-note jitter, fading out
          + 0.2 * grow * sin(entrySec * 5)       // bass term, growing in
  dot = clamp(dot, 0.02, 0.98)
  var w = 0.025 + 0.08 * bassNorm                // width breathes with bass
  var h = themeHue + (patternProg > 0.5 ? 0.5 : 0)  // snap to complement
  for (var i = 0; i < N; i++) {
    hueA[i] = h
    satA[i] = 1
    valA[i] = near(posOf(i), dot, w)
  }
}

function fpBassScope(dt) {
  decayVals(0.25, dt)
  // dot oscillates about center like a decaying bass note, chirping down
  var life = 1 - half                    // 0 -> 1 across the two beats
  var amp = half * half * half * 0.45
  var dot = 0.5 + amp * sin(life * (16 - 7 * life))
  var w = 0.04 + 0.04 * (1 - half)
  var h = themeHue + floor(frac(beatCount / 4) * 4) * 0.12  // step per beat
  for (var i = 0; i < N; i++) {
    var v = near(i / N, dot, w)
    if (v > valA[i]) {
      valA[i] = v
      hueA[i] = h
      satA[i] = 1
    }
  }
}

// paint fizzle — a texture brush: beat = new random sputtering stroke,
// claps extend it, everything decays (faster when loud) with a soft shimmer
var strokePos = 0.5, strokeLen = 0.2, strokeHue = 0
function sputter(start, len, h) {
  var n = max(1, floor(abs(len) * N))
  for (var k = 0; k < n; k++) {
    var idx = floor((start + len * k / n) * N)
    if (idx < 0 || idx >= N) continue
    if (random(1) < 0.8) {               // ~4/5 of pixels, biased bright
      valA[idx] = 0.35 + random(0.65)
      hueA[idx] = h
    }
  }
}
function fpFizzle(dt) {
  // spatially-rippled decay = continuous decay + slow shimmer in one pass
  var base = max(0, 1 - dt * (1.2 + 2.5 * volume))
  var ph = time(0.15) * PI2
  for (var i = 0; i < N; i++) {
    valA[i] *= base * (0.985 + 0.015 * sin(i / N * PI2 * 3 + ph))
    satA[i] = 0.35 + 0.65 * clamp(valA[i], 0, 1)  // dim grains go whitish
  }
  if (beatFired) {
    strokePos = random(1)
    strokeLen = (random(1) < 0.2 ? -1 : 1) * (0.08 + random(0.22))
    strokeHue = warpHue(time(0.3) + (random(1) < 0.5 ? 0.07 : 0))
    sputter(strokePos, strokeLen, strokeHue)
  }
  if (clapFired) sputter(strokePos + strokeLen, strokeLen * 0.6, strokeHue)
}

// build-up: segments multiply as the drop approaches, random on/off masks
var buMask = array(16)
var buLastTick = 0
function fpBuildup(dt) {
  var remaining = max(0, curDur - beatCount)
  var n = remaining > 16 ? 1 : remaining > 8 ? 2 : remaining > 4 ? 3 : 4
  var segs = n == 1 ? 2 : n == 2 ? 4 : n == 3 ? 8 : 16
  // reroll cadence: beats, then 8ths, then 16ths in the final bars
  var tickRate = remaining > 8 ? 1 : remaining > 4 ? 2 : 4
  var tick = floor(beatCount * tickRate)
  if (tick != buLastTick || beatFired || !setupDone) {
    buLastTick = tick
    setupDone = 1
    var lit = 0, k
    for (k = 0; k < segs; k++) {
      buMask[k] = random(1) < 0.5
      lit += buMask[k]
    }
    if (!lit) buMask[floor(random(segs))] = 1   // at least one always on
  }
  var h = themeHue + phraseProg * 0.5
  var s = remaining < 2 ? 0 : 1                 // white for the last beats
  var pulse = 0.35 + 0.65 * quarter
  for (var i = 0; i < N; i++) {
    var seg = min(segs - 1, floor(posOf(i) * segs))
    hueA[i] = h
    satA[i] = s
    valA[i] = buMask[seg] ? pulse : 0
  }
}

function fpOcean(dt) {
  // layered slow sines, cubed for contrast; volume scales brightness
  var fMod = 3 + sin(entrySec * 0.1)            // one frequency self-modulates
  var t1 = time(0.07), t2 = time(0.05), t3 = time(0.03)
  var lvl = 0.2 + 0.8 * volume
  var lateDrift = patternProg > 0.7 ? (patternProg - 0.7) * 0.5 : 0
  var crestGate = square(time(0.1), 0.07)       // occasional crest pulse
  var crestPos = frac(beatCount / 8)
  for (var i = 0; i < N; i++) {
    var p = i / N
    var v = (wave(p * 1.5 + t1) + wave(p * fMod - t2) + wave(p * 5 + t3)) / 3
    v = v * v * v * lvl
    var c = crestGate * near(p, crestPos, 0.05)
    hueA[i] = themeHue + 0.04 * sin(p * PI2 + entrySec * 0.2) + lateDrift
    satA[i] = 1 - c * 0.85                      // white crest
    valA[i] = max(v, c)
  }
}

function fpSplotch(dt) {
  decayVals(0.4, dt)
  if (beatFired) {
    // deep red/pink blob normally, cyan on the high-frequency variant
    var bp = random(1), bw = 0.05 + random(0.08)
    var bh = hhOn ? 0.5 : (random(1) < 0.5 ? 0.97 : 0.93)
    for (var k = 0; k < N; k++) {
      var v = near(k / N, bp, bw)
      if (v > valA[k]) {
        valA[k] = v
        hueA[k] = bh
        satA[k] = 1
      }
    }
  }
  for (var i = 0; i < N; i++) {
    var p = i / N
    if (hhOn && p < 0.12 && i % 3 == 0) {       // one end flicks warm white
      valA[i] = 0.8
      hueA[i] = 0.1
      satA[i] = 0.3
    }
    if (clapOn && p > 0.88 && i % 3 == 0) {     // the other end for claps
      valA[i] = 0.8
      hueA[i] = 0.1
      satA[i] = 0.3
    }
  }
}

// spectrum analyzer with a PI auto-gain toward a target fill
var binAvg = array(20), binHue = array(20)
var gainI = 1, markerPos = 0.5
function fpAnalyzer(dt) {
  var b, k
  for (b = 0; b < 20; b++) {
    var idx = floor(b * 1.55)
    var lvl = frequencyData[idx] + frequencyData[min(31, idx + 1)]
    binAvg[b] += (lvl - binAvg[b]) * min(1, dt * 8)
    var peaky = binAvg[b] > 0.0005 ? lvl / binAvg[b] : 0
    if (peaky > 2) binHue[b] = clamp(peaky / 5, 0, 0.8)  // jump on peaks
    else binHue[b] += (themeHue - binHue[b]) * min(1, dt)  // relax back
  }
  // PI controller: keep average lit fraction near a volume-tied target
  var fill = arraySum(valA) / N
  var err = (0.1 + 0.3 * volume) - fill
  gainI = clamp(gainI + err * dt * 2, 0, 40)
  var gain = clamp(gainI + err * 4, 0, 40)
  // smooth/sparkle crossfade oscillates over the phrase
  var sparkle = wave(phraseProg) * 0.5
  for (var i = 0; i < N; i++) {
    b = min(19, floor(i / N * 20))
    var v = clamp(binAvg[b] * 8 * gain, 0, 1)
    if (sparkle > 0.3) v *= 0.7 + random(0.5)
    hueA[i] = binHue[b]
    satA[i] = 1
    valA[i] = v
  }
  // second half: desaturated marker tracks the dominant frequency
  if (patternProg > 0.5 && maxFrequency > 20) {
    var mp = clamp(log2(maxFrequency / 55) / 6, 0, 1)
    markerPos += (mp - markerPos) * min(1, dt * 4)
    k = floor(markerPos * (N - 1))
    valA[k] = max(valA[k], 0.8)
    satA[k] = 0.15
  }
}

// elastic: a spring chain chasing a beat-jumping target
var EP = 5
var ex = array(EP), ev = array(EP)
var eTarget = 0.5, eLastQ = 0
function fpElastic(dt) {
  var j
  if (!setupDone) {
    setupDone = 1
    for (j = 0; j < EP; j++) {
      ex[j] = 0.5
      ev[j] = 0
    }
  }
  // trails linger periodically: the decay time itself oscillates
  decayVals(0.15 + 0.3 * wave(beatCount / 24), dt)

  // target jumps on detected beats; by clock without a board; wanders in silence
  var jump = beatFired
  var q = floor(beatCount)
  if ((!boardPresent || bassMax < 0.002) && q != eLastQ) jump = 1
  eLastQ = q
  if (jump) eTarget = frac(eTarget + 0.125 + random(0.75))  // >= 1/8 away
  if (volume < 0.02) eTarget = 0.5 + 0.2 * sin(entrySec * 0.5)

  var k = 3 + 6 * patternProg                    // spring stiffens over slot
  var dts = min(dt, 0.05)
  for (j = 0; j < EP; j++) {
    var anchor = j == 0 ? eTarget : ex[j - 1]
    var d = anchor - ex[j]
    if (j > 0) d -= clamp(d, -0.03, 0.03)        // rest length
    ev[j] += d * k * dts * 8
    ev[j] *= max(0, 1 - 3 * dts)                 // friction
    ex[j] = clamp(ex[j] + ev[j] * dts, 0, 1)
  }
  // plot with linear interpolation between adjacent pixels
  var baseH = 0.78                               // violet base, splayed by volume
  for (j = 0; j < EP; j++) {
    var f = ex[j] * (N - 1)
    var i0 = floor(f)
    var fr = f - i0
    var h = baseH + j * 0.02 * (1 + volume * 4)
    if (1 - fr > valA[i0]) {
      valA[i0] = 1 - fr
      hueA[i0] = h
      satA[i0] = 1
    }
    if (fr > valA[i0 + 1]) {
      valA[i0 + 1] = fr
      hueA[i0 + 1] = h
      satA[i0 + 1] = 1
    }
  }
}

function runPattern(id, dt) {
  if (id == OFF) fpOff()
  else if (id == PROGRESS) fpProgress(dt, 0)
  else if (id == MPROGRESS) fpProgress(dt, 1)
  else if (id == SWEEP) fpSweep(dt)
  else if (id == QUARTERS) fpQuarters(dt)
  else if (id == EIGHTHS) fpEighths(dt)
  else if (id == STROBE) fpStrobe(dt)
  else if (id == SURGE) fpSurge(dt)
  else if (id == DANCER) fpDancer(dt)
  else if (id == BSCOPE) fpBassScope(dt)
  else if (id == FIZZLE) fpFizzle(dt)
  else if (id == BUILDUP) fpBuildup(dt)
  else if (id == OCEAN) fpOcean(dt)
  else if (id == SPLOTCH) fpSplotch(dt)
  else if (id == ANALYZER) fpAnalyzer(dt)
  else if (id == ELASTIC) fpElastic(dt)
  else fpOff()
}

// =============================================================== frame loop
export function beforeRender(delta) {
  var dt = delta / 1000
  analyzeSound(delta)

  entrySec += dt
  beatSec = 60 / max(30, bpm)

  // queue management: commands run through; timed entries advance (possibly
  // early on beat / sound), skipping slightly into the next entry
  var guard = 0
  while (guard < 80 && qLen > 0) {
    guard += 1
    var mode = qMode[qIndex]
    var id = qPat[qIndex]
    if (mode == 3) {
      doCmd(id, qArg[qIndex])
      nextEntry(0)
      continue
    }
    curDur = qDur[qIndex]
    if (curDur <= 0) curDur = phraseBeats
    beatCount = entrySec / beatSec
    var adv = beatCount >= curDur
    if (mode == 1 && beatFired) adv = 1
    if (mode == 2 && soundSpike) adv = 1
    if (id == ANALYZER && !boardPresent) adv = 1  // skips without a board
    if (!adv) break
    nextEntry(0.04)
  }

  // the beat clock: ramp-DOWN note timers derived from time-in-entry
  beatCount = entrySec / beatSec
  quarter = 1 - frac(beatCount)
  eighth = 1 - frac(beatCount * 2)
  sixteenth = 1 - frac(beatCount * 4)
  half = 1 - frac(beatCount / 2)
  whole = 1 - frac(beatCount / 4)
  phraseProg = frac(beatCount / phraseBeats)
  patternProg = clamp(beatCount / curDur, 0, 1)

  runPattern(qPat[qIndex], dt)
}

// all three renderers export; content is 1D — 2D/3D forward to the shared look
export function render(index) {
  hsv(hueA[index], satA[index], valA[index])
}
export function render2D(index, x, y) {
  render(index)
}
export function render3D(index, x, y, z) {
  render(index)
}

// ========================================================== the demo show
// (loops forever; queue restarts when exhausted)
enq(CMD_TEMPO, 0, 3, 120)
enq(CMD_PHRASE, 0, 3, 16)
enq(OFF, 128, 2, 0)            // hold dark until any sound (up to ~4 min)
enq(CMD_SEEDBASS, 0, 3, 0.3)   // pre-seed bass gain so first beats detect
enq(CMD_THEME, 0, 3, 0.55)     // light blue
enq(OCEAN, 32, 1, 0)           // ocean until a beat is detected
enq(FIZZLE, 16, 0, 0)
enq(BUILDUP, 16, 0, 0)
enq(SURGE, 8, 0, 0)
enq(FIZZLE, 8, 0, 0)
enq(CMD_THEME, 0, 3, 0.8)      // violet
enq(OCEAN, 16, 0, 0)
enq(CMD_THEME, 0, 3, 0)        // red, wait for a downbeat
enq(OFF, 16, 1, 0)
// rhythmic-precision section
enq(PROGRESS, 8, 0, 0)
enq(SWEEP, 4, 0, 0)
enq(CMD_FLIP, 0, 3, 0)
enq(CMD_THEME, 0, 3, 0.03)     // tiny hue nudge between sweeps
enq(SWEEP, 4, 0, 0)
enq(CMD_FLIP, 0, 3, 0)
enq(QUARTERS, 8, 0, 0)
enq(EIGHTHS, 8, 0, 0)
enq(BSCOPE, 4, 0, 0)
enq(OFF, 2, 0, 0)
enq(BSCOPE, 4, 0, 0)
enq(STROBE, 2, 0, 0)
enq(CMD_THEME, 0, 3, 0.33)     // green dancing pixel
enq(DANCER, 16, 0, 0)
enq(BSCOPE, 8, 0, 0)
enq(ELASTIC, 16, 0, 0)
enq(SPLOTCH, 16, 0, 0)
enq(MPROGRESS, 8, 0, 0)
enq(ELASTIC, 16, 0, 0)
enq(ANALYZER, 32, 0, 0)        // long analyzer stint, then loop forever
// enq(manualPattern, 9999, 0, 0)  // manual-pattern debugging entry (see slider)
