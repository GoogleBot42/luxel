// name: Marbling
// tags: panel, ambient
// Curated original (hand-written showcase of the Luxel language/builtins).
// Suminagashi: ink drops land on still water one after another, each pushing
// the earlier rings outward (the area-preserving drop map, inverted in closed
// form); then a comb drags through and pulls the rings into chevrons, and the
// sheet clears to clean water for the next print. Each event is applied by
// inverse lookup into the baked sheet while it animates, then baked in, so a
// frame costs one transform per canvas cell. Four OKLCH inks, a new set per sheet.
// The first sheet opens already part-printed, so the pattern reads at once.

var CAP = 48
var B = array(CAP * CAP), O = array(CAP * CAP)   // baked sheet, animated output
var pal = array(40), c3 = array(3)

var dropS = 1.1                     // seconds between drops
//# min=0.5 max=5 step=0.1 default=1.1
export function sliderDropS(v) { dropS = clamp(v, 0.5, 5) }
var drops = 16                      // drops per sheet
//# min=4 max=30 step=1 default=16
export function sliderDrops(v) { drops = clamp(floor(v), 4, 30) }
var strokes = 1                     // comb passes before the sheet clears
//# min=0 max=2 step=1 default=1
export function sliderCombStrokes(v) { strokes = clamp(floor(v), 0, 2) }

function oklchRGB(l, c, h) {        // OKLCH -> gamma sRGB into c3
  var a = c * cos(h * PI2), b = c * sin(h * PI2)
  var L = l + 0.3963 * a + 0.2158 * b, M = l - 0.1056 * a - 0.0639 * b, S = l - 0.0895 * a - 1.2915 * b
  L = L * L * L; M = M * M * M; S = S * S * S
  c3[0] = 4.0767 * L - 3.3077 * M + 0.231 * S
  c3[1] = -1.2684 * L + 2.6098 * M - 0.3413 * S
  c3[2] = -0.0042 * L - 0.7034 * M + 1.7076 * S
  for (var i = 0; i < 3; i++) c3[i] = pow(clamp(c3[i], 0, 1), 0.42)
}
var hue0 = random(1), clearK = 1, inks = array(15)
function mixInks() {                // inks + water, each faded toward water by clearK
  var j, k, i
  for (k = 0; k < 5; k++) for (i = 0; i < 3; i++) {
    j = k * 8
    pal[j + 1 + i] = inks[i] + (inks[k * 3 + i] - inks[i]) * clearK
    pal[j + 5 + i] = pal[j + 1 + i]
  }
  setPalette(pal)
}
function buildPalette() {           // water 0..0.2, then four inks in 0.2 bands
  for (var k = 0; k < 5; k++) {
    if (k == 0) oklchRGB(0.3, 0.05, hue0 + 0.5)
    else oklchRGB(k % 2 ? 0.74 : 0.6, 0.16, hue0 + k * 0.27)
    for (var i = 0; i < 3; i++) inks[k * 3 + i] = c3[i]
    pal[k * 8] = k == 0 ? 0 : k * 0.2 + 0.0001
    pal[k * 8 + 4] = k * 0.2 + 0.2
  }
  mixInks()
}

// the event in flight: kind 1 = drop, 2 = comb; progress u 0..1
var kind = 0, u = 0, ex = 0, ey = 0, er = 0, ecol = 0.3, dirX = 1, dirY = 0, gap = 8, zMax = 0
var timer = 0.3, count = 0, combs = 0, hold = 0, ccx = 0.5, ccy = 0.5, onCentre = 0, ink = 1
var CW = 32, CH = 32, seeded = 0

function newSheet() {
  feedback(B, 0)                    // 0 is clean water
  count = 0; combs = 0; hold = 0; onCentre = 0
  hue0 = mod(hue0 + 0.13 + random(0.2), 1)
  buildPalette()
}
newSheet()

function startDrop() {
  if (onCentre <= 0) {              // a fresh centre; several drops land on it
    ccx = 0.2 + random(0.6); ccy = 0.2 + random(0.6); onCentre = 3 + floor(random(4))
    ink = 1 + floor(random(4))
  }
  onCentre--
  ink = ink % 4 + 1                 // alternate inks so concentric rings read
  ex = (ccx + random(0.04) - 0.02) * CW; ey = (ccy + random(0.04) - 0.02) * CH
  er = (0.09 + random(0.08)) * min(CW, CH)
  ecol = ink * 0.2 + 0.1
  kind = 1; u = 0; count++
}
function startComb() {
  var horiz = combs % 2 == 0
  dirX = horiz ? 1 : 0; dirY = horiz ? 0 : 1
  if (random(1) < 0.5) { dirX = -dirX; dirY = -dirY }
  gap = max(CW, CH) / (4 + floor(random(3)))
  zMax = gap * 1.3
  kind = 2; u = 0; combs++
}

function applyEvent(p) {            // O = B seen through the event at progress p
  var i = 0, r, c
  if (kind == 1) {
    var rr = er * easeOutCubic(p), r2 = rr * rr
    for (r = 0; r < CH; r++) {
      var dy = r + 0.5 - ey
      for (c = 0; c < CW; c++) {
        var dx = c + 0.5 - ex, d2 = dx * dx + dy * dy
        if (d2 < r2) O[i] = ecol
        else {
          var s = sqrt(1 - r2 / d2)
          O[i] = B[clamp(floor(ey + dy * s), 0, CH - 1) * CW + clamp(floor(ex + dx * s), 0, CW - 1)]
        }
        i++
      }
    }
  } else {
    var z = zMax * easeInOutSine(p), lam = gap * 0.18
    for (r = 0; r < CH; r++) {
      for (c = 0; c < CW; c++) {
        var t = mod(dirX != 0 ? r + 0.5 : c + 0.5, gap)
        var k = z * lam / (min(t, gap - t) + lam)
        O[i] = B[clamp(floor(r + 0.5 - dirY * k), 0, CH - 1) * CW + clamp(floor(c + 0.5 - dirX * k), 0, CW - 1)]
        i++
      }
    }
  }
}

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  var gw = gridWidth() > 0 ? gridWidth() : 32, gh = gridHeight() > 0 ? gridHeight() : 32
  var w = min(gw, CAP), h = min(gh, CAP)
  if (w != CW || h != CH) { CW = w; CH = h; newSheet(); kind = 0; seeded = 0 }
  if (!seeded) {                    // bake the first drops in at once
    seeded = 1
    for (var q = 0; q < 9; q++) {   // three centres, three rings each
      if (q % 3 == 0) onCentre = 0
      startDrop(); applyEvent(1); arrayMix(B, O, 1)
    }
    kind = 0
  }
  if (kind) {
    u = min(u + dt / (kind == 1 ? 0.9 : 2.2), 1)
    applyEvent(u)
    if (u >= 1) { arrayMix(B, O, 1); kind = 0 }
    return
  }
  timer -= dt
  if (timer > 0) return
  if (count < drops) { startDrop(); timer = dropS - 0.9 }
  else if (combs < strokes) { startComb(); timer = 0.6 }
  else if (hold < 1) { hold = 1; timer = 5 }
  else {                            // the sheet clears to water
    clearK = max(0, clearK - dt / 3)
    if (clearK <= 0) { clearK = 1; newSheet(); timer = 0.3 }
    else mixInks()
  }
}

export function renderBulk() {
  paintCanvas(kind ? O : B, CW, CH)
}
