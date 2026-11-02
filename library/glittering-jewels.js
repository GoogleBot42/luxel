// name: Glittering Jewels
// Curated original (hand-written showcase of the Luxel language/builtins).
// Soft gems bloom at random spots and fade: a wide half-strength halo, a hot
// core whose radius breathes with the bloom, and a staggered glint that
// whitens the core (on a panel it flares into a four-point star). Gem
// palette or one picked colour. renderBulk + a few anti-aliased splats per
// jewel: splatIndex on a strip, round splat()s and drawLine() rays on a panel.

var rate = 2           // blooms per second
var widthPct = 6       // jewel radius, % of the strip (or panel)
var glint = 40         // glint strength, %
var gems = 1           // 0 = picked colour, 1 = gem palette
var pickH = 0.78       // violet

// New jewels per second (twice that on a panel).
//# min=0.2 max=10 step=0.1 default=2
export function sliderBloomsPerSecond(v) { rate = clamp(v, 0.2, 10) }
// Jewel radius as a percentage of the strip length (or the panel width).
//# min=1 max=30 step=0.5 default=6
export function sliderWidthPercent(v) { widthPct = clamp(v, 1, 30) }
//# min=0 max=100 step=1 default=40
export function sliderGlintPercent(v) { glint = clamp(v, 0, 100) }
// 0 = every jewel in the picked colour, 1 = ruby, sapphire, emerald, amethyst, topaz, aquamarine.
//# min=0 max=1 step=1 default=1
export function sliderColorMode(v) { gems = clamp(floor(v), 0, 1) }
// Jewel colour for colour mode 0 (hue only).
export function hsvPickerColor(h, s, v) { pickH = h }

var MAXJ = 32
var jx = array(MAXJ)
var jy = array(MAXJ)
var jh = array(MAXJ)
var jw = array(MAXJ)    // width factor
var jage = array(MAXJ)
var jlife = array(MAXJ) // 0 = free slot
var jg = array(MAXJ)    // glint phase
var GEMS = [0.985, 0.63, 0.37, 0.78, 0.09, 0.5]
var acc = 0
var dt = 0
var glintT = 0
var seeded = 0

function spawn(age) {
  for (var i = 0; i < MAXJ; i++) {
    if (jlife[i] == 0) {
      jx[i] = 0.06 + random(0.88)
      jy[i] = 0.06 + random(0.88)
      jh[i] = (gems ? GEMS[floor(random(6))] : pickH) + random(0.03) - 0.015
      jw[i] = 0.7 + random(0.6)
      jlife[i] = 2.2 + random(1.2)
      jage[i] = age * jlife[i]
      jg[i] = random(1)
      return
    }
  }
}

export function beforeRender(delta) {
  dt = min(delta, 200) / 1000
  glintT = time(0.05)
  if (!seeded) {
    seeded = 1
    for (var k = 0; k < rate * 5; k++) spawn(random(0.9))
  }
  acc += rate * dt * (has2DMap() ? 2 : 1)       // a panel has room for twice as many
  while (acc >= 1) { acc -= 1; spawn(0) }
}

export function render(index) {}  // keeps a strip mapless; renderBulk paints

export function renderBulk() {
  clear()
  var panel = has2DMap()
  var R = widthPct / 100 * (panel ? 2.5 : pixelCount)
  var thin = 1 / max(gridWidth(), 32)
  for (var i = 0; i < MAXJ; i++) {
    if (jlife[i] == 0) continue
    jage[i] += dt
    var p = jage[i] / jlife[i]
    if (p >= 1) { jlife[i] = 0; continue }
    var bloom = sin(p * PI)
    var r = R * jw[i] * (0.4 + 0.6 * bloom)
    var s = max(sin((glintT + jg[i]) * PI2), 0)
    s = s * s
    s = s * s * s * bloom * glint / 50          // pow(.., 6): a short flash
    var h = jh[i]
    if (panel) {
      hsv(h, 0.95, 0.45 * bloom)
      splat(jx[i], jy[i], r * 1.4, 2)
      hsv(h, 0.8, bloom)
      splat(jx[i], jy[i], r * 0.8, 2)
      if (s > 0.03) {
        hsv(h, 0.25, min(s, 1))
        drawLine(jx[i] - r * s, jy[i], jx[i] + r * s, jy[i], thin, 1)
        drawLine(jx[i], jy[i] - r * s, jx[i], jy[i] + r * s, thin, 1)
        splat(jx[i], jy[i], r * 0.35, 1)
      }
    } else {
      var x = jx[i] * pixelCount
      hsv(h, 0.95, 0.45 * bloom)
      splatIndex(x, r * 1.4, 2)
      hsv(h, 0.8, bloom)
      splatIndex(x, r * 0.8, 2)
      if (s > 0.03) {
        hsv(h, 0.2, min(s, 1))
        splatIndex(x, max(r * 0.35, 0.6), 1)
      }
    }
  }
}
