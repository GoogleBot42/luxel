// name: scrolls
// Clean-room reimplementation from a prose functional description of the
// community pattern "scrolls"; original source never consulted.

// Soft sea-green mounds fade up at random spots, hold, and sink back like
// bioluminescent patches. Each frame builds a scalar intensity field over
// the strip from a pool of pulses; the render maps intensity through a
// six-stop black -> teal -> bright aqua -> deep blue-teal gradient, with a
// squared (gamma-ish) output curve. Honest pool management throughout
// (spawn into any free slot, age every live slot).

var POOL = 10
var LIFETIME = 5           // seconds each pulse lives
var SPAWN_GAP = 0.5        // seconds between spawns (LIFETIME / POOL: steady full pool)
var WIDTH = 0.2            // spatial bump spans ~a fifth of the strip
var GEN_S = 4.3            // a generation runs this long, then the field is wiped
var BLANK_S = 0.22         // ...and the strip sits fully dark for this long

var alive = array(POOL)
var birth = array(POOL)
var pos = array(POOL)
var field = array(pixelCount)

var now = 0
var nextSpawn = 0
var genEnd = GEN_S         // when the current generation dies
var blankUntil = 0         // dark hold before the next generation seeds

// Gradient stops: position, r, g, b. Dead zone through the first fifth,
// dark muted teal rising to a bright spring-green/aqua peak mid-scale,
// easing down into a deep blue-leaning teal that holds across the top.
var GN = 6
var gp = array(GN)
var gr = array(GN)
var gg = array(GN)
var gb = array(GN)
gp[0] = 0;    gr[0] = 0;    gg[0] = 0;    gb[0] = 0     // black
gp[1] = 0.2;  gr[1] = 0;    gg[1] = 0;    gb[1] = 0     // still black
gp[2] = 0.42; gr[2] = 0;    gg[2] = 0.27; gb[2] = 0.22  // dark muted teal
gp[3] = 0.6;  gr[3] = 0;    gg[3] = 0.82; gb[3] = 0.46  // bright spring-green/aqua
gp[4] = 0.82; gr[4] = 0;    gg[4] = 0.31; gb[4] = 0.4   // deep blue-teal
gp[5] = 1;    gr[5] = 0;    gg[5] = 0.266; gb[5] = 0.396 // holds steady

export function beforeRender(delta) {
  now += delta / 1000

  // Generations: the whole field is wiped every GEN_S seconds and the strip
  // holds black for a beat before a fresh crop seeds.
  if (now >= genEnd) {
    for (var g = 0; g < POOL; g++) alive[g] = 0
    blankUntil = now + BLANK_S
    nextSpawn = blankUntil
    genEnd = blankUntil + GEN_S
  }
  if (now < blankUntil) {
    feedback(field, 0)
    return
  }

  // Spawn at most one pulse per frame, into any free slot, once the
  // cooldown has elapsed.
  if (now >= nextSpawn) {
    for (var i = 0; i < POOL; i++) {
      if (!alive[i]) {
        alive[i] = 1
        birth[i] = now
        pos[i] = random(1)
        nextSpawn = now + SPAWN_GAP
        break
      }
    }
  }

  // Rebuild the intensity field: sum (temporal envelope x spatial bump)
  // for every live pulse.
  feedback(field, 0)   // clear the field (arrayReplace is a splat, not a fill)
  for (var i = 0; i < POOL; i++) {
    if (!alive[i]) continue
    var age = (now - birth[i]) / LIFETIME
    if (age >= 1) {
      alive[i] = 0
      continue
    }
    // Trapezoid: ramp up over the first quarter of life, hold at one for
    // the middle half, ramp down over the last quarter.
    var env = min(1, triangle(age) * 2)

    // Half-sine bump centered on the pulse, clipped at the strip ends.
    var lo = floor((pos[i] - WIDTH / 2) * pixelCount)
    var hi = ceil((pos[i] + WIDTH / 2) * pixelCount)
    if (lo < 0) lo = 0
    if (hi > pixelCount) hi = pixelCount
    for (var px = lo; px < hi; px++) {
      var prof = cos(PI * (px / pixelCount - pos[i]) / WIDTH)
      if (prof > 0) field[px] += env * prof
    }
  }
}

export function render(index) {
  // Clamp stacked pulses to two-thirds, then rescale so the ceiling lands
  // exactly on the top of the gradient (bright overlaps saturate into the
  // deep-teal top instead of blowing out).
  var v = clamp(min(field[index], 0.667) * 1.5, 0, 1)

  // Linear interpolation between gradient stops, per channel.
  var k = 0
  while (k < GN - 2 && v > gp[k + 1]) k++
  var t = (v - gp[k]) / (gp[k + 1] - gp[k])
  var r = gr[k] + (gr[k + 1] - gr[k]) * t
  var g = gg[k] + (gg[k + 1] - gg[k]) * t
  var b = gb[k] + (gb[k + 1] - gb[k]) * t

  // Squared output: deepens the dark end, keeps the glow soft-edged.
  rgb(r * r, g * g, b * b)
}
