// name: sound - spectromatrix agc
// Clean-room reimplementation from a prose functional description of the
// community pattern "sound - spectromatrix agc"; original source never
// consulted.
//
// Sound paints drifting plasma ribbons on a matrix: two interfering
// position+time waves pick a fractional spectrum band per cell, and only
// above-baseline energy in that band lights it — so onsets and beats
// flare, sustained tones fade into the baseline. Peaks bleach to white,
// lit areas leave decaying trails, and the (narrow) hue arc slowly
// rotates through the rainbow. A PI automatic-gain controller closed
// through the render pass itself regulates how much of the panel is lit,
// so the pattern stays alive from whispers to parties.
//
// Simulated on a 16x16 virtual canvas; render2D samples it (the original
// hardcoded a serpentine matrix width). With no sensor board the
// spectrum is all zeros and the panel idles dark.

export var frequencyData   // 32-band spectrum; engine stubs zeros

var W = 16
var H = 16
var CELLS = W * H
var BANDS = 32

// ---- tunables ----
var targetFill = 0.08      // aim: ~8% of the panel lit
var trailDecay = 0.88      // per-frame retention of the persistence buffer
var avgWindowMs = 1500     // per-band EMA window
var hueArc = 0.2           // hue span across the band range at any instant

// per-band running average (each band's recent baseline)
var averages = array(BANDS)

// per-cell persistence (trails) and hue
var vals = array(CELLS)
var hues = array(CELLS)

// ---- AGC: PI controller on lit-fraction error ----
var kp = 4                 // small proportional gain
var ki = 15                // integral gain, a few times larger
var integral = 30          // moderate positive start: responsive at power-on
var sensitivity = 30
var fillAccum = 0          // sum of clamped displayed brightness, last frame

var t1 = 0, t2 = 0

export function beforeRender(delta) {
  // two incommensurate time bases so the motion never visibly loops
  t1 = time(0.05)          // ~3.3 s
  t2 = time(0.0313)        // ~2.05 s, incommensurate with t1

  var dt = delta / 1000

  // PI update on coverage error from the previous frame's render
  var err = targetFill - fillAccum / CELLS
  integral = clamp(integral + err * ki * dt, 0, 200)
  sensitivity = max(0, kp * err + integral)
  fillAccum = 0

  // fold the sensitivity-scaled spectrum into the per-band baselines
  var a = min(1, delta / avgWindowMs)
  for (var i = 0; i < BANDS; i++) {
    averages[i] = max(averages[i] * (1 - a) + frequencyData[i] * sensitivity * a, 0.001)
  }

  // paint the virtual canvas
  var drift = wave(t1)
  for (var y = 0; y < H; y++) {
    var wy = wave(y / W - drift)
    for (var x = 0; x < W; x++) {
      var wx = wave(x / W + drift)
      // interfering waves -> fractional band coordinate, triangle-folded
      // so it bounces across the band range instead of jumping at wrap
      var band = triangle((wx + wy) / 2 + t2) * (BANDS - 1)
      var i0 = floor(band)
      var f = band - i0
      var i1 = min(i0 + 1, BANDS - 1)

      var level = (frequencyData[i0] * (1 - f) + frequencyData[i1] * f) * sensitivity
      var base = averages[i0] * (1 - f) + averages[i1] * f

      // only above-baseline energy shows; dominant bands bloom harder
      var br = (level - base) * (0.5 + min(base * 10, 10))
      br = clamp(br, 0, 2)
      br = br * br  // square for contrast/punch

      var k = y * W + x
      vals[k] = vals[k] * trailDecay + br
      hues[k] = t1 + (band / (BANDS - 1)) * hueArc
      fillAccum += clamp(vals[k], 0, 1)
    }
  }
}

export function render2D(index, x, y) {
  var k = floor(y * 15.99) * W + floor(x * 15.99)
  var v = clamp(vals[k], 0, 1)
  hsv(hues[k], 1 - v, v)  // hottest peaks whiten
}
