// name: 2D canvas example
// Clean-room reimplementation from a prose functional description of the
// community pattern "2D canvas example"; original source never consulted.

// Tutorial for the "offscreen canvas" idiom: draw into a small in-memory
// buffer once per frame, then sample it per pixel. A bright dot orbits a
// circle (~1/3 radius, centred), leaving a fading, slowly hue-cycling comet
// trail. The draw resolution (fixed 16x16 canvas) is decoupled from the
// display resolution (whatever the map provides). Brightness is squared only
// at display time; the stored canvas keeps linear values so decay is stable.

const DIM = 16
var canvasBri = array(DIM * DIM)   // linear brightness, row-major
var canvasHue = array(DIM * DIM)

export function beforeRender(delta) {
  // 1. geometric per-frame decay (~1 s trail)
  feedback(canvasBri, 0.95)

  // 2. dot position: steadily advancing angle, ~1.3 s orbit
  var a = time(0.02) * PI2
  var x = 0.5 + cos(a) * 0.333
  var y = 0.5 + sin(a) * 0.333

  // 3. write the dot into the canvas — canvasSet clamps to the edges,
  // so no manual bounds check is needed
  canvasSet(canvasBri, DIM, x, y, 1)
  canvasSet(canvasHue, DIM, x, y, time(0.03))   // slower hue drift, ~2 s per wheel
}

export function render2D(index, x, y) {
  var b = canvasGet(canvasBri, DIM, x, y)   // bilinear — smooth on any map
  hsv(canvasGet(canvasHue, DIM, x, y), 1, b * b)   // square at display time
}
