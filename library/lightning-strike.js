// name: Lightning Strike
// Curated original for the Luxel library: a thunderstorm on a strip, built
// from how real lightning actually behaves rather than from a blue blinker.
// Each event starts with a faint stepped leader flickering along the channel,
// then the return stroke — a full-brightness flash that arrives inside a
// single frame — followed by up to a few restrikes down the same channel at
// 50-300 ms, each one dimmer. Brightness decays very fast (tens of ms) and
// then leaves a long faint afterglow in the channel and in the sky. The
// channel is jagged (a random walk with dark kinks) and throws a couple of
// dimmer branch stubs past its ends. Distant sheet lightning occasionally
// flashes the whole strip with no bolt at all, and strikes cluster in bursts
// with seconds of darkness between them.
//
// Colour is the part most lightning patterns get wrong: the core is blinding
// white with only a faint blue-violet lean, and it cools to a dim blue-grey
// as it fades — never a saturated blue.
//
// Every control carries a //# directive, so the UI sends REAL units:
// strikes per minute, restrike count, afterglow seconds.

var chan = array(pixelCount)   // channel profile of the live bolt, 0..1
var hot = array(pixelCount)    // stroke brightness, very fast decay
var glow = array(pixelCount)   // afterglow left in the channel, slow decay

var skyHot = 0                 // diffuse sky flash, fast part
var skyGlow = 0                // diffuse sky flash, lingering part

// --- controls (real units) -------------------------------------------------
var strikesPerMin = 12         // storm rate
var maxRestrikes = 3           // extra strokes down the same channel
var afterglowSec = 1.2         // how long the faint glow hangs around
var sheetOn = 1                // distant sheet lightning allowed

// --- storm state machine ---------------------------------------------------
// 0 dark, 1 stepped leader, 2 return stroke / restrikes, 3 sheet flash
var state = 0
var timer = 0.6                // seconds until this phase ends
var strikesLeft = 0            // restrikes still owed this event
var strokeIndex = 0            // 0 = return stroke, 1.. = restrikes
var boltA = 0                  // channel extent, inclusive
var boltB = 0
var leaderDir = 1              // +1 travels up the strip, -1 down it
var leaderTotal = 0.08         // leader duration, seconds
var sheetRate = 0              // sky ramp rate during a sheet flash
var eventAge = 0               // seconds since this event started

// Interval to the next event, start-to-start. Half the time it is a tight
// cluster follow-up, the rest of the time a long dark wait — the weights are
// picked so the MEAN is exactly 60/strikesPerMin, i.e. the slider is honest.
function nextGap() {
  var base = 60 / strikesPerMin
  if (random(1) < 0.45) return base * (0.05 + random(0.2))
  return base * (0.7 + random(2))
}

// Jagged channel: a clamped random walk along a random span of the strip,
// with dark kinks, plus a couple of dim branch stubs past the ends.
function buildChannel() {
  var i = 0
  for (i = 0; i < pixelCount; i++) chan[i] = 0

  var span = floor(pixelCount * (0.45 + random(0.55)))
  if (span > pixelCount) span = pixelCount
  if (span < 3) span = min(3, pixelCount)
  boltA = floor(random(pixelCount - span + 1))
  boltB = boltA + span - 1

  var c = 0.7 + random(0.3)
  for (i = boltA; i <= boltB; i++) {
    c = c + random(0.36) - 0.18
    if (c > 1) c = 1
    if (c < 0.35) c = 0.35
    var v = c
    if (random(1) < 0.07) v = v * (0.2 + random(0.3))   // dark kink
    chan[i] = v
  }

  var b = 0
  for (b = 0; b < 3; b++) {
    if (random(1) < 0.45) continue
    var len = 2 + floor(random(3))
    var amp = 0.2 + random(0.25)
    var up = random(1) < 0.5
    var j = 0
    for (j = 0; j < len; j++) {
      var p = boltB + 1 + j
      if (up) p = boltA - 1 - j
      if (p < 0) continue
      if (p >= pixelCount) continue
      var a = amp * (1 - j / (len + 1))
      if (a > chan[p]) chan[p] = a
    }
  }
}

// Return stroke: the whole channel lights inside this one frame.
function fireStroke(amp) {
  var i = 0
  for (i = 0; i < pixelCount; i++) {
    var c = chan[i]
    if (c <= 0) continue
    var v = c * amp * (0.85 + random(0.3))    // each restrike re-jags a little
    if (v > 1) v = 1
    if (v > hot[i]) hot[i] = v
    var g = v * 0.13
    if (g > glow[i]) glow[i] = g
  }
  skyHot = skyHot + 0.16 * amp
  if (skyHot > 1) skyHot = 1
  skyGlow = skyGlow + 0.05 * amp
  if (skyGlow > 0.4) skyGlow = 0.4
}

// Storm rate in strikes per minute.
//# min=1 max=60 step=1 default=12
export function sliderStrikesPerMinute(v) {
  strikesPerMin = clamp(v, 1, 60)
}

