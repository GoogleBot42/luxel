// name: The Grinch
// Clean-room reimplementation from a prose functional description of the
// community pattern "The Grinch"; original source never consulted.

// Solid saturated blue field; a red block, a blue gap, and a green block
// (all the same length) march along the strip one pixel at a time,
// wrapping at the end. Roughly ten steps per second.

var BLOCK = 4        // pixels per block (red, gap, green all this long)
var STEP_MS = 100    // advance one pixel about every tenth of a second

var head = 0
var acc = 0

export function beforeRender(delta) {
  acc += delta
  while (acc >= STEP_MS) {
    acc -= STEP_MS               // keep the remainder for steady pacing
    head = (head + 1) % pixelCount
  }
}

export function render(index) {
  var off = mod(index - head, pixelCount)   // offset behind the head, wrapped
  if (off < BLOCK) {
    rgb(1, 0, 0)                 // red block at the head
  } else if (off >= BLOCK * 2 && off < BLOCK * 3) {
    rgb(0, 1, 0)                 // green block after an equal-sized blue gap
  } else {
    rgb(0, 0, 1)                 // blue background
  }
}
