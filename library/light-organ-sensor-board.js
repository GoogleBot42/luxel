// name: Light Organ -- sensor board
// Clean-room reimplementation from a prose functional description of the
// community pattern "Light Organ -- sensor board"; original source never
// consulted.

// Four-channel light organ: the strip tiles a repeating group of four
// colored bars (bass red / low orange / mid random / high blue-violet or
// slow-cycling), each pulsing with one frequency band through a per-band
// AGC (reciprocal-of-headroom gain against a decaying peak and a noise
// threshold). Bass beats rotate band-to-slot assignments, re-randomize
// bar width and some hues, and march a white accent (every 8th pixel,
// flashing on strong overall peaks) along the strip. Two-timescale energy
// averaging gates the whole strip dark when the music stops; the long
// reference freezes while gated so room chatter can't re-arm it.

export var frequencyData = array(32)
export var energyAverage = 0
export var maxFrequency = 0
export var maxFrequencyMagnitude = 0

// band bin boundaries: bass 0-5 (~<200 Hz), low 6-9 (~200-500 Hz),
// mid 10-17 (~500-2k), high 18-31 (~2k-10k)
var bandLo = array(4)
var bandHi = array(4)
bandLo[0] = 0;  bandHi[0] = 6
bandLo[1] = 6;  bandHi[1] = 10
bandLo[2] = 10; bandHi[2] = 18
bandLo[3] = 18; bandHi[3] = 32

var runMax = array(4)     // per-band recent maximum (decays every frame)
var threshEma = array(4)  // per-band average magnitude (slow EMA)
var peakB = array(4)      // per-band slow-relaxing peak (for gain)
var dispB = array(4)      // per-band displayed magnitude 0..1

var shortAvg = 0          // energy, ~seconds window
var longAvg = .005        // energy, ~minutes window, floored
var gated = 1
var wasGated = 1

var gMax = 0              // recent overall peak magnitude
var gPeak = 0
var flashV = 0            // white accent intensity

var peakTimer = 0
var prevBass = 0
var beatPhase = 0
var beatCount = 0
var layoutMode = 0
var marchOff = 0
var barWidth = 2
var midHue = .5
var highSel = 1
var highOff = 0

var slotBand = array(4)
var slotW = array(4)
var slotStart = array(4)
var bandHue = array(4)
var tileLen = 8

export function beforeRender(delta) {
  var dt = delta / 1000
  if (dt > .25) dt = .25
  var b
  var i

  // per-band max, and slow threshold EMA when not gated
  for (b = 0; b < 4; b++) {
    var mx = 0
    var sum = 0
    for (i = bandLo[b]; i < bandHi[b]; i++) {
      var m = frequencyData[i]
      if (m > mx) mx = m
      sum += m
    }
    var avg = sum / (bandHi[b] - bandLo[b])
    // recent max decays ~a dB per frame equivalent, delta-scaled
    runMax[b] = max(runMax[b] * (1 - dt * .8), mx)
    if (!gated) threshEma[b] += (avg - threshEma[b]) * min(dt / 2, 1)
    peakB[b] = max(peakB[b], runMax[b])
  }

  // two-timescale energy averages; long reference frozen while gated
  shortAvg += (energyAverage - shortAvg) * min(dt / 3, 1)
  if (!gated) {
    longAvg += (energyAverage - longAvg) * min(dt / 90, 1)
    longAvg = max(longAvg, .005)   // silent-room floor
  }

  // silence gate: short average well under half the long reference
  gated = shortAvg * 2.5 < longAvg
  if (gated && !wasGated) layoutMode = (layoutMode + 1) % 2  // differ on return
  wasGated = gated

  // slow peak relaxation, about once a second
  peakTimer += dt
  if (peakTimer >= 1) {
    peakTimer = 0
    for (b = 0; b < 4; b++) peakB[b] *= .98
    gPeak *= .98
  }

  // per-band AGC: reciprocal of headroom above the noise threshold
  for (b = 0; b < 4; b++) {
    if (gated) {
      dispB[b] = 0
    } else {
      var th = threshEma[b] * 3
      var gain = clamp(1 / max(peakB[b] - th, .002), 0, 500)
      dispB[b] = saturate((runMax[b] - th) * gain)
    }
  }

  // overall white-flash intensity from global peak vs energy reference
  gMax = max(gMax * (1 - dt * .8), maxFrequencyMagnitude)
  gPeak = max(gPeak, gMax)
  if (gated) {
    flashV = 0
  } else {
    var ref = max(shortAvg, longAvg) * 2.5
    var gg = clamp(1 / max(gPeak - ref, .002), 0, 500)
    flashV = saturate((gMax - ref) * gg)
  }

  // beat: bass display magnitude jumps up by more than ~a third of scale
  if (dispB[0] - prevBass > .33) {
    beatPhase = (beatPhase + 1) % 4
    marchOff = mod(marchOff - 3, pixelCount)
    barWidth = 1 + floor(random(4))
    midHue = random(1)
    highSel = random(1) < .5
    highOff = floor(random(3)) * .25
    beatCount += 1
    if (beatCount >= 40) {
      beatCount = 0
      layoutMode = (layoutMode + 1) % 2
    }
  }
  prevBass = dispB[0]

  // slot hues: bass red, low orange, mid random, high fixed or cycling
  bandHue[0] = 0
  bandHue[1] = .08
  bandHue[2] = midHue
  bandHue[3] = highSel ? .72 : time(.15) + highOff

  // build the four-bar tile for this frame
  tileLen = 0
  for (var s = 0; s < 4; s++) {
    slotBand[s] = (s + beatPhase) % 4
    if (layoutMode == 1) {
      // proportional bars: width breathes with the band's loudness
      var w = floor(dispB[slotBand[s]] * 5 + .5)
      if (slotBand[s] != 0 && w < 1) w = 1   // bass bar may collapse
      slotW[s] = min(w, 6)
    } else {
      slotW[s] = barWidth
    }
    slotStart[s] = tileLen
    tileLen += slotW[s]
  }
}

export function render(index) {
  var v = 0
  var h = 0
  var s = 1
  if (tileLen > 0) {
    var pos = mod(index, tileLen)
    var band = slotBand[0]
    for (var k = 3; k >= 0; k--) {
      if (pos >= slotStart[k] && slotW[k] > 0) {
        band = slotBand[k]
        break
      }
    }
    h = bandHue[band]
    v = dispB[band]
  }
  // marching white accent on strong overall peaks, every 8th pixel
  if (flashV > .6 && mod(index - marchOff, 8) == 0) {
    s = 0
    v = flashV
  }
  hsv(h, s, v)
}
