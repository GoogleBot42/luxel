// name: Fireworks Finale
// Curated original for the Luxel library.
//
// A real firework lifecycle on a 1D strip, simulated rather than faked:
// shells launch from the base (index 0) with a muzzle flash and a comet
// trail, decelerate under gravity, and burst at apogee in a white flash.
// Each burst throws sparks OUTWARD in both directions; air drag brakes them
// while gravity drags the embers back down the strip, and they die out in a
// twinkling fade. Shells launch on a jittered schedule so there is dark sky
// between shows, pick a color from a classic firework palette, and about a
// third of them are multi-break shells that crackle a second time.

var maxShells = 5
var maxSparksPerShell = 28        // storage ceiling; BurstSize picks how many fly
var maxSparks = maxShells * maxSparksPerShell

var gravity = 0.85                // strip-lengths per second^2 (rockets)
var emberGravity = 0.5            // embers are lighter: less pull
var dragPerSecond = 3             // air brake on sparks
var trailHalfLife = 0.055         // afterglow: comet trails and spark streaks

// ---- controls (real units) -------------------------------------------------

var launchRate = 15               // shells per minute
var burstSize = 16                // sparks per shell
var fadeSeconds = 2               // how long an ember burns
var spreadPercent = 25            // burst radius as a percent of the strip

//# min=2 max=60 step=1 default=15
export function sliderLaunchRate(v) { launchRate = max(1, floor(v)) }

//# min=4 max=28 step=1 default=16
export function sliderBurstSize(v) { burstSize = clamp(floor(v), 1, maxSparksPerShell) }

//# min=0.4 max=6 step=0.1 default=2
export function sliderFadeSeconds(v) { fadeSeconds = max(0.15, v) }

//# min=5 max=60 step=1 default=25
export function sliderSpreadPercent(v) { spreadPercent = clamp(v, 1, 100) }

// Rapid volley: shells as fast as the sky can clear them.
export function triggerFinale() { finaleLeft = 14; launchTimer = 0 }

// ---- state -----------------------------------------------------------------

var rBuf = array(pixelCount)
var gBuf = array(pixelCount)
var bBuf = array(pixelCount)

var stage = array(maxShells)      // 0 idle, 1 rising, 2 burst
var pos = array(maxShells)        // rocket position, 0..1 up the strip
var vel = array(maxShells)
var apogee = array(maxShells)
var flash = array(maxShells)      // burst-flash timer, seconds
var breakIn = array(maxShells)    // secondary-break countdown, 0 = single break
var used = array(maxShells)       // sparks spawned by this shell
var shR = array(maxShells)
var shG = array(maxShells)
var shB = array(maxShells)
var shSpread = array(maxShells)   // burst-velocity scale (shell type)
var shFade = array(maxShells)     // ember lifetime scale (shell type)
var shDroop = array(maxShells)    // ember gravity scale (shell type)

var sparkPos = array(maxSparks)
var sparkVel = array(maxSparks)
var sparkLife = array(maxSparks)

var col = array(3)
var launchTimer = 0.8
var finaleLeft = 0

// Classic firework colors, as hue/saturation pairs: gold, red, green, blue,
// violet, and a white "titanium salute".
var hues = array(6)
var sats = array(6)
hues[0] = 0.11; sats[0] = 0.8
hues[1] = 0.0;  sats[1] = 1
hues[2] = 0.33; sats[2] = 1
hues[3] = 0.6;  sats[3] = 1
hues[4] = 0.78; sats[4] = 0.9
hues[5] = 0.08; sats[5] = 0.12

// ---- helpers ---------------------------------------------------------------

// Additive sub-pixel deposit: split the sample between the two nearest LEDs
// so a particle drifting at a fraction of a pixel per frame still moves
// smoothly instead of stepping.
function splat(p, r, g, b) {
  var f = p * pixelCount - 0.5
  var i0 = floor(f)
  var w = f - i0
  if (i0 >= 0 && i0 < pixelCount) {
    rBuf[i0] += r * (1 - w)
    gBuf[i0] += g * (1 - w)
    bBuf[i0] += b * (1 - w)
  }
  var i1 = i0 + 1
  if (i1 >= 0 && i1 < pixelCount) {
    rBuf[i1] += r * w
    gBuf[i1] += g * w
    bBuf[i1] += b * w
  }
}

function launch() {
  var s = -1
  for (var i = 0; i < maxShells; i++) if (stage[i] == 0) { s = i; break }
  if (s < 0) return 0

  var h = 0.42 + random(0.42)         // apogee, 0.42..0.84 up the strip
  apogee[s] = h
  pos[s] = 0
  vel[s] = sqrt(2 * gravity * h)      // exactly enough to coast to a stop there
  stage[s] = 1
  flash[s] = 0
  used[s] = 0

  // Shell type — a show reads as a show because no two shells are alike.
  //   peony  : the default round break
  //   willow : slow gold embers that hang and pour back down the strip
  //   crackle: a tight, fast salute that always breaks a second time
  var roll = random(1)
  var c = floor(random(6))
  if (roll < 0.2) {
    shSpread[s] = 0.6; shFade[s] = 2; shDroop[s] = 1.6
    breakIn[s] = 0
    c = 0                              // willows are gold
  } else if (roll < 0.45) {
    shSpread[s] = 1.35; shFade[s] = 0.55; shDroop[s] = 0.9
    breakIn[s] = 0.28 + random(0.2)
  } else {
    shSpread[s] = 0.8 + random(0.4); shFade[s] = 0.85 + random(0.4); shDroop[s] = 1
    breakIn[s] = random(1) < 0.25 ? 0.3 + random(0.25) : 0
  }

  hsv2rgb(hues[c], sats[c], 1, col)
  shR[s] = col[0]
  shG[s] = col[1]
  shB[s] = col[2]

  // muzzle flash at the base
  splat(0, 0.9, 0.6, 0.25)
  splat(0.008, 0.5, 0.32, 0.12)
  return 1
}

