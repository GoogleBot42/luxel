// name: Sunrise Alarm
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/sunrise-alarm-clock.md, sunrise.md — reimagined, not a replica.
// The strip is a slice of sky living through your day: half an hour before wake
// time a dawn blooms (red in the east, navy in the west) and resolves into a
// blue sky with drifting clouds and a sun creeping end to end; half an hour
// before sleep time it sets mirrored and fades to a starry night. On start it
// previews a whole day in ~20 s; "Sunrise now" plays a wake-light sunrise over
// the chosen minutes. Sky, clouds and vignette are built on 49 samples a frame
// and laid down as 48 gradientRanges; the sun is an anti-aliased fillRangeAA.
// Dawn grows out of the night sky (and dusk back into it) with the stars fading
// through the half hour, so no transition passes through black.

var HALF = 0.5                 // sunrise/sunset half-phase, hours
var NS = 49                    // sky samples along the strip

var wakeTime = 6.5, weekendWakeTime = 8.5, sleepTime = 21.5
var cloudiness = 0.5, vignette = 0.45, riseMin = 20, preview = 1, stars = 1

var east = array(3), west = array(3), noon = array(3), sun = array(3), cloud = array(3)
east[0] = 0.6; east[1] = 0.07
west[1] = 0.02; west[2] = 0.2
noon[0] = 0.1; noon[1] = 0.24; noon[2] = 0.55
sun[0] = 1; sun[1] = 0.88; sun[2] = 0.6
cloud[0] = 0.3; cloud[1] = 0.32; cloud[2] = 0.38
var BLACK = array(3)
var NIGHT = array(3)           // the starry-night base the dawn grows out of
NIGHT[0] = 0.004; NIGHT[1] = 0.006; NIGHT[2] = 0.03

export function hsvPickerEastDawnColor(h, s, v) { hsv2rgb(h, s, v, east) }
export function hsvPickerWestDawnColor(h, s, v) { hsv2rgb(h, s, v, west) }
export function hsvPickerNoonSkyColor(h, s, v) { hsv2rgb(h, s, v, noon) }
export function hsvPickerSunColor(h, s, v) { hsv2rgb(h, s, v, sun) }
export function hsvPickerCloudColor(h, s, v) { hsv2rgb(h, s, v, cloud) }

// Weekday wake-up, hours (6.5 = half past six); the dawn starts half an hour before.
//# min=0 max=23.75 step=0.25 default=6.5
export function inputNumberWakeTime(v) {
  wakeTime = clamp(v, 0, 23.75)
  if (demo) simTime = wakeTime - 0.6         // rewind the preview to pre-dawn
}

// Saturday and Sunday wake-up, hours.
//# min=0 max=23.75 step=0.25 default=8.5
export function inputNumberWeekendWakeTime(v) { weekendWakeTime = clamp(v, 0, 23.75) }

// Bedtime, hours; the sunset ends half an hour after it.
//# min=0 max=23.75 step=0.25 default=21.5
export function inputNumberSleepTime(v) { sleepTime = clamp(v, 0, 23.75) }

//# min=0 max=100 step=1 default=50
export function sliderCloudinessPct(v) { cloudiness = clamp(v, 0, 100) / 100 }

// How far the strip ends fall off into shadow.
//# min=0 max=100 step=1 default=45
export function sliderVignettePct(v) { vignette = clamp(v, 0, 100) / 100; buildEnv() }

// Length of a "Sunrise now" dawn, minutes; full daylight then holds as long again.
//# min=1 max=60 step=1 default=20
export function sliderSunriseMinutes(v) { riseMin = clamp(v, 1, 60) }

export function triggerSunriseNow() { showT = 0; demo = 0 }

// Play a whole day fast when the pattern starts.
//# default=1
export function togglePreviewOnStart(v) { preview = v; if (!v) demo = 0 }

// A faint starry night outside the wake-to-sleep window instead of black.
//# default=1
export function toggleNightStars(v) { stars = v }

var demo = 1, simTime = wakeTime - 0.6, showT = -1
var msAccum = 0, lastSec = -1, tod = 0, dark = 0, t = 0, starAmt = 0
var skyL = array(3), skyR = array(3), cloudC = array(3)
var ENV = array(49), TR = array(49), TG = array(49), TB = array(49)
var sunLo = -9, sunHi = -9, sunEnv = 0

function buildEnv() {                      // half-sine hump, shaped by the vignette
  for (var k = 0; k < NS; k++) {
    var hump = sin(k / (NS - 1) * PI)
    ENV[k] = 1 - vignette * (1 - hump * sqrt(hump))
  }
}
buildEnv()

function mix3(a, b, p, out) {
  out[0] = mix(a[0], b[0], p); out[1] = mix(a[1], b[1], p); out[2] = mix(a[2], b[2], p)
}

