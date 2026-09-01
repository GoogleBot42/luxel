// name: Fireflies
// Curated example (hand-written showcase of the Luxel language/builtins).
// A particle system: eight fireflies wander the strip, each with its own
// position, drift, and flash clock held in parallel arrays. Flashes are
// eased triangles deposited into a decaying frame buffer.
MAXFLIES = 32
numFlies = 8
pos = array(MAXFLIES)
vel = array(MAXFLIES)
clocks = array(MAXFLIES)
leds = array(pixelCount)

// Tunables — the setters below convert real units into exactly these values,
// so an untouched pattern renders the same as it always did.
velScale = 0.012    // px per ms across the full drift range (+/-6 px/s)
clockRate = 0.0003  // clock units per ms (~3.3 s per flash cycle)
decayK = 0.06       // feedback exponent scale (~52 ms trail half-life)
duty = 4            // the flash fills 1/duty of the cycle

// The original eight are seeded from the RNG exactly as before; the spare
// slots are seeded from a deterministic low-discrepancy sequence so that
// raising the count never perturbs the random stream the first eight use.
for (i = 0; i < 8; i++) {
  pos[i] = random(pixelCount)
  vel[i] = (random(1) - 0.5) * velScale
  clocks[i] = random(1)
}
for (i = 8; i < MAXFLIES; i++) {
  pos[i] = frac(i * 0.6180339) * pixelCount
  vel[i] = (frac(i * 0.7548777) - 0.5) * velScale
  clocks[i] = frac(i * 0.3819661)
}

// How many fireflies wander the strip at once.
//# min=1 max=32 step=1 default=8
export function sliderFireflyCount(v) { numFlies = clamp(floor(v), 1, MAXFLIES) }

// Seconds between one firefly's flashes.
//# min=0.3 max=30 step=0.1 default=3.3
export function sliderFlashSeconds(v) { clockRate = 1 / (max(v, 0.2) * 1000) }

// Share of that cycle the flash itself occupies — bigger reads as a slow glow.
//# min=2 max=100 step=1 default=25
export function sliderFlashDutyPercent(v) { duty = 100 / clamp(v, 2, 100) }

// How far a firefly drifts, in pixels per second.
//# min=0 max=60 step=0.5 default=6
export function sliderDriftPixelsPerSecond(v) { velScale = clamp(v, 0, 60) / 500 }

export function beforeRender(delta) {
  feedback(leds, pow(0.8, delta * decayK))
  for (var i = 0; i < numFlies; i++) {
    pos[i] += vel[i] * delta
    if (pos[i] < 0) pos[i] += pixelCount
    if (pos[i] >= pixelCount) pos[i] -= pixelCount
    clocks[i] += delta * clockRate       // ~3.3 s per cycle by default
    if (clocks[i] >= 1) {
      clocks[i] -= 1
      vel[i] = (random(1) - 0.5) * velScale // pick a new drift each cycle
    }
    f = clocks[i] * duty                 // flash fills the first 1/duty
    if (f < 1) {
      leds[floor(pos[i])] += easeInOutQuad(triangle(f))
    }
  }
}

export function render(index) {
  v = min(leds[index], 1)
  // warm yellow-green, whitening slightly at full flash
  hsv(0.19 - v * 0.04, 1 - v * 0.4, v * v)
}
