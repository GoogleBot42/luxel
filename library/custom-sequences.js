// name: Custom Sequences
// Clean-room reimplementation from a prose functional description of the
// community pattern "Custom Sequences"; original source never consulted.

// A repeating user-defined color sequence for spaced bulbs/eaves: up to 12
// picked colors in equal-length runs, chasing either direction, with
// optional cross-blending, per-run trailing fade, white/off twinkles, and a
// periodic whole-strip blink. Defaults: alternating red and blue.

// sequence colors (HSV pickers) and their per-frame RGB
var colH = array(12), colS = array(12), colV = array(12)
var colR = array(12), colG = array(12), colB = array(12)
var i
for (i = 0; i < 12; i++) { colH[i] = i / 12; colS[i] = 1; colV[i] = 1 }
colH[0] = 0        // Color 1: red
colH[1] = 0.6667   // Color 2: blue

export function hsvPickerColor1(h, s, v) { colH[0] = h; colS[0] = s; colV[0] = v }
export function hsvPickerColor2(h, s, v) { colH[1] = h; colS[1] = s; colV[1] = v }
export function hsvPickerColor3(h, s, v) { colH[2] = h; colS[2] = s; colV[2] = v }
export function hsvPickerColor4(h, s, v) { colH[3] = h; colS[3] = s; colV[3] = v }
export function hsvPickerColor5(h, s, v) { colH[4] = h; colS[4] = s; colV[4] = v }
export function hsvPickerColor6(h, s, v) { colH[5] = h; colS[5] = s; colV[5] = v }
export function hsvPickerColor7(h, s, v) { colH[6] = h; colS[6] = s; colV[6] = v }
export function hsvPickerColor8(h, s, v) { colH[7] = h; colS[7] = s; colV[7] = v }
export function hsvPickerColor9(h, s, v) { colH[8] = h; colS[8] = s; colV[8] = v }
export function hsvPickerColor10(h, s, v) { colH[9] = h; colS[9] = s; colV[9] = v }
export function hsvPickerColor11(h, s, v) { colH[10] = h; colS[10] = s; colV[10] = v }
export function hsvPickerColor12(h, s, v) { colH[11] = h; colS[11] = s; colV[11] = v }

// one stable random number per pixel, generated once at startup:
// stable phase offsets for the twinkle effects
var rnd = array(pixelCount)
for (i = 0; i < pixelCount; i++) rnd[i] = random(1)

var numColors = 2
//# min=0 max=1 step=0.01 default=0.12
export function sliderNumberOfColorsUsed(v) { numColors = floor(1 + v * 11.99) }

var runLen = 3
//# min=0 max=1 step=0.01 default=0.3
export function sliderColorLength(v) {
  // cubic response: fine control at short lengths, up to ~3x strip length
  runLen = floor(1 + pow(v, 3) * pixelCount * 3)
}

var chase = 0.012
//# min=0 max=1 step=0.01 default=0.75
export function sliderChaseSpeed(v) {
  // bidirectional, wide center dead zone, ~fifth-power response
  var c = v * 2 - 1
  if (abs(c) < 0.15) chase = 0
  else chase = sign(c) * pow((abs(c) - 0.15) / 0.85, 5)
}

var fadeAmt = 0
//# min=0 max=1 step=0.01 default=0
export function sliderFadeOut(v) { fadeAmt = v * v }

var smoothing = 0
//# min=0 max=1 step=0.01 default=0
export function sliderColorSmoothing(v) { smoothing = v }

var twW = 0
//# min=0 max=1 step=0.01 default=0
export function sliderTwinkleWhite(v) { twW = v }

var twO = 0
//# min=0 max=1 step=0.01 default=0
export function sliderTwinkleOff(v) { twO = sqrt(sqrt(v)) }  // aggressive response

var blinkAmt = 0
//# min=0 max=1 step=0.01 default=0
export function sliderBlink(v) { blinkAmt = v }

var phase = 0
var offset = 0
var blinkGate = 1
var twTime = 0, twOffTime = 0
var rgbTmp = array(3)

export function beforeRender(delta) {
  // master phase: one full wrap slides the pattern one full sequence
  phase = mod(phase + delta * chase / 3000, 1)
  offset = phase * runLen * numColors

  // refresh RGB for the active sequence colors
  for (var k = 0; k < numColors; k++) {
    hsv2rgb(colH[k], colS[k], colV[k], rgbTmp)
    colR[k] = rgbTmp[0]; colG[k] = rgbTmp[1]; colB[k] = rgbTmp[2]
  }

  // twinkle cycles: slider shortens them; density scales with strip length
  var pxScale = 0.5 + pixelCount / 512
  twTime = time((0.05 + (1 - twW) * 0.4) * pxScale)
  twOffTime = time((0.08 + (1 - twO) * 0.5) * pxScale)

  // periodic whole-strip blink: brief fast strobe once per period
  blinkGate = 1
  if (blinkAmt > 0) {
    var bp = time(0.9 - blinkAmt * 0.87)   // ~1 min down to ~2 s
    if (bp < 0.05) blinkGate = square(bp * 80, 0.5)
  }
}

export function render(index) {
  var p = index + offset
  var slot = floor(p / runLen)
  var t = p / runLen - slot            // fractional position within the run
  var ci = mod(slot, numColors)        // floored modulo: sign-correct
  var ni = mod(slot + 1, numColors)

  var r = colR[ci], g = colG[ci], b = colB[ci]

  // cross-blend toward the next color
  if (smoothing > 0.01) {
    // when fading, skip blends involving black so gaps stay crisp
    var skip = fadeAmt > 0 && (colV[ci] < 0.02 || colV[ni] < 0.02)
    if (!skip) {
      var e
      if (fadeAmt > 0) {
        // one-sided ease-in so the blend doesn't fight the fade
        e = pow(t, 1 / max(smoothing, 0.08))
      } else {
        // signed odd-power ease mirrored about the midpoint:
        // near-step at low smoothing, fully linear at 1
        var u = 2 * t - 1
        e = 0.5 + 0.5 * sign(u) * pow(abs(u), max(smoothing, 0.08))
      }
      r += (colR[ni] - r) * e
      g += (colG[ni] - g) * e
      b += (colB[ni] - b) * e
    }
  }

  // per-run trailing fade, leading side picked by chase direction
  var fade = 1
  if (fadeAmt > 0) {
    var fp = chase >= 0 ? 1 - t : t
    fade = 1 - fadeAmt * (1 - fp * fp * fp)   // cubed tail
  }

  var bright = max(r, max(g, b)) * fade
  var boost = 0
  var offMult = 1

  // twinkles ride along with the chase (table indexed by index + offset)
  // and only bright pixels twinkle -- gaps stay clean
  if (twW > 0 && bright > 0.2) {
    var ri = floor(mod(index + offset, pixelCount))
    var tp = mod(rnd[ri] + twTime, 1)
    if (tp < 0.08) {
      // brief smooth spike; overshoot past full becomes an additive
      // white boost, so the peak both brightens and whitens
      var sp = pow(sin(tp / 0.08 * PI), 2) * 1.8
      boost = max(0, sp - 1) * 0.9
    }
  }

  // wink-outs: second, decorrelated table index (offset ~half the strip)
  if (twO > 0 && bright > 0.2) {
    var ri2 = floor(mod(index + offset + pixelCount / 2, pixelCount))
    var op = mod(rnd[ri2] + twOffTime, 1)
    if (op < 0.18) offMult = 1 - pow(sin(op / 0.18 * PI), 2)
  }

  var m = fade * offMult * blinkGate
  rgb(r * m + boost, g * m + boost, b * m + boost)
}
