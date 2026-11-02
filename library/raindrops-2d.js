// name: Raindrops
// tags: both, nature
// Clean-room reimplementation from the prose spec docs/pattern-specs/raindrops-2d.md — reimagined, not a replica.
// Rain on a pool seen from above. Sim mode: a two-buffer wave equation on a
// canvas of up to 32x32 cells (stencil2D + feedback, fixed 30 Hz step), lit
// from the top-left so every ring reads as a raised crest. Rings mode: crisp
// analytic rings drawn as short drawLine arcs. Tap the preview (or send an
// event) to drop a stone exactly there. On a strip: a dripping tap — drops
// swell, fall with motion trails, and splash into a pool, throwing droplets.

var CAP = 32
var prev = array(1024), cur = array(1024), lap = array(1024)
var vArr = array(1024), tex = array(1024)
var W = 32, H = 32, gwSeen = -1, flat = 1, dirty = 1
var simAcc = 0, dropAcc = 0, nextDrop = 300
var ev = array(4)
var pal = array(24)
var dropC = array(3)                  // the strip drops' colour (palette 0.85) as h, s, v

// analytic rings (mode 1): a small pool of expanding rings
var NR = 10
var rx = array(NR), ry = array(NR), rAge = array(NR), rAmp = array(NR)

// strip: drops in flight and splash droplets
var ND = 6, NS = 12
var dPos = array(ND), dVel = array(ND), dOn = array(ND)
var sPos = array(NS), sVel = array(NS), sLife = array(NS)
var swell = 0, poolE = 0, poolPh = 0

var rate = 2
// Mean drops per second (the gaps between them stay random).
//# min=0.2 max=20 step=0.1 default=2
export function sliderDropsPerSecond(v) { rate = clamp(v, 0.2, 20) }

var fadeS = 3
var damp = pow(0.02, 2 / (3 * 30))
// Seconds for a ripple to die away.
//# min=0.5 max=10 step=0.1 default=3
export function sliderDampingS(v) { fadeS = clamp(v, 0.5, 10); damp = pow(0.02, 2 / (fadeS * 30)) }

var mode = 0
// 0 sim (true wave interference) · 1 rings (clean analytic rings)
//# min=0 max=1 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 1); dirty = 1 }

var look = 0
// 0 pool · 1 moonlit · 2 ink
//# min=0 max=2 step=1 default=0
export function sliderPalette(v) { look = clamp(floor(v), 0, 2); build() }

// A mapless strip stays a strip (this export suppresses the default grid).
export function render(index) { hsv(0, 0, 0) }

function stop(k, pos, r, g, b) {
  pal[k * 4] = pos
  pal[k * 4 + 1] = r
  pal[k * 4 + 2] = g
  pal[k * 4 + 3] = b
}

// position 0 deep trough, 0.3 still water (>= 15 % bright), 1 white crest
function build() {
  if (look == 0) {
    stop(0, 0, 0.01, 0.06, 0.2)
    stop(1, 0.3, 0.03, 0.2, 0.42)
    stop(2, 0.55, 0.06, 0.55, 0.8)
    stop(3, 0.75, 0.3, 0.9, 1)
    stop(4, 0.9, 0.85, 1, 1)
    stop(5, 1, 1, 1, 1)
  } else if (look == 1) {
    stop(0, 0, 0.02, 0.03, 0.1)
    stop(1, 0.3, 0.1, 0.14, 0.32)
    stop(2, 0.55, 0.3, 0.4, 0.65)
    stop(3, 0.75, 0.62, 0.72, 0.9)
    stop(4, 0.9, 0.92, 0.95, 1)
    stop(5, 1, 1, 1, 1)
  } else {
    stop(0, 0, 0.06, 0.01, 0.12)
    stop(1, 0.3, 0.2, 0.05, 0.36)
    stop(2, 0.55, 0.5, 0.1, 0.62)
    stop(3, 0.75, 0.95, 0.35, 0.75)
    stop(4, 0.9, 1, 0.8, 0.9)
    stop(5, 1, 1, 1, 1)
  }
  setPalette(pal)
  // stops 3 and 4 bracket 0.85: the drops' colour, as HSV for gradientRange
  var f = (0.85 - pal[12]) / (pal[16] - pal[12])
  rgb2hsv(mix(pal[13], pal[17], f), mix(pal[14], pal[18], f), mix(pal[15], pal[19], f), dropC)
}
build()

