// name: Sunrise Alarm Clock
// Clean-room reimplementation from a prose functional description of the
// community pattern "Sunrise Alarm Clock"; original source never consulted.

// The strip is a slice of sky playing out a whole day. Dark until half an
// hour before wake time, then a sunrise blooms out of black (deep red on
// the "east" end, dark navy on the "west"), resolving into a soft daytime
// blue. A small bright sun creeps end-to-end in proportion to day
// progress while perlin-noise clouds drift by; half an hour before sleep
// time the whole sequence mirrors into a sunset and back to black. The
// ends of the strip are vignetted and the output is gamma-shaped.
//
// On startup (and when wake time is changed) it plays a preview day:
// simulated time starts just before sunrise and runs at one hour per real
// second; after the simulated day passes midnight it switches to tracking
// the real clock. Real mode smooths the clock's whole-second tick by
// accumulating frame deltas and re-anchoring on each seconds rollover.

var HALF = 0.5                     // each sunrise/sunset half-phase, hours
var DEMO_RATE = 1 / 1000           // demo: 1 ms of delta = 1/1000 h => 1 h/s

// -- schedule (decimal hours) --
var wakeTime = 6.5
var weekendWakeTime = 8.5
var sleepTime = 21.5

// -- colors (RGB triples; pickers overwrite the defaults below) --
var eastDawn = array(3)            // deep red
var westDawn = array(3)            // dark navy
var noonSky = array(3)             // soft light blue (stored pre-halved)
var sunColor = array(3)            // warm near-white yellow
var cloudColor = array(3)          // mid gray (stored pre-halved)
var black = array(3)
eastDawn[0] = 0.55
westDawn[2] = 0.28
noonSky[0] = 0.23; noonSky[1] = 0.33; noonSky[2] = 0.5
sunColor[0] = 1; sunColor[1] = 0.88; sunColor[2] = 0.55
cloudColor[0] = 0.35; cloudColor[1] = 0.35; cloudColor[2] = 0.35

// -- sliders --
var cloudiness = 0.5
var vignette = 0.4
var gammaAmt = 0.6                 // exponent = 1 + gammaAmt

// -- state --
var demo = 1
var simTime = wakeTime - 0.6       // demo starts just before sunrise
var msAccum = 0                    // sub-second estimate for the coarse clock
var lastSec = -1
var tod = 0                        // current time of day, fractional hours

var skyPal = array(12)             // 4 RGB corners: end0@p0, end1@p0, end0@p1, end1@p1
var cloudPal = array(12)
var phaseP = 0                     // progress through the current half-phase
var isOff = 1
var sunStart = 0                   // sun span in (fractional) pixels
var sunEnd = 0
var driftRate = 2                  // cloud-noise drift multiplier

function copy3(pal, slot, c) {
  pal[slot] = c[0]
  pal[slot + 1] = c[1]
  pal[slot + 2] = c[2]
}

function setPal(pal, a, b, c, d) {
  copy3(pal, 0, a)
  copy3(pal, 3, b)
  copy3(pal, 6, c)
  copy3(pal, 9, d)
}

export function beforeRender(delta) {
  var wake = wakeTime
  if (demo) {
    simTime += delta * DEMO_RATE
    if (simTime >= 24) {
      demo = 0                     // preview day is over: track the real clock
      simTime -= 24
    }
    tod = simTime
    driftRate = 2
  } else {
    // Weekend wake time on Saturday/Sunday (weekday numbering tolerated
    // as either 0-based or 1-based).
    var wd = clockWeekday()
    if (wd == 0 || wd == 6 || wd == 7) wake = weekendWakeTime
    // Millisecond tracking: accumulate deltas, re-anchor on the seconds tick.
    var sec = clockSecond()
    if (sec != lastSec) {
      lastSec = sec
      msAccum = 0
    } else {
      msAccum += delta
    }
    tod = clockHour() + clockMinute() / 60 + (sec + min(msAccum, 999) / 1000) / 3600
    driftRate = 400                // real hours crawl; keep clouds moving
  }

  // Classify the day into phases and rebuild the two 4-color palettes.
  isOff = 0
  var dayFrac = clamp((tod - wake) / max(0.01, sleepTime - wake), 0, 1)
  if (tod < wake - HALF || tod > sleepTime + HALF) {
    isOff = 1
    phaseP = 0
  } else if (tod < wake) {
    // Sunrise, first half: black -> dawn gradient.
    phaseP = (tod - (wake - HALF)) / HALF
    setPal(skyPal, black, black, eastDawn, westDawn)
    setPal(cloudPal, black, black, eastDawn, eastDawn)
  } else if (tod < wake + HALF) {
    // Sunrise, second half: dawn gradient -> uniform noon sky.
    phaseP = (tod - wake) / HALF
    setPal(skyPal, eastDawn, westDawn, noonSky, noonSky)
    setPal(cloudPal, eastDawn, eastDawn, cloudColor, cloudColor)
  } else if (tod < sleepTime - HALF) {
    // Day: hold noon; progress measures the day instead.
    phaseP = 0
    setPal(skyPal, noonSky, noonSky, noonSky, noonSky)
    setPal(cloudPal, cloudColor, cloudColor, cloudColor, cloudColor)
  } else if (tod < sleepTime) {
    // Sunset, first half: noon -> dawn gradient, ends swapped.
    phaseP = (tod - (sleepTime - HALF)) / HALF
    setPal(skyPal, noonSky, noonSky, westDawn, eastDawn)
    setPal(cloudPal, cloudColor, cloudColor, eastDawn, eastDawn)
  } else {
    // Sunset, second half: swapped dawn gradient -> black.
    phaseP = (tod - sleepTime) / HALF
    setPal(skyPal, westDawn, eastDawn, black, black)
    setPal(cloudPal, eastDawn, eastDawn, black, black)
  }

  // Sun span: proportional to day progress, pushed off both ends outside
  // the day. Width scales with the strip (the spec's suggested fix).
  var sunW = max(2, pixelCount * 0.04)
  var sunCenter = -sunW + dayFrac * (pixelCount + 2 * sunW)
  if (tod < wake || tod > sleepTime) sunCenter = -2 * sunW
  sunStart = sunCenter - sunW / 2
  sunEnd = sunCenter + sunW / 2
}

