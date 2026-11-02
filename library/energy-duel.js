// name: Energy Duel
// tags: panel, game
// Clean-room reimplementation from the prose spec docs/pattern-specs/dbzbattlefinal.md — reimagined, not a replica.
// Two glowing fighters dart, feint and clash in an endless anime-style duel: every clash
// throws a shockwave ring, a retreating fighter fires an energy shot, and after a few
// clashes both unleash beams that meet mid-panel and push for control until one side
// breaks through and the loser is blown back. renderBulk: splat auras and dash trails,
// fillCircle cores, rings as 16-segment drawLine polygons, beams as drawLine capsules.

var speedMul = 1        // fight pace, x the reference
var sizeMul = 1         // fighter size, x the reference
var hue1 = 32 / 360     // warm fighter
var hue2 = 285 / 360    // cool fighter

//# min=0.25 max=2.5 step=0.05 default=1 unitless-ok
export function sliderSpeed(v) { speedMul = clamp(v, 0.1, 4) }

//# min=0.5 max=2 step=0.05 default=1 unitless-ok
export function sliderScale(v) { sizeMul = clamp(v, 0.25, 3) }

//# min=0 max=360 step=1 default=32
export function sliderWarmHue(v) { hue1 = mod(v, 360) / 360 }

//# min=0 max=360 step=1 default=285
export function sliderRivalHue(v) { hue2 = mod(v, 360) / 360 }

var fx = array(2), fy = array(2), fvx = array(2), fvy = array(2)
var intent = array(2), timer = array(2), timerInit = array(2)   // 1 charge 2 back off 3 feint 4 retreat-and-fire
var bobF = array(2), bobA = array(2), ready = array(2), ly = array(2), lyT = array(2)   // lane height, target
var pActive = array(2), px = array(2), py = array(2)
var NT = 6                                   // dash-trail samples per fighter
var tx = array(2 * NT), ty = array(2 * NT), tHead = 0, tAcc = 0
var NR = 6, RING_LIFE = 0.8
var ringX = array(NR), ringY = array(NR), ringAge = array(NR), ringHue = array(NR)
var AH = array(16)                           // the arena's hue, left to right
var CS = array(17), SN = array(17)           // unit circle, 16 segments
for (var i = 0; i <= 16; i++) { CS[i] = cos(i * PI2 / 16); SN[i] = sin(i * PI2 / 16) }

var clashCount = 0, clashCd = 0, clashNeed = 2, flash = 0, flashX = 0.5, flashY = 0.5
var phase = 0          // 0 fight, 1 charge, 2 beam struggle, 3 breakthrough, 4 recover
var phaseT = 0, mx = 0.5, mv = 0, beamY = 0.5, pushLen = 3, winner = 0, totalTime = 0, dt = 0

function dir(i) { return i == 0 ? 1 : -1 }   // toward the opponent

function rollIntent(i) {
  var r = random(1)
  if (abs(fx[1] - fx[0]) < 0.16) intent[i] = r < 0.5 ? 2 : 3
  else if (r < 0.14) intent[i] = 4
  else if (r < 0.55) intent[i] = 1
  else if (r < 0.78) intent[i] = 3
  else intent[i] = 2
  timer[i] = 0.15 + random(0.85)
  timerInit[i] = timer[i]
  bobF[i] = 3 + random(6)
  bobA[i] = 0.03 + random(0.06)
  lyT[i] = 0.25 + random(0.5)
  ready[i] = 0
}

function spawnRing(x, y, h) {
  var o = 0
  for (var i = 1; i < NR; i++) if (ringAge[i] > ringAge[o]) o = i
  ringX[o] = x; ringY[o] = y; ringAge[o] = 0; ringHue[o] = h
}

function boom(x, y, k) { flash = max(flash, k); flashX = x; flashY = y }

function updateProj(i, spd) {
  if (!pActive[i]) return
  var o = 1 - i
  px[i] += dir(i) * spd * dt
  if (px[i] < -0.1 || px[i] > 1.1) { pActive[i] = 0; return }
  if (abs(px[i] - fx[o]) < 0.06 * sizeMul && abs(py[i] - fy[o]) < 0.12 * sizeMul) {
    pActive[i] = 0
    spawnRing(fx[o], fy[o], i == 0 ? hue1 : hue2)
    boom(fx[o], fy[o], 0.5)
    fvx[o] = dir(i) * speedMul * (0.7 + random(0.6))
    timer[o] = 0.05
  }
}

