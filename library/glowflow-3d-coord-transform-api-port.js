// name: GlowFlow (3D coord transform API port)
// Clean-room reimplementation from a prose functional description of the
// community pattern "GlowFlow (3D coord transform API port)"; original
// source never consulted.

// A container half-full of glowing rainbow liquid whose surface stays
// level with the real world. A smoothed accelerometer gravity vector
// drives a per-frame coordinate rotation aligning the renderer's vertical
// with true gravity: below the mid-plane the volume shows saturated
// rainbow strata by depth; just above it, a reddish-orange horizon glow
// fades sharply to black with height. Sound makes scattered pixels fizz
// briefly brighter and slightly whiter, with a PI controller auto-adapting
// microphone sensitivity to the room. With all-zero sensor input (engine
// stubs) it renders a calm, level liquid: no sparks, no tilt.
//
// The original's four source-constant switches are promoted to controls.

// sensor bindings (engine stubs these with zeros when hardware is absent)
export var accelerometer = array(3)
export var energyAverage = 0
export var light = 0

var soundOn = 1        // carbonation fizz from sound energy (default on)
export function toggleSoundReactive(v) { soundOn = v }

var lightDim = 0       // ambient-light brightness dimming (default off)
export function toggleAmbientDimming(v) { lightDim = v }

var cornerStand = 0    // fixed rotations for a cube stood on its corner
export function toggleCornerStand(v) { cornerStand = v }

var smoothing = 0.12   // gravity low-pass blend weight per frame
//# min=0 max=1 step=0.01 default=0.35
export function sliderTiltSmoothing(v) { smoothing = 0.02 + (1 - v) * 0.28 }

// NOTE: the accelerometer axis remap/sign-flip is build-specific in the
// original; adjust the order/signs of these three reads for your fixture.
var gx = 0, gy = 0, gz = 0    // smoothed gravity vector

var sparks = array(pixelCount)   // per-pixel fizz values
var sparkTotal = 0               // accumulated by the renderer each frame
var integ = 0                    // PI controller integral state
var gain = 1                     // auto mic sensitivity
var TARGET = 0.01                // aim: ~1% of brightness from sparks
var bright = 1                   // ambient-light multiplier

export function beforeRender(delta) {
  var dt = delta / 1000

  // fold each sample into the smoothed gravity vector (first-order IIR,
  // weighted mostly toward the previous value)
  gx += (accelerometer[0] - gx) * smoothing
  gy += (accelerometer[1] - gy) * smoothing
  gz += (accelerometer[2] - gz) * smoothing

  // spherical angles, with guards for the degenerate axis-aligned cases
  var azimuth = 0
  var polar = 0
  var horiz = hypot(gx, gy)
  if (hypot(horiz, gz) > 0.05) {          // no signal -> stay level
    if (horiz > 0.001) azimuth = atan2(gy, gx) + PI / 2
    polar = atan2(horiz, -gz)             // tilt away from straight down
  }

  // frame transform: center the unit cube, optional corner-stand
  // rotations, then align the render vertical with real-world gravity
  resetTransform()
  translate3D(-0.5, -0.5, -0.5)
  if (cornerStand) {
    rotateZ(PI / 4)
    rotateY(0.9553)                       // atan(sqrt(2)): onto the corner
  }
  rotateZ(azimuth)
  rotateY(-polar)

  // sound fizz: PI controller trims sensitivity so sparks contribute
  // roughly the target fraction of overall brightness
  if (soundOn) {
    var avg = sparkTotal / max(1, pixelCount)
    var err = TARGET - avg
    integ = clamp(integ + err * dt * 6, 0, 30)
    gain = max(0.25, err * 15 + integ)
    // decay every spark (time leak + loudness burn); re-seed dead ones
    var leak = dt * 0.5
    var burn = energyAverage * gain * dt * 4
    var seedMax = clamp(energyAverage * gain * 2, 0, 1)
    for (var i = 0; i < pixelCount; i++) {
      var s = sparks[i] - leak - burn
      if (s <= 0) s = random(seedMax)     // 0 when the room is silent
      sparks[i] = s
    }
  }
  sparkTotal = 0

  // ambient-light dimming, clamped away from zero, if enabled
  bright = lightDim ? clamp(light, 0.05, 1) : 1
}

export function render3D(index, x, y, z) {
  // fizz: scale up and square so only strong sparks show; feed the PI
  var sp = clamp(sparks[index] * 3, 0, 1)
  sp = sp * sp
  sparkTotal += sp

  var h, v
  if (z < 0) {
    // inside the liquid: hue by depth below the surface, clamped just
    // short of wrapping back to red
    h = min(0.9, -z * 1.6)
    v = 1
  } else {
    // above the surface: red-orange horizon glow hugging the mid-plane
    h = 0.03
    v = clamp(1 - z * 2, 0, 1)
    v = v * v * v
  }
  var s = 1 - min(0.35, sp)    // fizzing pixels whiten slightly
  hsv(h, s, min(1, v + sp) * bright)
}

// 2D shim (the original requires a 3D map): treat the matrix's y as the
// vertical axis so flat fixtures show a level cross-section of the liquid.
export function render2D(index, x, y) {
  render3D(index, x, 0, y)
}
