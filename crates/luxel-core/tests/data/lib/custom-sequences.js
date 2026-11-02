// name: Custom Sequences
// Clean-room reimplementation from a prose functional description of the
// community pattern "Custom Sequences"; original source never consulted.

// A configurable repeating color sequence for spaced bulbs / eaves: up to
// 12 picked colors shown in equal-length runs repeating down the strip,
// chasing in either direction (or holding still), with optional smooth
// cross-blending, per-run trailing fade, white / wink-out twinkles riding
// along with the chase, and a periodic whole-strip blink.
// Default look: alternating runs of red and blue.

var MAXC = 12

// sequence colors as RGB (filled from the HSV pickers)
var colR = array(MAXC)
var colG = array(MAXC)
var colB = array(MAXC)
var cvt = array(3)

function setSeq(i, h, s, v) {
  hsv2rgb(h, s, v, cvt)
  colR[i] = cvt[0]; colG[i] = cvt[1]; colB[i] = cvt[2]
}

// defaults: red / blue alternating
for (i = 0; i < MAXC; i++) setSeq(i, i % 2 == 0 ? 0 : 0.6667, 1, 1)

export function hsvPickerColor1(h, s, v)  { setSeq(0, h, s, v) }
export function hsvPickerColor2(h, s, v)  { setSeq(1, h, s, v) }
export function hsvPickerColor3(h, s, v)  { setSeq(2, h, s, v) }
export function hsvPickerColor4(h, s, v)  { setSeq(3, h, s, v) }
export function hsvPickerColor5(h, s, v)  { setSeq(4, h, s, v) }
export function hsvPickerColor6(h, s, v)  { setSeq(5, h, s, v) }
export function hsvPickerColor7(h, s, v)  { setSeq(6, h, s, v) }
export function hsvPickerColor8(h, s, v)  { setSeq(7, h, s, v) }
export function hsvPickerColor9(h, s, v)  { setSeq(8, h, s, v) }
export function hsvPickerColor10(h, s, v) { setSeq(9, h, s, v) }
export function hsvPickerColor11(h, s, v) { setSeq(10, h, s, v) }
export function hsvPickerColor12(h, s, v) { setSeq(11, h, s, v) }

var numColors = 2
//# min=0 max=1 step=0.09 default=0.09
export function sliderNumberOfColorsUsed(v) {
  numColors = clamp(floor(1 + v * 11.001), 1, MAXC)
}

// pixels per run (cubic response: fine control at short lengths)
var runLen = 5
//# min=0 max=1 step=0.01 default=0.4
export function sliderColorLength(v) {
  runLen = max(1, floor(1 + pow(v, 3) * pixelCount * 3))
}

// bidirectional with a wide center dead zone; ~fifth-power response
var chase = 0.05
//# min=0 max=1 step=0.01 default=0.56
export function sliderChaseSpeed(v) {
  var u = v * 2 - 1
  if (abs(u) < 0.15) {
    chase = 0
  } else {
    var m = (abs(u) - 0.15) / 0.85
    chase = sign(u) * pow(m, 5)
  }
}

var fadeAmt = 0
//# min=0 max=1 step=0.01 default=0
export function sliderFadeOut(v) { fadeAmt = v * v }

var smoothing = 0
//# min=0 max=1 step=0.01 default=0
export function sliderColorSmoothing(v) { smoothing = v }

var twWhite = 0
//# min=0 max=1 step=0.01 default=0
export function sliderTwinkleWhite(v) { twWhite = v }

var twOff = 0
//# min=0 max=1 step=0.01 default=0
export function sliderTwinkleOff(v) { twOff = sqrt(sqrt(v)) }  // aggressive

var blinkAmt = 0
//# min=0 max=1 step=0.01 default=0
export function sliderBlink(v) { blinkAmt = v }

// stable per-pixel random phases, rolled once at startup
var rnd = array(pixelCount)
for (i = 0; i < pixelCount; i++) rnd[i] = random(1)

// frame state
var phase = 0        // master animation phase, unit interval
var offset = 0       // signed pixel offset derived from phase
var blinkGate = 1
var twPhase = 0, toPhase = 0

export function beforeRender(delta) {
  // one full phase wrap slides the pattern by exactly one full sequence
  phase = mod(phase + delta * chase / 2000, 1)
  offset = phase * runLen * numColors

  // twinkle clocks: slider right = shorter cycle; cycle scales with strip
  // length so twinkle density per strip feels constant
  var scale = pixelCount / 50
  twPhase = time(mix(30, 3, twWhite) * scale / 65.536)
  toPhase = time(mix(40, 4, twOff) * scale / 65.536)

  // periodic whole-strip blink: brief fast strobe once per period
  blinkGate = 1
  if (blinkAmt > 0) {
    var bp = time(mix(60, 2, blinkAmt) / 65.536)
    if (bp < 0.06) blinkGate = square(time(0.0008), 0.5)
  }
}

export function render(index) {
  var pos = index + offset
  // floored, sign-correct modulo throughout or reverse chase breaks
  var slot = mod(floor(pos / runLen), numColors)
  var nxt = mod(slot + 1, numColors)
  var frac = mod(pos, runLen) / runLen   // position within the run

  var r = colR[slot], g = colG[slot], b = colB[slot]

  // per-run trailing fade, leading side picked by chase direction
  var fade = 1
  if (fadeAmt > 0) {
    var saw = chase >= 0 ? frac : 1 - frac
    fade = clamp(1 - fadeAmt * saw, 0, 1)
    fade = fade * fade * fade            // cubed for a pleasing tail
  }

  // cross-blend toward the next color
  if (smoothing > 0) {
    // skip blending around black entries when fading, or the fade and the
    // blend-to-black double up
    var curMax = max(r, max(g, b))
    var nxtMax = max(colR[nxt], max(colG[nxt], colB[nxt]))
    if (!(fadeAmt > 0 && (curMax < 0.05 || nxtMax < 0.05))) {
      var e = mix(0.08, 1, smoothing)    // step ... fully linear
      var bf
      if (fadeAmt > 0) {
        bf = pow(frac, 1 / e)            // one-sided ease-in under fade
      } else {
        var u = frac * 2 - 1             // signed curve mirrored at midpoint
        bf = 0.5 + 0.5 * sign(u) * pow(abs(u), e)
      }
      r = mix(r, colR[nxt], bf)
      g = mix(g, colG[nxt], bf)
      b = mix(b, colB[nxt], bf)
    }
  }

  // twinkles only touch reasonably bright pixels; they index the random
  // table offset by the chase so they ride along with the pattern
  var mult = 1
  var boost = 0
  if (max(r, max(g, b)) * fade > 0.25) {
    if (twWhite > 0) {
      var p1 = mod(twPhase + rnd[floor(mod(index + offset, pixelCount))], 1)
      var spike = max(0, 1 - abs(p1 - 0.5) * 25)   // brief spike per cycle
      spike = spike * spike * (3 - 2 * spike)      // smooth it
      var m = 0.85 + spike * 1.6                   // rests just below full
      boost = max(0, m - 1) * 0.7                  // overshoot -> add white
      mult = min(m, 1)
    }
    if (twOff > 0) {
      var i2 = floor(mod(index + offset + pixelCount / 2, pixelCount))
      var p2 = mod(toPhase + rnd[i2], 1)
      var dip = max(0, 1 - abs(p2 - 0.5) * 6)      // longer, deeper pulse
      mult = mult * (1 - dip)                      // wink toward zero
    }
  }

  var k = fade * mult * blinkGate
  rgb(r * k + boost, g * k + boost, b * k + boost)
}
