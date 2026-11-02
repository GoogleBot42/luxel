// name: Life
// tags: panel, ambient
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/cyclic-cellular-automata-2d.md
// — reimagined, not a replica (the Cyclic rule); the rest is a curated original.
// Conway's Life and four cousins on a torus sized to the panel (≤ 64 cells a
// side; bigger panels get 2x2-pixel cells and no ghosts). Newborns flash
// white and age through a hue ramp; the dead leave fading ghosts on the
// persistent frame, so gliders draw comet tails. A repeating population
// hash or a flat population starts a gentle reseed rain.
// Live cells are stored as 8 + hue, so one stencil2D pass gives every
// neighbour count (floor(sum / 8)) and the same array blits as the colour.

var CAP = 64
var st = array((CAP + 2) * (CAP + 2))   // cell state, with a 1-cell wrap halo
var cnt = array((CAP + 2) * (CAP + 2))  // neighbour sums
var born = array(9)
var surv = array(9)
var hp = array(8)                       // recent (population, signature) pairs
var hs = array(8)

var W = 32
var H = 32
var W2 = 34
var H2 = 34
var gwSeen = -1
var native = 1        // grid <= CAP: blit 1:1 over the persistent frame
var kind = 0          // 0 birth/survival, 1 Brian's Brain, 2 cyclic medium
var kd = 1            // diagonal weight: 1 Moore, 0 von Neumann
var h0 = 0.47         // newborn hue
var hStep = 0.02      // hue added per generation survived
var hEnd = 8.77       // the ramp stops here
var sat = 0.8
var dens = 0.3
var pop = 0
var sig = 0
var gen = 0
var stale = 0
var popLo = 0
var popHi = 0
var rain = 0
var acc = 0
var due = 0
var fadeK = 0.9
var hNow = 0
var REF = 12 / 4096

var rule = 0
//# min=0 max=4 step=1 default=0
export function sliderRule(v) {
  rule = clamp(floor(v), 0, 4)
  gwSeen = -1                            // re-seed under the new rule
}

var stepsPerSec = 10
//# min=2 max=30 step=1 default=10
export function sliderStepsPerSecond(v) { stepsPerSec = clamp(v, 2, 30) }

var ghostSec = 0.8
//# min=0 max=3 step=0.1 default=0.8
export function sliderGhostSeconds(v) { ghostSec = clamp(v, 0, 3) }

function setRule() {
  for (var n = 0; n < 9; n++) { born[n] = 0; surv[n] = 0 }
  kind = 0
  kd = 1
  sat = 0.8
  dens = 0.3
  if (rule == 0) {                        // Life B3/S23: cyan -> violet
    born[3] = 1; surv[2] = 1; surv[3] = 1
    h0 = 0.47; hStep = 0.02; hEnd = 0.77
  } else if (rule == 1) {                 // HighLife B36/S23: gold -> ember
    born[3] = 1; born[6] = 1; surv[2] = 1; surv[3] = 1
    h0 = 0.15; hStep = -0.006; hEnd = 0.01; sat = 0.9
  } else if (rule == 2) {                 // Day & Night B3678/S34678: lime -> teal
    born[3] = 1; born[6] = 1; born[7] = 1; born[8] = 1
    surv[3] = 1; surv[4] = 1; surv[6] = 1; surv[7] = 1; surv[8] = 1
    h0 = 0.25; hStep = 0.004; hEnd = 0.5; dens = 0.5
  } else if (rule == 3) {                 // Brian's Brain: fire, rest, wake
    born[2] = 1
    kind = 1; h0 = 0.55; sat = 0.7; dens = 0.25
  } else {                                // Greenberg-Hastings excitable medium
    kind = 2; kd = 0; h0 = 0.08; sat = 0.9
  }
  hEnd += 8
}

function cellAt(c, r) { return (mod(r, H) + 1) * W2 + mod(c, W) + 1 }

// one rain drop: a small random soup, or for the medium a broken wavefront
// (an excited bar over a refractory bar curls into a spiral pair)
function drop() {
  var cx = floor(random(W))
  var cy = floor(random(H))
  if (kind == 2) {
    for (var k = 0; k < 10; k++) {
      st[cellAt(cx + k, cy)] = 8 + hNow
      st[cellAt(cx + k, cy + 1)] = REF
    }
    return
  }
  for (var dy = 0; dy < 6; dy++) {
    for (var dx = 0; dx < 6; dx++) {
      if (random(1) < 0.45) st[cellAt(cx + dx, cy + dy)] = 8 + h0
    }
  }
}