export function render(index) {
  if (isOff) {
    rgb(0, 0, 0)
    return
  }
  var pos = index / (pixelCount - 1)

  // Sky and cloud colors: bilinear blend of the four palette corners
  // (position along the strip x phase progress).
  var r = mix(mix(skyPal[0], skyPal[3], pos), mix(skyPal[6], skyPal[9], pos), phaseP)
  var g = mix(mix(skyPal[1], skyPal[4], pos), mix(skyPal[7], skyPal[10], pos), phaseP)
  var b = mix(mix(skyPal[2], skyPal[5], pos), mix(skyPal[8], skyPal[11], pos), phaseP)

  // Clouds: perlin cloudiness, biased around one half, scaled and clamped.
  var n = perlin(pos * 12 + tod * driftRate, tod, 0, 1)
  var amt = clamp((0.5 + n) * cloudiness, 0, 1)
  r = mix(r, mix(mix(cloudPal[0], cloudPal[3], pos), mix(cloudPal[6], cloudPal[9], pos), phaseP), amt)
  g = mix(g, mix(mix(cloudPal[1], cloudPal[4], pos), mix(cloudPal[7], cloudPal[10], pos), phaseP), amt)
  b = mix(b, mix(mix(cloudPal[2], cloudPal[5], pos), mix(cloudPal[8], cloudPal[11], pos), phaseP), amt)

  // Sun: anti-aliased coverage of this pixel by the sun span, added on
  // top at roughly double strength.
  var cover = clamp(min(sunEnd, index + 1) - max(sunStart, index), 0, 1)
  if (cover > 0) {
    r += sunColor[0] * 2 * cover
    g += sunColor[1] * 2 * cover
    b += sunColor[2] * 2 * cover
  }

  // Vignette: half-sine hump raised to a slider-controlled power.
  var env = pow(sin(pos * PI), 0.2 + vignette * 3)

  // Gamma shaping (exponent between 1 and 2), then the envelope.
  var ga = 1 + gammaAmt
  r = pow(clamp(r, 0, 1), ga) * env
  g = pow(clamp(g, 0, 1), ga) * env
  b = pow(clamp(b, 0, 1), ga) * env
  rgb(r, g, b)
}

// -- controls --

export function hsvPickerEastDawnColor(h, s, v) {
  hsv2rgb(h, s, v, eastDawn)
}

export function hsvPickerWestDawnColor(h, s, v) {
  hsv2rgb(h, s, v, westDawn)
}

export function hsvPickerNoonSkyColor(h, s, v) {
  hsv2rgb(h, s, v, noonSky)
  noonSky[0] *= 0.5                // internally halved so it stays muted
  noonSky[1] *= 0.5
  noonSky[2] *= 0.5
}

export function hsvPickerSunColor(h, s, v) {
  hsv2rgb(h, s, v, sunColor)
}

export function hsvPickerCloudColor(h, s, v) {
  hsv2rgb(h, s, v, cloudColor)
  cloudColor[0] *= 0.5             // internally halved
  cloudColor[1] *= 0.5
  cloudColor[2] *= 0.5
}

export function inputNumberWakeTime(v) {
  wakeTime = v
  if (demo) simTime = wakeTime - 0.6   // rewind the preview to pre-dawn
}

export function inputNumberWeekendWakeTime(v) {
  weekendWakeTime = v
}

export function inputNumberSleepTime(v) {
  sleepTime = v
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderCloudiness(v) {
  cloudiness = v
}

//# min=0 max=1 step=0.01 default=0.4
export function sliderVignette(v) {
  vignette = v
}

//# min=0 max=1 step=0.01 default=0.6
export function sliderGamma(v) {
  gammaAmt = v
}
