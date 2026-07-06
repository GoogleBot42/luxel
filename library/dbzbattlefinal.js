// name: DBZBattleFinal
// Clean-room reimplementation from a prose functional description of the
// community pattern "DBZBattleFinal"; original source never consulted.

// An endless anime energy battle on a 2D panel: a warm fighter on the left
// and a cool fighter on the right dart at each other, clash in white
// shockwave rings, occasionally retreat to fire cyan projectiles, and after
// a random number of clashes both fire at once (the finale), get blown to
// their walls, recover, and start a fresh round. All motion integrates the
// real frame delta; vertical bob is a direct sine of total time so it stays
// smooth while horizontal motion is chaotic.

// ---- controls ----
var speedMul = 0.8       // ~5:1 range, never zero
var sizeMul = 1.25       // ~4:1 range
var hue1 = 0.09          // warm fighter: red..yellow
var hue2 = 0.8           // cool fighter: violet..magenta

//# min=0 max=1 step=0.01 default=0.55
export function sliderSpeed(v) { speedMul = 0.25 + v }
//# min=0 max=1 step=0.01 default=0.5
export function sliderScale(v) { sizeMul = 0.5 + v * 1.5 }
//# min=0 max=1 step=0.01 default=0.55
export function sliderSaiyanColor(v) { hue1 = v * 0.16 }
//# min=0 max=1 step=0.01 default=0.4
export function sliderRivalColor(v) { hue2 = 0.72 + v * 0.2 }

// ---- fighter state (index 0 = left/warm, 1 = right/cool) ----
var fx = array(2), fy = array(2), fvx = array(2), fvy = array(2)
// intents: 1 = charge, 2 = back off, 3 = feint, 4 = retreat-and-fire
var intent = array(2), timer = array(2), timerInit = array(2)
var bobF = array(2), bobA = array(2), ready = array(2)
// one projectile slot per fighter
var pActive = array(2), px = array(2), py = array(2)

// ---- shockwave ring pool (oldest slot recycled) ----
var NR = 8
var ringX = array(NR), ringY = array(NR), ringAge = array(NR)
var RING_LIFE = 0.9, RING_SPEED = 0.7, RING_BAND = 0.06

var clashCount = 0
var clashNeed = 2        // clashes before the finale, re-rolled each round
var phase = 0            // 0 fighting, 1 finale projectiles, 2 recovery
var recTimer = 0
var totalTime = 0
var dt = 0

function dir(i) { return i == 0 ? 1 : -1 }   // toward the opponent

function rollIntent(i) {
  var gap = abs(fx[1] - fx[0])
  var r = random(1)
  if (gap < 0.16) {
    // already close: only disengage moves
    intent[i] = r < 0.5 ? 2 : 3
  } else if (r < 0.125) intent[i] = 4
  else if (r < 0.55) intent[i] = 1
  else if (r < 0.78) intent[i] = 3
  else intent[i] = 2
  timer[i] = 0.15 + random(0.85)
  timerInit[i] = timer[i]
  bobF[i] = 3 + random(6)
  bobA[i] = 0.03 + random(0.08)
  ready[i] = 0
}

function spawnRing(x, y) {
  var o = 0
  for (var i = 1; i < NR; i++) if (ringAge[i] > ringAge[o]) o = i
  ringX[o] = x
  ringY[o] = y
  ringAge[o] = 0
}

function updateProj(i, spd, kb) {
  if (!pActive[i]) return 0
  var d = dir(i)
  var o = 1 - i
  px[i] += d * spd * dt
  if (px[i] < -0.1 || px[i] > 1.1) {
    pActive[i] = 0
    return 0
  }
  if (abs(px[i] - fx[o]) < 0.05 * sizeMul && abs(py[i] - fy[o]) < 0.1 * sizeMul) {
    pActive[i] = 0
    spawnRing(fx[o], fy[o])
    fvx[o] = d * kb * (0.7 + random(0.6))   // knocked away from center
    fvy[o] = kb * (random(0.6) - 0.3)
    timer[o] = 0.05                          // victim re-rolls intent shortly
  }
  return 0
}

