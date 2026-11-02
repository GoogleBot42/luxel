// name: Classic Christmas Strands
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/twinkling-classic-xmas-strands.md,
// static-christmas-lights-4-colors.md — reimagined, not a replica.
// An old-fashioned light strand: every bulb keeps its colour (a hashed walk that never
// repeats a neighbour; the RGBY mini-light palette runs in strict order), rests at a
// warm glow, and on its own hashed schedule swells white-hot or briefly winks out.
// Twinkle 0 is a steady strand. The strand can drift through its three palettes.
// renderBulk: one setPixel or splatIndex per bulb, no per-pixel arrays.

// palettes, 5 slots each, RGB: 0 C9 classic, 1 vintage painted glass, 2 RGBY minis (4 used)
var PAL = array(45)
function pc(p, s, r, g, b) { var i = (p * 5 + s) * 3; PAL[i] = r; PAL[i + 1] = g; PAL[i + 2] = b }
pc(0, 0, 1, 0.02, 0); pc(0, 1, 0, 0.85, 0.08); pc(0, 2, 1, 0.42, 0); pc(0, 3, 0.04, 0.16, 1); pc(0, 4, 0.75, 0, 0.9)
pc(1, 0, 1, 0.22, 0.35); pc(1, 1, 0.25, 0.85, 0.45); pc(1, 2, 1, 0.7, 0.15); pc(1, 3, 0.2, 0.55, 1); pc(1, 4, 0.75, 0.35, 1)
pc(2, 0, 1, 0, 0); pc(2, 1, 0, 0.9, 0); pc(2, 2, 0, 0.1, 1); pc(2, 3, 1, 0.75, 0); pc(2, 4, 1, 0, 0)

var twinkle = 50         // how lively the strand is
//# min=0 max=100 step=1 default=50
export function sliderTwinklePercent(v) { twinkle = clamp(v, 0, 100) }

var autoFade = 1         // drift through the palettes, starting at Palette
export function toggleAutoFade(on) { autoFade = on ? 1 : 0 } //# default=1

var palette = 0          // 0 classic C9 | 1 vintage | 2 RGBY minis
//# min=0 max=2 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 2) }

var paletteS = 12        // seconds each palette holds when drifting (the last fifth is the fade)
//# min=3 max=60 step=1 default=12
export function sliderPaletteSeconds(v) { paletteS = clamp(v, 3, 60) }

var bulbPx = 1
//# min=1 max=8 step=1 default=1
export function sliderBulbPixels(v) { bulbPx = clamp(floor(v), 1, 8) }

var T = 0, palT = 0, pa = 0, pb = 0, blend = 0, tmp = array(3)
export function beforeRender(delta) {
  T = mod(T + delta / 1000, 240)               // every twinkle period divides 240 s
  pa = palette; pb = palette; blend = 0
  if (autoFade) {
    palT = mod(palT + delta / 1000, paletteS * 3)
    var pos = palT / paletteS, k = floor(pos)
    pa = (palette + k) % 3; pb = (pa + 1) % 3
    blend = smoothstep(0.8, 1, pos - k)
  }
}

var cr = 0, cg = 0, cb = 0

// A mapless strip stays a strip; Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

export function renderBulk() {
  clear()
  var tw = twinkle / 100, rest = 1 - 0.6 * tw, part = 0.25 + 0.75 * tw
  var n = ceil(pixelCount / bulbPx), slot = 0, i = 0
  for (var k = 0; k < n; k++) {
    var h = hash(k)
    slot = k == 0 ? 0 : (slot + 1 + floor(frac(h * 5.3) * 3)) % 5   // never the neighbour's colour
    i = (pa * 5 + (pa == 2 ? k % 4 : slot)) * 3                     // RGBY minis run in order
    cr = PAL[i]; cg = PAL[i + 1]; cb = PAL[i + 2]
    if (blend > 0) {
      i = (pb * 5 + (pb == 2 ? k % 4 : slot)) * 3
      mixColors(cr, cg, cb, PAL[i], PAL[i + 1], PAL[i + 2], blend, tmp)
      cr = tmp[0]; cg = tmp[1]; cb = tmp[2]
    }
    var lv = 1, wh = 0
    if (tw > 0) {
      lv = rest
      if (frac(h * 31.7) < part) {                                 // this bulb twinkles
        var cyc = T * (40 + floor(frac(h * 13.1) * 57)) / 240 + frac(h * 7.3)  // 2.5..6 s cycles
        var ph = frac(cyc)
        if (ph < 0.3) {
          if (hash2(k, floor(cyc)) < 0.45) {                       // some cycles: a swell
            lv = rest + (1.2 - rest) * sin(ph / 0.3 * PI)
            wh = max(0, lv - 1) * 1.6                              // past full: white-hot
            lv = min(lv, 1)
          }
        } else if (ph > 0.62 && ph < 0.7 && hash2(k, floor(cyc) + 0.5) < 0.12 * tw) {
          lv = rest * (1 - sin((ph - 0.62) / 0.08 * PI))           // now and then, a wink out
        }
      }
    }
    rgb(cr * lv + wh, cg * lv + wh, cb * lv + wh)
    if (bulbPx == 1) setPixel(k)
    else splatIndex(k * bulbPx + bulbPx * 0.5 - 0.5, bulbPx * 0.5 + 0.25, 2)
  }
}
