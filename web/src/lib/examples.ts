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
  {
    name: "Pendulum Wave",
    layout: { kind: "strip", pixels: 60 },
    source: `// The physics-classroom classic: one pendulum per pixel, periods
// graduated so the row drifts from unison into traveling waves, apparent
// chaos, and back to unison. tSec wraps exactly at the realignment
// period — every pendulum completes a whole number of swings — so the
// cycle is seamless.
var cycle = 24  // seconds until the pendulums realign
tSec = 0

export function beforeRender(delta) {
  tSec = (tSec + delta * 0.001) % cycle
}

export function render(index) {
  // pendulum i completes (16 + i) full swings per cycle
  d = sin(PI2 * (16 + index) * tSec / cycle)
  hsv(0.72 + d * 0.14, 0.9, d * d)
}
`,
  },
  {
    name: "Drip",
    layout: { kind: "strip", pixels: 60 },
    source: `// A dripping faucet at pixel 0: the droplet swells, tears off, falls
// under gravity (scaled to the strip so any length works), and splashes.
// One tiny state machine plus a feedback trail.
leds = array(pixelCount)
state = 0  // 0 = growing, 1 = falling, 2 = splash settling
size = 0
pos = 0
vel = 0
pause = 0

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  feedback(leds, pow(0.78, delta * 0.06))
  if (state == 0) {
    size += dt * (0.5 + random(0.5))
    leds[0] = size * size
    if (size >= 1) {
      state = 1
      pos = 0
      vel = pixelCount * 0.05
    }
  } else if (state == 1) {
    vel += pixelCount * 2.2 * dt  // gravity: ~1 s to fall any strip
    pos += vel * dt
    if (pos >= pixelCount - 1) {
      // splash: a burst that the feedback decay fades out
      leds[pixelCount - 1] = 1
      leds[pixelCount - 2] = 0.7
      leds[pixelCount - 3] = 0.4
      leds[pixelCount - 4] = 0.2
      state = 2
      pause = 0.4
    } else {
      leds[floor(pos)] = 1
    }
  } else {
    pause -= dt
    if (pause <= 0) {
      state = 0
      size = 0
    }
  }
}

export function render(index) {
  v = leds[index]
  hsv(0.55 + v * 0.06, 1 - v * v * 0.55, v * v)
}
`,
  },
  {
    name: "Ocean",
    layout: { kind: "strip", pixels: 60 },
    source: `// An ocean in the spirit of FastLED's Pacifica (original construction):
// layered waves scrolling at different speeds and scales, summed through
// a deep palette. Whitecaps break where the layers crest together.
// Integer time multipliers keep every layer seamless when time() wraps.
setPalette([
  0.0,  0,    0.02, 0.10,
  0.45, 0,    0.10, 0.35,
  0.75, 0,    0.35, 0.45,
  0.92, 0.25, 0.60, 0.55,
  1.0,  0.85, 0.95, 0.90
])

export function beforeRender(delta) {
  t1 = time(0.11)
  t2 = time(0.07)
  t3 = time(0.05)
  swell = 0.7 + 0.3 * wave(time(0.19))  // slow set-wave rolling through
}

export function render(index) {
  x = index / pixelCount
  v = 0.35 * wave(x * 3 + t1 * 2)
  v += 0.30 * wave(x * 5 + t2 * 3)
  v += 0.20 * wave(x * 8 - t3 * 4)  // one counter-current layer for chop
  v *= swell
  foam = saturate(v - 0.68) * 4     // crests break past the threshold
  paint(saturate(v + foam), 0.25 + v * 0.75 + foam)
}
`,
  },
  {
    name: "TV Simulator",
    layout: { kind: "strip", pixels: 60 },
    source: `// The glow of a TV through the curtains: scene cuts at random intervals,
// a one-pole flicker inside each scene, rare full-screen flashes. A
// vacant-house utility pattern more than a looker (WLED ships one too).
sceneT = 0
hue = 0.6
sat = 0.4
lvl = 0.8
flick = 1
flash = 0

export function beforeRender(delta) {
  sceneT -= delta * 0.001
  if (sceneT <= 0) {
    sceneT = 0.3 + random(5)  // scene length
    // mostly cool broadcast blues, sometimes a warm interior shot
    hue = random(1) < 0.6 ? 0.55 + random(0.15) : random(0.14)
    sat = 0.15 + random(0.55)
    lvl = 0.25 + random(0.75)
  }
  // in-scene luminance wander (one-pole low-pass over random targets)
  flick += (0.55 + random(0.45) - flick) * min(delta * 0.02, 1)
  flash = random(1) < delta * 0.0007  // explosion / lightning on screen
}

export function render(index) {
  if (flash) {
    hsv(0, 0, 1)
  } else {
    // slight spatial falloff so it reads as spill, not a solid panel
    hsv(hue, sat, lvl * flick * (0.8 + 0.2 * triangle(index / pixelCount)))
  }
}
`,
  },
  {
    name: "Boids 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Flocking: cohesion, alignment, and separation over parallel state
// arrays, with dist() doing the pair math. The flock draws into a 16×16
// virtual canvas that render2D samples by coordinate, so any map works.
gw = 16
n = 6
px = array(n)
py = array(n)
vx = array(n)
vy = array(n)
canvas = array(gw * gw)
wt = 0

for (i = 0; i < n; i++) {
  px[i] = random(1)
  py[i] = random(1)
  a = random(PI2)
  vx[i] = cos(a) * 0.2
  vy[i] = sin(a) * 0.2
}

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  wt += dt * 0.3
  if (wt > 256) wt -= 256
  feedback(canvas, pow(0.93, delta * 0.06))
  // flock center + mean velocity (cohesion and alignment targets)
  mx = 0
  my = 0
  mvx = 0
  mvy = 0
  for (var i = 0; i < n; i++) {
    mx += px[i]
    my += py[i]
    mvx += vx[i]
    mvy += vy[i]
  }
  mx /= n
  my /= n
  mvx /= n
  mvy /= n
  for (var i = 0; i < n; i++) {
    ax = (mx - px[i]) * 1.7 + (mvx - vx[i]) * 1.6 + (0.5 - px[i]) * 0.7
    ay = (my - py[i]) * 1.7 + (mvy - vy[i]) * 1.6 + (0.5 - py[i]) * 0.7
    for (var j = 0; j < n; j++) {
      if (j != i) {
        d = dist(px[i], py[i], px[j], py[j])
        if (d < 0.16) {  // separation: push off close flockmates
          f = (0.16 - d) * 9 / (d + 0.02)
          ax += (px[i] - px[j]) * f
          ay += (py[i] - py[j]) * f
        }
      }
    }
    // smooth per-boid wander so the flock never settles
    ax += simplex2(i * 3.7, wt, 21) * 0.5
    ay += simplex2(i * 3.7 + 40, wt, 21) * 0.5
    vx[i] += ax * dt
    vy[i] += ay * dt
    s = hypot(vx[i], vy[i])
    if (s > 0.32) {
      vx[i] *= 0.32 / s
      vy[i] *= 0.32 / s
    }
    px[i] = clamp(px[i] + vx[i] * dt, 0, 0.999)
    py[i] = clamp(py[i] + vy[i] * dt, 0, 0.999)
    canvas[floor(py[i] * 15.99) * gw + floor(px[i] * 15.99)] = 1
  }
  t1 = time(0.08)
}

