// name: Twinkling Classic Xmas Strands
// Clean-room reimplementation from a prose functional description of the
// community pattern "Twinkling Classic Xmas Strands"; original source never consulted.
//
// Each pixel keeps one fixed color slot from a five-color holiday palette
// (assigned once, urn-style, so colors stay locally balanced and never repeat
// on neighbors). Bulbs twinkle from a dim resting glow up to a bright swell,
// while the whole strand slowly cross-fades between three palette moods.

var SLOTS = 5           // red, green, amber, blue, purple
var PALETTES = 3
var URN_LAG = 3         // draws before a used color's ball returns to the urn
var REST = 0.25         // dim resting brightness multiplier

// ---- palettes: h, s, v per (palette, slot) ----------------------------------
var pal = array(PALETTES * SLOTS * 3)
function setPal(p, slot, h, s, v) {
  var i = (p * SLOTS + slot) * 3
  pal[i] = h
  pal[i + 1] = s
  pal[i + 2] = v
}
// Palette A "classic": fully vivid
setPal(0, 0, 0.00, 1, 1)      // red
setPal(0, 1, 0.333, 1, 1)     // green
setPal(0, 2, 0.09, 1, 1)      // amber
setPal(0, 3, 0.667, 1, 1)     // blue
setPal(0, 4, 0.85, 1, 1)      // purple, near magenta
// Palette B "aged pastels, warm"
setPal(1, 0, 0.02, 0.60, 0.85)  // washed warm red
setPal(1, 1, 0.36, 0.35, 0.40)  // very dim desaturated green
setPal(1, 2, 0.10, 0.55, 0.90)  // washed amber
setPal(1, 3, 0.55, 0.50, 0.45)  // dim muted teal-ish blue
setPal(1, 4, 0.88, 0.80, 0.85)  // strong pinkish purple, slightly dimmed
// Palette C "cool"
setPal(2, 0, 0.98, 0.35, 0.55)  // very pale dusty rose
setPal(2, 1, 0.333, 1, 1)       // vivid green
setPal(2, 2, 0.09, 0.15, 0.95)  // soft warm-tinged white (replaces amber)
setPal(2, 3, 0.667, 1, 1)       // vivid blue
setPal(2, 4, 0.78, 1, 1)        // vivid violet

// ---- once-at-startup per-pixel state ----------------------------------------
var slots = array(pixelCount)   // fixed color slot per pixel
var phase = array(pixelCount)   // stable twinkle phase offset per pixel

// Urn draw with delayed replacement: one ball per color; each draw removes the
// picked ball and, once past the lag, returns the ball drawn URN_LAG pixels ago.
// Never repeats the previous pixel's color; keeps short windows nearly balanced.
var balls = array(SLOTS)
var history = array(URN_LAG)
var cand = array(SLOTS)
function assignColors() {
  var i, c, n, prev, pick
  for (c = 0; c < SLOTS; c++) balls[c] = 1
  for (i = 0; i < pixelCount; i++) {
    if (i >= URN_LAG) balls[history[i % URN_LAG]] += 1   // delayed replacement
    prev = i > 0 ? slots[i - 1] : -1
    n = 0
    for (c = 0; c < SLOTS; c++) {
      if (balls[c] > 0 && c != prev) {
        cand[n] = c
        n += 1
      }
    }
    if (n > 0) pick = cand[floor(random(n))]
    else pick = floor(random(SLOTS))                     // can't happen; safety
    balls[pick] -= 1
    slots[i] = pick
    history[i % URN_LAG] = pick
    phase[i] = random(1)                                 // stable twinkle phase
  }
}
assignColors()

// ---- controls ----------------------------------------------------------------
var cycleSec = 15
//# min=0 max=1 step=0.01 default=0.45
export function sliderCycleTime(v) {
  cycleSec = 3 + v * 27          // several seconds up to about half a minute
}

var twinkles = 0.5
//# min=0 max=1 step=0.01 default=0.5
export function sliderTwinkles(v) {
  twinkles = v
}

var autoFade = 1
//# min=0 max=1 step=1 default=1
export function sliderAutoFadePalettes(v) {
  autoFade = v > 0.5             // acts as a toggle: on above midpoint
}

var manualSel = 0
//# min=0 max=1 step=0.01 default=0
export function sliderManualPaletteSelect(v) {
  manualSel = v
}

// ---- per-frame ----------------------------------------------------------------
var palAcc = 0
var pi = 0, pj = 1, blend = 0
var twPhase = 0
var twWindow = 0.15

export function beforeRender(delta) {
  // palette cross-fade position
  palAcc = mod(palAcc + delta / 1000, cycleSec)
  var pos
  if (autoFade) pos = palAcc / cycleSec * PALETTES
  else pos = min(manualSel, 0.999) * PALETTES
  pi = floor(pos) % PALETTES
  blend = pos - floor(pos)
  pj = (pi + 1) % PALETTES

  // twinkle clock: period scales with strip length (constant twinkles/second
  // per strand) and shrinks as the twinkles control rises
  var period = pixelCount * 0.15 / (0.25 + twinkles * 4)
  twPhase = frac(twPhase + delta / 1000 / period)
  twWindow = clamp(1 / period, 0.06, 0.5)   // about a second's worth of swell
}

export function render(index) {
  var s = slots[index]
  var i1 = (pi * SLOTS + s) * 3
  var i2 = (pj * SLOTS + s) * 3

  // hue interpolates the short way around the circle
  var dh = pal[i2] - pal[i1]
  if (dh > 0.5) dh -= 1
  if (dh < -0.5) dh += 1
  var h = pal[i1] + dh * blend
  var sat = mix(pal[i1 + 1], pal[i2 + 1], blend)
  var val = mix(pal[i1 + 2], pal[i2 + 2], blend)

  // twinkle multiplier
  var m = REST
  if (twinkles > 0.01) {
    var c = frac(twPhase + phase[index])
    if (c < twWindow) {
      var u = c / twWindow
      m = REST + sin(u * PI) * 1.15          // swells past 1 for a held peak
    }
    if (twinkles > 0.97) m = frac(m)         // max: wrap -> blink out at peak
    else m = min(m, 1)                       // otherwise clip: flattened top
    if (random(1) < twinkles * 0.02) m = 0   // sparkly momentary blanks
  }

  // gamma-ish easing so mid-fade colors don't wash out or muddy
  hsv(h, sqrt(sat), val * val * m)
}
