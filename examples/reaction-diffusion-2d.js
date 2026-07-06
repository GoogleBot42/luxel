// Gray–Scott reaction–diffusion on a 16×16 virtual canvas: two chemicals
// feed, react, and diffuse; spots and worms self-organize. 16×16 is at
// the small edge of what Gray–Scott needs, so this reseeds itself
// whenever the culture dies out.
gw = 16
cells = gw * gw
A = array(cells)
B = array(cells)
A2 = array(cells)
B2 = array(cells)

f = 0.037   // feed
k = 0.06    // kill: f/k in the "mitosis" regime
seedT = 0

function seed() {
  arrayReplace(A, 1)
  arrayReplace(B, 0)
  for (var s = 0; s < 3; s++) {
    var cx = 3 + floor(random(gw - 6))
    var cy = 3 + floor(random(gw - 6))
    for (var dy = -1; dy <= 1; dy++) {
      for (var dx = -1; dx <= 1; dx++) {
        B[(cy + dy) * gw + cx + dx] = 1
      }
    }
  }
}
seed()

function step() {
  last = cells - 1
  for (var i = 0; i <= last; i++) {
    var x = i % gw
    var up = i >= gw ? i - gw : i
    var dn = i <= last - gw ? i + gw : i
    var lf = x > 0 ? i - 1 : i
    var rt = x < gw - 1 ? i + 1 : i
    var a = A[i]
    var b = B[i]
    var abb = a * b * b
    A2[i] = a + 0.9 * ((A[up] + A[dn] + A[lf] + A[rt]) * 0.25 - a) - abb + f * (1 - a)
    B2[i] = b + 0.35 * ((B[up] + B[dn] + B[lf] + B[rt]) * 0.25 - b) + abb - (f + k) * b
  }
  tmp = A
  A = A2
  A2 = tmp
  tmp = B
  B = B2
  B2 = tmp
}

export function beforeRender(delta) {
  step()
  step()
  // reseed if the culture died (all B consumed) or saturated
  seedT += delta * 0.001
  if (seedT > 3) {
    seedT = 0
    if (arraySum(B) < 0.3) seed()
  }
}

export function render2D(index, x, y) {
  b = B[floor(y * 15.99) * gw + floor(x * 15.99)]
  v = saturate(b * 2.8)
  hsv(0.52 + v * 0.35, 1 - v * 0.3, v)
}