export function render2D(index, x, y) {
  v = canvas[floor(y * 15.99) * gw + floor(x * 15.99)]
  hsv(t1 + v * 0.12, 1 - v * v * 0.6, v * v)
}
`,
  },
  {
    name: "Flow Field 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Streamlines of a drifting simplex field: sample the noise, read it as
// a heading, step along it. Trails on a 16×16 virtual canvas remember a
// hue per cell — streams stay colored by the direction they flowed.
gw = 16
n = 20
px = array(n)
py = array(n)
canvas = array(gw * gw)
hues = array(gw * gw)
z = 0

for (i = 0; i < n; i++) {
  px[i] = random(1)
  py[i] = random(1)
}

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  z += dt * 0.045  // the field itself slowly morphs
  if (z > 512) z -= 512
  feedback(canvas, pow(0.9, delta * 0.06))
  for (var i = 0; i < n; i++) {
    a = simplex3(px[i] * 1.6, py[i] * 1.6, z, 3) * PI2
    px[i] += cos(a) * 0.22 * dt
    py[i] += sin(a) * 0.22 * dt
    if (px[i] < 0 || px[i] >= 1 || py[i] < 0 || py[i] >= 1) {
      px[i] = random(0.999)
      py[i] = random(0.999)
    } else {
      idx = floor(py[i] * 15.99) * gw + floor(px[i] * 15.99)
      canvas[idx] = 1
      hues[idx] = a / PI2  // heading = hue (wraps naturally)
    }
  }
  t1 = time(0.2)
}

export function render2D(index, x, y) {
  idx = floor(y * 15.99) * gw + floor(x * 15.99)
  v = canvas[idx]
  hsv(hues[idx] + t1, 1 - v * v * 0.4, v * v)
}
`,
  },
  {
    name: "Typing Heatmap 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// QMK's typing-heatmap idea: phantom keystrokes deposit heat, heat
// diffuses to its four neighbors and cools, and a thermal palette maps
// it black → blue → red → white. The trigger hammers a few keys at once.
// Diffusion is the hand-rolled 2D blur — see docs/ideas.md for blur2D.
gw = 16
heat = array(gw * gw)
scratch = array(gw * gw)
acc = 0
burst = 0

setPalette([
  0.0,  0,    0,    0,
  0.2,  0,    0.05, 0.3,
  0.45, 0.25, 0,    0.5,
  0.7,  0.9,  0.1,  0,
  0.88, 1,    0.55, 0,
  1.0,  1,    1,    0.9
])

export function triggerKeys() { burst = 6 }

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  // phantom typist: ~8 keys/s, clustered toward the middle rows
  acc += dt * 10 + burst
  burst = 0
  while (acc >= 1) {
    acc -= 1
    kx = floor(random(gw))
    ky = floor(clamp(8 + (random(1) - 0.5) * 9, 0, gw - 1))
    heat[ky * gw + kx] = min(heat[ky * gw + kx] + 0.85, 1.3)
  }
  // diffuse to 4-neighbors (edges clamp), then cool
  k = min(dt * 3.5, 0.18)
  cool = 1 - min(dt * 0.35, 0.9)
  last = gw * gw - 1
  for (var i = 0; i <= last; i++) {
    x = i % gw
    up = i >= gw ? heat[i - gw] : heat[i]
    dn = i <= last - gw ? heat[i + gw] : heat[i]
    lf = x > 0 ? heat[i - 1] : heat[i]
    rt = x < gw - 1 ? heat[i + 1] : heat[i]
    scratch[i] = (heat[i] * (1 - k) + k * (up + dn + lf + rt) / 4) * cool
  }
  tmp = heat
  heat = scratch
  scratch = tmp
}

