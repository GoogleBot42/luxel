export var speed = 0.5
var arr = array(8)
arr.mutate((v, i) => i / 8)
assert(pixelCount > 0, "needs pixels")
export function sliderSpeed(v) { speed = v }
export function beforeRender(delta) { t = time(0.05 * speed) }
export function render(index) {
  var h = t + index / pixelCount + arr[index % 8]
  hsv(h, 1, triangle(h) > 0.5 ? 1 : 0.2)
}
