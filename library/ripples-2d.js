// name: Ripples 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// Rain on a pond: three expanding rings, each a pure function of dist()
// from its drop point. Drops reposition themselves when their ring fades
// out. REAL drops arrive as injected events (click/drag the preview, or
// POST /api/events / the MQTT event topic — [type, x, y, value]) and land
// exactly where you poked; the trigger button splashes one at random.
numDrops = 3
cx = array(numDrops)
cy = array(numDrops)
ph = array(numDrops)
ev = array(4)

for (i = 0; i < numDrops; i++) {
  cx[i] = random(1)
  cy[i] = random(1)
  ph[i] = random(1)
}

// Restart the ring that has expanded furthest (the least-missed one).
function splash(x0, y0) {
  var best = 0
  for (var i = 1; i < numDrops; i++) {
    if (ph[i] > ph[best]) best = i
  }
  cx[best] = x0
  cy[best] = y0
  ph[best] = 0
}

export function triggerSplash() { splash(random(1), random(1)) }

export function beforeRender(delta) {
  while (readEvent(ev)) splash(ev[1], ev[2])
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
