// name: sparks
// Clean-room reimplementation from a prose functional description of the
// community pattern "sparks"; original source never consulted.

var MAX_SPARKS = 64              // buffer size; the count control caps here
var numSparks = 24               // (control) live sparks
var timeScale = 0.1              // (control) delta scale for the whole shower
var persistence = 0.2            // (control) share of a trail kept per frame
var sparkHue = 0.02              // (control) ember hue

var pixels = array(pixelCount)   // per-pixel energy buffer, persists across frames
var energy = array(MAX_SPARKS)
var pos = array(MAX_SPARKS)

// Stagger the initial shower so the strip is busy from the first frame.
// Only the initially live sparks are seeded; raising the count later hands
// the extra ones zero energy, so they respawn on the next frame.
var i
for (i = 0; i < numSparks; i++) {
  energy[i] = random(1)
  pos[i] = random(pixelCount)
}

// How many sparks are in flight at once.
//# min=1 max=64 step=1 default=24
export function sliderSparks(v) { numSparks = clamp(floor(v), 1, MAX_SPARKS) }

// Speed of the whole shower, as a percentage of the stock rate.
//# min=10 max=300 step=10 default=100
export function sliderSpeed(v) { timeScale = 0.1 * max(1, v) / 100 }

// Share of a trail's brightness carried into the next frame: low values
// leave pinpoint sparks, high values leave long comet tails.
//# min=0 max=90 step=5 default=20
export function sliderTrailPersistence(v) { persistence = clamp(v, 0, 95) / 100 }

//# min=0 max=360 step=1 default=7
export function sliderSparkHue(v) { sparkHue = v / 360 }

export function beforeRender(delta) {
  delta *= timeScale                 // scale time down ~an order of magnitude
  feedback(pixels, persistence)      // multiplicative decay: ~a fifth survives each frame

  for (i = 0; i < numSparks; i++) {
    if (energy[i] <= 0) {
      // respawn: energy a bit above unity, position in the first few pixels
      energy[i] = 1 + random(0.3)
      pos[i] = random(4)
    }
    // friction ~ delta, inversely proportional to strip length, so a spark
    // travels roughly the whole strip regardless of pixelCount
    energy[i] -= delta * 0.5 / pixelCount
    // velocity ~ energy squared: flare fast, then die slow
    pos[i] += energy[i] * energy[i] * delta
    if (pos[i] >= pixelCount) {
      pos[i] = 0
      energy[i] = 0                  // respawns next frame
    } else {
      pixels[floor(pos[i])] += max(0, energy[i])   // additive deposit makes the trail
    }
  }
}

export function render(index) {
  var p = pixels[index]
  var v = p * p                      // squared for punchy gamma
  // hot cores desaturate toward white; faint trails stay deep ember orange
  hsv(sparkHue, 1.1 - v, v)
}
