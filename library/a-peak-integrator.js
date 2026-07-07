// name: A Peak Integrator
// Clean-room reimplementation from a prose functional description of the
// community pattern "A Peak Integrator"; original source never consulted.

// Beat-detection engine with a debug-style visualization. The strip is
// split into eight equal segments, one per coarse frequency band (lows at
// the start). Dark at rest; a detected peak in a band flashes its segment
// and adds lit time proportional to the pulse's duration and the
// integration slider, so sustained or repeated hits keep a segment
// glowing. Hue is a rainbow along the strip; a peak's strength relative
// to the loudest ever seen in that band sets saturation and (scaled down
// to about a third) brightness.
//
// This is a cleaned-up reimplementation: the prose notes the original had
// several index/state bugs (per-pixel timer drain, wrong-band writes,
// bass-only aggregation). Here each coarse band really averages its four
// raw bands, all state is per-band, and timers drain once per frame.

export var frequencyData = array(32)   // stubbed with zeros if no sensor

const BANDS = 8
const ANALYZE_MS = 20        // throttle: analysis every ~20 ms
const TRIGGER_RATIO = 1.3    // energy must beat long average by this...
const TRIGGER_FLOOR = 0.03   // ...and clear a small absolute floor
const RELEASE_RATIO = 1.1    // peak ends once energy nears short average
const MIN_PEAK_S = 0.03      // shortest committable pulse
const WATCHDOG_S = 0.4       // force-end a peak that wedges
const TIME_GAIN = 8          // lit seconds per pulse-second at slider = 1
const MUTE_FLOOR = 0.01      // auto-mute threshold on the lowest band
const WARMUP_S = 1           // let the averages fill before triggering

var integration = 0.5
//# min=0 max=1 step=0.01 default=0.5
export function sliderIntegrationTime(v) { integration = v }

var debugFreeze = 0          // internal flag: freezes analysis when set

// per-coarse-band state
var longAvg = array(BANDS)     // ~1-2 s energy history
var shortAvg = array(BANDS)    // fraction-of-a-second history
var peaking = array(BANDS)
var peakElapsed = array(BANDS) // seconds since this band's trigger
var peakMag = array(BANDS)
var allTimeMax = array(BANDS)  // loudest committed peak per band
var ledTime = array(BANDS)     // remaining lit seconds
var ledStrength = array(BANDS) // saturation/brightness driver, 0..1

var sinceAnalyze = 0
var runTime = 0

export function beforeRender(delta) {
  var dt = delta / 1000
  runTime = min(runTime + dt, 100)     // saturating warmup clock

  sinceAnalyze += delta
  if (sinceAnalyze >= ANALYZE_MS && !debugFreeze) {
    analyze(sinceAnalyze / 1000)
    sinceAnalyze = 0
  }

  // drain the segment timers once per frame (the original drained them
  // once per *pixel* — a noted bug, fixed here)
  var b
  for (b = 0; b < BANDS; b++) {
    ledTime[b] = max(0, ledTime[b] - dt)
  }
}

function analyze(dt) {
  var b, k, e

  // auto-mute in quiet rooms: bail when the lowest band is near-silent
  var muted = shortAvg[0] < MUTE_FLOOR && !peaking[0]

  for (b = 0; b < BANDS; b++) {
    // collapse 4 raw bands into one coarse band, log-compressing each so
    // quiet signals are boosted and loud ones compress
    e = 0
    for (k = 0; k < 4; k++) {
      e += log(1 + 200 * frequencyData[b * 4 + k])
    }
    e = e / 4

    // long and short moving averages of the compressed energy
    longAvg[b] += (e - longAvg[b]) * 0.03
    shortAvg[b] += (e - shortAvg[b]) * 0.2

    if (peaking[b]) {
      peakElapsed[b] += dt
      if (e > peakMag[b]) peakMag[b] = e
      // peak over: energy fell back near the short average (after a
      // minimum span), or the watchdog fired
      if ((peakElapsed[b] >= MIN_PEAK_S && e <= shortAvg[b] * RELEASE_RATIO)
          || peakElapsed[b] > WATCHDOG_S) {
        commitPeak(b)
      }
    } else if (!muted && runTime > WARMUP_S
               && e > longAvg[b] * TRIGGER_RATIO + TRIGGER_FLOOR) {
      peaking[b] = 1
      peakElapsed[b] = 0
      peakMag[b] = e
    }
  }
}

function commitPeak(b) {
  peaking[b] = 0
  if (peakMag[b] > allTimeMax[b]) allTimeMax[b] = peakMag[b]
  // strength relative to the loudest peak ever seen in this band
  // (divide-by-zero yields 0 in this language, so a fresh band is safe)
  ledStrength[b] = peakMag[b] / allTimeMax[b]
  // integrate: pulse span x slider x gain becomes lit time
  ledTime[b] += peakElapsed[b] * integration * TIME_GAIN
}

export function render(index) {
  // segment from the actual pixel count (the original hardcoded ~150px)
  var b = floor(index * BANDS / pixelCount)
  if (b >= BANDS) b = BANDS - 1
  if (ledTime[b] <= 0) {
    rgb(0, 0, 0)
  } else {
    // rainbow along the strip, red at the start through blue/violet;
    // modest value so it reads as colored pulses, not a VU blast
    hsv(0.85 * index / pixelCount, ledStrength[b], ledStrength[b] * 0.35)
  }
}
