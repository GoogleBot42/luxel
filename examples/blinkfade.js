// Frame-buffer idiom: random pixels flare up and decay each frame.
values = array(pixelCount)
hues = array(pixelCount)
fade = 0.02

export var speed = 0.5
export function sliderSpeed(v) { speed = v }

export function beforeRender(delta) {
  t1 = time(.05)
  for (var i = 0; i < pixelCount; i++) {
    values[i] -= fade * delta * (0.05 + speed)
    if (values[i] <= 0) {
      values[i] = random(1)
      hues[i] = t1 + random(.2)
    }
  }
}

export function render(index) {
  v = values[index]
  hsv(hues[index], 1, v * v)
}