export function render2D(index, x, y) {
  h = heat[floor(y * 15.99) * gw + floor(x * 15.99)]
  paint(min(h, 1), 1)
}
`,
  },
  {
    name: "Interference 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Interference fringes: three orbiting wave sources, wave(dist·f)
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
`,
  },
  {
    name: "Crosshair Pulse 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// A keyboard idea (QMK "reactive nexus"): each hit fires four pulses
// racing outward along the hit's row and column, plus a center flash.
// Phantom hits arrive at random; the trigger lands one dead-center.
m = 4
ex = array(m)
ey = array(m)
age = array(m)
ehue = array(m)

for (i = 0; i < m; i++) age[i] = 9  // all slots idle

function spawn(x0, y0) {
  var best = 0
  for (var i = 1; i < m; i++) {
    if (age[i] > age[best]) best = i
  }
  ex[best] = x0
  ey[best] = y0
  age[best] = 0
  ehue[best] = t1 + random(0.25)
}

export function triggerHit() { spawn(0.5, 0.5) }

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  t1 = time(0.11)
  if (random(1) < dt * 2.5) spawn(random(1), random(1))
  for (var i = 0; i < m; i++) age[i] += dt * 1.2
}

export function render2D(index, x, y) {
  v = 0
  h = 0
  for (var i = 0; i < m; i++) {
    if (age[i] < 1) {
      fade = 1 - age[i]
      r = age[i] * 0.85  // how far the pulses have traveled
      row = saturate(1 - abs(y - ey[i]) * 12) * saturate(1 - abs(abs(x - ex[i]) - r) * 9)
      col = saturate(1 - abs(x - ex[i]) * 12) * saturate(1 - abs(abs(y - ey[i]) - r) * 9)
      p = (row + col) * fade * 1.4 + saturate(1 - age[i] * 5) * saturate(1 - dist(x, y, ex[i], ey[i]) * 6)
      if (p > v) {
        v = p
        h = ehue[i]
      }
    }
  }
  v = min(v, 1)
  hsv(h, 1 - v * 0.5, v * v)
}
`,
  },
  {
    name: "Starfield 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Warp field: stars fly at the viewer via the classic perspective divide
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
`,
  },
  {
    name: "DNA Helix 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// A rotating double helix: two phase-offset sine strands, cos() depth
// dimming whichever strand is "behind", and rungs on a square() mask
// between them. Pure per-pixel math — no state.
export function beforeRender(delta) {
  t1 = time(0.05) * PI2
}

export function render2D(index, x, y) {
  ph = x * PI2 + t1  // one helix turn across the panel
  y1 = 0.5 + sin(ph) * 0.3
  y2 = 0.5 - sin(ph) * 0.3
  d1 = 0.55 + 0.45 * cos(ph)  // depth cue
  d2 = 1.1 - d1
  v1 = saturate(1 - abs(y - y1) * 7)
  v1 = v1 * v1 * d1
  v2 = saturate(1 - abs(y - y2) * 7)
  v2 = v2 * v2 * d2
  // rungs: bars fixed to the helix so they ride around with it
  lo = min(y1, y2)
  hi = max(y1, y2)
  vr = 0
  if (y > lo + 0.04 && y < hi - 0.04) {
    vr = square(ph / PI2 * 6, 0.28) * 0.6 * min(d1, d2)
  }
  if (v1 > v2 && v1 > vr) hsv(0.5, 0.85, v1)        // cyan strand
  else if (v2 > vr) hsv(0.88, 0.85, v2)             // magenta strand
  else hsv(0.12, 0.35, vr)                          // pale gold rungs
}
`,
  },
  {
    name: "Radar 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Radar: no buffer needed — phosphor persistence is just "how long ago
// did the beam pass this angle", straight from atan2. Contacts flare
// when painted and relocate every third sweep.
nb = 5
bx = array(nb)
by = array(nb)
sweep = 0
prev = 1
epoch = 0

function placeBlips() {
  for (var i = 0; i < nb; i++) {
    var a = hash2(i, epoch) * PI2
    var r = 0.12 + hash2(i + 7, epoch) * 0.33
    bx[i] = 0.5 + cos(a) * r
    by[i] = 0.5 + sin(a) * r
  }
}
placeBlips()

export function beforeRender(delta) {
  sweep = time(0.04)  // one revolution ≈ 2.6 s
  if (sweep < prev) {
    epoch = (epoch + 1) % 64
    if (epoch % 3 == 0) placeBlips()
  }
  prev = sweep
  beamA = sweep * PI2
}

export function render2D(index, x, y) {
  dx = x - 0.5
  dy = y - 0.5
  r = hypot(dx, dy)
  behind = mod(beamA - atan2(dy, dx), PI2) / PI2
  edge = saturate(1.15 - r * 2.2)
  v = max(saturate(1 - behind * 24), pow(1 - behind, 6) * 0.55) * edge  // beam + afterglow
  v = max(v, saturate(1 - abs(r - 0.44) * 30) * 0.2)  // bezel ring
  for (var i = 0; i < nb; i++) {
    db = dist(x, y, bx[i], by[i])
    if (db < 0.09) {
      bb = mod(beamA - atan2(by[i] - 0.5, bx[i] - 0.5), PI2) / PI2
      v = max(v, (1 - db / 0.09) * pow(1 - bb, 3) * 1.3)
    }
  }
  hsv(0.35, 1 - saturate(v - 0.9) * 2, min(v * v, 1))  // phosphor green
}
`,
  },
  {
    name: "Chevron 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// The smallest possible render2D (a QMK keyboard classic): abs() folds
// the gradient into a V, time() scrolls it. Whole pattern, one line.
export function render2D(index, x, y) {
  hsv(time(0.06) + (x - abs(y - 0.5)) * 0.7, 1, 1)
}
`,
  },
  {
    name: "Tetrix 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Stacker (WLED's "Tetrix", reimagined per-column): every column drops
// blocks onto its pile at its own pace; a full column flashes and dumps.
// All state — no canvas — render2D reads the arrays directly.
gw = 16
rows = 16
stack = array(gw)  // settled height per column
fy = array(gw)     // falling block's bottom edge, in rows from the top
fh = array(gw)     // falling block height
fhue = array(gw)
flash = array(gw)  // > 0 while a full column celebrates + clears

function startBlock(c) {
  fh[c] = 1 + floor(random(3))
  fy[c] = 0
  fhue[c] = random(1)
}
for (i = 0; i < gw; i++) startBlock(i)

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  for (var c = 0; c < gw; c++) {
    if (flash[c] > 0) {
      flash[c] -= dt * 1.5
      if (flash[c] <= 0) {
        stack[c] = 0
        startBlock(c)
      }
    } else {
      fy[c] += dt * (4 + hash(c * 5.1) * 5)
      if (fy[c] >= rows - stack[c]) {  // landed
        stack[c] += fh[c]
        if (stack[c] >= rows) flash[c] = 1
        else startBlock(c)
      }
    }
  }
}

export function render2D(index, x, y) {
  c = floor(x * 15.99)
  row = y * rows  // 0 = top edge
  if (flash[c] > 0) {
    hsv(0.14, 0.3, square(flash[c] * 5, 0.5))
  } else if (row >= rows - stack[c]) {
    hsv(0.02 + (rows - row) * 0.04, 1, 0.75)  // the pile, banded by depth
  } else if (row >= fy[c] - fh[c] && row < fy[c]) {
    hsv(fhue[c], 1, 1)  // the falling block
  } else {
    hsv(0, 0, 0)
  }
}
`,
  },
  {
    name: "Falling Sand 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Falling sand, the classic cellular automaton: grains pour from the
// spout, drop one cell per tick, and slide down the pile's slopes. When
// the pile reaches the spout the panel fades out and pours again. Each
// grain keeps its own shade, so the pile ends up naturally speckled.
gw = 16
rows = 16
grid = array(gw * rows)
tickMs = 0
fading = 0

function step() {
  // bottom-up scan: a grain moves at most one cell per tick
  for (var y = rows - 2; y >= 0; y--) {
    for (var x = 0; x < gw; x++) {
      var i = y * gw + x
      if (grid[i] > 0) {
        var below = i + gw
        if (grid[below] == 0) {
          grid[below] = grid[i]
          grid[i] = 0
        } else {
          var d = random(1) < 0.5 ? -1 : 1  // coin-flip slide direction
          if (x + d >= 0 && x + d < gw && grid[below + d] == 0 && grid[i + d] == 0) {
            grid[below + d] = grid[i]
            grid[i] = 0
          } else if (x - d >= 0 && x - d < gw && grid[below - d] == 0 && grid[i - d] == 0) {
            grid[below - d] = grid[i]
            grid[i] = 0
          }
        }
      }
    }
  }
  spout = floor(gw / 2)
  if (grid[spout] == 0) grid[spout] = 0.5 + random(0.5)
  else if (grid[spout + gw] > 0) fading = 1  // pile reached the spout
}

export function beforeRender(delta) {
  if (fading) {
    feedback(grid, pow(0.85, delta * 0.06))
    if (arraySum(grid) < 0.5) {
      arrayReplace(grid, 0)
      fading = 0
    }
  } else {
    tickMs += min(delta, 100)
    while (tickMs >= 40) {
      tickMs -= 40
      step()
    }
  }
}

export function render2D(index, x, y) {
  v = grid[floor(y * 15.99) * gw + floor(x * 15.99)]
  hsv(0.09 + v * 0.03, 0.85 - v * 0.35, v * 0.9)
}
`,
  },
  {
    name: "Soap 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Domain warping (WLED "Soap"): one noise field pushes the coordinates
// around before a second one is sampled through them. The double
// indirection is what makes it smear and swirl instead of just drift.
z = 0

export function beforeRender(delta) {
  z = (z + delta * 0.00008) % 512
  hueT = time(0.23)
}

export function render2D(index, x, y) {
  wx = simplex3(x * 1.3, y * 1.3, z, 11) * 0.5
  wy = simplex3(x * 1.3 + 5, y * 1.3 + 5, z, 12) * 0.5
  n = simplex3((x + wx) * 2, (y + wy) * 2, z * 1.7, 13)
  hsv(hueT + n * 0.18, 0.75, saturate(0.55 + n * 0.7))
}
`,
  },
  {
    name: "Spirograph 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Spirograph: two stacked rotations trace epicycles into a trail canvas.
// The gear ratio breathes slowly through non-integer values, so the
// rosette never closes — it keeps evolving instead of repeating.
gw = 16
canvas = array(gw * gw)
a1 = 0
a2 = 0

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  feedback(canvas, pow(0.975, delta * 0.06))
  ratio = 2.5 + wave(time(0.37)) * 3
  for (var s = 0; s < 4; s++) {  // substeps keep the trace a line, not dots
    a1 = mod(a1 + dt * 0.9, PI2)
    a2 = mod(a2 + dt * 0.9 * ratio, PI2)
    px = 0.5 + 0.27 * cos(a1) + 0.16 * cos(a2)
    py = 0.5 + 0.27 * sin(a1) + 0.16 * sin(a2)
    canvas[floor(py * 15.99) * gw + floor(px * 15.99)] = 1
  }
  t1 = time(0.09)
}

