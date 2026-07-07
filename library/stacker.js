// name: Stacker
// Clean-room reimplementation from a prose functional description of the
// community pattern "Stacker"; original source never consulted.

// 1D "Tetris-ish" filler. Only one half-segment is simulated; the per-pixel
// path replicates it to every segment (index % segLen) and mirrors both
// halves (fold around the midpoint), so all segments converge in sync.

var segments = 3
var segLen = floor(pixelCount / segments)
var halfLen = max(1, floor(segLen / 2))

var blockPos = 0       // traveling block position within the half-segment
var stackH = 0         // pixels piled at the center of the half-segment
var acc = 0            // ms since the block last moved
var stepMs = 12        // ms per single-pixel step (~80 steps/s default)
var blockW = 2         // traveling block width in pixels
var mode = 0           // 0 solid, 1 animated rainbow, 2 color bands
var rainbowPhase = 0

// pile color (Color 1) and traveling block color (Color 2)
var h1 = 0, s1 = 1, v1 = 1
var h2 = 0.55, s2 = 1, v2 = 1

function reinit() {
  segLen = floor(pixelCount / segments)
  halfLen = max(1, floor(segLen / 2))
  blockPos = 0
  stackH = 0
  acc = 0
}

export function hsvPickerColor1(h, s, v) {
  h1 = h
  s1 = s
  v1 = v
}

export function hsvPickerColor2(h, s, v) {
  h2 = h
  s2 = s
  v2 = v
}

//# min=0 max=1 step=0.01 default=0.8
export function sliderSpeed(v) {
  // inverted: right = faster. ~250 ms/step at the slow end, ~every frame fast
  var u = 1 - v
  stepMs = 4 + 246 * u * u
}

//# min=0 max=1 step=0.01 default=0.15
export function sliderSize(v) {
  blockW = 1 + floor(v * 9)
}

//# min=0 max=1 step=0.2 default=0.4
export function sliderSegments(v) {
  var n = 1 + floor(v * 5.99)
  if (n != segments) {
    segments = n
    reinit()
  }
}

//# min=0 max=1 step=0.5 default=0
export function sliderColorMode(v) {
  mode = floor(v * 2.99)
}

export function beforeRender(delta) {
  acc += delta
  while (acc >= stepMs) {
    acc -= stepMs
    blockPos += 1
    // reached the inner boundary of the pile: lock on, grow, restart
    if (blockPos >= halfLen - stackH) {
      stackH += blockW
      blockPos = 0
      if (stackH >= halfLen) stackH = 0   // segment full: clear
    }
  }
  rainbowPhase = time(0.03)   // full hue drift in ~2 s
}

// hue of a piled pixel at folded position d, per color mode
function pileHue(d) {
  if (mode == 1) return d / halfLen + rainbowPhase      // scrolling rainbow
  if (mode == 2) return round(d / blockW) * 0.618       // well-spread bands
  return h1                                             // solid
}

export function render(index) {
  // blank the leftover partial segment at the far end of the strip
  if (index >= segments * segLen) {
    rgb(0, 0, 0)
    return
  }
  var s = index % segLen
  var d = min(s, segLen - 1 - s)   // fold: distance from the segment edge

  if (stackH > 0 && d >= halfLen - stackH) {
    hsv(pileHue(d), s1, v1)                      // the pile
  } else if (abs(d - blockPos) < blockW / 2 + 0.01) {
    hsv(h2, s2, v2)                              // the traveling block
  } else {
    rgb(0, 0, 0)
  }
}
