// name: Example: modes and waveforms
// Clean-room reimplementation from a prose functional description of the
// community pattern "Example: modes and waveforms"; original source never
// consulted.

// Educational demo: the strip shows a static white-on-black picture of a
// waveform (about four repeats across the strip), snapping to the next
// shape in a fixed list roughly every two-thirds of a second. The point is
// the idiom: shaping functions stored as first-class values in an array,
// dispatched by a rollover mode timer.

const MODE_MS = 650        // hold time per mode
const REPEATS = 4          // waveform repeats along the strip
const NUM_MODES = 13

var modes = array(NUM_MODES)
modes[0] = (v) => frac(v)                              // sawtooth ramp
modes[1] = (v) => triangle(v)                          // linear up/down
modes[2] = (v) => wave(v)                              // smooth sine hump
modes[3] = (v) => square(v, 0.5)                       // hard on/off bars
modes[4] = (v) => triangle(triangle(v))                // folded triangle
modes[5] = (v) => wave(triangle(v))                    // sine of triangle
modes[6] = (v) => triangle(wave(v))                    // triangle of sine
modes[7] = (v) => wave(wave(v))                        // sine of sine
modes[8] = (v) => square(wave(triangle(v)), 0.7)       // dash-dot grouping
modes[9] = (v) => wave(v) * triangle(v * 1.3)          // detuned product
modes[10] = (v) => (wave(v) + triangle(v * 3.7)) / 2   // detuned blend
modes[11] = (v) => max(0, triangle(v * 2) - wave(v))   // clipped interference
modes[12] = (v) => abs(triangle(v) - wave(v * 2))      // waveform distance

var elapsed = 0
var mode = 0
// var pinnedMode = -1     // set >= 0 to pin a mode for study

export function beforeRender(delta) {
  elapsed += delta
  if (elapsed >= MODE_MS) {
    elapsed -= MODE_MS     // carry the leftover so durations stay accurate
    mode += 1
    if (mode >= NUM_MODES) mode = 0
  }
  // if (pinnedMode >= 0) mode = pinnedMode
}

export function render(index) {
  var f = modes[mode]
  var v = f(index / pixelCount * REPEATS)
  hsv(0, 0, v)             // grayscale: brightness only
}
