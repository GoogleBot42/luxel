// name: Butterfly 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Butterfly 2D"; original source never consulted.

// A single procedurally generated butterfly centered on the matrix: wings
// as a polar signed-distance field (a sum of angular harmonics) deformed by
// seeded perlin noise, textured by a second perlin lookup quantized into
// golden-ratio-spaced hue bands. It rests, flaps faster and faster, then
// drifts up and off-screen; a brand-new butterfly (shape, texture, colors,
// tilt) is born each cycle.

var GOLDEN = 0.61803399    // golden-ratio conjugate hue step
var FMAX = 3.0             // peak flap frequency (beats/sec)
var R0 = 0.24              // base wing radius

// state kept across frames
var elapsed = 0            // accumulated time base
var flapPhase = 0          // integrates flap frequency
var flapScale = 1
var flapFreq = 0
var shapeSeedA = 0, shapeSeedB = 0
var colorSeed = 0
var baseHue = 0
var tilt = 0
var flyOffset = 0

function reseed() {
  shapeSeedA = random(64)
  shapeSeedB = random(64)
  colorSeed = random(64)
  flyOffset = 0
  tilt = (random(1) - 0.5) * (PI / 1.5)   // +/- ~1/3 of a half-turn
  baseHue = frac(baseHue + GOLDEN)
}
reseed()

export function beforeRender(delta) {
  var dt = delta / 1000
  elapsed = mod(elapsed + dt, 3600)

  // Slow sine sweep of the flap frequency: still -> flutter -> still,
  // period ~5 s.
  var sweep = (sin(time(0.076) * PI2) * 0.5 + 0.5)
  flapFreq = sweep * FMAX

  // Flap oscillator at the current frequency; scale between well under 1
  // and roughly double (anisotropic horizontal squash/stretch = flapping).
  flapPhase = flapPhase + flapFreq * dt
  flapScale = 0.4 + wave(flapPhase) * 1.6

  if (flapFreq < 0.03) {
    reseed()                         // reborn at the still moment
  } else if (flapFreq > FMAX * 0.9) {
    flyOffset = flyOffset + dt * 0.15   // slide off the top near peak flutter
  }

  resetTransform()
  translate(-0.5, -0.5)              // origin at display center
  rotate(tilt)
}

function wingSDF(hx, vy) {
  // polar coordinates
  var ang = atan2(vy, hx)
  var r = sqrt(hx * hx + vy * vy)

  // boundary radius: base + weighted angular harmonics (low odd/even + a
  // high-frequency ripple) -> scalloped, lobed outline
  var b = R0
        + 0.060 * sin(ang)
        + 0.050 * cos(ang * 2)
        + 0.040 * sin(ang * 3)
        + 0.030 * cos(ang * 4)
        + 0.025 * sin(ang * 5)
        + 0.020 * cos(ang * 6)
        + 0.018 * sin(ang * 13)     // high-frequency ripple

  // seeded perlin deforms the outline uniquely per butterfly
  b = b + 0.06 * perlin(cos(ang) + shapeSeedA, sin(ang) + shapeSeedB, 0, 0)

  var field = b - r                 // >0 inside
  return smoothstep(-0.04, 0.04, field)   // antialiased edge band
}

export function render2D(index, x, y) {
  // Mirror horizontal about the body axis; apply fly-away to vertical.
  var hx = abs(x)
  var fy = y + flyOffset

  // Fixed shaping: flip + stretch vertical; mild vertical-dependent pinch.
  var vy = -fy * 1.3
  var hxs = hx * (0.9 + 0.1 * sin(vy))

  // Flap by anisotropic horizontal scaling.
  var hf = hxs * flapScale

  var sil = wingSDF(hf, vy)

  // Wing texture: zoomed perlin, horizontal also beats with the flap.
  var tex = perlin(hf * 6 + colorSeed, vy * 6, colorSeed, 0)
  tex = clamp(tex * 0.5 + 0.5, 0, 1)

  // Quantize into 5 bands -> up to 5 golden-spaced complementary hues.
  var band = floor(tex * 5)
  var hue = frac(baseHue + band * GOLDEN)

  var sat = clamp(1.15 - tex, 0, 1)          // densest texture -> white sparkle
  var bri = sil * max(tex * tex * tex, 0.06) // cubing carves dark veins

  hsv(hue, sat, clamp(bri, 0, 1))
}
