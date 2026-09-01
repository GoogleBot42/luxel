// name: Scary Pumpkin
// Clean-room reimplementation from a prose functional description of the
// community pattern "Scary Pumpkin"; original source never consulted.

// A jack-o'-lantern on a 2D matrix: a round pumpkin body filled with a
// swirling fiery tunnel texture, with green triangular eyes, a nose, and a
// crescent smile that flash erratically. Black outside the pumpkin.

// Re-center the panel so (0,0) is the middle (applied once at startup;
// transforms persist across frames).
translate(-0.5, -0.5)

// Animation speed of the fire texture (also driven by sliderFireSpeed below).
export var speed = 0.1

var tb = 0        // time base, seconds
var s1 = 0        // noise scroll offsets
var s2 = 0
var flash = 0     // face brightness this frame

var pumpkinR = 0.48
var flicker = 1   // face-flash rate multiplier
var faceH = 0.33  // face hue (green)

// Point-up wedge (triangle) inside-test: apex at (cx, topY), height h,
// half-width w at the base. y grows downward.
function inTri(px, py, cx, topY, h, w) {
  var dy = py - topY
  return dy >= 0 && dy <= h && abs(px - cx) <= w * dy / h
}

// Crescent smile: a disc with a second disc (shifted up) cut out of it,
// so the crescent opens upward.
function inSmile(px, py) {
  return hypot(px, py - 0.18) < 0.26 && hypot(px, py - 0.06) > 0.26
}

export function beforeRender(delta) {
  tb += delta / 1000
  if (tb > 3600) tb -= 3600   // wrap after ~1 h to protect precision

  // Two scroll offsets; the second at half rate so the fire doesn't read
  // as a rigid translation.
  s1 = tb * speed
  s2 = tb * speed * 0.5

  // Deterministic chaotic flicker: tangent of a cosine of (time + a fast
  // sine wiggle). No random calls — smooth in time, erratic in cadence.
  // The face flash rate is independent of the fire speed.
  var ft = tb * flicker
  flash = clamp(tan(cos(ft * 2.6 + sin(ft * 9.3) * 0.7)), 0, 1)
}

// How fast the fire texture inside the pumpkin scrolls and churns, in scroll
// units per second. 0 freezes it into a still ember pattern.
//# min=0 max=0.5 step=0.005 default=0.1
export function sliderFireSpeed(v) {
  speed = clamp(v, 0, 0.5)
}

// Rate of the erratic face flicker: 0 holds the face at whatever level it
// froze on, 1 is the natural cadence, 4 is a frantic strobe.
//# min=0 max=4 step=0.05 default=1
export function sliderFlickerRate(v) {
  flicker = clamp(v, 0, 4)
}

// Color of the eyes, nose and smile, as a position on the color wheel
// (0.33 = the classic green glow, 0.08 = candle orange, 0 = blood red).
//# min=0 max=1 step=0.01 default=0.33
export function sliderFaceHue(v) {
  faceH = clamp(v, 0, 1)
}

// Radius of the pumpkin as a fraction of the panel's short side; 0.5 fills the
// panel edge to edge, smaller values leave a black border.
//# min=0.15 max=0.7 step=0.01 default=0.48
export function sliderPumpkinSize(v) {
  pumpkinR = clamp(v, 0.15, 0.7)
}

export function render2D(index, x, y) {
  var r = hypot(x, y)

  // Cheap bounding test: everything outside the pumpkin is black.
  if (r > pumpkinR) {
    rgb(0, 0, 0)
    return
  }

  // Face mask: two eyes, a nose, and the smile.
  var face = inTri(x, y, -0.17, -0.27, 0.16, 0.11) ||
             inTri(x, y,  0.17, -0.27, 0.16, 0.11) ||
             inTri(x, y,  0,    -0.05, 0.12, 0.08) ||
             inSmile(x, y)

  if (face) {
    // Vivid green, whole face flashing in unison.
    hsv(faceH, 1, flash)
    return
  }

  // Pumpkin body: tunnel warp (rotation inversely proportional to radius,
  // so the texture spirals inward), then fractal noise.
  var ang = atan2(y, x) + 0.18 / max(r, 0.03)
  var wx = r * cos(ang)
  var wy = r * sin(ang)

  var n = perlinFbm(wx * 3 + s1, wy * 3 + s2, tb * speed * 1.5, 2, 0.8, 3)

  // Brightness: |noise| scaled down, with a small floor so embers never
  // go fully black.
  var v = abs(n) * 0.45 + 0.06

  // Fire palette from one scalar: dim = deep red-orange, bright = toward
  // yellow-orange; saturation eases off slightly as brightness rises.
  hsv(0.015 + v * 0.12, 1 - v * 0.25, v)
}