export function beforeRender(delta) {
  dt = min(delta / 1000, 0.05)
  totalTime += dt
  var base = 0.55 * speedMul

  for (var i = 0; i < NR; i++) if (ringAge[i] < 100) ringAge[i] += dt

  if (phase == 1) {
    // finale phase A: both projectiles in flight, fighters coasting
    updateProj(0, base * 3, 1.6 * speedMul)
    updateProj(1, base * 3, 1.6 * speedMul)
    for (var i = 0; i < 2; i++) {
      fvx[i] *= max(0, 1 - 3 * dt)
      fvy[i] *= max(0, 1 - 3 * dt)
      fx[i] += fvx[i] * dt
      fy[i] += fvy[i] * dt
    }
    fx[0] = clamp(fx[0], 0.05, 0.47)
    fx[1] = clamp(fx[1], 0.53, 0.95)
    fy[0] = clamp(fy[0], 0.15, 0.85)
    fy[1] = clamp(fy[1], 0.15, 0.85)
    if (!pActive[0] && !pActive[1]) {
      phase = 2
      recTimer = 0.4 + random(0.8)
    }
  } else if (phase == 2) {
    // finale phase B: recover toward mid-height, drift back together
    for (var i = 0; i < 2; i++) {
      fy[i] += (0.5 - fy[i]) * min(2 * dt, 1)
      fvx[i] *= max(0, 1 - 3 * dt)
      fx[i] += fvx[i] * dt
    }
    if (fx[1] - fx[0] > 0.7) {
      fvx[0] += 0.4 * dt
      fvx[1] -= 0.4 * dt
    }
    recTimer -= dt
    if (recTimer <= 0) {
      // round reset: counters/intents only — positions persist (no snap)
      phase = 0
      clashCount = 0
      clashNeed = 1 + floor(random(3))
      rollIntent(0)
      rollIntent(1)
      if (fx[1] - fx[0] > 0.6) {
        fx[0] += 0.05
        fx[1] -= 0.05
      }
    }
  } else {
    // normal battle
    updateProj(0, base * 2, speedMul)
    updateProj(1, base * 2, speedMul)

    for (var i = 0; i < 2; i++) {
      timer[i] -= dt
      if (timer[i] <= 0) {
        // retreat-and-fire is kept until it actually fires
        if (intent[i] == 4 && !ready[i]) timer[i] = 0.3
        else rollIntent(i)
      }
      var d = dir(i)
      var tv = 0
      if (intent[i] == 1) tv = d * base
      else if (intent[i] == 2) tv = -d * base * 0.5
      else if (intent[i] == 3) {
        // feint: dash in for the first half of the timer, then pull out
        tv = timer[i] > timerInit[i] * 0.5 ? d * base : -d * base * 0.7
      } else if (intent[i] == 4) {
        var nearWall = i == 0 ? fx[0] < 0.12 : fx[1] > 0.88
        if (nearWall) {
          ready[i] = 1
          tv = 0
          if (!pActive[i]) {
            pActive[i] = 1
            px[i] = fx[i]
            py[i] = fy[i]
            intent[i] = 1
            timer[i] = 0.2 + random(0.5)
            timerInit[i] = timer[i]
          }
        } else tv = -d * base * 1.4
      }
      // first-order easing toward the target velocity (~1/4 s)
      fvx[i] += (tv - fvx[i]) * min(dt / 0.25, 1)
      fx[i] += fvx[i] * dt
      // vertical bob: direct sine of total time, per-fighter, out of phase
      fy[i] = 0.5 + sin(totalTime * bobF[i] + i * PI) * bobA[i] * sizeMul
    }
    fx[0] = clamp(fx[0], 0.04, 0.96)
    fx[1] = clamp(fx[1], 0.04, 0.96)
    fy[0] = clamp(fy[0], 0.12, 0.88)
    fy[1] = clamp(fy[1], 0.12, 0.88)

    // clash detection
    if (abs(fx[1] - fx[0]) < 0.07 * sizeMul) {
      spawnRing((fx[0] + fx[1]) / 2, (fy[0] + fy[1]) / 2)
      fvx[0] = -(0.9 + random(0.7)) * speedMul
      fvx[1] = (0.9 + random(0.7)) * speedMul
      for (var i = 0; i < 2; i++) {
        bobF[i] = 5 + random(6)        // faster, bigger bob after a clash
        bobA[i] = 0.05 + random(0.08)
      }
      clashCount += 1
      if (clashCount >= clashNeed) {
        // finale: both fire simultaneously
        phase = 1
        for (var i = 0; i < 2; i++) {
          pActive[i] = 1
          px[i] = fx[i]
          py[i] = fy[i]
        }
      } else {
        // fight resumes quickly
        for (var i = 0; i < 2; i++) {
          intent[i] = random(1) < 0.75 ? 1 : 3
          timer[i] = 0.15 + random(0.35)
          timerInit[i] = timer[i]
          ready[i] = 0
        }
      }
    }
  }
}

export function render2D(index, x, y) {
  // 1) shockwave rings (pure white), gated so faint tails don't occlude
  var rb = 0
  for (var i = 0; i < NR; i++) {
    var a = ringAge[i]
    if (a < RING_LIFE) {
      var front = a * RING_SPEED
      var d = hypot(x - ringX[i], y - ringY[i])
      var band = 1 - abs(d - front) / RING_BAND
      if (band > 0) {
        var b = (1 - a / RING_LIFE) * band
        if (b > rb) rb = b
      }
    }
  }
  if (rb > 0.05) {
    rgb(rb, rb, rb)
    return
  }

  // 2) fighters: whitish-hot core, saturated aura fading to the edge
  for (var i = 0; i < 2; i++) {
    var d = hypot(x - fx[i], y - fy[i])
    var core = 0.035 * sizeMul
    var aura = 0.11 * sizeMul
    if (d < core) {
      hsv(i == 0 ? hue1 : hue2, i == 0 ? 0.3 : 0.55, 1)
      return
    }
    if (d < aura) {
      hsv(i == 0 ? hue1 : hue2, 1, (1 - (d - core) / (aura - core)) * 0.6)
      return
    }
  }

  // 3) projectiles: small solid cyan discs
  for (var i = 0; i < 2; i++) {
    if (pActive[i] && hypot(x - px[i], y - py[i]) < 0.025 * sizeMul) {
      rgb(0, 0.9, 1)
      return
    }
  }

  rgb(0, 0, 0)
}

// ---- initial state ----
fx[0] = 0.25
fx[1] = 0.75
fy[0] = 0.5
fy[1] = 0.5
for (i = 0; i < NR; i++) ringAge[i] = 1000   // huge sentinel = inactive
rollIntent(0)
rollIntent(1)