function resize() {
  var gw = gridWidth(), gh = gridHeight()
  if (gw == gwSeen) return
  gwSeen = gw
  if (gw == 0) { gw = 32; gh = 32 }
  var k = ceil(max(gw, gh) / CAP)
  W = ceil(gw / k)
  H = ceil(gh / k)
  feedback(prev, 0)
  feedback(cur, 0)
  flat = 1
  // still-water mottle: a faint, fixed sandy texture under the surface
  fillNoise2D(tex, W, H, 0.3, 0.3, 0, 0, 7)
  feedback(tex, 0.035)
  dirty = 1
}

// a stone at normalized (x, y): a soft bump of height a, radius ~1.5 cells
function splash(x, y, a) {
  var cx = floor(x * W), cy = floor(y * H)
  for (var oy = -2; oy <= 2; oy++) for (var ox = -2; ox <= 2; ox++) {
    var px = cx + ox, py = cy + oy
    if (px >= 0 && px < W && py >= 0 && py < H) {
      var d = (ox * ox + oy * oy) / 6.5
      if (d < 1) cur[py * W + px] += a * (1 - d)
    }
  }
  flat = 0
  dirty = 1
}

function ring(x, y, a) {
  var best = 0
  for (var i = 1; i < NR; i++) if (rAge[i] > rAge[best]) best = i
  rx[best] = x
  ry[best] = y
  rAge[best] = 0
  rAmp[best] = a
}
for (var i0 = 0; i0 < NR; i0++) rAge[i0] = 99

function drop(x, y, a) {
  if (mode == 0) splash(x, y, a)
  else ring(x, y, a)
}

function step() {
  if (flat) return
  var t = prev
  prev = cur
  cur = t
  feedback(lap, 0)
  stencil2D(lap, prev, W, H, -4, 1, 0)
  feedback(lap, 0.22)
  feedback(cur, -1)
  arrayAdd(cur, prev)
  arrayAdd(cur, prev)
  arrayAdd(cur, lap)
  feedback(cur, damp)
  dirty = 1
  if (arrayMaxAbs(cur) < 0.004) {
    feedback(prev, 0)
    feedback(cur, 0)
    flat = 1
  }
}

function stripTick(dt) {
  var g = 2.4                                   // strip lengths per s^2
  swell += dt * rate * 1.1
  if (swell >= 1) {
    for (var i = 0; i < ND; i++) if (!dOn[i]) {
      dOn[i] = 1
      dPos[i] = 0
      dVel[i] = 0.05
      break
    }
    swell = 0
  }
  for (i = 0; i < ND; i++) if (dOn[i]) {
    dVel[i] += g * dt
    dPos[i] += dVel[i] * dt
    if (dPos[i] >= 0.92) {
      dOn[i] = 0
      poolE = min(poolE + 0.8, 1.5)
      var thrown = 0
      for (var j = 0; j < NS && thrown < 3; j++) if (sLife[j] <= 0) {
        sPos[j] = 0.92
        sVel[j] = -(0.25 + random(0.35))
        sLife[j] = 1
        thrown++
      }
    }
  }
  for (j = 0; j < NS; j++) if (sLife[j] > 0) {
    sVel[j] += g * dt
    sPos[j] += sVel[j] * dt
    sLife[j] -= dt * 0.8
    if (sPos[j] >= 0.92 && sVel[j] > 0) sLife[j] = 0
  }
  poolE *= pow(0.02, dt / fadeS)
  poolPh = mod(poolPh + dt * 1.5, 1)
}

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  var gridded = has2DMap()
  if (gridded) resize()
  while (readEvent(ev)) if (gridded) drop(ev[1], ev[2], 1.2)
  if (!gridded) { stripTick(dt); return }
  dropAcc += delta
  var made = 0
  while (dropAcc > nextDrop && made < 4) {
    drop(random(1), random(1), 0.6 + random(0.6))
    dropAcc -= nextDrop
    nextDrop = random(2000 / rate)
    made++
  }
  if (made == 4) dropAcc = 0
  for (var i = 0; i < NR; i++) rAge[i] += dt / fadeS
  simAcc += delta
  var n = 0
  while (simAcc >= 33.333 && n < 4) { step(); simAcc -= 33.333; n++ }
  if (n == 4) simAcc = 0
}

