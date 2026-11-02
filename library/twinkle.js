// name: Twinkle
// tags: both, ambient, sound, holiday
// Clean-room reimplementation from the prose specs docs/pattern-specs/twinkle.md,
// twinkle-2.md, twinkly-stars.md, chill-confetti.md, blink-fade.md, quiet-blinkfade.md,
// sound-blinkfade.md, autumn-colors.md, spring-colors.md, christmas-rg-fade.md,
// halloween-color-twinkles.md, firework-dust.md — reimagined, not a replica.
// Stars ease up over Attack, then the persistent frame's fade() decays them over
// Fade, so the cost is O(births), not O(pixels): no per-pixel buffers, and a
// 128x128 panel is as cheap as a strip. Ten palettes (frost, picked, hue range,
// rainbow, drifting confetti, autumn, spring pastel, Halloween, holiday, classic
// bulbs); an optional lit backdrop the stars decay back into (max-blended); a
// Sound toggle lets loudness drive the birth rate. Index space: on a panel the
// births scatter across the whole matrix.

var palette = 0        // 0 frost | 1 picked | 2 hue range | 3 rainbow | 4 drift | 5 autumn | 6 spring | 7 Halloween | 8 holiday | 9 classic
var density = 10       // births per second per 100 pixels
var attack = 0.08      // seconds to flare up
var fadeSec = 1.5      // seconds to decay to ~2 %
var minPeak = 50       // % — each star peaks somewhere between this and full
var backdrop = 0       // % level of the field the stars decay back into
var lowDeg = 180       // hue range palette, degrees
var highDeg = 300
var sound = 0
var pickH = 0.11       // picked colour (palette 1)
var pickS = 0.55
var fieldH = 0.64      // backdrop colour
var fieldS = 1

// 0 frost | 1 picked | 2 hue range | 3 rainbow | 4 drift | 5 autumn | 6 spring pastel | 7 Halloween | 8 holiday red/green | 9 classic RGBY
//# min=0 max=9 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 9) }
// Births per second for every 100 pixels — a panel twinkles as densely as a strip.
//# min=0.5 max=60 step=0.5 default=10
export function sliderDensity(v) { density = clamp(v, 0.5, 60) }
//# min=0.02 max=1 step=0.01 default=0.08
export function sliderAttackSeconds(v) { attack = clamp(v, 0.02, 1) }
//# min=0.1 max=8 step=0.1 default=1.5
export function sliderFadeSeconds(v) { fadeSec = clamp(v, 0.1, 8) }
// Faintest a star may peak; 100 makes every star flare to full.
//# min=20 max=100 step=1 default=50
export function sliderMinPeakPercent(v) { minPeak = clamp(v, 20, 100) }
// Level of the Field colour behind the stars (0 = black sky).
//# min=0 max=100 step=1 default=0
export function sliderBackdropPercent(v) { backdrop = clamp(v, 0, 100) }
//# min=0 max=360 step=1 default=180
export function sliderLowHueDegrees(v) { lowDeg = clamp(v, 0, 360) }
//# min=0 max=360 step=1 default=300
export function sliderHighHueDegrees(v) { highDeg = clamp(v, 0, 360) }
// Loudness (energyAverage, auto-ranged) drives the birth rate.
//# default=0
export function toggleSound(v) { sound = v > 0.5 }
// Star colour for palette 1 (hue + saturation; V is ignored).
export function hsvPickerColor(h, s, v) { pickH = h; pickS = s }
// Backdrop colour (hue + saturation; its level is Backdrop %).
export function hsvPickerField(h, s, v) { fieldH = h; fieldS = s }

export var energyAverage = 0

var POOL = 256         // stars still in their attack phase
var aIdx = array(POOL)
var aAge = array(POOL)
var aH = array(POOL)
var aS = array(POOL)
var aV = array(POOL)
var live = 0
var dt = 0
var births = 0
var keep = 1           // fade factor owed to the frame, applied in >= 6 % steps
var seg = 0
var seeded = 0
var loud = 0.05
var driftH = 0
var bh = 0
var bs = 1
var bv = 1

