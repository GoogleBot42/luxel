/** How pixels are arranged: a strip, a W×H grid, or an arbitrary 2D/3D
 *  map produced by the mapper (one [x,y(,z)] per pixel, any units). */
export type Layout =
  | { kind: "strip"; pixels: number }
  | { kind: "grid"; w: number; h: number }
  | { kind: "map"; coords: number[][] };

export interface Example {
  name: string;
  layout: Layout;
  source: string;
}

export const EXAMPLES: Example[] = [
  {
    name: "Rainbow",
    layout: { kind: "strip", pixels: 60 },
    source: `// The canonical default pattern: a moving rainbow.
export function render(index) {
  hsv(time(.1) + index / pixelCount, 1, 1)
}
`,
  },
  {
    name: "Blink Fade",
    layout: { kind: "strip", pixels: 60 },
    source: `// Frame-buffer idiom: random pixels flare up and decay each frame.
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
`,
  },
  {
    name: "Spinning Plasma 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// 2D showcase: perlin noise sampled through a rotating transform.
// The //# comment bounds the slider (Luxel extension; PB ignores it).
export var zoom = 0.45
export function sliderZoom(v) { zoom = v } //# min=0.1 max=1.5 step=0.01 default=0.45

export function beforeRender(delta) {
  t1 = time(.05)
  resetTransform()
  translate(-0.5, -0.5)
  rotate(t1 * PI2)
}

export function render2D(index, x, y) {
  n = perlin(x * 4 * zoom + 10, y * 4 * zoom + 10, time(.1) * 4, 7)
  hsv(0.6 + n * 0.5, 1, clamp(n + 0.6, 0, 1))
}
`,
  },
  {
    name: "Palette Fire 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// setPalette / paint with fbm noise driving a fire look.
setPalette([
  0.0,  0,    0,    0,
  0.3,  0.6,  0,    0,
  0.6,  1,    0.4,  0,
  0.85, 1,    0.9,  0.1,
  1.0,  1,    1,    0.8
])

export function render2D(index, x, y) {
  heat = perlinFbm(x * 3, y * 3 - time(.03) * 8, 0, 2, 0.5, 3)
  paint(clamp(heat + 0.9 - y * 1.4, 0, 1))
}
`,
  },
  {
    name: "KITT",
    layout: { kind: "strip", pixels: 60 },
    source: `// Classic scanner with a decaying trail.
leds = array(pixelCount)

export function beforeRender(delta) {
  for (var i = 0; i < pixelCount; i++) leds[i] *= 0.92
  pos = floor(triangle(time(.05)) * (pixelCount - 1))
  leds[pos] = 1
}

export function render(index) {
  v = leds[index]
  rgb(v * v * 1.2, v * v * v * 0.2, 0)
}
`,
  },
];
