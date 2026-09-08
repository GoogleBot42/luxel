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

// The warm ambient underlay used to be added per pixel in `render`. A whole-
// frame `fillRGB` takes arrays, and the language has no array-plus-scalar
// builtin (`arrayAdd`/`arraySub` want two arrays, `arrayScale` multiplies;
// Gitea #373's proposed `arrayAffine(dst, src, k, c)` is exactly this gap), so
// the two constants live in constant arrays that are added before the fill and
// taken straight back off after. A fixed-point add and its inverse round-trip
// with no rounding at all, so the frame is bit-for-bit what the per-pixel
// readout produced.
var ambR = array(pixelCount)
var ambG = array(pixelCount)
arrayMutate(ambR, (v) => 0.05)
arrayMutate(ambG, (v) => 0.01)

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
    // additive with saturation so overlaps brighten toward white.
    // Mirror is applied HERE rather than at read-out: a whole-frame fill is
    // indexed by pixel and cannot reverse, so the volley is fired the other
    // way down the strip instead of being reflected on the way to the LEDs.
    // Steady state is identical; the difference is that flipping the toggle
    // mid-run now turns the volley around instead of instantly reflecting the
    // trail already in the air, and the trail follows within a few frames.
    for (var j = from; j <= to; j++) {
      if (j >= pixelCount) break
      var w = mirror ? pixelCount - 1 - j : j
      bufR[w] = min(1, bufR[w] + palR[c])
      bufG[w] = min(1, bufG[w] + palG[c])
      bufB[w] = min(1, bufB[w] + palB[c])
    }

    if (boltPos[i] >= pixelCount) {
      boltPos[i] = 0                  // re-fire from the start
      boltVel[i] = 1 + random(1.2)    // fresh random speed; color kept
    }
  }
}

// One `fillRGB` for the whole strip. The trail buffers are already indexed by
// pixel, the ambient underlay is folded in with the two constant arrays above,
// and the blue-lightning quirk is a genuine scalar — the whole strip takes the
// launch pixel's blue — which `fillRGB` broadcasts for free. `min(v, 1)` is
// gone: the fill quantizes through the same clamp `rgb()` applies.
// Index space, so this stays a mapless strip pattern and asks for no geometry.
export function renderFrame() {
  // with the volley mirrored, the bolts re-fire from the far end, so that is
  // where the lightning flash is read from
  var b = blueLightning ? bufB[mirror ? pixelCount - 1 : 0] : bufB
  arrayAdd(bufR, ambR)
  arrayAdd(bufG, ambG)
  fillRGB(bufR, bufG, b)
  arraySub(bufR, ambR)
  arraySub(bufG, ambG)
}
