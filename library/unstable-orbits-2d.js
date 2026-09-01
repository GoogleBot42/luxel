// name: Unstable Orbits 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// Clean-room reimplementation from a prose description of the community
// pattern "Unstable Orbits" (no source consulted), plotting into an
// explicit canvas instead of trusting pixel-index order. Lissajous dots
// whose vertical clock is a wave *of* the horizontal clock, so the
// orbits stretch and tumble forever; reciprocal per-dot phase offsets
// cluster the swarm organically instead of spacing it evenly.
gw = 16
n = 24
vbuf = array(gw * gw)
hbuf = array(gw * gw)

rate = 0.025    // primary clock period, in time() units (~1.6 s)
tumble = 1.5    // depth of the nested vertical clock
persist = 0.85  // trail brightness kept per frame-ish tick

export function beforeRender(delta) {
  feedback(vbuf, pow(persist, delta * 0.06))  // brightness-only trail fade
  t1 = time(rate)           // primary clock, ~1.6 s
  t2 = wave(t1) * tumble    // nested clock: speeds up and slows down
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

// How many dots orbit at once. They are spaced by reciprocal phase offsets, so
// extra dots pile into the head of the swarm rather than spreading it out.
//# min=1 max=48 step=1 default=24
export function sliderDots(v) {
  n = clamp(floor(v), 1, 48)
}

// Orbit speed: 1 is the natural ~1.6-second lap, 4 is four times as fast.
//# min=0.1 max=4 step=0.05 default=1
export function sliderSpeed(v) {
  rate = 0.025 / clamp(v, 0.1, 4)
}

// Depth of the nested vertical clock. 1 traces a plain figure-eight; higher
// values wind the vertical orbit faster than the horizontal one, so the shape
// tumbles and never repeats the same way twice.
//# min=0.25 max=4 step=0.05 default=1.5
export function sliderTumble(v) {
  tumble = clamp(v, 0.25, 4)
}

// Fraction of a dot's brightness kept each tick — low values give bare dots,
// high values smear them into long comet trails.
//# min=0.5 max=0.98 step=0.01 default=0.85
export function sliderTrail(v) {
  persist = clamp(v, 0.5, 0.98)
}
