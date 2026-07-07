// name: MidpointDisplacement1D
// Clean-room reimplementation from a prose functional description of the
// community pattern "MidpointDisplacement1D"; original source never
// consulted.

// A fractal mountain-range silhouette: brightness = terrain height, with a
// compressed slice of the hue wheel scrolling through the height field so
// ridgelines ripple with color. The terrain regenerates every few seconds.
//
// The terrain is built by midpoint displacement on a fixed 2^MAX_DEPTH + 1
// breakpoint array (levels processed iteratively), then min-max normalized;
// pixels sample it with linear interpolation, so any pixel count works and
// all fractal cost happens only at (re)generation.

var MAX_DEPTH = 7            // performance cap from the original
var N = 128                  // 2^MAX_DEPTH segments -> N+1 breakpoints
var heights = array(N + 1)

// Tunables (also watchable) -------------------------------------------------
export var detail = 1        // fraction of MAX_DEPTH actually recursed
export var lifetime = 8      // seconds each terrain lives; 0 = forever
export var speed = 0.5       // hue scroll rate (cycles/second)
export var paletteWidth = 1  // fraction of the hue wheel the terrain spans
export var paletteOffset = 0 // base hue of the palette slice
export var roughness = 1.7   // per-level displacement falloff factor

var phase = 0
var age = 0

function generate() {
  var depth = max(1, floor(MAX_DEPTH * detail))

  // Endpoints get small random heights.
  heights[0] = random(0.3)
  heights[N] = random(0.3)

  // Displace midpoints level by level; the displacement half-width shrinks
  // by the roughness factor each level (>1 smooths, <1 goes wild).
  var seg = N
  var amp = 0.5
  var i
  for (var level = 0; level < depth; level++) {
    for (i = 0; i < N; i += seg) {
      heights[i + seg / 2] =
        (heights[i] + heights[i + seg]) / 2 + (random(2) - 1) * amp
    }
    seg = seg / 2
    amp = amp / roughness
  }

  // Straight lines across any segments the recursion never split.
  if (seg > 1) {
    for (i = 0; i < N; i += seg) {
      for (var j = 1; j < seg; j++) {
        heights[i + j] = heights[i] + (heights[i + seg] - heights[i]) * j / seg
      }
    }
  }

  // Min-max normalize to 0..1.
  var lo = heights[0]
  var hi = heights[0]
  for (i = 1; i <= N; i++) {
    lo = min(lo, heights[i])
    hi = max(hi, heights[i])
  }
  var span = hi - lo
  if (span <= 0) span = 1
  for (i = 0; i <= N; i++) heights[i] = (heights[i] - lo) / span

  age = 0
}

generate()

// UI controls ---------------------------------------------------------------
//# min=0 max=1 step=0.01 default=1
export function sliderDetailLevel(v) { detail = v; generate() }

//# min=0 max=1 step=0.01 default=0.27
export function sliderMapLifetime(v) { lifetime = v * 30; generate() }

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) { speed = 0.05 + v }  // right = faster

//# min=0 max=1 step=0.01 default=1
export function sliderPaletteWidth(v) { paletteWidth = v }

//# min=0 max=1 step=0.01 default=0
export function sliderPaletteOffset(v) { paletteOffset = v }

//# min=0 max=1 step=0.01 default=0.45
export function sliderRoughness(v) {
  // Left = smooth (fast falloff), right = jagged (slow / amplifying falloff).
  roughness = 3 - 2.4 * v
  generate()
}

export function beforeRender(delta) {
  phase = mod(phase + delta / 1000 * speed, 1)
  if (lifetime > 0) {
    age += delta / 1000
    if (age > lifetime) generate()
  }
}

export function render(index) {
  // Sample the breakpoint array with linear interpolation.
  var p = index / max(1, pixelCount - 1) * N
  var i0 = floor(p)
  var i1 = min(N, i0 + 1)
  var h = heights[i0] + (heights[i1] - heights[i0]) * (p - i0)

  // Height + scroll wrapped, compressed to a palette slice off a base hue.
  var hue = mod(h + phase, 1) * paletteWidth + paletteOffset
  hsv(hue, 1, h)
}
