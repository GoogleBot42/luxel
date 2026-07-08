// name: Radar 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// Radar: no buffer needed — phosphor persistence is just "how long ago
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
