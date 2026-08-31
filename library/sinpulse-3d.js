// name: sinpulse 3D
// Clean-room reimplementation from a prose functional description of the
// community pattern "sinpulse 3D"; original source never consulted.

// Classic rainbow plasma: two drifting phases interfere across x/y/z while
// a slow triangle "zoom" breathes the blob scale between coarse and fine.
// Brightness is the field cubed, so dim valleys separate glowing crests.

var p1 = 0    // first phase angle (full circle sawtooth, ~3 s)
var p2 = 0    // second phase angle (~7 s; ~2x period ratio, never repeats)
var zoom = 1  // spatial scale, breathing between ~1 and ~4 over ~10 s

// time(n) laps in n * 65.536 s, hence the divisions below. The second phase
// keeps its ~2.15x period ratio against the first so the two never lock.
var P2_RATIO = 0.101 / 0.047
var driftInterval = 0.047  // ~3.1 s for the first phase
var drift2Interval = 0.101 // ~6.6 s for the second
var zoomInterval = 0.15    // ~9.8 s for one zoom breath
var zoomMin = 1            // coarsest blob scale
var zoomMax = 4            // finest blob scale
var hueShift = 0           // turns added to the plasma hue

// Seconds for the faster of the two interfering phases to come around.
//# min=0.5 max=30 step=0.1 default=3.1
export function sliderDriftSeconds(v) {
  driftInterval = max(0.25, v) / 65.536
  drift2Interval = driftInterval * P2_RATIO
}

// Seconds for one full coarse -> fine -> coarse breath.
//# min=1 max=60 step=0.1 default=9.8
export function sliderZoomSeconds(v) { zoomInterval = max(0.25, v) / 65.536 }

// Tightest blob scale reached at the top of the breath (1 = no breathing).
//# min=1 max=12 step=0.5 default=4
export function sliderMaxZoom(v) { zoomMax = max(zoomMin, v) }

//# min=0 max=360 step=5 default=0
export function sliderHueShift(v) { hueShift = v / 360 }

export function beforeRender(delta) {
  p1 = time(driftInterval) * PI2
  p2 = time(drift2Interval) * PI2
  zoom = zoomMin + (zoomMax - zoomMin) * triangle(time(zoomInterval))
}

export function render3D(index, x, y, z) {
  var v = (1 + sin(x * zoom + p1) + cos(y * zoom + p2) + sin(z * zoom + p1 - p2)) / 2
  hsv(v + hueShift, 1, v * v * v / 2)
}

export function render2D(index, x, y) {
  render3D(index, x, y, 0)
}

export function render(index) {
  render3D(index, index / pixelCount, 0, 0)
}
