// name: Lightning clouds
// Clean-room reimplementation from a prose functional description of the
// community pattern "Lightning clouds"; original source never consulted.

// A dim night sky of deep-blue clouds drifts slowly sideways, the shapes
// morphing over tens of seconds. At random moments a flash erupts at a
// random spot (often partly off-screen — distant lightning): for ~half a
// second nearby clouds light up from behind — dense areas glow hardest,
// squared density gives the lit-from-within look — and flash cores wash out
// toward white before decaying back to the faint ambient blue.

var SEED = 8.15
var FLASH_MS = 500       // flash duration
var RADIUS = 1.5         // flash glow radius in world units
var HUE = 0.62           // deep sky blue
var STRIKE_P = 0.0015    // strike chance per millisecond (~1.5 / s)
var ZOOM = 4             // world units the display spans (bigger = smaller clouds)

// ---- controls ------------------------------------------------------------
// Average lightning strikes per second (a strike cannot start while
// another is still lit, so the flashes actually seen run a little lower).
//# min=0 max=5 step=0.1 default=1.5
export function sliderStrikeRate(v) {
  STRIKE_P = clamp(v, 0, 5) / 1000
}

// How long one flash lasts, in seconds.
//# min=0.05 max=2 step=0.05 default=0.5
export function sliderFlashTime(v) {
  FLASH_MS = max(v, 0.05) * 1000
}

// Radius of a flash's glow, in world units — the display is ZOOM units
// across, so the default 1.5 lights a bit over a third of the width.
//# min=0.5 max=4 step=0.1 default=1.5
export function sliderFlashRadius(v) {
  RADIUS = clamp(v, 0.5, 4)
}

// How many world units of cloud the display spans: small = a close-up of
// a few big clouds, large = a wide sky full of small ones.
//# min=1 max=12 step=0.5 default=4
export function sliderCloudZoom(v) {
  ZOOM = clamp(v, 1, 12)
}

// Sky color, as a position on the color wheel (0.62 = deep night blue).
//# min=0 max=1 step=0.01 default=0.62
export function sliderSkyHue(v) {
  HUE = clamp(v, 0, 1)
}

var flashMs = 0          // countdown; 0 when idle
var flashX = 0
var flashY = 0
var flashFrac = 0        // fraction of flash remaining this frame

var driftX = 0           // slow sideways cloud translation
var driftT = 0           // noise time axis — cloud shapes evolve

export function beforeRender(delta) {
  flashMs = max(0, flashMs - delta)
  if (flashMs <= 0) {
    // per-second-normalized strike chance (~1.5 strikes/s on average)
    if (random(1) < delta * STRIKE_P) {
      flashMs = FLASH_MS
      // uniform over a region 1.5 units larger than the visible window on
      // every side, so strikes often land partly off-screen
      var half = ZOOM / 2 + 1.5
      flashX = -half + random(half * 2)
      flashY = -half + random(half * 2)
    }
  }
  flashFrac = flashMs / FLASH_MS

  // two very slow clocks, scaled up: one drives the noise time axis
  // (shape evolution), the other the steady sideways drift
  driftT = time(0.9) * 30
  driftX = time(0.35) * 16
}

// two-octave perlin turbulence: lacunarity 2, high per-octave gain
function cloudDensity(px, py) {
  var d = abs(perlin(px + driftX, py, driftT, SEED))
       + 0.9 * abs(perlin((px + driftX) * 2, py * 2, driftT * 2, SEED))
  return clamp(d / 1.4, 0, 1)
}

export function render2D(index, x, y) {
  // recenter and zoom out: the window shows a ZOOM-unit-wide slice
  var px = (x - 0.5) * ZOOM
  var py = (y - 0.5) * ZOOM

  var dens = cloudDensity(px, py)

  // broad radial glow, fading linearly over the flash lifetime
  var li = 0
  if (flashMs > 0) {
    li = max(0, (RADIUS - dist(px, py, flashX, flashY)) * flashFrac)
  }

  // squared density: thick cloud lights up disproportionately (backlit look)
  var glow = dens * dens * li * 2

  // ambient floor, raised slightly while any flash is active
  var floorAmt = max(0.04, flashFrac * 0.2)
  var base = dens * floorAmt

  var v = max(base, glow)
  var s = max(0, 0.55 - glow)   // flash cores whiten
  hsv(HUE, s, clamp(v, 0, 1))
}
