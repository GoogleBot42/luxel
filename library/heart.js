// name: heart
// Clean-room reimplementation from a prose functional description of the
// community pattern "heart"; original source never consulted. A filled,
// antialiased heart built from construction geometry (a diamond bottom plus
// two circular lobes), beating about once a second, filled with a vertical
// rainbow gradient that sweeps a whole wheel per beat, and true black outside.

// Local frame: the heart spans p in -1..1 and q in -1..0.5, lobes UP and the
// point DOWN, its bounding box centred on q = -0.25. BX/BY convert display
// coordinates into that frame at the top of the beat: the figure then fills
// 14 of 16 columns and 13 of 16 rows and never touches the outer column.
var BX = 1.954                  // display half-width 1/BX = 0.512
var BY = 1.578                  // display half-height 0.75/BY = 0.475
var BEAT_LOW = 0.694            // smallest beat scale (area swing ~2x)

// interval argument is a period of 65.536 s, hence the divisor.
export var beatSeconds = 1.304  // one pulse — and one full rainbow — per beat
var beatInterval = beatSeconds / 65.536
var aaDist = 0.045              // antialias distance (fraction of display)

export var heartScale = 1       // updated per frame by the beat
var scale = 1
var phase = 0                   // beat phase, 0..1 — also the hue sweep

// Edge softness, as a percentage of the display's width.
//# min=1 max=12 step=0.5 default=4.5
export function sliderEdgeSoftness(v) {
  aaDist = v / 100
}

// Seconds per heartbeat. The rainbow stays locked to it: one full wheel
// per beat, whatever the rate.
//# min=0.4 max=3 step=0.05 default=1.3
export function sliderBeatSeconds(v) {
  beatSeconds = max(v, 0.1)
  beatInterval = beatSeconds / 65.536
}

export function beforeRender(delta) {
  // sawtooth -> half-sine swells the figure between BEAT_LOW and full size,
  // anchored so the centre stays put; the same sawtooth drives the hue
  phase = time(beatInterval)
  scale = BEAT_LOW + (1 - BEAT_LOW) * sin(phase * PI)
  heartScale = scale
}

export function render2D(index, x, y) {
  var kx = BX / scale
  var ky = BY / scale
  var p = (x - 0.5) * kx         // mirror via abs -> only half is evaluated
  var q = (0.5 - y) * ky - 0.25  // +q is UP the display: lobes on top
  var au = abs(p)

  var aaLocal = aaDist * kx
  var perp = -1                  // signed distance inside the boundary

  if (q < 0) {
    // lower diamond: 45-degree edges from the bottom tip to the shoulders
    perp = ((q + 1) - au) / SQRT2
  } else {
    // upper lobes: circle radius 0.5 centered at (0.5, 0); the two mirrored
    // circles meet at the origin, carving the notch between the lobes
    perp = 0.5 - hypot(au - 0.5, q)
  }

  var cover = clamp(perp / aaLocal, 0, 1)
  if (cover <= 0) {
    hsv(0, 0, 0)
    return
  }

  // ~half a wheel top-to-bottom, and a whole wheel per heartbeat: the fill
  // colour resets in step with the pulse
  var hue = y * 0.5 + phase
  hsv(hue, 1, cover * cover)     // squared coverage softens the falloff
}
