// name: _Fairies
// tags: both, ambient
// Clean-room reimplementation from the prose spec docs/pattern-specs/fairies.md
// — reimagined, not a replica.
// Dense, stationary fairy dust: points pop to full in the picked hue (magenta-pink
// by default, optionally spread around it) and fade while new ones pop elsewhere,
// with occasional pure-white glints among them. Built on the persistent frame —
// one fade() plus a constant birth rate of setPixel() — so it costs O(births),
// keeps no per-pixel buffers and runs unchanged on any strip or panel.

var hue = 0.9          // magenta-pink
var speed = 1          // twinkle rate multiplier: 1 = ~0.75 s half-life
var density = 16       // births per second per 100 pixels
var spread = 0         // hue spread, degrees either side of the picked hue

// Fairy colour — only the hue is used (fairies are fully saturated).
export function hsvPickerPrimaryColor(h, s, v) { hue = h }
// Twinkle rate, HIGHER IS FASTER: 1 fades a fairy to half in ~0.75 s.
//# min=0.2 max=6 step=0.1 default=1
export function sliderSpeed(v) { speed = clamp(v, 0.05, 20) }
// New fairies per second for every 100 pixels.
//# min=1 max=200 step=1 default=16
export function sliderDensity(v) { density = clamp(v, 1, 200) }
// Scatter each fairy's hue up to this many degrees either side of the colour.
//# min=0 max=180 step=1 default=0
export function sliderHueSpreadDegrees(v) { spread = clamp(v, 0, 180) }

var births = 0
var keep = 1           // fade owed to the frame, applied in >= 6 % steps
var seeded = 0

function fairy(v) {
  if (random(1) < 0.12) hsv(0, 0, v)                  // white glint
  else hsv(hue + (random(2) - 1) * spread / 360, 1, v)
  setPixel(random(pixelCount))
}

export function beforeRender(delta) {
  var dt = min(delta, 200) / 1000
  // births this frame; the 15 % cap keeps 16.16 safe on a 128x128 chain
  births = births + min(density * speed / 100 * dt, 0.15) * pixelCount
  keep = keep * pow(0.5, dt * speed / 0.75)
}

export function renderBulk() {
  var i
  if (!seeded) {
    // Open on a full field instead of an empty one.
    seeded = 1
    var n = min(pixelCount * 0.3, 3000)
    for (i = 0; i < n; i++) { var e = random(1); fairy(e * e) }
  }
  if (keep < 0.94) { fade(keep); keep = 1 }
  var n = min(floor(births), 3000)
  births = births - floor(births)
  for (i = 0; i < n; i++) fairy(1)
}
