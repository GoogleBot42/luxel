// name: Twinkling Classic Xmas Strands
// Clean-room reimplementation from a prose functional description of the
// community pattern "Twinkling Classic Xmas Strands"; original source never consulted.

// A classic multicolor Christmas bulb strand. Each pixel is permanently
// assigned one of five palette slots (red / green / amber / blue / purple)
// by an urn draw with delayed ball replacement — no adjacent repeats, and
// colors stay locally balanced like a manufactured strand. The whole strand
// slowly cross-fades among three palette moods (vivid classic, warm aged
// pastels, cool/wintry). Individual bulbs twinkle: mostly a dim resting
// glow, with brief smooth swells to a flattened bright peak at stable
// per-pixel phases; higher "twinkles" adds random blank flicker and, at
// max, wraps the swell for a harsh strobe sparkle.

var NUMC = 5                 // palette slots
var LAG = 4                  // urn ball returns ~one palette-length later
var REST = 0.25              // dim resting brightness multiplier
var PEAK = 1.35              // swell target (clipped at 1 => flattened top)

// ---- controls -------------------------------------------------------------
var cycleSec = 15            // full fade through all three palettes
var twinkles = 0.5           // twinkle density/intensity
var autoFade = 1             // >0.5: palettes cross-fade automatically
var manualSel = 0            // manual palette position when autoFade is off

//# min=0 max=1 step=0.01 default=0.4
export function sliderCycleTime(v) { cycleSec = 3 + v * 27 }
//# min=0 max=1 step=0.01 default=0.5
export function sliderTwinkles(v) { twinkles = v }
//# min=0 max=1 step=1 default=1
export function sliderAutoFadePalettes(v) { autoFade = v }
//# min=0 max=1 step=0.01 default=0
export function sliderManualPaletteSelect(v) { manualSel = v }

// ---- palettes (h, s, v per slot: red, green, amber, blue, purple) ---------
var palH = array(15)
var palS = array(15)
var palV = array(15)
function setSlot(pal, slot, h, s, v) {
  palH[pal * NUMC + slot] = h
  palS[pal * NUMC + slot] = s
  palV[pal * NUMC + slot] = v
}
// A: classic vivid
setSlot(0, 0, 0.00, 1, 1)        // red
setSlot(0, 1, 0.33, 1, 1)        // green
setSlot(0, 2, 0.09, 1, 1)        // amber/gold
setSlot(0, 3, 0.66, 1, 1)        // blue
setSlot(0, 4, 0.85, 1, 1)        // purple, near magenta
// B: aged pastels, warm
setSlot(1, 0, 0.02, 0.60, 0.80)  // washed warm red
setSlot(1, 1, 0.32, 0.40, 0.30)  // very dim desaturated green
setSlot(1, 2, 0.10, 0.50, 0.80)  // washed amber
setSlot(1, 3, 0.53, 0.50, 0.40)  // dim muted teal-ish blue
setSlot(1, 4, 0.88, 0.90, 0.80)  // strong pinkish purple, slightly dimmed
// C: cool / wintry
setSlot(2, 0, 0.98, 0.35, 0.50)  // pale dim dusty rose
setSlot(2, 1, 0.33, 1.00, 1.00)  // vivid green
setSlot(2, 2, 0.09, 0.25, 0.90)  // soft warm-tinged white (replaces amber)
setSlot(2, 3, 0.66, 1.00, 1.00)  // vivid blue
setSlot(2, 4, 0.78, 1.00, 1.00)  // vivid violet

// ---- per-pixel setup (runs once at startup) -------------------------------
var slots = array(pixelCount)    // fixed color slot per pixel
var phases = array(pixelCount)   // stable twinkle phase offset per pixel
var urn = array(NUMC)

function assignColors() {
  var i, c
  for (c = 0; c < NUMC; c++) urn[c] = 1
  var prev = -1
  for (i = 0; i < pixelCount; i++) {
    // ball drawn LAG pixels ago goes back in the urn
    if (i >= LAG) urn[slots[i - LAG]] += 1
    // count available balls, excluding the previous pixel's color
    var avail = 0
    for (c = 0; c < NUMC; c++) if (c != prev) avail += urn[c]
    var pick = 0
    if (avail <= 0) {
      pick = mod(prev + 1 + floor(random(NUMC - 1)), NUMC)  // safety net
    } else {
      var r = floor(random(avail))
      for (c = 0; c < NUMC; c++) {
        if (c == prev || urn[c] <= 0) continue
        r -= urn[c]
        if (r < 0) { pick = c; break }
      }
    }
    slots[i] = pick
    if (urn[pick] > 0) urn[pick] -= 1
    prev = pick
  }
  for (i = 0; i < pixelCount; i++) phases[i] = random(1)
}
assignColors()

// ---- per-frame state -------------------------------------------------------
var fadePos = 0              // 0..1 through the whole palette cycle
var twClock = 0              // 0..1 through the shared twinkle cycle
var twPeriodSec = 10
var twWindow = 0.1           // fraction of the cycle spent swelling (~1 s)
// current blended palette, one entry per slot
var curH = array(NUMC)
var curS = array(NUMC)
var curV = array(NUMC)

export function beforeRender(delta) {
  var dSec = delta / 1000

  // palette position: auto cross-fade or manual sweep
  var palPos
  if (autoFade > 0.5) {
    fadePos += dSec / cycleSec
    fadePos = frac(fadePos)
    palPos = fadePos * 3
  } else {
    palPos = manualSel * 2.999
  }
  var pa = floor(palPos) % 3
  var pb = (pa + 1) % 3
  var blend = frac(palPos)

  // blend the five slot colors once per frame
  var s
  for (s = 0; s < NUMC; s++) {
    var ia = pa * NUMC + s, ib = pb * NUMC + s
    var dh = mod(palH[ib] - palH[ia] + 0.5, 1) - 0.5   // shortest way around
    curH[s] = mod(palH[ia] + dh * blend + 1, 1)
    var sm = mix(palS[ia], palS[ib], blend)
    var vm = mix(palV[ia], palV[ib], blend)
    curS[s] = sqrt(sm)       // ease saturation up: mid-fades stay rich
    curV[s] = vm * vm        // ease brightness down: mid-fades stay deep
  }

  // twinkle clock: period scales with pixelCount -> constant twinkles/sec
  // per strand; more "twinkles" = shorter cycle
  twPeriodSec = pixelCount * 0.25 / (0.05 + twinkles * 3)
  twClock = frac(twClock + dSec / twPeriodSec)
  twWindow = clamp(1.2 / twPeriodSec, 0.02, 1)         // ~1.2 s swell window
}

export function render(index) {
  var slot = slots[index]
  var mult = REST
  if (twinkles > 0.01) {
    var ph = frac(twClock + phases[index])
    if (ph < twWindow) {
      var u = ph / twWindow
      mult = REST + (PEAK - REST) * sin(PI * u)        // smooth bump
    }
    if (twinkles > 0.97) mult = frac(mult)             // wrap: strobe sparkle
    else mult = min(mult, 1)                           // clip: flattened peak
    // tiny random blackouts, more frequent as the control rises
    if (random(1) < 0.02 * twinkles * twinkles) mult = 0
  }
  hsv(curH[slot], curS[slot], curV[slot] * mult)
}
