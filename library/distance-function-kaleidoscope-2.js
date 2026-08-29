// name: distance function kaleidoscope 2
// Clean-room reimplementation from a prose functional description of the
// community pattern "distance function kaleidoscope 2"; original source never
// consulted. Nested diamond/square distance bands radiate from a drifting,
// rotating center; the XOR of two distance ramps (on their fixed-point bits)
// shatters them into Sierpinski-like fractal filigree, graded by ridged noise,
// hue washing the wheel, everything temporally smoothed to feel liquid.

var N = pixelCount > 0 ? pixelCount : 256
var bMem = array(N)      // smoothed brightness, per pixel
var hMem = array(N)      // smoothed hue, per pixel

var band = 0             // fast ramp for the distance bands
var slow2 = 0            // very slow noise-axis scrolls
var slow3 = 0
var angPhase = 0         // wandering angular phase (0..1)
var baseHue = 0
var driftX = 0
var driftY = 0
var rotC = 1
var rotS = 0

export function beforeRender(delta) {
  band = time(0.15) * 16                 // ~10 s per band cycle when multiplied
  slow2 = time(30)                       // tens of minutes: imperceptible morph
  slow3 = time(45)
  // wander an angle by amplifying slow noise through a sine wave
  angPhase = wave(perlin(time(0.7), 11.7, 0) * 3)
  baseHue = time(1.5)                    // full wheel in a couple minutes

  // rebuild the drifting, rotating, zoomed-in transform each frame
  driftX = sin(perlin(time(2.2), 0, 0) * PI2) * 0.33
  driftY = sin(perlin(time(2.6), 40, 0) * PI2) * 0.33
  var rot = PI2 * wave(time(3.5))        // full turn eased sinusoidally
  rotC = cos(rot)
  rotS = sin(rot)
}

export function render2D(index, x, y) {
  // center, zoom in (visible area ~half a unit), drift, rotate
  var ax = (x - 0.5) * 0.5 + driftX
  var ay = (y - 0.5) * 0.5 + driftY
  var rx = ax * rotC - ay * rotS
  var ry = ax * rotS + ay * rotC

  var angle = atan2(ry, rx) / PI2 + 0.5

  // two distance metrics, both chased inward by the animated ramp
  var taxi = (abs(rx) + abs(ry) - 0.1) - band
  var cheb = max(abs(rx), abs(ry)) - band

  // signature trick: XOR the fixed-point bits, then fold with a triangle wave
  var interf = triangle(taxi ^ cheb)

  var i = index
  if (i < 0) i = 0
  if (i >= N) i = N - 1

  // hue: interference + slow base, lagged into per-pixel memory, then folded
  var hTarget = 0.5 * interf + baseHue
  hMem[i] = hMem[i] + 0.1 * (hTarget - hMem[i])
  var hue = triangle(hMem[i])

  // brightness: ridged multifractal with a slowly writhing 3-fold symmetry
  var sym = interf + 0.5 * triangle(3 * angle + angPhase)
  // ridge noise peaks low (~0.33) and is flat near the lattice origin, so
  // spread the sample coordinate and keep the slow axes off-origin, then gain
  // Explicit single octave, offset 0 → noise² — the bare 3-arg call's old
  // behavior under the min-1 octave clamp (stb/PB-exact would run zero).
  var nb = perlinRidge(sym * 4 + 0.5, slow2 + 1.3, slow3 + 2.7, 2, 0.5, 0, 1) * 4 - 0.05
  bMem[i] = bMem[i] + 0.33 * (nb - bMem[i])

  var bv = smoothstep(0.0, 0.6, bMem[i])
  bv = bv * bv * bv * bv                 // fourth power -> deep contrast

  hsv(hue, 1, bv)
}
