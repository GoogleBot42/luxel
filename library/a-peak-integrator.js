// name: A Peak Integrator
// Clean-room reimplementation from a prose functional description of the
// community pattern "A Peak Integrator"; original source never consulted.

// Beat-detection engine with a debug-style visualization. The meter is a
// fixed 8 x 18 = 144 pixel strip of segments (the original sized them for
// a ~150 px strip and never derived them from pixelCount, so a longer
// install leaves the tail dark and a shorter one truncates the meter).
// Dark at rest; a detected peak lights the meter and adds lit time
// proportional to the pulse's duration and the integration slider. The
// timer is drained once per PIXEL rather than once per frame, which is
// what turns each segment into a little bar graph: the segment lights
// from its start and cuts off where the timer runs out, so a beat reads
// as every segment filling and then draining back in a few frames.
// Hue is one rainbow across the whole 144-pixel meter; peak strength
// relative to the loudest peak ever seen sets saturation and (scaled down
// to about a third) brightness.
//
// The detector below is a cleaned-up reimplementation — each coarse band
// really averages its four raw bands and all analysis state is per-band —
// but the DISPLAY keeps the original's shared meter level (its LED writes
// all landed on one index), because that shared level is what the pattern
// visibly does: every segment shows the same fill.

export var frequencyData = array(32)   // stubbed with zeros if no sensor

const BANDS = 8
const SEG = 18               // pixels per segment (the original's 150 / 8)
const METER = BANDS * SEG    // 144: pixels past this stay dark
const ANALYZE_MS = 20        // throttle: analysis every ~20 ms
const TRIGGER_RATIO = 1.3    // energy must beat long average by this...
const TRIGGER_FLOOR = 0.03   // ...and clear a small absolute floor
const RELEASE_RATIO = 1.1    // peak ends once energy nears short average
const LONG_TAU = 1.67        // seconds of history in the long average
const SHORT_TAU = 0.25       // ...and in the short one (both time-based, so
                             // the detector behaves the same at any frame rate)
const MIN_PEAK_S = 0.03      // shortest committable pulse
const WATCHDOG_S = 0.4       // force-end a peak that wedges
const TIME_GAIN = 13.2       // lit seconds per pulse-second at slider = 1
const MUTE_FLOOR = 0.01      // auto-mute threshold on the lowest band
const WARMUP_S = 0.5         // let the averages fill before triggering

var integration = 1
//# min=0 max=1 step=0.01 default=1
export function sliderIntegrationTime(v) { integration = v }

var debugFreeze = 0          // internal flag: freezes analysis when set

// per-coarse-band state
var longAvg = array(BANDS)     // ~1-2 s energy history
var shortAvg = array(BANDS)    // fraction-of-a-second history
var peaking = array(BANDS)
var peakElapsed = array(BANDS) // seconds since this band's trigger
var peakMag = array(BANDS)
var allTimeMax = array(BANDS)  // loudest committed peak per band
var bandE = array(BANDS)       // this pass's compressed energy per band
var loudBand = 0               // band carrying the most energy right now

// meter state: every segment carries the SAME remaining lit time (see
// header) but drains its own copy, so each draws its own fill bar
var ledTime = array(BANDS)
var ledStrength = 0            // shared 0..1 saturation/brightness driver

var sinceAnalyze = 0
var runTime = 0
var frameDt = 0              // seconds in the current frame, read by render

export function beforeRender(delta) {
  var dt = delta / 1000
  frameDt = dt
  runTime = min(runTime + dt, 100)     // saturating warmup clock

  sinceAnalyze += delta
  if (sinceAnalyze >= ANALYZE_MS && !debugFreeze) {
    analyze(sinceAnalyze / 1000)
    sinceAnalyze = 0
  }

  // NOTE: the meter timer is NOT drained here — render() drains it once per
  // pixel, which is what draws the per-segment fill bar (see header).
}

function analyze(dt) {
  var b, k, e

  // auto-mute in quiet rooms: bail when the lowest band is near-silent
  var muted = shortAvg[0] < MUTE_FLOOR && !peaking[0]

  // pass 1: compressed energy per coarse band, and which band is loudest
  loudBand = 0
  for (b = 0; b < BANDS; b++) {
    // collapse 4 raw bands into one coarse band, log-compressing each so
    // quiet signals are boosted and loud ones compress
    e = 0
    for (k = 0; k < 4; k++) {
      e += log(1 + 200 * frequencyData[b * 4 + k])
    }
    bandE[b] = e / 4
    if (bandE[b] > bandE[loudBand]) loudBand = b
  }

  // pass 2: averages and the per-band peak state machine
  for (b = 0; b < BANDS; b++) {
    e = bandE[b]

    // long and short moving averages of the compressed energy
    longAvg[b] += (e - longAvg[b]) * min(1, dt / LONG_TAU)
    shortAvg[b] += (e - shortAvg[b]) * min(1, dt / SHORT_TAU)

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
  // only the band carrying the transient drives the meter; the quieter
  // bands still run their own detectors, they just don't repaint it
  if (b != loudBand) return
  // strength relative to the loudest peak ever seen in this band
  // (divide-by-zero yields 0 in this language, so a fresh band is safe)
  ledStrength = peakMag[b] / allTimeMax[b]
  // integrate: pulse span x slider x gain becomes lit time, handed to
  // every segment (one shared level, see header)
  var lit = peakElapsed[b] * integration * TIME_GAIN
  var s
  for (s = 0; s < BANDS; s++) {
    ledTime[s] += lit
  }
}

export function render(index) {
  // fixed-size meter: 8 segments of 18 px, anything past it stays dark
  if (index >= METER) {
    rgb(0, 0, 0)
    return
  }
  var b = floor(index / SEG)
  if (ledTime[b] <= 0) {
    rgb(0, 0, 0)
  } else {
    // drain once per PIXEL (see header) — this is what fills each segment
    // from its start and cuts it off where the timer runs out
    ledTime[b] -= frameDt
    // one rainbow across the whole meter, red at the start; modest value
    // so it reads as colored pulses, not a VU blast
    hsv(index / METER, ledStrength, ledStrength * 0.33)
  }
}
