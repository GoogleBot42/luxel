// name: Pew-Pew-Pew!
// Clean-room reimplementation from a prose functional description of the
// community pattern "Pew-Pew-Pew!"; original source never consulted.

// A volley of neon laser bolts firing down the strip, each in a color
// from a hot-pink-to-blue palette, each at its own random speed, dragging
// a fast-fading trail over a faint warm-red ambient glow. By default the
// whole strip flashes blue whenever a bolt re-fires from pixel zero
// ("blue lightning" — the blue channel is deliberately read from pixel 0).

// The original keeps these as hand-edit constants; here they are the pattern's
// controls. Top-level values are the originals, so an untouched pattern renders
// exactly as it did.
const MAXBOLTS = 32       // allocation ceiling for the bolt roster
var numBolts = 10         // bolts in flight
var mirror = 0            // 1 = run the whole effect backward
var blueLightning = 1     // 1 = whole-strip blue flash on each launch
var fade = 0.8            // per-frame trail decay (~1/5 lost per frame)
var speedK = 0.04         // pixels per ms per unit velocity

// five palette stops: hot pink -> magenta -> purple -> violet -> blue
// (every stop contains some blue, which feeds the lightning effect)
var palR = array(5)
var palG = array(5)
var palB = array(5)
palR[0] = 1;   palG[0] = 0.1; palB[0] = 0.5
palR[1] = 1;   palG[1] = 0;   palB[1] = 1
palR[2] = 0.7; palG[2] = 0;   palB[2] = 1
palR[3] = 0.4; palG[3] = 0;   palB[3] = 1
palR[4] = 0.1; palG[4] = 0.1; palB[4] = 1

var boltPos = array(MAXBOLTS)
var boltVel = array(MAXBOLTS)
var boltCol = array(MAXBOLTS)   // palette index, round-robin
var ready = 10                  // bolts given a starting position + speed

// per-channel trail buffers (the original packs channels into one number;
// that's an artifact of its storage, not the effect)
var bufR = array(pixelCount)
var bufG = array(pixelCount)
var bufB = array(pixelCount)

var i
for (i = 0; i < numBolts; i++) {
  boltCol[i] = i % 5
  boltPos[i] = random(pixelCount)
  boltVel[i] = 1 + random(1.2)
}

// How many bolts are in flight at once. New bolts are seeded only when the
// control is raised, so the untouched pattern draws the same random roster.
//# min=1 max=32 step=1 default=10
export function sliderBoltCount(v) {
  var n = clamp(floor(v), 1, MAXBOLTS)
  var q
  for (q = ready; q < n; q++) {
    boltCol[q] = q % 5
    boltPos[q] = random(pixelCount)
    boltVel[q] = 1 + random(1.2)
  }
  if (n > ready) ready = n
  numBolts = n
}

// Base bolt speed in pixels per second (each bolt rolls its own multiplier
// between 1x and about 2.2x this).
//# min=5 max=300 step=5 default=40
export function sliderBoltSpeed(v) { speedK = max(v, 1) / 1000 }

// Percentage of a bolt's trail that survives each frame: 0 leaves bare heads,
// high values smear long comet tails.
//# min=0 max=99 step=1 default=80
export function sliderTrailPersistPercent(v) { fade = clamp(v, 0, 99) / 100 }

// Fire the volley the other way down the strip.
//# default=0
export function toggleMirror(v) { mirror = v }

// The signature quirk: flash the whole strip blue as each bolt relaunches.
// Off shows each bolt in its true palette color instead.
//# default=1
export function toggleBlueLightning(v) { blueLightning = v }

export function beforeRender(delta) {
  feedback(bufR, fade)
  feedback(bufG, fade)
  feedback(bufB, fade)

  for (var i = 0; i < numBolts; i++) {
    var from = floor(boltPos[i])
    boltPos[i] += delta * speedK * boltVel[i]
    var to = floor(boltPos[i])

    var c = boltCol[i]
    // paint every integer pixel swept this frame so fast bolts stay solid;
    // additive with saturation so overlaps brighten toward white
    for (var j = from; j <= to; j++) {
      if (j >= pixelCount) break
      bufR[j] = min(1, bufR[j] + palR[c])
      bufG[j] = min(1, bufG[j] + palG[c])
      bufB[j] = min(1, bufB[j] + palB[c])
    }

    if (boltPos[i] >= pixelCount) {
      boltPos[i] = 0                  // re-fire from the start
      boltVel[i] = 1 + random(1.2)    // fresh random speed; color kept
    }
  }
}

export function render(index) {
  var p = mirror ? pixelCount - 1 - index : index
  var r = bufR[p] + 0.05   // faint warm-red ambient underlay
  var g = bufG[p] + 0.01
  var b = blueLightning ? bufB[0] : bufB[p]
  rgb(min(r, 1), min(g, 1), min(b, 1))
}