function setup() {
  var gw = gridWidth()
  gwSeen = gw
  native = gw > 0 && gw <= CAP && gridHeight() <= CAP
  // a grid wider than CAP gets an integer block size, so cells stay square
  var k = gw > 0 ? ceil(max(gw, gridHeight()) / CAP) : 1
  W = gw > 0 ? floor(gw / k) : 32
  H = gw > 0 ? floor(gridHeight() / k) : 32
  if (!native) { W -= 2; H -= 2 }         // the halo is on screen there
  W2 = W + 2
  H2 = H + 2
  setRule()
  feedback(st, 0)
  for (var r = 0; r < H; r++) {
    for (var c = 0; c < W; c++) {
      var i = cellAt(c, r)
      if (kind == 2) {
        var p = random(1)
        if (p < 0.08) st[i] = 8 + hNow
        else st[i] = (1 + floor(random(12))) / 4096
      } else if (random(1) < dens) st[i] = 8 + h0
    }
  }
  clear()
  stale = 0
  rain = 0
  gen = 0
  popLo = 30000
  popHi = 0
}

function wrapHalo() {
  var a = H * W2
  var b = (H + 1) * W2
  for (var c = 1; c <= W; c++) { st[c] = st[a + c]; st[b + c] = st[W2 + c] }
  for (var r = 0; r < H2; r++) {
    a = r * W2
    st[a] = st[a + W]
    st[a + W + 1] = st[a + 1]
  }
}

function stepLife() {
  for (var r = 1; r <= H; r++) {
    var i = r * W2 + 1
    for (var c = 0; c < W; c++) {
      var s = st[i]
      var n = floor(cnt[i] * 0.125)
      if (s >= 8) {
        if (surv[n]) {
          pop++
          sig += i
          if ((hEnd - s) * hStep > 0) st[i] = s + hStep
        } else st[i] = 0
      } else if (born[n]) {
        st[i] = 8 + h0
        pop++
        sig += i
        setPixel(gridIndex(c, r - 1))
      }
      i++
    }
  }
}

// firing -> refractory -> off; off fires on exactly two firing neighbours
function stepBrain() {
  for (var r = 1; r <= H; r++) {
    var i = r * W2 + 1
    for (var c = 0; c < W; c++) {
      var s = st[i]
      if (s >= 8) st[i] = 0.66
      else if (s > 0) st[i] = 0
      else if (floor(cnt[i] * 0.125) == 2) {
        st[i] = 8 + h0
        pop++
        sig += i
        setPixel(gridIndex(c, r - 1))
      }
      i++
    }
  }
}

// excited -> 12 refractory steps -> rest. Refractory cells hold a near-zero
// value, so the wake on screen is the front's own ghost fading behind it.
function stepMedium() {
  for (var r = 1; r <= H; r++) {
    var i = r * W2 + 1
    for (var c = 0; c < W; c++) {
      var s = st[i]
      if (s >= 8) st[i] = REF
      else if (s > 0) st[i] = s - 1 / 4096
      else if (cnt[i] >= 8) {
        st[i] = 8 + hNow
        pop++
        setPixel(gridIndex(c, r - 1))
      }
      i++
    }
  }
}

function generation() {
  wrapHalo()
  feedback(cnt, 0)
  stencil2D(cnt, st, W2, H2, 0, 1, kd)
  pop = 0
  sig = 0
  rgb(1, 1, 1)                            // the newborn flash
  if (kind == 0) stepLife()
  else if (kind == 1) stepBrain()
  else {
    hsv(hNow, sat, 1)                     // the medium's fronts carry their hue
    stepMedium()
  }
  gen = gen >= 32000 ? 1 : gen + 1

  // stagnation: the same (population, signature) as 1..6 generations ago
  var hit = 0
  for (var k = 1; k <= 6; k++) {
    var j = (gen - k + 8) % 8
    if (hp[j] == pop && hs[j] == sig) hit = 1
  }
  hp[gen % 8] = pop
  hs[gen % 8] = sig
  stale = hit && kind != 2 ? stale + 1 : 0
  popLo = min(popLo, pop)
  popHi = max(popHi, pop)
  if (gen % 64 == 0) {
    // a population that barely moved in 64 generations is ash and gliders
    if (kind != 2 && popHi - popLo <= 3 + popHi * 0.04) rain = 3 + floor(W * H / 300)
    popLo = 30000
    popHi = 0
  }
  if (stale > 8 || pop < W * H / 200) { rain = max(rain, 3 + floor(W * H / 300)); stale = 0 }
  if (rain > 0 && gen % 3 == 0) { drop(); rain-- }
}

export function beforeRender(delta) {
  acc += delta
  var tick = 1000 / stepsPerSec
  due = 0
  while (acc >= tick && due < 3) { acc -= tick; due++ }
  if (acc > tick) acc = tick
  hNow = time(0.9)                        // each new wave of the medium drifts in hue
  fadeK = ghostSec > 0 ? pow(0.02, delta / (ghostSec * 1000)) : 0
}

export function renderBulk() {
  if (gridWidth() != gwSeen) setup()
  if (native) fade(fadeK)
  for (var k = 0; k < due; k++) generation()
  // hue = the fractional part, value saturates at 1 for a live cell
  if (native) blit(st, sat, st, W2, H2, -1, -1, 2)
  else fillCanvas(st, sat, st, W2, H2)
}
