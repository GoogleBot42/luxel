// Theater-marquee chase built from square(), with a UI-controls tour:
// an hsvPicker for the bulb color, a slider for block count, and a
// toggle for direction.
var h = 0.04
var s = 0.9
var v = 1
export function hsvPickerColor(_h, _s, _v) { h = _h; s = _s; v = _v }

var blocks = 5
export function sliderBlocks(x) { blocks = 1 + floor(x * 9.99) } //# min=1 max=10 step=1 default=5

var dir = 1
export function toggleReverse(x) { dir = x ? -1 : 1 }

export function beforeRender(delta) {
  t1 = time(0.04) * dir
}

export function render(index) {
  on = square(index * blocks / pixelCount + t1, 0.4)
  // unlit bulbs keep a faint warm glow, like real marquee filaments
  hsv(h, s, v * max(on, 0.04))
}