function burst(s) {
  stage[s] = 2
  flash[s] = 0.1
  var n = burstSize
  used[s] = n
  // Uniform velocities are the correct 1D projection of an isotropic shell:
  // a solid expanding cloud with two crisp fronts. vMax is chosen so drag
  // stops the fastest spark right at the requested spread.
  var vMax = spreadPercent * 0.01 * dragPerSecond * shSpread[s]
  for (var k = 0; k < n; k++) {
    var i = s * maxSparksPerShell + k
    sparkPos[i] = pos[s]
    sparkVel[i] = vMax * (random(2) - 1)
    sparkLife[i] = 0.75 + random(0.25)
  }
}

// ---- frame -----------------------------------------------------------------

export function beforeRender(delta) {
  var dt = min(delta, 60) * 0.001

  var decay = pow(0.5, dt / trailHalfLife)
  feedback(rBuf, decay)
  feedback(gBuf, decay)
  feedback(bBuf, decay)

  // schedule
  launchTimer -= dt
  if (launchTimer <= 0) {
    var fired = launch()
    if (finaleLeft > 0) {
      // a volley is limited by how fast the sky clears: only count a finale
      // shell once it actually got a slot, otherwise wait and retry
      if (fired) { finaleLeft--; launchTimer = 0.22 + random(0.16) }
      else launchTimer = 0.1
    } else {
      // jittered gap around the requested rate: the spread is what makes a
      // burst feel like an event instead of a metronome.
      launchTimer = (60 / launchRate) * (0.55 + random(0.9))
    }
  }

  var emberDrop = emberGravity * dt
  var dragStep = min(dragPerSecond * dt, 0.9)
  var lifeStep = dt / fadeSeconds
  // a fat break splits the same payload over more stars — without this a
  // 28-star burst piles up past white and loses the shell's color
  var briScale = clamp(14 / burstSize, 0.5, 1.3)

  for (var s = 0; s < maxShells; s++) {
    if (stage[s] == 0) continue

    if (stage[s] == 1) {
      // rising: decelerating comet, drawn along the segment it covered so a
      // fast rocket leaves a continuous streak, not dashes
      var v = vel[s] - gravity * dt
      vel[s] = v
      var p0 = pos[s]
      var p = p0 + v * dt
      pos[s] = p
      // one deposit per pixel crossed, so the streak stays continuous on a
      // 300-LED strip instead of breaking into dashes
      var steps = clamp(ceil(abs(p - p0) * pixelCount), 1, 16)
      for (var k = 0; k < steps; k++) {
        var pk = p0 + (p - p0) * ((k + 1) / steps)
        splat(pk, 0.85, 0.55, 0.22)
      }
      if (p >= apogee[s] || v <= 0.02) burst(s)
      continue
    }

    // burst flash: a hot white core that blooms a couple of pixels wide
    if (flash[s] > 0) {
      flash[s] -= dt
      var fb = flash[s] > 0 ? 1 : 0.4
      splat(pos[s], fb, fb, fb)
      splat(pos[s] + 1.5 / pixelCount, fb * 0.6, fb * 0.6, fb * 0.55)
      splat(pos[s] - 1.5 / pixelCount, fb * 0.6, fb * 0.6, fb * 0.55)
    }

    // multi-break shell: a second crackle out of the same cloud
    if (breakIn[s] > 0) {
      breakIn[s] -= dt
      if (breakIn[s] <= 0) {
        breakIn[s] = 0
        flash[s] = 0.06
        var kick = spreadPercent * 0.005 * dragPerSecond
        for (var k = 0; k < used[s]; k++) {
          var i = s * maxSparksPerShell + k
          if (sparkLife[i] <= 0) continue
          sparkVel[i] += kick * (random(2) - 1)
          sparkLife[i] = max(sparkLife[i], 0.7)
        }
      }
    }

    var living = 0
    var r = shR[s]
    var g = shG[s]
    var b = shB[s]
    var drop = emberDrop * shDroop[s]
    var step = lifeStep / shFade[s]
    for (var k = 0; k < used[s]; k++) {
      var i = s * maxSparksPerShell + k
      var life = sparkLife[i]
      if (life <= 0) continue

      var sv = sparkVel[i]
      sv -= drop                 // gravity pulls embers back toward the base
      sv -= sv * dragStep        // air brake
      var sp = sparkPos[i] + sv * dt
      if (sp < 0) { sp = 0; sv = 0; life -= step * 3 }   // burned out on the ground
      sparkVel[i] = sv
      sparkPos[i] = sp

      life -= step
      sparkLife[i] = life
      if (life <= 0) continue
      living++

      var bri = life * life * briScale
      // dying embers strobe: the classic guttering twinkle
      if (life < 0.45) bri *= random(1) < 0.45 ? 0.15 : 1.4
      splat(sp, r * bri, g * bri, b * bri)
    }
    if (living == 0 && flash[s] <= 0) stage[s] = 0
  }
}

export function render(index) {
  rgb(saturate(rBuf[index]), saturate(gBuf[index]), saturate(bBuf[index]))
}
