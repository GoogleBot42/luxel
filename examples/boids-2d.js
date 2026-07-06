// Flocking: cohesion, alignment, and separation over parallel state
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
