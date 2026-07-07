// name: index walk
// Clean-room reimplementation from a prose functional description of the
// community pattern "index walk"; original source never consulted.

// Minimal teaching pattern: one lit pixel at a time marches from the start
// of the strip to the end, then jumps back and repeats. Its rainbow hue
// cycles fast (well under a second per full cycle) and also shifts with
// position. Per the spec's cleanup notes the walk is time-based (scaled by
// the frame delta, so speed is frame-rate independent) and the speed is
// promoted from a source constant to a slider.

var speed = 4.5   // pixels per second

//# min=0 max=1 step=0.01 default=0.2
export function sliderSpeed(v) { speed = 0.5 + v * v * 100 }

var cursor = 0    // fractional pixel position
var huePhase = 0

export function beforeRender(delta) {
  cursor += speed * delta / 1000
  cursor = mod(cursor, pixelCount)   // wrap back to the start
  huePhase = time(0.01)              // full hue cycle in ~0.66 s
}

export function render(index) {
  if (index == floor(cursor)) {
    hsv(huePhase + index / pixelCount, 1, 1)
  } else {
    // explicitly paint black: unwritten pixels would keep their old color
    rgb(0, 0, 0)
  }
}
