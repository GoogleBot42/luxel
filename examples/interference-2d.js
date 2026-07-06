// Interference fringes: three orbiting wave sources, wave(dist·f)
// summed — physics does the pattern-making. Rendered through oklch so
// the fringes stay perceptually smooth from trough to crest.
sx = array(3)
sy = array(3)

export var freq = 6
export function sliderFrequency(v) { freq = 2 + v * 10 } //# min=2 max=12 step=0.1 default=6

export function beforeRender(delta) {
  t1 = time(0.05) * PI2
  t2 = time(0.083) * PI2
  t3 = time(0.031) * PI2
  sx[0] = 0.5 + 0.38 * cos(t1)
  sy[0] = 0.5 + 0.38 * sin(t1)
  sx[1] = 0.5 + 0.30 * cos(-t2)
  sy[1] = 0.5 + 0.30 * sin(-t2)
  sx[2] = 0.5 + 0.42 * sin(t3)
  sy[2] = 0.35 + 0.20 * cos(t3)
  hueT = time(0.17)
}

export function render2D(index, x, y) {
  v = wave(dist(x, y, sx[0], sy[0]) * freq)
  v += wave(dist(x, y, sx[1], sy[1]) * freq)
  v += wave(dist(x, y, sx[2], sy[2]) * freq)
  v /= 3
  oklch(v * 0.7, 0.16, hueT + v * 0.25)
}
