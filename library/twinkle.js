// name: Twinkle
// Clean-room reimplementation from a prose functional description of the
// community pattern "Twinkle"; original source never consulted.

// Icy star-field on black: pixels flare quickly to a frosty blue-white
// glint, then die away slowly. New stars appear in small random bursts,
// spread evenly along the strip by a round-robin segment walk.

var LIFETIME = 2          // seconds each star lives
var SPAWN_INTERVAL = 0.25 // seconds between spawn bursts
var MAX_BURST = 12        // up to this many new stars per burst
var STAR_HUE = 0.63       // cool blue...
var STAR_SAT = 0.22       // ...at low saturation: frosty blue-white

var bri = array(pixelCount)  // per-pixel brightness
var age = array(pixelCount)  // per-pixel elapsed lifetime; 0 = dark
var spawnAccum = 0
var segment = 0              // round-robin segment counter
var segLen = pixelCount / MAX_BURST

export function beforeRender(delta) {
  var dt = delta / 1000
  var i
  for (i = 0; i < pixelCount; i++) {
    if (age[i] > LIFETIME) {
      // reclaim: by now the envelope has faded essentially to black
      age[i] = 0
      bri[i] = 0
    } else if (age[i] > 0) {
      // fast-attack / slow-decay envelope: k^2 * e^-k (gamma-like bump).
      // Peaks near k = 2 at ~0.54, so divide to reach ~full brightness
      // early in the lifetime, then tail off long and smooth.
      var k = age[i] / LIFETIME * 8
      bri[i] = min(1, k * k * exp(-k) / 0.54)
      age[i] += dt
    }
  }

  spawnAccum += dt
  if (spawnAccum > SPAWN_INTERVAL) {
    spawnAccum = 0
    // a burst of 0..MAX_BURST new stars, each placed at a random spot
    // inside the current segment; walking segments round-robin keeps
    // twinkles evenly spread instead of clumping
    var burst = floor(random(MAX_BURST + 1))
    for (i = 0; i < burst; i++) {
      var p = floor(segment * segLen + random(segLen))
      if (p < pixelCount) age[p] = 0.001  // tiny nonzero age = alive
      segment = (segment + 1) % MAX_BURST
    }
  }
}

export function render(index) {
  hsv(STAR_HUE, STAR_SAT, bri[index])
}
