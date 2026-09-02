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
zoom = 2.6
flow = 0.09
fade = 0.95
horizon = 0
t1 = 0

arrayMutate(tint, (v) => 0.64)  // unlit cells share the band, so bilinear edges do not smear toward red

for (i = 0; i < np; i++) {
  px[i] = random(1)
  py[i] = random(1)
  ph[i] = 0.52 + random(0.24)  // one narrow hue band: bilinear blends stay clean
}

//# min=0 max=1 default=0.4
export function sliderFlow(v) { flow = 0.02 + v * 0.18 }

//# min=0 max=1 default=0.5
export function sliderTrails(v) { fade = 0.9 + v * 0.09 }

//# min=0 max=1 default=0.4
export function sliderScale(v) { zoom = 1.2 + v * 3.5 }

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  drift += dt * 0.05  // slide the potential: the whole field flows past
  if (drift > 512) drift -= 512
  t1 = time(0.4)
  horizon = wave(time(0.13))
  feedback(canvas, pow(fade, delta * 0.06))
  for (var i = 0; i < np; i++) {
    // curl2 writes the vector into vel and returns it; components are
    // noise DERIVATIVES, so they run to roughly +/-6 — hence the small flow
    curl2(px[i] * zoom + drift, py[i] * zoom, vel, 9)
    px[i] = mod(px[i] + vel[0] * flow * dt, 1)
    py[i] = mod(py[i] + vel[1] * flow * dt, 1)
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
