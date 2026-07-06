// name: sound - rays
// Clean-room reimplementation from a prose functional description of the
// community pattern "sound - rays"; original source never consulted.

// A scrolling "chart recorder" of the room's sound: a write head crawls along
// the strip stamping brightness (dominant-tone loudness) and hue (dominant
// pitch) into circular buffers; the whole trail is drawn offset by the head
// position so history streams steadily along the strip, opposite the native
// index direction. A PI auto-gain loop keeps the recorded brightness hovering
// near mid-scale, so it self-calibrates to quiet or loud rooms.

// Sound sensor bindings (engine stubs them with zeros when absent)
export var energyAverage           // declared for the sensor hookup; unused
export var maxFrequency            // Hz of the loudest frequency bin
export var maxFrequencyMagnitude   // magnitude of that bin

var vals = array(pixelCount)   // recorded brightness history (circular)
var hues = array(pixelCount)   // recorded pitch-hue history (circular)
var pos = 0                    // fractional write-head position
var lastVal = 0                // last brightness written (feeds the AGC)

// PI auto-gain: chases "recorded brightness ~ mid-scale". Boots with a high
// integral bias (very sensitive), then settles over a few seconds.
var integral = 150
var gain = integral
var hueT = 0

export function beforeRender(delta) {
  // PI controller: error = mid-scale minus last written value
  var err = 0.5 - lastVal
  integral = clamp(integral + err * delta * 0.02, 0, 400)  // wide, non-negative
  gain = max(0, err * 20 + integral)

  // slow global hue rotation, one lap in ~5 s
  hueT = time(0.08)

  // write head crawls ~30 px/s, wrapping at pixelCount
  pos = mod(pos + delta * 0.03, pixelCount)

  // stamp the current dominant tone at the head (truncate the fractional
  // head position when indexing)
  var head = floor(pos)
  var v = clamp(maxFrequencyMagnitude * gain, 0, 1)
  lastVal = v * v                       // recorded squared: crushes quiet to black
  vals[head] = lastVal
  hues[head] = clamp(maxFrequency / 3000, 0, 1)  // pitch vs a few-kHz ceiling
}

export function render(index) {
  // reversed index + head offset picks the buffer slot; moving the read
  // offset (not the data) is what makes the history scroll
  var slot = floor(mod(pixelCount - 1 - index + pos, pixelCount))
  var v = vals[slot]
  // squared again at render time: a fourth-power curve overall, so only real
  // peaks glow; all-zero (silent/stubbed) input idles dark
  hsv(hues[slot] + index / pixelCount * 0.25 + hueT, 1, v * v)
}