function pick() {
  var r = random(1)
  bs = 1
  bv = 1
  if (palette == 0) { bh = 0.55 + random(0.1); bs = 0.12 + random(0.3) }
  else if (palette == 1) { bh = pickH + (r - 0.5) * 0.03; bs = pickS }
  else if (palette == 2) {
    var span = highDeg - lowDeg
    if (span < 0) span += 360
    bh = (lowDeg + r * span) / 360
  }
  else if (palette == 3) bh = r
  else if (palette == 4) bh = driftH + (r - 0.5) * 0.09
  else if (palette == 5) {
    if (r < 0.3) bh = 0
    else if (r < 0.6) { bh = 0.045; bv = 0.6 }
    else if (r < 0.97) bh = 0.075
    else bh = 0.14
  }
  else if (palette == 6) {
    bs = 0.45
    if (r < 0.3) bh = 0.94
    else if (r < 0.6) bh = 0.77
    else if (r < 0.97) bh = 0.4
    else { bh = 0.14; bs = 0.6 }
  }
  else if (palette == 7) {
    if (r < 0.48) bh = 0.06
    else if (r < 0.96) bh = 0.76 + random(0.05)
    else bh = 0.3
  }
  else if (palette == 8) bh = r < 0.5 ? 0 : 0.333
  else {
    var k = floor(r * 4)
    bh = k == 0 ? 0 : k == 1 ? 0.333 : k == 2 ? 0.64 : 0.12
  }
  bv = bv * (minPeak + random(100 - minPeak)) / 100
}

// Next birth slot: twelve segments visited in a 5-step stride, so a burst
// spreads across the strip (or panel) instead of clumping.
function spot() {
  seg = (seg + 5) % 12
  return floor((seg + random(1)) * (pixelCount / 12))
}

export function beforeRender(delta) {
  dt = min(delta, 200) / 1000
  driftH = time(0.73)
  var rate = density / 100 * pixelCount   // (16.16: divide first)
  if (sound) {
    loud = max(max(energyAverage, loud * pow(0.5, dt / 4)), 0.02)
    var l = energyAverage / loud
    rate = rate * (0.2 + 2.3 * l * l)
  }
  births = births + rate * dt
  keep = keep * pow(0.02, dt / fadeSec)
}

export function renderBulk() {
  var i
  if (!seeded) {
    // Start in steady state rather than on a black frame.
    seeded = 1
    var n = min(min(density / 100 * pixelCount, 1000) * fadeSec * 0.6, 2000)
    for (i = 0; i < n; i++) {
      pick()
      var e = random(1)
      hsv(bh, bs, bv * e * e)
      setPixel(spot())
    }
  }
  if (keep < 0.94) { fade(keep); keep = 1 }
  if (backdrop > 0) {
    hsv(fieldH, fieldS, backdrop / 100)
    fillRangeAA(0, pixelCount, 2)
  }
  var n = min(floor(births), 2000)
  births = births - floor(births)
  for (i = 0; i < n; i++) {
    pick()
    var p = spot()
    if (live < POOL && attack > dt) {
      aIdx[live] = p
      aAge[live] = 0
      aH[live] = bh
      aS[live] = bs
      aV[live] = bv
      live++
    } else {
      hsv(bh, bs, bv)
      fillRangeAA(p, p + 1, 2)
    }
  }
  // Stamp the flaring stars (smoothstep rise), max-blended over the decay.
  i = 0
  while (i < live) {
    aAge[i] += dt
    var u = min(aAge[i] / attack, 1)
    hsv(aH[i], aS[i], aV[i] * u * u * (3 - 2 * u))
    fillRangeAA(aIdx[i], aIdx[i] + 1, 2)
    if (u >= 1) {
      live--
      aIdx[i] = aIdx[live]
      aAge[i] = aAge[live]
      aH[i] = aH[live]
      aS[i] = aS[live]
      aV[i] = aV[live]
    } else i++
  }
}