function fight() {
  var base = 0.55 * speedMul
  updateProj(0, base * 2)
  updateProj(1, base * 2)
  for (var i = 0; i < 2; i++) {
    timer[i] -= dt
    if (timer[i] <= 0) {
      if (intent[i] == 4 && !ready[i]) timer[i] = 0.3
      else rollIntent(i)
    }
    var d = dir(i), tv = 0
    if (intent[i] == 1) tv = d * base
    else if (intent[i] == 2) tv = -d * base * 0.5
    else if (intent[i] == 3) tv = timer[i] > timerInit[i] * 0.5 ? d * base : -d * base * 0.7
    else if (intent[i] == 4) {
      if (i == 0 ? fx[0] < 0.12 : fx[1] > 0.88) {
        ready[i] = 1
        if (!pActive[i]) {
          pActive[i] = 1; px[i] = fx[i]; py[i] = fy[i]
          intent[i] = 1; timer[i] = 0.2 + random(0.5); timerInit[i] = timer[i]
        }
      } else tv = -d * base * 1.4
    }
    fvx[i] += (tv - fvx[i]) * min(dt * 4, 1)
    fx[i] = clamp(fx[i] + fvx[i] * dt, 0.06, 0.94)
    // close in: both lanes pull together, so clashes meet head-on
    var goal = abs(fx[1] - fx[0]) < 0.3 ? (ly[0] + ly[1]) / 2 : lyT[i]
    ly[i] += (goal - ly[i]) * min(dt * 3, 1)
    fy[i] = clamp(ly[i] + sin(totalTime * bobF[i] + i * PI) * bobA[i] * sizeMul, 0.12, 0.88)
  }
  if (clashCd > 0) clashCd -= dt
  if (clashCd <= 0 && abs(fx[1] - fx[0]) < 0.09 * sizeMul) {
    clashCd = 0.3
    var cx = (fx[0] + fx[1]) / 2, cy = (fy[0] + fy[1]) / 2
    spawnRing(cx, cy, -1)
    boom(cx, cy, 0.7)
    fvx[0] = -(0.9 + random(0.7)) * speedMul
    fvx[1] = (0.9 + random(0.7)) * speedMul
    for (i = 0; i < 2; i++) {
      bobF[i] = 5 + random(6); bobA[i] = 0.04 + random(0.06); lyT[i] = 0.25 + random(0.5)
      intent[i] = random(1) < 0.75 ? 1 : 3
      timer[i] = 0.15 + random(0.35); timerInit[i] = timer[i]; ready[i] = 0
    }
    clashCount++
    if (clashCount >= clashNeed) { phase = 1; phaseT = 0; beamY = 0.3 + random(0.4) }
  }
}

export function beforeRender(delta) {
  dt = min(delta / 1000, 0.05)
  totalTime = mod(totalTime + dt, 3600)
  phaseT += dt
  flash = max(0, flash - dt * 2.5)
  for (var i = 0; i < NR; i++) if (ringAge[i] < RING_LIFE) ringAge[i] += dt
  tAcc += dt
  if (tAcc > 0.03) {                                   // sample the dash trails
    tAcc = 0
    tHead = (tHead + 1) % NT
    tx[tHead] = fx[0]; ty[tHead] = fy[0]; tx[NT + tHead] = fx[1]; ty[NT + tHead] = fy[1]
  }
  if (phase == 0) { fight(); return }
  if (phase == 1 || phase == 2) {                      // square up at the walls
    var ease = min(dt * 5, 1)
    fx[0] += (0.13 - fx[0]) * ease; fx[1] += (0.87 - fx[1]) * ease
    for (i = 0; i < 2; i++) { fy[i] += (beamY + sin(totalTime * 7 + i * 2) * 0.02 - fy[i]) * ease; ly[i] = fy[i] }
    if (phase == 1 && phaseT > 0.9) { phase = 2; phaseT = 0; mx = 0.5; mv = 0; pushLen = 2.2 + random(1.6) }
    if (phase == 2) {                                  // the push: a jittery tug of war
      mv += (random(2) - 1) * 2.2 * speedMul * dt - (mx - 0.5) * 0.8 * dt
      mv *= 1 - min(dt * 1.5, 1)
      mx = clamp(mx + mv * dt, 0.3, 0.7)
      if (phaseT > pushLen) {
        phase = 3; phaseT = 0
        winner = mx > 0.5 ? 0 : 1                      // the side pushing forward wins
      }
    }
    return
  }
  if (phase == 3) {                                    // the winner's beam breaks through
    var target = fx[1 - winner]
    mx += (winner == 0 ? 1 : -1) * 2.2 * speedMul * dt
    if (winner == 0 ? mx >= target : mx <= target) {
      var l = 1 - winner
      spawnRing(fx[l], fy[l], winner == 0 ? hue1 : hue2)
      spawnRing(fx[l], fy[l], -1)
      ringAge[0] = min(ringAge[0], 0.15)
      boom(fx[l], fy[l], 1)
      fvx[l] = dir(winner) * 2.5 * speedMul
      phase = 4; phaseT = 0
    }
    return
  }
  // phase 4: the loser tumbles to the wall, both drift back in for the next round
  for (i = 0; i < 2; i++) {
    fvx[i] *= 1 - min(dt * 2.5, 1)
    fx[i] = clamp(fx[i] + fvx[i] * dt, 0.06, 0.94)
    ly[i] = fy[i]
  }
  if (phaseT > 1.3) {
    phase = 0; clashCount = 0; clashNeed = 1 + floor(random(3))
    rollIntent(0); rollIntent(1)
  }
}

