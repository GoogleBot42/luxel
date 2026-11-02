// name: Tunnel
// tags: panel, ambient
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/tunnel-of-squares-2d.md — reimagined, not a replica.
// Fly down an endless polygon tunnel. Rings is the corkscrewing neon spiral of
// log-spaced squares (or triangles..octagons); Textured lines the tube with bevelled
// bricks and passing lamp courses. Both fog to a dark vanishing point that wanders,
// bending the tube. Each pixel's twisted angle and polygon log-radius are tabled (a
// sixteenth re-aimed per frame as the camera roams); bricks are a 64x16 texture.
// Above 4,096 px a grid renders at <= 4,096 cells (64x64 on 128x128) and the engine
// expands it bilinearly (setRenderSize): a quarter of the per-pixel work, no 2x2 blocks.

var TN = min(pixelCount, 4096), TW = 64
var A = array(TN), L = array(TN)  // per pixel: twisted angle (turns), log2 polygon radius
var TX = array(TW * 16)           // brick texture: hue step (integer part) + value
var mode = 0, cw = 0, ch = 0, TS = 0, key = -1, aspX = 1, aspY = 1, slice = 0, la = 0, half = 16
var rs = 0, rsW = 0, rsH = 0      // 1 once a big grid renders at rsW x rsH

var style = 0                     // 0 rings, 1 textured
var speed = 1.2                   // rings per second
var rings = 3                     // rings per octave of radius
var twist = 20                    // polygon roll per octave, degrees
var sides = 4
var wander = 50                   // percent
var flow = 0, roll = 0, hue0 = 0, wt = 0, ox = 0, oy = 0, fb = 0, kv = 0, fl = 0

//# min=0 max=1 step=1 default=0
export function sliderStyle(v) { style = clamp(floor(v), 0, 1) }
//# min=0.1 max=4 step=0.1 default=1.2
export function sliderSpeedRingsPerSecond(v) { speed = clamp(v, 0.1, 4) }
//# min=1 max=7 step=1 default=3
export function sliderRingsPerOctave(v) { rings = clamp(floor(v), 1, 7) }
//# min=-90 max=90 step=1 default=20
export function sliderTwistDegrees(v) { twist = clamp(v, -90, 90); key = -1 }
//# min=3 max=8 step=1 default=4
export function sliderSides(v) { sides = clamp(floor(v), 3, 8); key = -1 }
// How far the vanishing point roams, percent of the half-panel.
//# min=0 max=100 step=1 default=50
export function sliderWanderPercent(v) { wander = clamp(v, 0, 100) }

// 16 bricks round, 4 courses deep (4x4 texels each); course 0 is a lamp strip.
for (var ty = 0; ty < 16; ty++) for (var tx = 0; tx < TW; tx++) {
  var c = floor(ty / 4), fy = ty % 4, bx = tx + (c % 2) * 2, fx = bx % 4
  var k = hash2(floor(bx / 4) % 16, c), v = 0.04, hs = 0
  if (c == 0) { v = fy == 0 ? 0.1 : (fy == 2 ? 0.999 : 0.7); hs = 25 }
  else if (fx > 0 && fy > 0) { v = (0.45 + 0.4 * k) * (fy == 1 ? 1.15 : (fy == 3 ? 0.7 : 1)); hs = floor(k * 4) }
  TX[ty * TW + tx] = hs + min(v, 0.999)
}

function geom(x, y) {              // -> log2 polygon radius of the bent tube; angle in la
  x = (x - 0.5) * 2 * aspX; y = (y - 0.5) * 2 * aspY
  var w = 1 / (1 + 3 * (x * x + y * y))   // the far end follows the camera, the mouth stays
  x -= ox * w; y -= oy * w
  var l = log2(max(hypot(x, y), 0.004))
  var a = atan2(y, x) / PI2 + 0.5 - l * twist / 360
  var u = a * sides
  la = mod(a, 1)
  return l + log2(cos((u - floor(u) - 0.5) * PI2 / sides))
}

function aim(i) {
  if (mode == 1) L[i] = geom(pixelCoord(i, 0), pixelCoord(i, 1))
  else L[i] = geom((i % cw + 0.5) / cw, (floor(i / cw) + 0.5) / ch)
  A[i] = la
}

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  wt = mod(wt + dt, 3600)
  var px = ox, py = oy
  ox = sin(wt * 0.21) * 0.45 * wander / 100
  oy = sin(wt * 0.13 + 1) * 0.4 * wander / 100
  if (!rs && pixelCount > TN && has2DMap() && gridWidth() > 0 && gridHeight() > 1) {
    var gw = gridWidth(), gh = gridHeight()
    rsW = min(gw, floor(64 * sqrt(gw / gh) + 0.001)); rsH = min(gh, floor(TN / rsW))
    setRenderSize(rsW, rsH, 1)      // pixelCount is rsW * rsH from here on
    rs = 1
  }
  var w = rs ? rsW : gridWidth(), h = rs ? rsH : gridHeight()
  if (w * 4096 + h != key) {
    key = w * 4096 + h
    var m = max(max(w, h), 1)
    aspX = w > 0 ? w / m : 1; aspY = h > 0 ? h / m : 1
    half = w > 0 ? m / 2 : 16
    // 1: table per pixel by map; 2: render-size table built by cell; 0: no table
    mode = rs ? 2 : (pixelCount <= TN ? 1 : 0)
    cw = w; ch = h
    TS = mode == 1 ? pixelCount : (mode == 2 ? cw * ch : 0)
    for (var i = 0; i < TS; i++) aim(i)
  } else if (mode && (px != ox || py != oy)) {
    for (i = slice; i < TS; i += 16) aim(i)
    slice = (slice + 1) % 16        // interleaved sixteenths: no visible seam
  }
  flow = mod(flow + dt * speed, 64)
  roll = mod(roll + dt * 2, TW)
  hue0 = mod(hue0 + dt / 30, 1)
  // fog where rings (or brick courses) pack tighter than ~1.5 px
  var dens = style ? rings * 1.2 : rings
  fb = 1 - log2(1.7 * dens / half) * 0.7
  kv = rings * 4.8; fl = flow * 4.8
}

export function render2D(index, x, y) {
  var a, l
  if (mode) { a = A[index]; l = L[index] }
  else { l = geom(x, y); a = la }
  var fog = clamp(l * 0.7 + fb, 0, 1)
  if (style == 0) {
    var v = triangle(rings * l + a - flow)
    hsv(hue0 + l * 0.15, 1, v * v * v * fog)
  } else {
    var p = TX[(floor(l * kv - fl) & 15) * TW + (floor(a * TW + roll) & 63)]
    var hs = floor(p)
    hsv(hue0 + hs * 0.02, 0.85, (p - hs) * fog)
  }
}