export function render2D(index, x, y) {
  v = canvas[floor(y * 15.99) * gw + floor(x * 15.99)]
  hsv(t1 + v * 0.1, 1 - v * v * 0.5, v * v)
}
`,
  },
  {
    name: "Reaction Diffusion 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Gray–Scott reaction–diffusion on a 16×16 virtual canvas: two chemicals
// feed, react, and diffuse; spots and worms self-organize. 16×16 is at
// the small edge of what Gray–Scott needs, so this reseeds itself
// whenever the culture dies out.
gw = 16
cells = gw * gw
A = array(cells)
B = array(cells)
A2 = array(cells)
B2 = array(cells)

f = 0.037   // feed
k = 0.06    // kill: f/k in the "mitosis" regime
seedT = 0

function seed() {
  arrayReplace(A, 1)
  arrayReplace(B, 0)
  for (var s = 0; s < 3; s++) {
    var cx = 3 + floor(random(gw - 6))
    var cy = 3 + floor(random(gw - 6))
    for (var dy = -1; dy <= 1; dy++) {
      for (var dx = -1; dx <= 1; dx++) {
        B[(cy + dy) * gw + cx + dx] = 1
      }
    }
  }
}
seed()

function step() {
  last = cells - 1
  for (var i = 0; i <= last; i++) {
    var x = i % gw
    var up = i >= gw ? i - gw : i
    var dn = i <= last - gw ? i + gw : i
    var lf = x > 0 ? i - 1 : i
    var rt = x < gw - 1 ? i + 1 : i
    var a = A[i]
    var b = B[i]
    var abb = a * b * b
    A2[i] = a + 0.9 * ((A[up] + A[dn] + A[lf] + A[rt]) * 0.25 - a) - abb + f * (1 - a)
    B2[i] = b + 0.35 * ((B[up] + B[dn] + B[lf] + B[rt]) * 0.25 - b) + abb - (f + k) * b
  }
  tmp = A
  A = A2
  A2 = tmp
  tmp = B
  B = B2
  B2 = tmp
}

export function beforeRender(delta) {
  step()
  step()
  // reseed if the culture died (all B consumed) or saturated
  seedT += delta * 0.001
  if (seedT > 3) {
    seedT = 0
    if (arraySum(B) < 0.3) seed()
  }
}

export function render2D(index, x, y) {
  b = B[floor(y * 15.99) * gw + floor(x * 15.99)]
  v = saturate(b * 2.8)
  hsv(0.52 + v * 0.35, 1 - v * 0.3, v)
}
`,
  },
  {
    name: "Edgeburst",
    layout: { kind: "strip", pixels: 60 },
    source: `// Clean-room reimplementation from a prose description of the community
// pattern "Edgeburst" (no source consulted). A rainbow front bursts from
// the center, sweeps to the ends, and collapses back. One clamped blend
// of a spatial tent and a time bounce yields position, brightness, and
// hue all at once.
export function beforeRender(delta) {
  t1 = triangle(time(0.09))  // bounce, ~6 s round trip
}

export function render(index) {
  tent = triangle(index / pixelCount)
  e = clamp(tent + t1 * 4 - 2, 0, 1)
  hsv(e * e - 0.2, 1, triangle(e))
}
`,
  },
  {
    name: "Rainbow Melt",
    layout: { kind: "strip", pixels: 60 },
    source: `// Clean-room reimplementation from a prose description of the community
// pattern "rainbow melt" (no source consulted). A mirrored rainbow that
// slowly rotates while brightness churns in fluid waves — made by nesting
// a sinusoid inside itself, each stage phase-modulated by the clock, over
// two near-but-not-equal periods so it never seems to repeat.
export function beforeRender(delta) {
  t1 = time(0.1)
  t2 = time(0.13)
}

export function render(index) {
  c = 1 - abs(2 * index / pixelCount - 1)  // 1 at center, 0 at ends
  v = wave(wave(wave(c) + t1) + t1)
  hsv(c + t2, 1, v * v)
}
`,
  },
  {
    name: "Color Twinkles",
    layout: { kind: "strip", pixels: 60 },
    source: `// Clean-room reimplementation from a prose description of the community
// pattern "color twinkles" (no source consulted). Stateless, random-free
// twinkles: two incommensurate spatial sines phase-modulate each other
// into a noise-like field; a fourth power plus a hard floor carves it
// into crisp sparks. The same field on a slower clock picks the hues.
export function beforeRender(delta) {
  tf = time(0.16) * PI2  // brightness clock, ~10 s
  ts = time(0.44) * PI2  // color clock, ~29 s
}

export function render(index) {
  b = (1 + sin(index * 0.31 + sin(index * 0.171 + tf) * PI2)) * 0.5
  b = b * b
  b = b * b  // ^4: crush the midtones, keep sparse peaks
  if (b < 0.22) b = 0  // hard floor: clean black between twinkles
  h = sin(index * 0.31 + sin(index * 0.171 + ts) * PI2)
  hsv(h, 1, b * 0.5)
}
`,
  },
  {
    name: "Thunderstorm",
    layout: { kind: "strip", pixels: 60 },
    source: `// Clean-room reimplementation from a prose description of the community
// pattern "Thunderstorm" (no source consulted), with two improvements the
// description recommended: the cloud anchors to the strip's end (not a
// hard-coded index) and timing is delta-based (not frame-counted).
// Layered overwrites, no blending: rain, ember glints, lightning, and the
// cloud painted last so flashes appear to come from behind it.
cloudSize = 8
lightning = 0
nextFlash = 2
edgeT = 0
gap = 6

export function sliderRainDensity(v) {
  gap = 2 + floor((1 - v) * pixelCount / 4)
} //# min=0 max=1 step=0.01 default=0.7

export function beforeRender(delta) {
  dt = delta * 0.001
  edgeT += dt
  if (edgeT > 0.25) {  // the cloud's edge shimmers a few times a second
    edgeT = 0
    cloudSize = floor(pixelCount / 9 + random(pixelCount / 14))
  }
  nextFlash -= dt
  if (lightning > 0) lightning -= dt
  if (nextFlash <= 0) {
    lightning = 0.12
    nextFlash = 0.4 + random(6)  // irregular gaps, constant flash length
  }
  scan = floor(time(0.09) * pixelCount)  // rain marches away from the cloud
  cloudV = 0.35 + 0.65 * min(time(0.06) * 1.5, 1)  // swell, then hold
}

export function render(index) {
  h = 0.66
  s = 1
  v = 0
  if ((index + scan) % gap == 0) v = 0.8  // deep blue drops
  if (random(1) < 0.02) {  // stray ember glints
    h = 0.07
    v = 1
  }
  if (lightning > 0) {  // whole-strip amber flash
    h = 0.09
    s = 0.55
    v = 1
  }
  if (index >= pixelCount - cloudSize) {  // white cloud, always on top
    s = 0
    v = cloudV
  }
  hsv(h, s, v)
}
`,
  },
  {
    name: "Glittering Jewels",
    layout: { kind: "strip", pixels: 60 },
    source: `// Clean-room reimplementation from a prose description of the community
// pattern "Glittering Jewels" (no source consulted). Soft gems bloom at
// random spots — the glow radius breathes with the bloom, a hot core sits
// in a wider half-strength halo, and a slow staggered clock fires narrow
// "glints" confined to each core. Overlaps composite winner-take-all so
// jewels stay crisp. The invisible tail of each lifetime doubles as a
// randomized respawn delay.
maxJewels = 20
jpos = array(maxJewels)
jhue = array(maxJewels)
jphase = array(maxJewels)
jrate = array(maxJewels)
jwid = array(maxJewels)
jvis = array(maxJewels)
jspark = array(maxJewels)

export var speed = 0.35
export function sliderSpeed(v) { speed = 0.05 + v * v * 1.5 } //# min=0 max=1 step=0.01 default=0.45
export var width = 0.09
export function sliderWidth(v) { width = 0.02 + v * 0.2 } //# min=0 max=1 step=0.01 default=0.35
sparkle = 0.6
export function sliderSparkle(v) { sparkle = v } //# min=0 max=1 step=0.01 default=0.6
count = 6
export function sliderJewels(v) { count = 1 + floor(v * (maxJewels - 1)) } //# min=1 max=20 step=1 default=6
rainbow = 0
export function toggleRainbow(v) { rainbow = v }
pickH = 0.78
export function hsvPickerColor(h, s, v) { pickH = h }

function respawn(i) {
  jpos[i] = random(1)
  jhue[i] = random(1)
  jrate[i] = 0.7 + random(0.7)
  jwid[i] = 0.7 + random(0.6)  // per-jewel width factor
  jvis[i] = 0.65 + random(0.2) // visible fraction of the lifetime
  jspark[i] = random(1)
}
for (i = 0; i < maxJewels; i++) {
  respawn(i)
  jphase[i] = random(1)  // stagger so they never pulse in unison
}

export function beforeRender(delta) {
  dt = delta * 0.001
  for (var i = 0; i < count; i++) {
    jphase[i] += dt * speed * jrate[i]
    if (jphase[i] >= 1) {
      jphase[i] -= 1
      respawn(i)
    }
  }
  glintT = time(0.1 + sparkle * 0.3)  // stronger sparkle = slower cycle
}

export function render(index) {
  x = index / (pixelCount - 1)
  bv = 0
  bh = 0
  for (var i = 0; i < count; i++) {
    p = jphase[i] / jvis[i]
    if (p < 1) {
      bloom = sin(p * PI)
      r = width * jwid[i] * (0.33 + 0.67 * bloom)
      d = abs(x - jpos[i])
      core = saturate(1 - (d * d) / (r * r))
      rh = r * 1.3
      g = max(core, saturate(1 - (d * d) / (rh * rh)) * 0.5) * bloom
      if (g > 0.01) {
        sp = 0
        if (sparkle > 0) {
          sp = pow(max(sin((glintT + jspark[i]) * PI2), 0), 24)
          sp *= core * core * core * sparkle * 2
        }
        b = g * (1 + sp)
        if (b > bv) {
          bv = b
          bh = (rainbow ? jhue[i] : pickH) + sp * 0.04
        }
      }
    }
  }
  hsv(bh, 0.85, min(bv, 1))
}
`,
  },
  {
    name: "Doom Fire 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Clean-room reimplementation from a prose description of the community
// pattern "Doom Fire (v2.0) 2D" (no source consulted) — itself the PSX
// Doom fire: pure cellular propagation, no noise fields. Heat rises from
// a hidden source row, cooled by a random draw that's harshest near the
// bottom (survivors get carried high). Guard columns skip bounds checks;
// the sim is double-buffered and runs on its own clock, decoupled from
// the render rate. Dragon mode makes the whole bed exhale in surges.
gw = 16
rows = 16
sw = gw + 2           // guard column each side
cells = sw * (rows + 1)  // +1 hidden source row at the bottom
A = array(cells)
B = array(cells)
wind = 0
stepT = 0

baseH = 0.01
bright = 1
export function hsvPickerColor(h, s, v) { baseH = h; bright = v }
flame = 0.75
export function sliderFlameHeight(v) { flame = v } //# min=0 max=1 step=0.01 default=0.75
windAmt = 0.3
export function sliderWind(v) { windAmt = v } //# min=0 max=1 step=0.01 default=0.3
stepMs = 40
export function sliderSpeed(v) { stepMs = 15 + (1 - v) * 120 } //# min=0 max=1 step=0.01 default=0.8
dragon = 0
export function toggleDragonBreath(v) { dragon = v }

function step() {
  tmp = A
  A = B
  B = tmp
  if (random(1) < windAmt * windAmt) wind = floor(random(3)) - 1
  coolMax = 0.035 + (1 - flame) * (1 - flame) * 0.45
  for (var y = 0; y < rows; y++) {
    bend = 1 - abs(y / rows - 0.5) * 2  // wind bites hardest mid-flame
    cool = coolMax * (0.35 + 0.65 * (y + 1) / rows)  // harsh low, gentle high
    for (var x = 1; x <= gw; x++) {
      var sx = x
      if (wind != 0 && random(1) < bend * 0.7) sx = x + wind
      v = B[(y + 1) * sw + sx] - random(cool)
      A[y * sw + x] = max(v, 0)
    }
  }
  // refresh the hidden source row
  base = (rows) * sw
  if (dragon) {
    pulse = pow(wave(time(0.06)), 2)  // the exhale, ~4 s cycle
    for (var x = 1; x <= gw; x++) {
      A[base + x] = pulse * (0.65 + 0.35 * wave(x / gw * 2))
    }
  } else {
    ph = triangle(time(0.3)) * 3  // bed shimmer drifts over ~20 s
    for (var x = 1; x <= gw; x++) {
      A[base + x] = 0.82 + 0.18 * wave(x * 0.13 + ph)
    }
  }
}

export function beforeRender(delta) {
  stepT += min(delta, 100)
  while (stepT >= stepMs) {
    stepT -= stepMs
    step()
  }
}

export function render2D(index, x, y) {
  heat = A[floor(y * 15.99) * sw + 1 + floor(x * 15.99)]
  v = heat * heat * heat * bright
  hsv(baseH + heat * 0.06, 1 - saturate(heat - 0.75) * 1.2, min(v, 1))
}
`,
  },
  {
    name: "Voronoi 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Clean-room reimplementation from a prose description of the community
// pattern "Voronoi Mix 2D" (no source consulted). Bouncing seed points
// partition the panel by nearest-seed; swapping the distance metric and
// the draw mode morphs stained-glass cells into orbs, rings, and bands.
maxP = 8
px = array(maxP)
py = array(maxP)
vx = array(maxP)
vy = array(maxP)

count = 5
export function sliderPoints(v) { count = 1 + floor(v * (maxP - 1)) } //# min=1 max=8 step=1 default=5
metric = 0
export function sliderMetric(v) { metric = floor(v * 3.99) } //# min=0 max=3 step=1 default=0
mode = 1
export function sliderStyle(v) { mode = floor(v * 3.99) } //# min=0 max=3 step=1 default=1
export function sliderSpeed(v) {
  for (var i = 0; i < maxP; i++) {
    px[i] = random(1)
    py[i] = random(1)
    vx[i] = (random(1) - 0.5) * (0.05 + v * 0.4)
    vy[i] = (random(1) - 0.5) * (0.05 + v * 0.4)
  }
} //# min=0 max=1 step=0.01 default=0.3
sliderSpeed(0.3)

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  for (var i = 0; i < count; i++) {
    px[i] += vx[i] * dt
    py[i] += vy[i] * dt
    if (px[i] < 0) { px[i] = 0; vx[i] = -vx[i] }
    if (px[i] > 1) { px[i] = 1; vx[i] = -vx[i] }
    if (py[i] < 0) { py[i] = 0; vy[i] = -vy[i] }
    if (py[i] > 1) { py[i] = 1; vy[i] = -vy[i] }
  }
}

export function render2D(index, x, y) {
  bd = 9
  bi = 0
  for (var i = 0; i < count; i++) {
    dx = x - px[i]
    dy = y - py[i]
    if (metric == 0) d = hypot(dx, dy)                 // classic cells
    else if (metric == 1) d = wave((dx * dx + dy * dy) * 5)  // rings
    else if (metric == 2) d = max(abs(dx), abs(dy))    // chessboard
    else d = abs(dx + dy) * 0.7                        // diagonal bands
    if (d < bd) {
      bd = d
      bi = i
    }
  }
  h = bi / count
  bd = saturate(bd)
  if (mode == 0) hsv(h, 1, 1)                          // flat cells
  else if (mode == 1) {                                // glowing orbs
    v = 1 - bd
    hsv(h, 1, v * v * v)
  } else if (mode == 2) hsv(h + bd, 1, 1 - bd)         // rainbow fringe
  else {                                               // far-field glow
    v = bd * bd
    hsv(h + bd, 1, v * v)
  }
}
`,
  },
  {
    name: "Kaleidoscope 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Clean-room reimplementation from a prose description of the community
// pattern "Perlin Kaleidoscope 2D" (no source consulted). Three
// noise-displaced lines — one per additive primary — get folded into
// mirrored pie wedges (the abs() against the wedge bisector is what makes
// true mirrors instead of rotated copies) and the whole thing spins.
zt = 0
rot = 0

slices = 5
export function sliderSlices(v) { slices = 1 + floor(v * 6) } //# min=1 max=7 step=1 default=5
spd = 0.5
export function sliderSpeed(v) { spd = 0.1 + v * v * 2 } //# min=0 max=1 step=0.01 default=0.5
lineW = 0.14
export function sliderLineWidth(v) { lineW = 0.05 + v * 0.3 } //# min=0 max=1 step=0.01 default=0.3

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  zt += dt * 0.1 * spd
  if (zt > 512) zt -= 512
  rot = mod(rot + dt * 0.4 * spd, PI2)
}