// Most events restrike down the same channel; this is how many at most.
//# min=0 max=4 step=1 default=3
export function sliderMaxRestrikes(v) {
  maxRestrikes = clamp(floor(v), 0, 4)
}

// Seconds of faint glow left behind after a flash dies.
//# min=0.2 max=6 step=0.1 default=1.2
export function sliderAfterglowSeconds(v) {
  afterglowSec = clamp(v, 0.2, 6)
}

// Distant sheet lightning: a soft whole-strip flash with no visible bolt.
//# default=1
export function toggleSheetLightning(on) {
  sheetOn = on
}

// The next gap is measured start-to-start, so subtract however long this
// event ran; the slider then means what it says even at 60 strikes/min.
function endEvent() {
  state = 0
  timer = nextGap() - eventAge
  if (timer < 0.03) timer = 0.03
}

function beginEvent() {
  eventAge = 0
  if (sheetOn > 0.5 && random(1) < 0.22) {
    state = 3
    timer = 0.05 + random(0.05)
    sheetRate = (0.1 + random(0.16)) / timer
    return
  }
  buildChannel()
  leaderDir = 1
  if (random(1) < 0.5) leaderDir = -1
  leaderTotal = 0.05 + random(0.09)
  timer = leaderTotal
  strokeIndex = 0
  strikesLeft = 0
  if (maxRestrikes > 0 && random(1) < 0.78) strikesLeft = 1 + floor(random(maxRestrikes))
  state = 1
}

// The leader crawls along the channel in discrete steps at a few percent
// brightness, dropping out at random — the flicker before the bang.
function stepLeader() {
  var span = boltB - boltA
  if (span < 1) span = 1
  var f = 1 - timer / leaderTotal
  if (f < 0) f = 0
  if (f > 1) f = 1
  var stepPix = floor(pixelCount / 14)
  if (stepPix < 1) stepPix = 1
  var travelled = floor(f * span / stepPix) * stepPix
  var front = boltA + travelled
  if (leaderDir < 0) front = boltB - travelled

  var w = 0
  for (w = 0; w < stepPix + 1; w++) {
    var p = front - leaderDir * w
    if (p < 0) continue
    if (p >= pixelCount) continue
    if (chan[p] <= 0) continue
    var v = (0.04 + random(0.1)) * chan[p]
    if (random(1) < 0.35) v = v * 0.3
    if (v > hot[p]) hot[p] = v
  }
}

export function beforeRender(delta) {
  var dt = delta / 1000
  if (dt > 0.2) dt = 0.2

  // fast channel decay (~35 ms), then the slow afterglow tail
  var keepHot = exp(-dt / 0.035)
  var keepGlow = exp(-dt / max(0.05, afterglowSec / 3))
  var i = 0
  for (i = 0; i < pixelCount; i++) {
    var h = hot[i] * keepHot - 0.002
    hot[i] = h > 0 ? h : 0
    var g = glow[i] * keepGlow - 0.0008
    glow[i] = g > 0 ? g : 0
  }
  skyHot = skyHot * exp(-dt / 0.05) - 0.002
  if (skyHot < 0) skyHot = 0
  skyGlow = skyGlow * keepGlow - 0.0008
  if (skyGlow < 0) skyGlow = 0

  timer -= dt
  if (state > 0) eventAge += dt

  if (state == 0) {
    if (timer <= 0) beginEvent()
  } else if (state == 1) {
    stepLeader()
    if (timer <= 0) {
      fireStroke(1)                       // return stroke: full scale, one frame
      state = 2
      if (strikesLeft > 0) timer = 0.05 + random(0.25)
      else timer = 0.15 + random(0.2)
    }
  } else if (state == 2) {
    if (timer <= 0) {
      if (strikesLeft > 0) {
        strikesLeft -= 1
        strokeIndex += 1
        var amp = pow(0.62, strokeIndex) * (0.8 + random(0.3))
        if (amp > 1) amp = 1
        fireStroke(amp)
        if (strikesLeft > 0) timer = 0.05 + random(0.25)
        else timer = 0.15 + random(0.2)
      } else {
        endEvent()
      }
    }
  } else {
    skyHot = skyHot + sheetRate * dt      // sheet flash: soft rise, no bolt
    if (skyHot > 1) skyHot = 1
    if (timer <= 0) {
      skyGlow = skyGlow + skyHot * 0.35
      if (skyGlow > 0.4) skyGlow = 0.4
      endEvent()
    }
  }
}

export function render(index) {
  var v = hot[index] + glow[index] + skyHot + skyGlow
  if (v > 1) v = 1
  if (v <= 0) {
    rgb(0, 0, 0)
  } else {
    // hot core: blinding white with a faint blue-violet lean; everything
    // dimmer cools toward blue-grey, which is what the sky flash looks like
    var h = v / 0.55
    if (h > 1) h = 1
    rgb(v * (0.34 + 0.62 * h), v * (0.42 + 0.52 * h), v * (0.7 + 0.3 * h))
  }
}
