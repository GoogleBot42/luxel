// name: Sunrise Alarm Clock
// Clean-room reimplementation from a prose functional description of the
// community pattern "Sunrise Alarm Clock"; original source never consulted.

// The strip is a horizontal slice of sky playing out a whole day. Dark
// before the wake window; half an hour before wake time a sunrise blooms
// out of black (deep red on the "east" end blending to dark navy on the
// "west"), resolving over another half hour into a soft daytime blue. A
// small bright sun creeps end to end in proportion to day progress while
// perlin clouds drift by; half an hour before sleep time the sequence
// mirrors into a sunset (dawn colors swapped end for end) and back to
// black. The strip is vignetted at the ends and gamma-shaped.
//
// On startup (and whenever wake time is changed) it plays a preview day:
// simulated time starts just before sunrise at one simulated hour per real
// second; once the simulated day passes midnight it switches to tracking
// the real clock. Real mode smooths the clock's whole-second granularity
// by accumulating frame deltas and re-anchoring on each seconds rollover.

var HALF = 0.5                 // sunrise/sunset half-phase length, hours

// -- schedule (decimal hours; 6.5 = half past six) --
var wakeTime = 6.5
var weekendWakeTime = 8.5
var sleepTime = 21.5

// -- colors (RGB; pickers below overwrite the defaults) --
var east = array(3)            // east dawn: deep red
var west = array(3)            // west dawn: dark navy
var noon = array(3)            // noon sky: soft light blue (stored halved)
var sun = array(3)             // sun: warm near-white yellow
var cloud = array(3)           // clouds: mid gray (stored halved)
var BLACK = array(3)
east[0] = 0.6
west[2] = 0.3
noon[0] = 0.25; noon[1] = 0.35; noon[2] = 0.5
sun[0] = 1; sun[1] = 0.9; sun[2] = 0.6
cloud[0] = 0.3; cloud[1] = 0.3; cloud[2] = 0.3

// -- sliders --
var cloudiness = 0.5
var vigPow = 1.4               // vignette exponent
var gammaExp = 1.5             // per-channel output exponent, 1..2

// -- state --
var demo = 1                   // preview-day mode until simulated midnight
var simTime = wakeTime - 0.6   // demo clock, fractional hours
var msAccum = 0                // sub-second estimate for the coarse clock
var lastSec = -1
var tod = 0                    // current time of day, fractional hours
var dark = 1
var skyL = array(3)            // this frame's sky color at each end
var skyR = array(3)
var cloudC = array(3)          // this frame's cloud tint
var drift = 0                  // cloud-noise x drift
var sunLo = 0                  // sun span, fractional pixels
var sunHi = 0

function mix3(a, b, p, out) {
  out[0] = mix(a[0], b[0], p)
  out[1] = mix(a[1], b[1], p)
  out[2] = mix(a[2], b[2], p)
}