// lit from the top-left: brightness = rest + height + facing slope
function shade() {
  var i = 0
  for (var r = 0; r < H; r++) {
    var dn = r < H - 1 ? W : 0
    for (var c = 0; c < W; c++) {
      var h = cur[i]
      var rt = c < W - 1 ? cur[i + 1] : h
      vArr[i] = clamp(0.3 + tex[i] + h * 0.4 + (2 * h - rt - cur[i + dn]) * 0.8, 0, 0.995)
      i++
    }
  }
}

function rings() {
  paint(0.3)
  fill()
  var gw = max(gridWidth(), 8)
  var lw = 1.3 / gw
  for (var i = 0; i < NR; i++) {
    var a = rAge[i]
    if (a < 1) {
      for (var k = 0; k < 2; k++) {
        var rr = a * 0.9 - k * 0.07
        if (rr > 0.01) {
          var b = rAmp[i] * (1 - a) * (k ? 0.45 : 1)
          paint(0.3 + 0.7 * min(b, 1))
          var seg = clamp(floor(rr * gw * 0.8), 12, 48)
          var x0 = rx[i] + rr, y0 = ry[i]
          for (var s = 1; s <= seg; s++) {
            var th = s * PI2 / seg
            var x1 = rx[i] + rr * cos(th), y1 = ry[i] + rr * sin(th)
            drawLine(x0, y0, x1, y1, lw, 2)
            x0 = x1
            y0 = y1
          }
        }
      }
    }
  }
}

function strip() {
  var n = pixelCount, p0 = floor(n * 0.92)
  paint(0.3, 0.3)
  fillRange(0, p0)
  // the pool at the far end: still, it is the same water as the rest of the
  // strip; each splash sends ripples out both ways from where it landed,
  // brightening as they crest and dying away with distance
  var reach = max(n - p0, 4) * 1.6, e = min(poolE, 1)
  for (var i = max(0, floor(p0 - reach * 0.5)); i < n; i++) {
    var d = abs(i - p0) / reach
    var rip = e * wave(d * 4 - poolPh * 3) * max(0, 1 - d)
    paint(clamp(0.3 + rip * 0.4, 0, 0.99), 0.3 + rip * 0.55)
    setPixel(i)
  }
  // the swelling drop at the tap
  paint(0.3 + 0.6 * swell, 0.3 + 0.7 * swell)
  fillRangeAA(0, max(1, n * 0.012), 2)
  // falling drops with a motion trail as long as their speed
  for (i = 0; i < ND; i++) if (dOn[i]) tail(dPos[i] * (n - 1), dVel[i] * n * 0.06, 1)
  for (i = 0; i < NS; i++) if (sLife[i] > 0) tail(sPos[i] * (n - 1), sVel[i] * n * 0.04, sLife[i])
}

// an anti-aliased streak: the drop is one full pixel at p, its trail fades to
// nothing L pixels behind it (the same light as a 1-px head plus a linear ramp)
function tail(p, len, b) {
  var L = clamp(abs(len), 1, 24), h = dropC[0], s = dropC[1], v = dropC[2] * b
  hsv(h, s, v)
  fillRangeAA(p, p + 1, 2)
  if (L > 1) {
    if (len > 0) gradientRange(p, p + 1 - L, h, s, v, h, s, 0, 2)
    else gradientRange(p + 1, p + L, h, s, v, h, s, 0, 2)
  }
}

export function renderBulk() {
  if (!has2DMap()) { strip(); return }
  if (mode == 1) { rings(); return }
  if (dirty) shade()
  dirty = 0
  paintCanvas(vArr, W, H, 1, gridWidth() > W ? 1 : 0)   // bilinear upscale
}
