// name: fast pulse 3d
// Clean-room reimplementation from a prose functional description of the
// community pattern "fast pulse 3d"; original source never consulted.

// Sharp, narrow pulses sweep sinusoidally through the display — whipping
// through the middle of their travel, lingering at the extremes. Each has
// a white-hot core and a saturated fringe whose hue cycles through the
// whole rainbow over several seconds. In 3D the pulses are glowing planes
// whose orientation slowly tumbles (three mismatched sine oscillators act
// as the direction-vector components); 2D gets a flat slice of the same
// field; 1D gets racing bands. Fully deterministic, no per-frame state
// carried over.

var t, ox, oy, oz, off1, off3

export function beforeRender(delta) {
  t = time(0.08)                 // master phase: hue + motion, ~5.2 s
  ox = sin(t * PI2)              // axis weights: sines with mismatched
  oy = sin(time(0.04) * PI2)     // periods (~half the master...
  oz = sin(time(0.053) * PI2)    // ...and ~two-thirds), so planes tumble
  off1 = sin(t * PI2) * 2        // sinusoidal sweep offset, 1D scale
  off3 = sin(t * PI2) * 3        // wider sweep through the 3D volume
}

export function render(index) {
  var v = triangle(off1 + index / pixelCount)  // folded moving crest
  v = pow(v, 5)                  // 5th power: thin hard pulses, dark gaps
  hsv(t, v < 0.9, v)             // top ~tenth desaturates: white-hot core
}

export function render3D(index, x, y, z) {
  // position term = projection onto the tumbling direction vector
  var v = triangle(off3 + x * ox + y * oy + z * oz)
  v = pow(v, 5)
  hsv(t, v < 0.8, v)             // slightly more generous white core
}

export function render2D(index, x, y) {
  render3D(index, x, y, 0)       // flat matrices get a 2D slice
}
