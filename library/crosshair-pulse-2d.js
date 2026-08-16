// name: Crosshair Pulse 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// A keyboard idea (QMK "reactive nexus"): each hit fires four pulses
// racing outward along the hit's row and column, plus a center flash.
// REAL hits arrive as injected events (click/drag the preview, or POST
// /api/events — [type, x, y, value]); a phantom generator fills the idle
// time and pauses whenever real input is flowing. The trigger lands one
// dead-center.
m = 4
ex = array(m)
ey = array(m)
age = array(m)
ehue = array(m)
ev = array(4)
quiet = 0  // seconds of phantom silence left after real input

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
  while (readEvent(ev)) {
    spawn(ev[1], ev[2])
    quiet = 4
  }
  quiet -= dt
  if (quiet <= 0 && random(1) < dt * 2.5) spawn(random(1), random(1))
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
