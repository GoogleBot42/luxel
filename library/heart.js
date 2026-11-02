// name: Heart
// Clean-room reimplementation from the prose spec docs/pattern-specs/heart.md
// — reimagined, not a replica.
// A filled, antialiased heart (a diamond point plus two circular lobes, as a
// signed distance) that beats — a single swell or a lub-dub double beat — and
// fills with a beat-locked rainbow, a picked colour, or an even OKLCH
// pink-to-red that flushes brighter on every beat. True black outside.

// The heart spans p -1..1 and q -1..0.5 in its own frame (lobes up); BX/BY
// map the display into it so the figure fills ~7/8 of the panel at full beat.
var BX = 1.954
var BY = 1.578
var BEAT_LOW = 0.694            // smallest beat scale (area swing ~2x)

var beatSeconds = 1.3           // one beat — and one rainbow wheel — per period
//# min=0.4 max=3 step=0.05 default=1.3
export function sliderBeatSeconds(v) { beatSeconds = clamp(v, 0.4, 3) }

var aaDist = 0.045              // edge softness, fraction of the display width
//# min=1 max=12 step=0.5 default=4.5
export function sliderEdgeSoftness(v) { aaDist = clamp(v, 1, 12) / 100 }

var fillMode = 0                // 0 rainbow, 1 picked colour, 2 OKLCH pink-red
//# min=0 max=2 step=1 default=0
export function sliderFill(v) { fillMode = clamp(floor(v), 0, 2) }

var beatMode = 0                // 0 single swell, 1 lub-dub
//# min=0 max=1 step=1 default=0
export function sliderBeat(v) { beatMode = clamp(floor(v), 0, 1) }

// Fill colour for Fill 1: hue and saturation (the picker's brightness is ignored).
var pickH = 0.95, pickS = 0.9
export function hsvPickerColor(h, s, v) { pickH = h; pickS = s }

var phase = 0, scale = 1, env = 0, kx = BX, ky = BY, aaLocal = 0.1

function bump(t) { return t > 0 && t < 1 ? sin(t * PI) : 0 }

export function beforeRender(delta) {
  phase = mod(phase + delta / 1000 / beatSeconds, 1)
  if (beatMode == 0) env = sin(phase * PI)
  else env = max(bump(phase / 0.24), 0.72 * bump((phase - 0.27) / 0.24))
  scale = BEAT_LOW + (1 - BEAT_LOW) * env
  kx = BX / scale
  ky = BY / scale
  aaLocal = aaDist * kx
}

export function render2D(index, x, y) {
  var au = abs((x - 0.5) * kx)    // mirrored: only one half is evaluated
  var q = (0.5 - y) * ky - 0.25   // +q is up the display
  var perp = q < 0 ? ((q + 1) - au) * 0.7071 : 0.5 - hypot(au - 0.5, q)
  var cover = perp / aaLocal
  if (cover <= 0) { rgb(0, 0, 0); return }
  if (cover > 1) cover = 1
  cover = cover * cover
  if (fillMode == 0) hsv(y * 0.5 + phase, 1, cover)
  else if (fillMode == 1) hsv(pickH, pickS, cover * (0.72 + 0.28 * env))
  else oklch((0.5 + 0.17 * env + 0.1 * (1 - y)) * cover, 0.21 * cover, 0.97 + y * 0.12)
}
