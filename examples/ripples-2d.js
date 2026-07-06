// Rain on a pond: three expanding rings, each a pure function of dist()
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
