// name: GlowFlow (3D coord transform API port)
// Clean-room reimplementation from a prose functional description of the
// community pattern "GlowFlow (3D coord transform API port)"; original
// source never consulted.

// A container half-full of glowing rainbow liquid. The liquid surface
// stays level with the real world: the smoothed accelerometer gravity
// vector drives a per-frame coordinate rotation, so the lower half of the
// (transformed) volume shows saturated rainbow strata by depth while a
// narrow red-orange horizon glow hugs the surface from above. Sound makes
// scattered pixels fizz brighter and slightly whiter, with a PI
// controller auto-adapting microphone sensitivity. With zero sensor input
// (engine stubs) it renders a static level liquid — no sparks, no tilt.
//
// Behavior switches (source constants, per the original):
var SOUND_ENABLED = 1      // carbonation fizz from sound energy
var LIGHT_ENABLED = 0      // ambient-light brightness dimming
var CORNER_STAND = 0       // extra fixed rotations for a corner-standing cube
var SMOOTHING = 0.12       // gravity low-pass blend per frame (0..1)

// Sensor bindings (engine stubs these with zeros when absent).
export var accelerometer = array(3)
export var energyAverage = 0
export var light = 0

// Axis remap for the physical build: which accelerometer axis is "down".
// Build-specific in the original; adjust signs/order for your fixture.
var gx = 0, gy = 0, gz = 0          // smoothed gravity vector

var sparks = array(pixelCount)      // per-pixel fizz values
var sparkTotal = 0                  // accumulated by render3D
var avgSpark = 0                    // last frame's average, read by the PI
var integ = 0                       // PI integral state
var gain = 1                        // mic sensitivity
var TARGET = 0.01                   // want ~1% of brightness from sparks
var bright = 1                      // ambient-light multiplier

export function beforeRender(delta) {
  var dt = delta / 1000

  // 1. Fold accelerometer into the smoothed gravity vector (IIR low-pass,
  // weighted mostly toward the previous value).
  gx += (accelerometer[0] - gx) * SMOOTHING
  gy += (accelerometer[1] - gy) * SMOOTHING
  gz += (accelerometer[2] - gz) * SMOOTHING

  // 2. Spherical angles, guarded against the degenerate zero/axis cases.
  var azimuth = 0
  var polar = 0
  var horiz = hypot(gx, gy)
  if (hypot(horiz, gz) > 0.05) {
    if (horiz > 0.001) azimuth = atan2(gy, gx)
    polar = atan2(horiz, -gz)       // tilt away from straight down
  }

  // 3. Frame transform: center the unit cube, optional corner-stand
  // rotations, then align the renderer's vertical with real gravity.
  resetTransform()
  translate3D(-0.5, -0.5, -0.5)
  if (CORNER_STAND) {
    rotateZ(PI / 4)
    rotateY(-0.6155)                // atan(1/sqrt(2)): cube onto its corner
  }
  rotateZ(-azimuth)
  rotateY(-polar)

  // 4. Sound: PI controller trims sensitivity toward the target fill.
  if (SOUND_ENABLED) {
    avgSpark = sparkTotal / pixelCount
    var err = TARGET - avgSpark
    integ = clamp(integ + err * dt * 8, 0, 40)
    gain = max(0.3, err * 20 + integ)
    var i = 0
    var burn = dt * 0.4 + energyAverage * gain * dt * 3
    for (i = 0; i < pixelCount; i++) {
      var s = sparks[i] - burn
      if (s <= 0) s = random(clamp(energyAverage * gain * 3, 0, 1))
      sparks[i] = s
    }
  }
  sparkTotal = 0

  // 5. Ambient light dimming (clamped away from zero) if enabled.
  bright = LIGHT_ENABLED ? clamp(light, 0.05, 1) : 1
}

export function render3D(index, x, y, z) {
  // Fizz: scale up and square so only strong sparks show; feed the PI.
  var sp = clamp(sparks[index] * 3, 0, 1)
  sp = sp * sp
  sparkTotal += sp

  var h, v
  if (z < 0) {
    // Inside the liquid: hue by depth, clamped short of wrapping.
    h = min(0.88, -z * 1.7)
    v = 1
  } else {
    // Above the surface: red-orange horizon glow, cubed falloff.
    h = 0.03
    v = saturate(1 - z * 2.5)
    v = v * v * v
  }
  var s = 1 - min(0.4, sp)          // fizzing pixels whiten slightly
  hsv(h, s, min(1, v + sp) * bright)
}

// 2D shim (the original is 3D-only): treat y as the vertical axis so a
// flat matrix shows a level cross-section of the liquid.
export function render2D(index, x, y) {
  render3D(index, x, 0, y)
}
