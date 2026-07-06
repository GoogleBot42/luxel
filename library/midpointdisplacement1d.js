// name: MidpointDisplacement1D
// Clean-room reimplementation from a prose functional description of the
// community pattern "MidpointDisplacement1D"; original source never consulted.

// A fractal mountain-range silhouette drawn as brightness along the strip:
// peaks bright, valleys black. The terrain is static, but a compressed slice
// of the hue wheel scrolls through the height field, so ridgelines ripple
// with moving color. Every mapLifetime seconds a fresh random range is
// generated (lifetime 0 = keep the map forever). All the fractal work
// happens only at (re)generation; per-frame cost is trivial.

var maxDepth = min(7, ceil(log2(pixelCount))) // device-perf depth cap

// Defaults, exported so they're watchable/tweakable
export var detail = 1        // fraction of maxDepth actually recursed
export var mapLifetime = 8   // seconds per terrain; 0 = forever
export var speed = 0.5       // color scroll rate (higher = faster)
export var paletteWidth = 1  // fraction of the hue wheel spanned
export var paletteOffset = 0 // base hue of the palette slice
export var roughness = 0.5   // 0 = rolling hills, 1 = spiky seismograph

var heights = array(pixelCount)
var usedDepth = maxDepth
var falloff = 1.85           // per-level displacement divisor, from roughness
var phase = 0
var age = 0

// Recursive midpoint displacement over heights[lo..hi]. Once past the depth
// budget, fill the remaining span with a straight line.
function displace(lo, hi, level, amp) {
  if (hi - lo < 2) return
  var mid = floor((lo + hi) / 2)
  if (level >= usedDepth) {
    var i
    for (i = lo + 1; i < hi; i++) {
      heights[i] = mix(heights[lo], heights[hi], (i - lo) / (hi - lo))
    }
    return
  }
  heights[mid] = (heights[lo] + heights[hi]) / 2 + (random(2) - 1) * amp
  displace(lo, mid, level + 1, amp / falloff)
  displace(mid, hi, level + 1, amp / falloff)
}

function regenerate() {
  usedDepth = max(1, floor(detail * maxDepth + 0.5))
  // roughness slider -> per-level falloff: big divisor = smooth,
  // divisor below 1 = deeper levels get wilder
  falloff = 2.6 - roughness * 1.8
  heights[0] = random(0.25)
  heights[pixelCount - 1] = random(0.25)
  displace(0, pixelCount - 1, 0, 0.5)

  // min-max normalize to 0..1
  var lo = heights[0], hi = heights[0], i
  for (i = 1; i < pixelCount; i++) {
    if (heights[i] < lo) lo = heights[i]
    if (heights[i] > hi) hi = heights[i]
  }
  var span = hi - lo
  if (span <= 0) span = 1
  for (i = 0; i < pixelCount; i++) {
    heights[i] = (heights[i] - lo) / span
  }
  age = 0
}

//# min=0 max=1 step=0.05 default=1
export function sliderDetailLevel(v) {
  detail = v
  regenerate()
}

//# min=0 max=1 step=0.05 default=0.27
export function sliderMapLifetime(v) {
  mapLifetime = v * 30 // up to ~half a minute; 0 = forever
  regenerate()
}

//# min=0 max=1 step=0.05 default=0.5
export function sliderSpeed(v) {
  speed = v // inverted into the period below: right = faster
}

//# min=0 max=1 step=0.05 default=1
export function sliderPaletteWidth(v) {
  paletteWidth = v
}

//# min=0 max=1 step=0.05 default=0
export function sliderPaletteOffset(v) {
  paletteOffset = v
}

//# min=0 max=1 step=0.05 default=0.5
export function sliderRoughness(v) {
  roughness = v
  regenerate()
}

regenerate()

export function beforeRender(delta) {
  // scroll period from ~4 s (slider left) down to ~0.4 s (right)
  var period = mix(4000, 400, speed)
  phase = frac(phase + delta / period)

  if (mapLifetime > 0) {
    age += delta / 1000
    if (age > mapLifetime) regenerate()
  }
}

export function render(index) {
  var h = heights[index]
  var hue = frac(h + phase) * paletteWidth + paletteOffset
  hsv(hue, 1, h)
}
