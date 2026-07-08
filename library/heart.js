// name: heart
// Clean-room reimplementation from a prose functional description of the
// community pattern "heart"; original source never consulted. A filled,
// antialiased heart built from construction geometry (a diamond bottom plus
// two circular lobes), beating once a second, with a drifting vertical
// rainbow gradient fill and true black outside.

export var heartHeight = 0.65   // updated per frame by the beat
export var aaDist = 0.045       // antialias distance (fraction of display)
export var beatRate = 0.016     // time() interval for the beat (~1s)

var curH = 0.65

//# min=0.01 max=0.12 step=0.005 default=0.045
export function sliderAntialias(v) {
  aaDist = 0.01 + v * 0.11
}

//# min=0.005 max=0.04 step=0.001 default=0.016
export function sliderBeatRate(v) {
  beatRate = 0.005 + v * 0.035
}

export function beforeRender(delta) {
  // slow sawtooth -> half-sine swells height between ~0.5 and ~0.8 of display,
  // anchored so the vertical center stays put
  var phase = time(beatRate)
  curH = 0.5 + 0.3 * sin(phase * PI)
  heartHeight = curH
}

export function render2D(index, x, y) {
  // scale display into a local frame 1.5 tall; heart spans local q -1..0.5,
  // recentered so display-center 0.5 is fixed as the height beats
  var k = 1.5 / curH
  var p = (x - 0.5) * k          // mirror via abs -> only half is evaluated
  var q = (y - 0.5) * k - 0.25
  var au = abs(p)

  var aaLocal = aaDist * k
  var perp = -1                  // signed distance inside the boundary

  if (q < 0) {
    // lower diamond: 45-degree edges from the bottom tip to the shoulders
    perp = ((q + 1) - au) / SQRT2
  } else {
    // upper lobe: circle radius 0.5 centered at (0.5, 0); the two mirrored
    // circles meet at the origin, carving the notch between the lobes
    perp = 0.5 - hypot(au - 0.5, q)
  }

  var cover = clamp(perp / aaLocal, 0, 1)
  if (cover <= 0) {
    hsv(0, 0, 0)
    return
  }

  var hue = y * 0.5 + time(0.05)     // ~half wheel top-to-bottom, drifting
  hsv(hue, 1, cover * cover)         // squared coverage softens the falloff
}
