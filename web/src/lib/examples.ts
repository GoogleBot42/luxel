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
  {
    name: "Comets",
    layout: { kind: "strip", pixels: 60 },
    source: `// Comets with glowing trails. The whole trail idiom is two builtin calls:
// feedback() decays the buffer, blur1D() softens it into a glow. Heads are
// deposited after the blur so they stay crisp.
leds = array(pixelCount)

export var speed = 0.5
export function sliderSpeed(v) { speed = v } //# min=0 max=1 step=0.01 default=0.5

export function beforeRender(delta) {
  feedback(leds, pow(0.92, delta * 0.06))  // frame-rate-independent decay
  blur1D(leds, 1)
  for (var c = 0; c < 3; c++) {
    // three comets bouncing at slightly different rates, staggered
    p = triangle(time(0.03 + c * 0.011) * (0.5 + speed) + c / 3)
    leds[floor(p * (pixelCount - 1))] = 1
  }
  t1 = time(0.1)
}

export function render(index) {
  v = leds[index]
  // bright heads desaturate toward white; tails stay vivid
  hsv(t1 + index / pixelCount * 0.3, 1 - v * v * 0.7, v * v)
}
`,
  },
  {
    name: "Glitter",
    layout: { kind: "strip", pixels: 60 },
    source: `// Stable sparkle without a frame buffer: hash2(index, slot) re-rolls each
// pixel once per slot instead of every frame, and hash(index) staggers the
// slot boundaries so the sparks don't blink in unison.
tick = 0

export var density = 0.25
export function sliderDensity(v) { density = v } //# min=0 max=1 step=0.01 default=0.25

export function beforeRender(delta) {
  tick = (tick + delta * 0.003) % 64  // 3 sparkle slots per second
  hueBase = time(0.15)
}

export function render(index) {
  // per-pixel local clock: same speed, offset boundaries
  t = (tick + hash(index) * 64) % 64
  slot = floor(t)
  phase = t - slot
  spark = hash2(index, slot) < density * 0.4
  if (spark) {
    v = 1 - easeOutQuad(phase)  // pop on, fade over the slot
    hsv(hueBase, 0.25, v * v)   // near-white glint
  } else {
    hsv(hueBase + 0.5, 0.9, 0.05)  // dim complementary backdrop
  }
}
`,
  },
  {
    name: "Breathing Gradient",
    layout: { kind: "strip", pixels: 60 },
    source: `// Perceptual color showcase: two drifting hues blended along the strip
// with mixColors (OKLab — no muddy midpoints), an eased breathing dim,
// and setGamma so the low end of the fade stays visually even.
setGamma(2.2)

var a = array(3)
var b = array(3)
var c = array(3)

export function beforeRender(delta) {
  breath = 0.15 + 0.85 * easeInOutCubic(triangle(time(0.1)))
  hsv2rgb(time(0.29), 0.9, 1, a)        // endpoint colors wander slowly,
  hsv2rgb(time(0.37) + 0.35, 0.9, 1, b) // at different rates
}

export function render(index) {
  mixColors(a[0], a[1], a[2], b[0], b[1], b[2], index / (pixelCount - 1), c)
  rgb(c[0] * breath, c[1] * breath, c[2] * breath)
}
`,
  },
  {
    name: "Fireflies",
    layout: { kind: "strip", pixels: 60 },
    source: `// A particle system: eight fireflies wander the strip, each with its own
// position, drift, and flash clock held in parallel arrays. Flashes are
// eased triangles deposited into a decaying frame buffer.
numFlies = 8
pos = array(numFlies)
vel = array(numFlies)
clocks = array(numFlies)
leds = array(pixelCount)

for (i = 0; i < numFlies; i++) {
  pos[i] = random(pixelCount)
  vel[i] = (random(1) - 0.5) * 0.012
  clocks[i] = random(1)
}

export function beforeRender(delta) {
  feedback(leds, pow(0.8, delta * 0.06))
  for (var i = 0; i < numFlies; i++) {
    pos[i] += vel[i] * delta
    if (pos[i] < 0) pos[i] += pixelCount
    if (pos[i] >= pixelCount) pos[i] -= pixelCount
    clocks[i] += delta * 0.0003          // ~3.3 s per cycle
    if (clocks[i] >= 1) {
      clocks[i] -= 1
      vel[i] = (random(1) - 0.5) * 0.012 // pick a new drift each cycle
    }
    f = clocks[i] * 4                    // flash fills the first quarter
    if (f < 1) {
      leds[floor(pos[i])] += easeInOutQuad(triangle(f))
    }
  }
}

export function render(index) {
  v = min(leds[index], 1)
  // warm yellow-green, whitening slightly at full flash
  hsv(0.19 - v * 0.04, 1 - v * 0.4, v * v)
}
`,
  },
  {
    name: "Marquee Chase",
    layout: { kind: "strip", pixels: 60 },
    source: `// Theater-marquee chase built from square(), with a UI-controls tour:
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
`,
  },
  {
    name: "Beat Bounce",
    layout: { kind: "strip", pixels: 60 },
    source: `// Tempo-locked motion with no audio hardware: beatSin() sweeps a hot core
// back and forth while beat() spikes its width on every beat. A gauge
// readout shows the beat phase.
var c = array(3)

export var bpm = 96
export function inputNumberBpm(v) { bpm = clamp(v, 30, 220) } //# min=30 max=220 step=1 default=96

export function gaugeBeat() { return beatPhase }

export function beforeRender(delta) {
  pos = beatSin(bpm * 0.25, 0.1, 0.9)  // one full sweep every 4 beats
  beatPhase = beat(bpm)
  thump = (1 - beatPhase) * (1 - beatPhase)
  width = 0.05 + 0.09 * thump
}

export function render(index) {
  x = index / (pixelCount - 1)
  d = abs(x - pos)
  core = saturate(1 - d / width)
  halo = saturate(1 - d / 0.3) * 0.25 * thump
  mixColors(1, 0.25, 0, 0.35, 0, 1, saturate(d / 0.12), c)  // ember → violet
  b = core * core + halo
  rgb(c[0] * b, c[1] * b, c[2] * b)
}
`,
  },
  {
    name: "Ripples 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Rain on a pond: three expanding rings, each a pure function of dist()
// from its drop point. Drops reposition themselves when their ring fades
// out; the trigger button splashes one immediately.
numDrops = 3
cx = array(numDrops)
cy = array(numDrops)
ph = array(numDrops)

for (i = 0; i < numDrops; i++) {
  cx[i] = random(1)
  cy[i] = random(1)
  ph[i] = random(1)
}

export function triggerSplash() {
  ph[0] = 0
  cx[0] = random(1)
  cy[0] = random(1)
}

export function beforeRender(delta) {
  for (var i = 0; i < numDrops; i++) {
    ph[i] += delta * (0.00035 + i * 0.00006)
    if (ph[i] >= 1) {
      ph[i] -= 1
      cx[i] = random(1)
      cy[i] = random(1)
    }
  }
}

export function render2D(index, x, y) {
  v = 0
  for (var i = 0; i < numDrops; i++) {
    d = dist(x, y, cx[i], cy[i])
    ring = saturate(1 - abs(d - ph[i] * 0.7) * 9)
    v += ring * ring * (1 - ph[i])  // rings dim as they expand
  }
  v = min(v, 1)
  // deep water backdrop; crests whiten
  hsv(0.58 - v * 0.05, 1 - v * 0.6, 0.04 + v * v)
}
`,
  },
  {
    name: "Aurora 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Aurora curtains: simplex2 draws the slowly-waving band, simplex3 adds
// vertical shimmer rays, and a palette paints black → green → violet.
// Simplex is smoother than perlin here — no axis-aligned artifacts.
setPalette([
  0.0,  0,    0,    0,
  0.25, 0,    0.07, 0.03,
  0.55, 0,    0.55, 0.18,
  0.8,  0.15, 0.95, 0.5,
  1.0,  0.75, 0.45, 0.95
])

z = 0

export function beforeRender(delta) {
  z = (z + delta * 0.00015) % 1024  // slow drift along one noise axis
}

export function render2D(index, x, y) {
  band = 0.45 + simplex2(x * 1.8, z, 5) * 0.25
  glow = saturate(1 - abs(y - band) * 2)
  shimmer = 0.6 + 0.4 * simplex3(x * 6, y * 2, z * 4, 9)
  v = saturate(glow * shimmer * 1.4)
  paint(v, v * v)
}
`,
  },
  {
    name: "Spiral 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Polar coordinates from scratch: atan2 + hypot turn (x, y) into angle
// and radius, and one wave() of both makes rotating spiral arms. Integer
// arm counts keep the wave continuous around the seam.
export var arms = 3
export function sliderArms(v) { arms = 1 + floor(v * 5.99) } //# min=1 max=6 step=1 default=3

export function beforeRender(delta) {
  t1 = time(0.05)
  t2 = time(0.13)
}

export function render2D(index, x, y) {
  dx = x - 0.5
  dy = y - 0.5
  a = atan2(dy, dx) / PI2  // turns, -0.5..0.5
  r = hypot(dx, dy) * 1.4
  v = wave(a * arms - t1 * 2 + r * 2.5)
  v = v * v * v
  hsv(0.7 + r * 0.4 + t2, 1 - v * 0.4, v * saturate(1.3 - r * 1.6))
}
`,
  },
  {
    name: "Digital Rain 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Falling code: 16 virtual columns, each with its own speed and phase
// accumulated in beforeRender (frac() keeps them continuous forever).
// hash2 flickers the "glyphs" so cells shimmer as the trail passes.
cols = 16
phases = array(cols)
speeds = array(cols)

for (i = 0; i < cols; i++) {
  speeds[i] = 0.4 + hash(i * 13.7) * 0.9
  phases[i] = hash(i * 7.3)
}

glyphTick = 0

export function beforeRender(delta) {
  for (var i = 0; i < cols; i++) {
    phases[i] = frac(phases[i] + delta * 0.0004 * speeds[i])
  }
  glyphTick = (glyphTick + delta * 0.008) % 64  // ~8 glyph re-rolls per second
}

export function render2D(index, x, y) {
  col = floor(x * 15.999)
  behind = mod(phases[col] - y, 1)  // how far the head has passed this cell
  trail = saturate(1 - behind / 0.45)
  glyph = 0.4 + 0.6 * hash2(col * 16 + floor(y * 15.999), floor(glyphTick))
  head = saturate((0.06 - behind) * 25)  // ≈1 at the head, 0 in the trail
  v = trail * trail * glyph
  hsv(0.36, 1 - head * 0.8, max(v, head))
}
`,
  },
];
