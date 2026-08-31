// name: Example: modes and waveforms
// Clean-room reimplementation from a prose functional description of the
// community pattern "Example: modes and waveforms"; original source never
// consulted.

// Educational demo: the strip shows a static white-on-black picture of a
// waveform (about four repeats across the strip), snapping to the next
// shape in a fixed list roughly every two-thirds of a second. The point is
// the idiom: shaping functions stored as first-class values in an array,
// dispatched by a rollover mode timer.

const NUM_MODES = 13

// --- controls (defaults reproduce the original constants) ---------------
var holdSecs = 0.65        // hold time per mode, seconds (was 650 ms)
var repeats = 4            // waveform repeats along the strip
var autoAdvance = 1        // 1 = walk the list, 0 = hold the picked shape
var pinnedMode = 0         // shape held when not auto-advancing
var traceHue = 0           // color of the trace; saturation 0 = the original white
var traceSat = 0

//# min=0.1 max=5 step=0.05 default=0.65
export function sliderModeSeconds(v) { holdSecs = max(v, 0.05) }

//# min=1 max=12 step=1 default=4
export function sliderRepeats(v) { repeats = max(floor(v), 1) }

//# default=1
export function toggleAutoAdvance(v) { autoAdvance = v > 0.5 }

//# min=0 max=12 step=1 default=0
export function sliderMode(v) { pinnedMode = clamp(floor(v), 0, NUM_MODES - 1) }

// tint the trace; the pattern starts white (saturation 0), so the picker's
// S channel is what turns the hue on
export function hsvPickerTraceColor(h, s, v) { traceHue = h; traceSat = s }

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

export function beforeRender(delta) {
  // 650 * (holdSecs / 0.65) is exactly 650 ms at the control's default
  var holdMs = 650 * (holdSecs / 0.65)
  elapsed += delta
  if (elapsed >= holdMs) {
    elapsed -= holdMs      // carry the leftover so durations stay accurate
    mode += 1
    if (mode >= NUM_MODES) mode = 0
  }
  if (!autoAdvance) mode = pinnedMode
}

export function render(index) {
  var f = modes[mode]
  var v = f(index / pixelCount * repeats)
  hsv(traceHue, traceSat, v)   // white by default: brightness only
}