export function beforeRender(delta) {
  var wake = wakeTime
  var driftRate
  if (demo) {
    simTime += delta / 1000          // one simulated hour per real second
    if (simTime >= 24) {
      demo = 0                       // preview over: track the real clock
      simTime -= 24
    }
    tod = simTime
    driftRate = 2                    // modest cloud drift in demo
  } else {
    // weekend wake time on Saturday/Sunday (tolerate 0- or 1-based days)
    var wd = clockWeekday()
    if (wd == 0 || wd >= 6) wake = weekendWakeTime
    // millisecond tracking: accumulate deltas, re-anchor each seconds tick
    var sec = clockSecond()
    if (sec != lastSec) {
      lastSec = sec
      msAccum = 0
    } else {
      msAccum += delta
    }
    tod = clockHour() + clockMinute() / 60 + (sec + min(msAccum, 999) / 1000) / 3600
    driftRate = 300                  // real hours crawl; keep clouds moving
  }
  drift = tod * driftRate

  // classify the day into phases; build this frame's end colors
  dark = 0
  var p
  if (tod < wake - HALF || tod > sleepTime + HALF) {
    dark = 1
  } else if (tod < wake) {
    // sunrise, first half: black -> dawn gradient; clouds toward east tint
    p = (tod - (wake - HALF)) / HALF
    mix3(BLACK, east, p, skyL)
    mix3(BLACK, west, p, skyR)
    mix3(BLACK, east, p, cloudC)
  } else if (tod < wake + HALF) {
    // sunrise, second half: dawn gradient -> uniform noon sky
    p = (tod - wake) / HALF
    mix3(east, noon, p, skyL)
    mix3(west, noon, p, skyR)
    mix3(east, cloud, p, cloudC)
  } else if (tod < sleepTime - HALF) {
    // day: hold noon sky / daytime clouds
    mix3(noon, noon, 0, skyL)
    mix3(noon, noon, 0, skyR)
    mix3(cloud, cloud, 0, cloudC)
  } else if (tod < sleepTime) {
    // sunset, first half: noon -> dawn gradient, ends swapped
    p = (tod - (sleepTime - HALF)) / HALF
    mix3(noon, west, p, skyL)
    mix3(noon, east, p, skyR)
    mix3(cloud, east, p, cloudC)
  } else {
    // sunset, second half: swapped dawn gradient -> black
    p = (tod - sleepTime) / HALF
    mix3(west, BLACK, p, skyL)
    mix3(east, BLACK, p, skyR)
    mix3(east, BLACK, p, cloudC)
  }

  // sun span: position proportional to day progress; before wake / after
  // sleep it sits off the ends. Width scales with the strip (the spec's
  // suggested fix for the original's fixed couple of pixels).
  var sunW = max(2, pixelCount * 0.04)
  var dayFrac = (tod - wake) / max(0.01, sleepTime - wake)
  var center = -sunW + dayFrac * (pixelCount + 2 * sunW)
  if (dayFrac < 0 || dayFrac > 1) center = -3 * sunW
  sunLo = center - sunW / 2
  sunHi = center + sunW / 2
}

export function render(index) {
  if (dark) {
    rgb(0, 0, 0)
    return
  }
  var pos = index / pixelCount

  // sky: blend the two end colors along the strip
  var r = mix(skyL[0], skyR[0], pos)
  var g = mix(skyL[1], skyR[1], pos)
  var b = mix(skyL[2], skyR[2], pos)

  // clouds: perlin cloudiness biased around one half, scaled, clamped
  var amt = clamp((perlin(pos * 10 + drift, tod, 0, 3) + 0.5) * cloudiness, 0, 1)
  r = mix(r, cloudC[0], amt)
  g = mix(g, cloudC[1], amt)
  b = mix(b, cloudC[2], amt)

  // sun: anti-aliased coverage of this pixel by the span, added on top at
  // roughly double strength so it blooms over the sky
  var cover = clamp(min(sunHi, index + 1) - max(sunLo, index), 0, 1)
  if (cover > 0) {
    r += sun[0] * 2 * cover
    g += sun[1] * 2 * cover
    b += sun[2] * 2 * cover
  }

  // vignette (half-sine hump to a slider power), then gamma shaping
  var env = pow(sin(pos * PI), vigPow)
  rgb(pow(clamp(r, 0, 1), gammaExp) * env,
      pow(clamp(g, 0, 1), gammaExp) * env,
      pow(clamp(b, 0, 1), gammaExp) * env)
}

// -- controls --

export function hsvPickerEastDawnColor(h, s, v) { hsv2rgb(h, s, v, east) }

export function hsvPickerWestDawnColor(h, s, v) { hsv2rgb(h, s, v, west) }

export function hsvPickerNoonSkyColor(h, s, v) {
  hsv2rgb(h, s, v * 0.5, noon)     // internally halved so it stays muted
}

export function hsvPickerSunColor(h, s, v) { hsv2rgb(h, s, v, sun) }

export function hsvPickerCloudColor(h, s, v) {
  hsv2rgb(h, s, v * 0.5, cloud)    // internally halved
}

export function inputNumberWakeTime(v) {
  wakeTime = v
  if (demo) simTime = wakeTime - 0.6   // rewind the preview to pre-dawn
}

export function inputNumberWeekendWakeTime(v) { weekendWakeTime = v }

export function inputNumberSleepTime(v) { sleepTime = v }

//# min=0 max=1 step=0.01 default=0.5
export function sliderCloudiness(v) { cloudiness = v }

//# min=0 max=1 step=0.01 default=0.45
export function sliderVignette(v) { vigPow = 0.1 + v * 3 }

//# min=0 max=1 step=0.01 default=0.5
export function sliderGamma(v) { gammaExp = 1 + v }
