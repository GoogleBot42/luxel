// name: Holiday_Diagonal_Stripes
// Clean-room reimplementation from a prose functional description of the
// community pattern "Holiday_Diagonal_Stripes"; original source never consulted.

// Solid red / white / green diagonal stripes scrolling steadily across the
// display. The banding comes from a sinusoidal wave split into thirds, so
// the sequence mirrors (...red, white, green, white, red...) and the red
// and green bands are wider than the white ones — preserved on purpose.

var slope = 0.5
//# min=0 max=1 step=0.01 default=0.5
export function sliderSlope(v) {
  slope = v                // 0 = near-vertical boundaries, 1 = steep diagonal
}

var phase = 0

export function beforeRender(delta) {
  phase = time(0.1)        // one full scroll cycle ~6.5 s
}

export function render2D(index, x, y) {
  // stripe coordinate: x plus y times twice the slope, plus the scroll phase
  var w = wave((x + y * 2 * slope + phase) * 6)
  if (w < 1 / 3) {
    rgb(1, 0, 0)           // solid red
  } else if (w < 2 / 3) {
    rgb(1, 1, 1)           // solid white
  } else {
    rgb(0, 1, 0)           // solid green
  }
}
