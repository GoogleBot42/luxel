// name: scrolls
// Clean-room reimplementation from a prose functional description of the
// community pattern "scrolls"; original source never consulted.

// Soft mounds of sea-green light fade up at random spots on a black
// strip, hold, and sink away — bioluminescent patches surfacing. A fixed
// pool of ten pulse slots; each pulse has a trapezoid life envelope
// (ramp up a quarter, hold half, ramp down a quarter) and a half-sine
// spatial bump spanning about a fifth of the strip. Summed intensity is
// clamped then rescaled so overlaps saturate into the deep blue-teal top
// of a six-stop gradient (dark cores inside bright aqua rings), and the
// output is squared for soft gamma. Honest pool management replaces the
// original's buggy slot bookkeeping. No scrolling despite the name.

var SLOTS = 10
var LIFETIME = 5        // seconds per pulse
var SPAWN_GAP = 0.5     // seconds between spawns (lifetime/gap = pool size)
var HALF_WIDTH = 0.1    // half of the ~one-fifth-of-strip bump span

var alive = array(SLOTS)
var birth = array(SLOTS)
var pos = array(SLOTS)
var env = array(SLOTS)  // per-frame temporal envelope per slot

var clock = 0
var nextSpawn = 0

// Six-stop intensity gradient: black through the first fifth, rising
// through dark muted teal to a bright spring-green/aqua peak, easing
// into a deep blue-leaning teal that holds across the top.
var gp = array(6)
var gr = array(6)
var gg = array(6)
var gb = array(6)
gp[0] = 0;    gr[0] = 0;    gg[0] = 0;    gb[0] = 0
gp[1] = 0.2;  gr[1] = 0;    gg[1] = 0;    gb[1] = 0
gp[2] = 0.42; gr[2] = 0;    gg[2] = 0.28; gb[2] = 0.24
gp[3] = 0.6;  gr[3] = 0.12; gg[3] = 1;    gb[3] = 0.55
gp[4] = 0.82; gr[4] = 0;    gg[4] = 0.38; gb[4] = 0.48
gp[5] = 1;    gr[5] = 0;    gg[5] = 0.32; gb[5] = 0.5

export function beforeRender(delta) {
  clock += delta / 1000

  // Rebase occasionally so the fixed-point clock never wraps.
  if (clock > 10000) {
    var j
    for (j = 0; j < SLOTS; j++) birth[j] = birth[j] - 10000
    nextSpawn = nextSpawn - 10000
    clock = clock - 10000
  }

  // Spawn at most one pulse per frame into any free slot.
  if (clock >= nextSpawn) {
    var i
    for (i = 0; i < SLOTS; i++) {
      if (!alive[i]) {
        alive[i] = 1
        birth[i] = clock
        pos[i] = random(1)
        nextSpawn = clock + SPAWN_GAP
        break
      }
    }
  }

  // Age every live slot; trapezoid envelope = triangle doubled, clamped.
  var k
  for (k = 0; k < SLOTS; k++) {
    if (alive[k]) {
      var lf = (clock - birth[k]) / LIFETIME
      if (lf >= 1) {
        alive[k] = 0
        env[k] = 0
      } else {
        env[k] = min(1, 2 * triangle(lf))
      }
    }
  }
}

export function render(index) {
  var p = index / pixelCount

  // Sum envelope x half-sine bump over every live pulse.
  var sum = 0
  var i
  for (i = 0; i < SLOTS; i++) {
    if (alive[i]) {
      var d = abs(p - pos[i])
      if (d < HALF_WIDTH) {
        sum += env[i] * cos(d / HALF_WIDTH * PI / 2)
      }
    }
  }

  // Clamp then rescale so stacked pulses saturate exactly at the
  // gradient's top (deep teal cores inside bright overlaps).
  var t = min(sum, 0.667) * 1.5

  // Piecewise-linear six-stop gradient lookup.
  var s = 1
  while (s < 5 && t > gp[s]) s += 1
  var f = clamp((t - gp[s - 1]) / (gp[s] - gp[s - 1]), 0, 1)
  var r = gr[s - 1] + (gr[s] - gr[s - 1]) * f
  var g = gg[s - 1] + (gg[s] - gg[s - 1]) * f
  var b = gb[s - 1] + (gb[s] - gb[s - 1]) * f

  // Gamma-style squaring deepens the dark end, keeps glows soft-edged.
  rgb(r * r, g * g, b * b)
}
