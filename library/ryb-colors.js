// name: RYB colors
// Clean-room reimplementation from a prose functional description of the
// community pattern "RYB colors"; original source never consulted.

// A slowly rotating color wheel. Wheel-type slider picks the painter's
// RYB (red-yellow-blue) wheel or the standard HSV wheel; brightness and
// saturation behave HSL-style in RYB mode.

var wheelType = 1     // 0 = HSV wheel, 1 = RYB painter's wheel
var brightness = 1
var saturation = 1

// wheel selector in real units: 0 = HSV, 1 = RYB, one step per mode, so the
// slider snaps between the two wheel types instead of sliding continuously
//# min=0 max=1 step=1 default=1
export function sliderWheelType(v) {
  wheelType = clamp(round(v), 0, 1)
}

//# min=0 max=1 step=0.01 default=1
export function sliderBrightness(v) {
  brightness = v
}

//# min=0 max=1 step=0.01 default=1
export function sliderSaturation(v) {
  saturation = v
}

export function beforeRender(delta) {
  resetTransform()
  translate(-0.5, -0.5)             // wheel spins about the display center
  rotate(time(0.15) * PI2)          // ~10 s per revolution
}

// One channel of the classic HSL hue sextant conversion
function hue2chan(p, q, t) {
  t = mod(t, 1)
  if (t < 1 / 6) return p + (q - p) * 6 * t
  if (t < 1 / 2) return q
  if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6
  return p
}

// smoothstep-style cubic ease used on each RYB cube axis
function ease(t) {
  return t * t * (3 - 2 * t)
}

// eased trilinear weights, shared by the three channel blends
var wr, wy, wb, ir, iy, ib

// Blend one output channel over the 8 corners of the RYB cube.
// Corner colors (r,y,b axes): 000 white, 100 red, 010 yellow, 001 blue,
// 110 orange, 011 green, 101 purple, 111 black.
function rybBlend(c000, c100, c010, c110, c001, c101, c011, c111) {
  return ir * iy * ib * c000 + wr * iy * ib * c100 +
         ir * wy * ib * c010 + wr * wy * ib * c110 +
         ir * iy * wb * c001 + wr * iy * wb * c101 +
         ir * wy * wb * c011 + wr * wy * wb * c111
}

function paintWheel(h) {
  h = mod(h, 1)
  if (wheelType < 0.5) {
    // plain HSV wheel
    hsv(h, saturation, brightness)
    return
  }

  // RYB wheel: hue -> HSL-semantics sextant conversion. The channels are
  // *paint* amounts (more paint = darker), so the brightness slider maps
  // inverted onto the sextant's lightness: bottom = all paint = black,
  // top = pure-hue paint (l = 0.5), matching HSL-lightness behavior.
  var l = 1 - brightness / 2
  var s = saturation
  var q = l < 0.5 ? l * (1 + s) : l + s - l * s
  var p = 2 * l - q
  var cr = hue2chan(p, q, h + 1 / 3)
  var cy = hue2chan(p, q, h)
  var cb = hue2chan(p, q, h - 1 / 3)

  // ...per-channel squaring (LED-friendly shaping)...
  cr = cr * cr
  cy = cy * cy
  cb = cb * cb

  // ...then reinterpret the channels as R/Y/B paint amounts and map to RGB
  // by cubic-eased trilinear interpolation over the RYB reference cube.
  wr = ease(cr); ir = 1 - wr
  wy = ease(cy); iy = 1 - wy
  wb = ease(cb); ib = 1 - wb

  var outR = rybBlend(1, 1, 1, 1, 0, 0.5, 0, 0)
  var outG = rybBlend(1, 0, 1, 0.5, 0, 0, 1, 0)
  var outB = rybBlend(1, 0, 0, 0, 1, 1, 0, 0)
  rgb(outR, outG, outB)
}

export function render2D(index, x, y) {
  paintWheel(atan2(y, x) / PI2)     // radius ignored: constant along rays
}

// 1D adaptation: normalized strip position is the angle
export function render(index) {
  paintWheel(index / pixelCount + time(0.15))
}
