// name: Newfire
// Clean-room reimplementation from a prose functional description of the
// community pattern "Newfire"; original source never consulted.
//
// 1D "Doom fire": a heat array one longer than the strip, cell 0 is the
// constant source. Fixed ~40 ms simulation ticks advect heat away from
// the source with random jitter and cooling; occasional sparks/sputters
// near the base. Saturation headroom makes a white-hot core appear only
// where heat is near maximum.

var heat = array(pixelCount + 1)   // [0] = source, [1..pixelCount] = cells

// control state (defaults match the //# bounds)
var pickH = 0.03
var satHead = 1.5                  // picked saturation, scaled up (~x1.8): white-core headroom
var pickV = 1
var cooling = 0.06
var sourceHeat = 1
var sparkProb = 0.15
var modeV = 0

var tickAcc = 0

function fireTick() {
  heat[0] = sourceHeat
  // advect from the cool end toward the source so it runs in place:
  // each cell pulls from 1-2 cells closer to the source, minus random cooling
  var i
  for (i = pixelCount; i >= 1; i--) {
    var src = i - 1 - floor(random(2))
    if (src < 0) src = 0
    heat[i] = max(0, heat[src] - random(cooling))
  }
  // sparks and sputters near the base: usually a bright flare, sometimes
  // a dark spot (range ~3x wider above zero than below)
  if (random(1) < sparkProb) {
    var s = 1 + floor(random(max(1, pixelCount / 8)))
    var amt = random(0.9) - 0.225
    heat[s] = clamp(heat[s] + amt, 0, max(sourceHeat, 0.6))
  }
}

export function beforeRender(delta) {
  tickAcc += delta
  if (tickAcc > 200) tickAcc = 200
  while (tickAcc >= 40) {          // fixed ~25 Hz simulation tick
    tickAcc -= 40
    fireTick()
  }
}

export function render(index) {
  // layout mode maps the pixel into the heat field (offset past the source)
  var mode = floor(modeV * 3.99)
  var i
  if (mode == 0) {
    i = index + 1                                  // base at the start
  } else if (mode == 1) {
    i = pixelCount - index                         // base at the far end
  } else if (mode == 2) {
    i = floor(abs(index - pixelCount / 2)) + 1     // base at center, flames outward
  } else {
    // bases at both ends, flames meeting in the middle
    if (index < pixelCount / 2) i = index + 1
    else i = pixelCount - index
  }
  i = clamp(i, 1, pixelCount)

  var h = heat[i]
  h = h * h * h                    // cube for gamma
  // hottest cells shift hue slightly upward and desaturate toward white;
  // only heat near max can eat through the saturation headroom
  hsv(pickH + h * 0.05, satHead - h, pickV * h)
}

export function hsvPickerColor(h, s, v) {
  pickH = h
  satHead = s * 1.8                // deliberate over-saturation headroom
  pickV = v
}

//# min=0 max=1 step=0.01 default=0.85
export function sliderFlameHeight(v) {
  // inverse blend: small cooling = tall flames, large = short and stubby
  cooling = 0.3 - v * 0.28
}

//# min=0 max=1 step=0.01 default=1
export function sliderHeat(v) {
  sourceHeat = 0.4 + v * 0.6
}

//# min=0 max=1 step=0.01 default=0.3
export function sliderSparks(v) {
  sparkProb = v * 0.5
}

//# min=0 max=1 step=0.34 default=0
export function sliderMode(v) {
  modeV = v
}
