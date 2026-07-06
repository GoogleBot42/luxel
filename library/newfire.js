// name: Newfire
// Clean-room reimplementation from a prose functional description of the
// community pattern "Newfire"; original source never consulted.

// 1D Doom-style fire: a heat array one cell longer than the strip, where
// cell 0 is the constant heat source. On a fixed ~40 ms tick, each cell pulls
// heat from a randomized 1-2 cells nearer the source minus random cooling;
// occasional sparks (and darker sputters) hit the base zone. Rendering cubes
// the heat and uses saturation headroom so only near-maximum heat pulls
// saturation below full and shows a white-hot core.

var heat = array(pixelCount + 1)
var TICK = 40 // ms, ~25 simulation steps per second
var tickAccum = 0

// defaults (overridden by the controls)
var cooling = 0.07
var srcHeat = 1
var sparkProb = 0.25
var mode = 0
var pickH = 0.02
var pickS = 1.8 // saturation headroom: picked saturation nearly doubled
var pickV = 1

export function hsvPickerColor(h, s, v) {
  pickH = h
  // repurposed saturation: headroom above 1 controls how much white-hot
  // core appears at the base (lower picked s = more white)
  pickS = s * 1.8
  pickV = v
}

export function sliderFlameHeight(v) {
  //# min=0 max=1 step=0.01 default=0.75
  // inverse: small cooling = tall flames, large = short stubby ones
  cooling = mix(0.25, 0.02, v)
}

export function sliderHeat(v) {
  //# min=0 max=1 step=0.01 default=1
  srcHeat = 0.4 + v * 0.6
}

export function sliderSparks(v) {
  //# min=0 max=1 step=0.01 default=0.5
  sparkProb = v * 0.5
}

export function sliderMode(v) {
  //# min=0 max=1 step=0.34 default=0
  mode = floor(v * 3.999)
}

function fireTick() {
  heat[0] = srcHeat

  // advect from the cool end toward the source, in place:
  // pull from 1-2 cells closer, minus random cooling
  for (var i = pixelCount; i >= 1; i--) {
    var src = i - 1 - floor(random(2))
    if (src < 0) src = 0
    heat[i] = max(0, heat[src] - random(cooling))
  }

  // sparks and sputters near the base
  if (random(1) < sparkProb) {
    var zone = max(1, floor(pixelCount / 8))
    var j = 1 + floor(random(zone))
    // usually a bright spark, sometimes a dark spot
    var kick = random(0.6) - 0.15
    heat[j] = clamp(heat[j] + kick, 0, max(srcHeat, 0.55))
  }
}

export function beforeRender(delta) {
  tickAccum += delta
  if (tickAccum >= TICK) {
    tickAccum -= TICK
    if (tickAccum > TICK) tickAccum = 0 // don't spiral after a stall
    fireTick()
  }
}

export function render(index) {
  var half = floor(pixelCount / 2)
  var i
  if (mode == 0) {
    i = index + 1 // base at the start
  } else if (mode == 1) {
    i = pixelCount - index // base at the far end
  } else if (mode == 2) {
    i = abs(index - half) + 1 // base at the center, flames radiate outward
  } else {
    i = min(index, pixelCount - 1 - index) + 1 // bases at both ends
  }

  var k = heat[i]
  k = k * k * k // cubed gamma
  // hottest parts shift hue slightly upward and desaturate toward white
  hsv(pickH + k * 0.05, pickS - k, pickV * k)
}
