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
    if (random(1) < delta * 0.0015) {
      flashMs = FLASH_MS
      // uniform over a region noticeably larger than the +-2 visible window
      flashX = -3.5 + random(7)
      flashY = -3.5 + random(7)
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
  // recenter and zoom out: the window shows a 4-unit-wide slice, +-2
  var px = (x - 0.5) * 4
  var py = (y - 0.5) * 4

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