export function beforeRender(delta) {
  var wake = wakeTime, rate = 300, p, k, dt = min(delta, 500) / 1000
  t = mod(t + dt, 3600)
  var sec = clockSecond()
  if (sec != lastSec) { lastSec = sec; msAccum = 0 } else msAccum = min(msAccum + delta, 999)
  var real = clockHour() + clockMinute() / 60 + (sec + msAccum / 1000) / 3600
  var wd = clockWeekday()                  // 1 = Sunday .. 7 = Saturday
  if (wd == 1 || wd == 7) wake = weekendWakeTime
  if (demo && preview) {
    simTime += dt                          // one simulated hour per real second
    if (simTime >= 24) demo = 0
    tod = simTime; wake = wakeTime; rate = 2
  } else if (showT >= 0) {                 // Sunrise now: dawn over riseMin, then hold
    showT += dt / 60
    var q = showT / riseMin
    if (q >= 2) showT = -1
    tod = wake - HALF + min(q, 1) * 2 * HALF
    rate = 30
  } else tod = real
  if (!(demo && preview) && showT < 0) demo = 0

  dark = 0
  starAmt = 0
  var base = stars ? NIGHT : BLACK
  if (tod < wake - HALF || tod > sleepTime + HALF) dark = 1
  else if (tod < wake) {
    p = (tod - (wake - HALF)) / HALF
    starAmt = 1 - p
    mix3(base, east, p, skyL); mix3(base, west, p, skyR); mix3(base, east, p, cloudC)
  } else if (tod < wake + HALF) {
    p = (tod - wake) / HALF
    mix3(east, noon, p, skyL); mix3(west, noon, p, skyR); mix3(east, cloud, p, cloudC)
  } else if (tod < sleepTime - HALF) {
    mix3(noon, noon, 0, skyL); mix3(noon, noon, 0, skyR); mix3(cloud, cloud, 0, cloudC)
  } else if (tod < sleepTime) {
    p = (tod - (sleepTime - HALF)) / HALF
    mix3(noon, west, p, skyL); mix3(noon, east, p, skyR); mix3(cloud, east, p, cloudC)
  } else {
    p = (tod - sleepTime) / HALF
    starAmt = p
    mix3(west, base, p, skyL); mix3(east, base, p, skyR); mix3(east, base, p, cloudC)
  }
  if (dark) return

  var drift = tod * rate
  for (k = 0; k < NS; k++) {               // sky + clouds + vignette at 49 samples
    var pos = k / (NS - 1)
    var amt = clamp((perlin(pos * 5 + drift, tod * 3, 0, 3) + 0.5) * cloudiness * 1.6 - 0.25 * cloudiness, 0, 1)
    TR[k] = mix(mix(skyL[0], skyR[0], pos), cloudC[0], amt) * ENV[k]
    TG[k] = mix(mix(skyL[1], skyR[1], pos), cloudC[1], amt) * ENV[k]
    TB[k] = mix(mix(skyL[2], skyR[2], pos), cloudC[2], amt) * ENV[k]
  }

  var n = pixelCount, sunW = max(2, n * 0.04)
  var dayFrac = (tod - wake) / max(0.01, sleepTime - wake)
  var c = -sunW + dayFrac * (n + 2 * sunW)
  if (dayFrac < 0 || dayFrac > 1) c = -3 * sunW
  sunLo = c - sunW / 2; sunHi = c + sunW / 2
  sunEnv = ENV[clamp(round(c / max(1, n - 1) * (NS - 1)), 0, NS - 1)]
}

// A mapless strip stays a strip: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

// the twinkling stars, added over whatever sky is down, at strength `amt`
function drawStars(n, amt) {
  for (var i = 0; i < n; i++) {
    var hs = hash(i * 3.7)
    if (hs <= 0.9) continue
    var tw = 0.18 * wave(t / (3 + hs * 4) + hs * 9) * amt
    rgb(tw, tw, tw)
    fillRangeAA(i, i + 1, 1)
  }
}

var HC = array(3)
export function renderBulk() {
  var n = pixelCount
  if (dark) {
    if (!stars) { clear(); return }
    rgb(NIGHT[0], NIGHT[1], NIGHT[2])
    fill()
    drawStars(n, 1)
    return
  }
  // sample k sits on pixel k * (n - 1) / 48: one HSV gradientRange per pair of
  // samples (hue unwrapped), added so the shared end pixels sum to whole, plus
  // a half pixel of the first and last sample at the ends
  clear()
  var step = max(1, n - 1) / (NS - 1), h0 = 0, s0 = 0, v0 = 0, x0 = 0.5
  for (var k = 0; k < NS; k++) {
    rgb2hsv(TR[k], TG[k], TB[k], HC)
    var h1 = k == 0 ? HC[0] : h0 + mod(HC[0] - h0 + 0.5, 1) - 0.5, x1 = k * step + 0.5
    if (k == 0) { hsv(h1, HC[1], HC[2]); fillRangeAA(0, 0.5, 1) }
    else gradientRange(x0, x1, h0, s0, v0, h1, HC[1], HC[2], 1)
    h0 = h1; s0 = HC[1]; v0 = HC[2]; x0 = x1
  }
  hsv(h0, s0, v0)
  fillRangeAA(x0, n, 1)
  rgb(sun[0] * sunEnv, sun[1] * sunEnv, sun[2] * sunEnv)   // the sun, blooming: added twice
  fillRangeAA(sunLo, sunHi, 1)
  fillRangeAA(sunLo, sunHi, 1)
  if (stars && starAmt > 0) drawStars(n, starAmt)
}
