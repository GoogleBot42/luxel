// Warp field: stars fly at the viewer via the classic perspective divide
// (screen = world / depth), streaking into a feedback canvas. The slider
// goes from drifting-in-space to hyperspace.
gw = 16
n = 18
sx = array(n)
sy = array(n)
sz = array(n)
canvas = array(gw * gw)

export var warp = 0.45
export function sliderWarp(v) { warp = v } //# min=0 max=1 step=0.01 default=0.45

for (i = 0; i < n; i++) {
  sx[i] = random(1) - 0.5
  sy[i] = random(1) - 0.5
  sz[i] = 0.05 + random(0.95)
}

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  // more warp = longer streaks
  feedback(canvas, pow(0.86 + warp * 0.1, delta * 0.06))
  spd = 0.2 + warp * 1.3
  for (var i = 0; i < n; i++) {
    sz[i] -= spd * dt
    x = 0.5 + sx[i] / sz[i] * 0.5
    y = 0.5 + sy[i] / sz[i] * 0.5
    if (sz[i] < 0.05 || x < 0 || x >= 1 || y < 0 || y >= 1) {
      sx[i] = random(1) - 0.5
      sy[i] = random(1) - 0.5
      sz[i] = 1
    } else {
      idx = floor(y * 15.99) * gw + floor(x * 15.99)
      canvas[idx] = max(canvas[idx], saturate(1.25 - sz[i] * 1.25))
    }
  }
}

export function render2D(index, x, y) {
  v = canvas[floor(y * 15.99) * gw + floor(x * 15.99)]
  hsv(0.62, 0.35 - v * 0.25, v * v)  // hot-white cores, blue-tinged tails
}
