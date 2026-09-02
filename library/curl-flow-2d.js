// name: Curl Flow 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// Specks advected by curl2(): the curl of a simplex potential is
// divergence-free, so the flow has no sources or sinks and the specks keep
// circulating instead of collecting in the noise's peaks the way a plain
// "sample noise, read it as a heading" field makes them. Trails live on a
// 16x16 canvas that render2D reads bilinearly, and the whole field slides
// sideways so the streamlines never settle. The 1D fallback flies a
// horizon line across the same canvas, so a bare strip still shows flow.
gw = 16
np = 24
px = array(np)
py = array(np)
ph = array(np)
vel = array(2)
canvas = array(gw * gw)
tint = array(gw * gw)
drift = 0
swirls = 2.5     // vortices across the display (the field's spatial period)
speed = 0.25     // speck travel, display widths per second
trail = 0.25     // trail half-life, seconds
horizon = 0
t1 = 0

arrayMutate(tint, (v) => 0.64)  // unlit cells share the band, so bilinear edges do not smear toward red

for (i = 0; i < np; i++) {
  px[i] = random(1)
  py[i] = random(1)
  ph[i] = 0.52 + random(0.24)  // one narrow hue band: bilinear blends stay clean
}

// Real units throughout, so each default= IS the constant above it:
// Speed = display widths/second, Trail = half-life in seconds, Swirls =
// vortices across the display.
//# min=0.05 max=1.5 step=0.05 default=0.25
export function sliderSpeed(v) { speed = clamp(v, 0.01, 3) }

//# min=0.05 max=1.5 step=0.05 default=0.25
export function sliderTrail(v) { trail = clamp(v, 0.02, 3) }

//# min=1 max=6 step=0.5 default=2.5
export function sliderSwirls(v) { swirls = clamp(v, 0.5, 12) }

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  drift += dt * 0.05  // slide the potential: the whole field flows past
  if (drift > 512) drift -= 512
  t1 = time(0.4)
  horizon = wave(time(0.13))
  // half-life in seconds -> per-frame decay, delta-correct at any frame rate
  feedback(canvas, pow(0.5, dt / trail))
  // curl2's mean vector length is ~2.7 (docs/lang.md), so dividing by it
  // turns the dial's display-widths-per-second into the advection gain
  step = speed * dt / 2.7
  for (var i = 0; i < np; i++) {
    // curl2 writes the vector into vel and returns it; components are
    // noise DERIVATIVES running to roughly +/-6, not a unit heading
    curl2(px[i] * swirls + drift, py[i] * swirls, vel, 9)
    px[i] = mod(px[i] + vel[0] * step, 1)
    py[i] = mod(py[i] + vel[1] * step, 1)
    canvasAdd(canvas, gw, px[i], py[i], 0.8)
    canvasSet(tint, gw, px[i], py[i], ph[i])
  }
}

function shade(x, y) {
  d = min(canvasGet(canvas, gw, x, y), 1)
  hsv(canvasGet(tint, gw, x, y) + t1 * 0.15, 1 - d * 0.5, d * d)
}

export function render2D(index, x, y) {
  shade(x, y)
}

export function render(index) {
  shade(index / pixelCount, horizon)
}