function fighter(i) {
  var h = i == 0 ? hue1 : hue2
  var x = fx[i], y = fy[i]
  var p = 0.85 + 0.15 * sin(totalTime * 19 + i * 3)
  for (var k = 1; k < NT; k++) {                       // dash trail, newest first
    var j = i * NT + (tHead - k + NT) % NT
    var d = abs(tx[j] - x) + abs(ty[j] - y)
    if (d > 0.025) { hsv(h, 1, 0.5 * (1 - k / NT)); splat(tx[j], ty[j], (0.1 - k * 0.012) * sizeMul, 1) }
  }
  hsv(h, 1, 0.6 * p)
  splat(x, y, 0.17 * sizeMul * p, 1)
  hsv(h, 0.7, 0.9)
  splat(x, y, 0.08 * sizeMul, 1)
  hsv(h, i == 0 ? 0.25 : 0.4, 1)
  fillCircle(x, y, 0.04 * sizeMul)
}

function beam(i) {
  var h = i == 0 ? hue1 : hue2
  var x0 = fx[i] + dir(i) * 0.03 * sizeMul
  hsv(h, 1, 0.75)
  drawLine(x0, fy[i], mx, beamY, (0.075 + 0.012 * sin(totalTime * 40 + i)) * sizeMul, 1)
  hsv(h, 0.25, 1)
  drawLine(x0, fy[i], mx, beamY, 0.03 * sizeMul, 2)
}

export function renderBulk() {
  var h2 = hue2 - hue1 > 0.5 ? hue2 - 1 : (hue1 - hue2 > 0.5 ? hue2 + 1 : hue2)
  var i, s
  for (i = 0; i < 16; i++) AH[i] = mix(hue1, h2, i / 15)
  fillCanvas(AH, 0.9, 0.07, 16, 1)                   // a split arena, the short way round
  for (i = 0; i < NR; i++) {                           // shockwave rings
    var a = ringAge[i]
    if (a >= RING_LIFE) continue
    var r = 0.02 + a * 0.75, b = 1 - a / RING_LIFE
    if (ringHue[i] < 0) rgb(b, b, b)
    else hsv(ringHue[i], 0.5, b)
    for (s = 0; s < 16; s++)
      drawLine(ringX[i] + CS[s] * r, ringY[i] + SN[s] * r, ringX[i] + CS[s + 1] * r, ringY[i] + SN[s + 1] * r, 0.035, 1)
  }
  if (phase >= 1 && phase <= 3) {
    if (phase == 1) {                                  // charging up
      var c = min(phaseT / 0.9, 1)
      for (i = 0; i < 2; i++) {
        hsv(i == 0 ? hue1 : hue2, 0.5, c)
        splat(fx[i] + dir(i) * 0.06, fy[i], 0.1 * c * sizeMul, 1)
      }
    } else {
      if (phase == 2 || winner == 0) beam(0)
      if (phase == 2 || winner == 1) beam(1)
      var q = 0.1 + 0.03 * sin(totalTime * 31)         // the contact point
      hsv(phase == 3 ? (winner == 0 ? hue1 : hue2) : 0, phase == 3 ? 0.6 : 0, 1)
      splat(mx, beamY, q * sizeMul * 1.4, 1)
      for (s = 0; s < 5; s++) {                        // crackle
        hsv(random(1) < 0.5 ? hue1 : hue2, 0.6, 1)
        splat(mx + (random(2) - 1) * 0.12 * sizeMul, beamY + (random(2) - 1) * 0.12 * sizeMul, 0.03 * sizeMul, 1)
      }
    }
  }
  fighter(0)
  fighter(1)
  for (i = 0; i < 2; i++) {                            // energy shots
    if (!pActive[i]) continue
    hsv(0.5, 0.8, 0.8)
    drawLine(px[i] - dir(i) * 0.1, py[i], px[i], py[i], 0.025 * sizeMul, 1)
    hsv(0.5, 0.2, 1)
    fillCircle(px[i], py[i], 0.028 * sizeMul)
  }
  if (flash > 0) { rgb(flash, flash, flash); splat(flashX, flashY, 0.5 * flash + 0.1, 1) }
}

fx[0] = 0.25; fx[1] = 0.75; fy[0] = 0.5; fy[1] = 0.5; ly[0] = 0.5; ly[1] = 0.5
for (i = 0; i < NR; i++) ringAge[i] = RING_LIFE
for (i = 0; i < 2 * NT; i++) { tx[i] = i < NT ? 0.25 : 0.75; ty[i] = 0.5 }
rollIntent(0)
rollIntent(1)
