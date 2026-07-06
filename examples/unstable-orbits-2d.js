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
