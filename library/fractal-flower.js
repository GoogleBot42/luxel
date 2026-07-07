// name: fractal flower
// Clean-room reimplementation from a prose functional description of the
// community pattern "fractal flower"; original source never consulted.

// A ring of identical recursive binary-tree "petals" drawn as glowing
// dots into a 16x16 offscreen buffer each frame. Two slowly oscillating
// branch angles (incommensurate periods) make the flower fold and unfurl
// between stars, ferns, and pinwheels; hue cycles and shifts with branch
// depth; deposits blend hues as a brightness-weighted circular average.
// Trails fade the buffer, and a slow-tracking max normalizer acts as
// auto-exposure so dense overlap reveals structure instead of clipping.

const SIZE = 16
var briBuf = array(SIZE * SIZE)
var hueBuf = array(SIZE * SIZE)

export var nodes = 0   // total recursion visits (monitoring)

// control state (defaults)
var maxDepth = 6
var drawLevels = 5
var stepLen = .026
var speedF = 1
var range1 = .25
var range2 = .2
var trails = .8
var replicas = 3
var spacing = .22
var whiteMode = 1
var pinwheelMode = 1
var wrapMode = 0

var ang1 = -1
var ang2 = .35
var head0 = 0
var baseHue = 0
var expNorm = 1
var frameMax = 0

//# min=0 max=1 step=0.01 default=0.6
export function sliderIterations(v) { maxDepth = floor(1 + v * 8.99) }

//# min=0 max=1 step=0.01 default=0.5
export function sliderDrawLevels(v) { drawLevels = floor(1 + v * 8.99) }

//# min=0 max=1 step=0.01 default=0.57
export function sliderScale(v) { stepLen = v * v * .08 }   // squared response

//# min=0 max=1 step=0.01 default=0.33
export function sliderSpeed(v) { speedF = .2 + v * 2.8 }

//# min=0 max=1 step=0.01 default=0.25
export function sliderAngleRange1(v) { range1 = v }

//# min=0 max=1 step=0.01 default=0.2
export function sliderAngleRange2(v) { range2 = v }

//# min=0 max=1 step=0.01 default=0.8
export function sliderTrails(v) { trails = v }

//# min=0 max=1 step=0.01 default=0.2
export function sliderReplicas(v) { replicas = floor(1 + v * 11.99) }

//# min=0 max=1 step=0.01 default=0.5
export function sliderSpacing(v) { spacing = v * .45 }

export function toggleWhiteMode(v) { whiteMode = v }
export function togglePinwheelMode(v) { pinwheelMode = v }
export function toggleWrapMode(v) { wrapMode = v }

// recursive binary tree: step, deposit, then branch twice
function branch(x, y, heading, depth, isRoot) {
  nodes += 1
  if (!isRoot) {
    var d = depth * stepLen   // steps shrink toward the branch tips
    x += cos(heading) * d
    y += sin(heading) * d
    if (wrapMode) {
      x = mod(x, 1)
      y = mod(y, 1)
    }
  }
  if (depth <= drawLevels && x >= 0 && x < 1 && y >= 0 && y < 1) {
    var ci = floor(y * 15.99) * SIZE + floor(x * 15.99)
    var newHue = mod(baseHue + depth * .045, 1)   // depth gradient
    var b0 = briBuf[ci]
    // brightness-weighted circular hue average with the cell's hue
    var dh = mod(newHue - hueBuf[ci] + .5, 1) - .5
    hueBuf[ci] = mod(hueBuf[ci] + dh / (b0 + 1), 1)
    briBuf[ci] = b0 + 1
    if (briBuf[ci] > frameMax) frameMax = briBuf[ci]
  }
  if (depth > 1) {
    branch(x, y, heading + ang1, depth - 1, 0)
    branch(x, y, heading + ang2, depth - 1, 0)
  }
}

export function beforeRender(delta) {
  // fade: ghost trails persist by the trails factor
  feedback(briBuf, trails * .97)

  // three slow oscillators at mutually incommensurate periods
  ang1 = -1 + sin(time(.31 / speedF) * PI2) * PI * range1
  ang2 = .35 + sin(time(.47 / speedF) * PI2) * PI * range2
  head0 = sin(time(.73 / speedF) * PI2) * PI
  baseHue = time(.09 / speedF)   // hue cycle: several seconds

  // draw the petal ring
  frameMax = 0
  for (var r = 0; r < replicas; r++) {
    var cx = .5
    var cy = .5
    var hd = head0
    if (replicas > 1) {
      var slot = r / replicas * PI2
      if (pinwheelMode) {
        // petals fixed on the ring, spinning in place
        cx = .5 + cos(slot) * spacing
        cy = .5 + sin(slot) * spacing
        hd = head0 + slot
      } else {
        // the whole ring revolves around the center
        var a = slot + head0
        cx = .5 + cos(a) * spacing
        cy = .5 + sin(a) * spacing
        hd = a
      }
    }
    branch(cx, cy, hd, maxDepth, 1)
  }

  // auto-exposure: relax the normalizer a few percent toward this
  // frame's max so stacked deposits reveal structure without flicker
  expNorm += (max(frameMax, 1) - expNorm) * .04
  expNorm = clamp(expNorm, 1, 1000)
}

export function render2D(index, x, y) {
  var ci = floor(y * 15.99) * SIZE + floor(x * 15.99)
  var v = saturate(briBuf[ci] / expNorm)
  v = v * v   // contrast
  var s = whiteMode ? 1 - v * .85 : 1   // hot spots bleach to white
  hsv(hueBuf[ci], s, v)
}
