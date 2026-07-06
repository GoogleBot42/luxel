// Clean-room reimplementation from a prose description of the community
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