export function render2D(index, x, y) {
  dx = x - 0.5
  dy = y - 0.5
  if (slices > 1) {
    r = hypot(dx, dy)
    seg = PI2 / slices
    a = abs(mod(atan2(dy, dx), seg) - seg / 2) + rot  // mirror fold + spin
    dx = cos(a) * r
    dy = sin(a) * r
  }
  u = dx + 0.5
  w = dy + 0.5
  // three independently wandering lines, one per channel
  ry = 0.5 + simplex3(u * 1.2, zt, 0, 31) * 0.35
  gy = 0.5 + simplex3(zt, u * 1.7, 1, 32) * 0.35
  by = 0.5 + simplex3(u * 0.9, 3, zt * 1.4, 33) * 0.3
  rgb(
    saturate((lineW - abs(w - ry)) / lineW * 1.4),
    saturate((lineW - abs(w - gy)) / lineW * 1.4),
    saturate((lineW - abs(w - by)) / lineW * 1.4)
  )
}
`,
  },
  {
    name: "Coronal Ejection 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Clean-room reimplementation from a prose description of the community
// pattern "Coronal Mass Ejection 2D" (no source consulted). A white-hot
// core with turbulence-carved flares streaming outward. The noise wrap
// period is matched to the angular scale so the texture tiles seamlessly
// around the circle; a smooth threshold carves the field into discrete
// tongues; saturation grows with radius so the core whitens for free.
rDrift = 0
zt = 0

density = 6
export function sliderDensity(v) { density = 1 + floor(v * 11) } //# min=1 max=12 step=1 default=6
export function showNumberDensity() { return density }
mirror = 0
export function toggleMirror(v) { mirror = v }
cutoff = 0.35
export function sliderCutoff(v) { cutoff = 0.15 + v * 0.55 } //# min=0 max=1 step=0.01 default=0.35

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  hueT = time(0.2)          // full color-wheel lap, ~13 s
  rDrift += dt * 0.25       // flares stream outward
  if (rDrift > 512) rDrift -= 512
  zt += dt * 0.03
  if (zt > 256) zt -= 256
  lobes = density * (mirror ? 2 : 1)
  setPerlinWrap(lobes, 0, 0)
}

export function render2D(index, x, y) {
  dx = x - 0.5
  dy = y - 0.5
  r = hypot(dx, dy)
  a = (atan2(dy, dx) / PI2 + 0.5) * lobes  // spans the wrap period exactly
  n = 1 - perlinTurbulence(a, r * 3 - rDrift, zt, 2, 0.55, 3)
  flare = smoothstep(cutoff, 0.9, n) * saturate(1.3 - r * 1.9)
  core = saturate(1.3 - r * 8 * (0.35 + n * 0.5))  // edge nibbled by turbulence
  v = max(flare, core)
  v = v * v
  hsv(hueT - v * 0.05, saturate(r * 4.5 - v * 0.6), v)
}
`,
  },
  {
    name: "Unstable Orbits 2D",
    layout: { kind: "grid", w: 16, h: 16 },
    source: `// Clean-room reimplementation from a prose description of the community
// pattern "Unstable Orbits" (no source consulted), plotting into an
// explicit canvas instead of trusting pixel-index order. Lissajous dots
// whose vertical clock is a wave *of* the horizontal clock, so the
// orbits stretch and tumble forever; reciprocal per-dot phase offsets
// cluster the swarm organically instead of spacing it evenly.
gw = 16
n = 24
vbuf = array(gw * gw)
hbuf = array(gw * gw)

export function beforeRender(delta) {
  feedback(vbuf, pow(0.85, delta * 0.06))  // brightness-only trail fade
  t1 = time(0.025)          // primary clock, ~1.6 s
  t2 = wave(t1) * 1.5       // nested clock: speeds up and slows down
  for (var i = 0; i < n; i++) {
    off = 1 / (i + 1)       // reciprocal spacing → dense head, stragglers
    xx = 0.5 + 0.44 * sin((t1 + off) * PI2)
    yy = 0.5 + 0.44 * sin((t2 + off) * PI2)
    idx = floor(yy * 15.99) * gw + floor(xx * 15.99)
    vbuf[idx] = 1
    hbuf[idx] = i / n       // rainbow by rank
  }
}

export function render2D(index, x, y) {
  idx = floor(y * 15.99) * gw + floor(x * 15.99)
  v = vbuf[idx]
  hsv(hbuf[idx], 1, v * v)
}
`,
  },
];
