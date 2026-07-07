// name: amoeba
// Clean-room reimplementation from a prose functional description of the
// community pattern "amoeba"; original source never consulted.

// A dim purple strip over which fuzzy blue blobs drift both ways, while
// brief dark "dimples" pop in and out at random spots. Three particle
// pools (rightward drifters, leftward drifters, stationary dimples) each
// add half-sine bumps with triangle lifetime envelopes into per-pixel
// fields; the fields become color in beforeRender and render just reads.

var SLOTS = 8        // max concurrent particles per pool
var BUMPW = 0.2      // bump width as a fraction of the strip
var HALFW = BUMPW / 2

// pool storage: position, velocity, age, lifetime, live flag
var rPos = array(SLOTS)
var rVel = array(SLOTS)
var rAge = array(SLOTS)
var rLife = array(SLOTS)
var rLive = array(SLOTS)

var lPos = array(SLOTS)
var lVel = array(SLOTS)
var lAge = array(SLOTS)
var lLife = array(SLOTS)
var lLive = array(SLOTS)

var sPos = array(SLOTS)
var sVel = array(SLOTS)
var sAge = array(SLOTS)
var sLife = array(SLOTS)
var sLive = array(SLOTS)

// per-pixel scalar fields, rebuilt every frame
var flies = array(pixelCount)   // drifter blobs (both directions summed)
var dimples = array(pixelCount) // star/dimple darkening field

var clock = 0        // integrated wall-clock seconds
var nextRight = 0
var nextLeft = 0.4
var nextStar = 0

// spawn into the first free slot; returns 1 on success
function spawn(pos, vel, age, life, live, v, lf) {
  var i
  for (i = 0; i < SLOTS; i++) {
    if (!live[i]) {
      pos[i] = random(1)
      vel[i] = v
      age[i] = 0
      life[i] = lf
      live[i] = 1
      return 1
    }
  }
  return 0
}

// age, move, and retire particles (every live slot, regardless of order)
function updatePool(pos, vel, age, life, live, dt) {
  var i
  for (i = 0; i < SLOTS; i++) {
    if (!live[i]) continue
    age[i] += dt
    pos[i] += vel[i] * dt
    if (age[i] >= life[i]) live[i] = 0
    if (pos[i] + HALFW < 0 || pos[i] - HALFW > 1) live[i] = 0  // fully off-strip
  }
}

// add each live particle's bump (half-sine arch x triangle envelope)
function drawPool(pos, age, life, live, field) {
  var i, j, p, env, lo, hi, dx
  for (i = 0; i < SLOTS; i++) {
    if (!live[i]) continue
    env = 1 - abs(2 * age[i] / life[i] - 1)   // fade in, peak mid-life, fade out
    p = pos[i]
    lo = max(0, ceil((p - HALFW) * pixelCount))
    hi = min(pixelCount - 1, floor((p + HALFW) * pixelCount))
    for (j = lo; j <= hi; j++) {
      dx = j / pixelCount - p
      if (abs(dx) < HALFW) {
        field[j] += env * sin(PI * (dx / BUMPW + 0.5))
      }
    }
  }
}

export function beforeRender(delta) {
  var dt = delta / 1000
  clock += dt

  // drifters: ~1/s per direction, jittered ~20%; wait for a free slot
  if (clock >= nextRight) {
    if (spawn(rPos, rVel, rAge, rLife, rLive, 0.02 + random(0.04), 3 + random(2))) {
      nextRight = clock + 0.8 + random(0.4)
    }
  }
  if (clock >= nextLeft) {
    if (spawn(lPos, lVel, lAge, lLife, lLive, -0.02 - random(0.04), 3 + random(2))) {
      nextLeft = clock + 0.8 + random(0.4)
    }
  }
  // dimples: brisk fixed cadence, short-lived, stationary
  if (clock >= nextStar) {
    spawn(sPos, sVel, sAge, sLife, sLive, 0, 0.8 + random(0.4))
    nextStar = clock + 0.22
  }

  updatePool(rPos, rVel, rAge, rLife, rLive, dt)
  updatePool(lPos, lVel, lAge, lLife, lLive, dt)
  updatePool(sPos, sVel, sAge, sLife, sLive, dt)

  arrayReplace(flies, 0)
  arrayReplace(dimples, 0)
  drawPool(rPos, rAge, rLife, rLive, flies)
  drawPool(lPos, lAge, lLife, lLive, flies)
  drawPool(sPos, sAge, sLife, sLive, dimples)
}

export function render(index) {
  // two-stop gradient: half purple at 0 -> pure full blue at 1+
  var f = clamp(flies[index], 0, 1)
  var r = 0.5 * (1 - f)
  var b = 0.5 + 0.5 * f

  // dimples cap brightness (per-channel min against a scalar mask)
  var mask = 1 - 0.85 * clamp(dimples[index], 0, 1)
  r = min(r, mask)
  b = min(b, mask)

  rgb(r * r, 0, b * b)   // squaring gamma per channel
}
