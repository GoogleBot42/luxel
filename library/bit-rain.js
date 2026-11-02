// name: Bit Rain
// tags: both, retro
// Original work from the prose idea specs docs/pattern-specs/patternflow/dither-bit-rain.md and kaleido-bit-rain.md (Patternflow presets, CC BY-SA 4.0 — prose-described ideas only, no code).
// A 1-bit glitch screen: chunky data streams rain through a 4x4 Bayer dither,
// rings pulse out of the centre lighting an XOR bit lattice, heads glow neon.
// Symmetry folds it into a mirror or a swaying four-fold kaleidoscope with
// sky-blue axis beams; bands of rows tear sideways on the beat and slide back.
// One small canvas (streams deposited, then dithered) and one paintCanvas.

var src = array(4096), out = array(4096)     // signal → codes; composed frame
var du2 = array(64), dv2 = array(64), tr = array(64)
var mu = array(160), rip = array(256), rq = array(256), lph = array(64), tmp = array(3)
var bay = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5]
var tearR = array(2), tearH = array(2), tearA = array(2), tearT = [9, 9]
// codes: 0 black · .25 halo · .5 white · .75 accent · 1 beam
var pal = [0, 0, 0, 0, 0.25, 0.07, 0.02, 0.22, 0.5, 1, 1, 1, 0.75, 1, 0.1, 0.6, 1, 0.15, 0.65, 1]
setPalette(pal)
for (var k = 0; k < 256; k++) rq[k] = sqrt(k * 8 + 4)
for (k = 0; k < 16; k++) bay[k] = (bay[k] + 0.5) / 16
for (k = 0; k < 64; k++) lph[k] = random(64)

// 0 none · 1 mirror · 2 four-fold kaleidoscope
var sym = 1
//# min=0 max=2 step=1 default=1
export function sliderSymmetry(v) { sym = clamp(floor(v), 0, 2) }

// Stream speed in cells per second.
var speed = 10
//# min=2 max=40 step=1 default=10
export function sliderSpeed(v) { speed = clamp(v, 2, 40) }

// Dither cells across the panel; each cell is a whole number of pixels.
var cells = 32
//# min=16 max=64 step=8 default=32
export function sliderCells(v) { cells = clamp(floor(v), 16, 64) }

// XOR lattice bit mask: 0 off, 1 fine checker … 7 wide diagonals.
var mask = 5
//# min=0 max=7 step=1 default=5
export function sliderLattice(v) { mask = clamp(floor(v), 0, 7) }

// Chance per beat that a band of rows tears sideways, percent.
var tears = 40
//# min=0 max=100 step=1 default=40
export function sliderTears(v) { tears = clamp(v, 0, 100) }

var bpm = 120
//# min=60 max=180 step=1 default=120
export function sliderTempoBpm(v) { bpm = clamp(v, 60, 180) }

// Accent (stream head) hue in degrees.
var accent = 330
//# min=0 max=359 step=1 default=330
export function sliderAccentHue(v) { accent = mod(v, 360) }

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

var cw = 32, ch = 32, sw = 32, sh = 32, strip = 0, t = 0, dt = 0, lastBeat = 0, pulse = 0

export function beforeRender(delta) {
  dt = min(delta, 200) / 1000
  t = mod(t + dt, 3600)
  var b = beat(bpm)
  pulse = (1 - b) * (1 - b)
  if (b < lastBeat && random(100) < tears) {
    var j = tearT[0] > tearT[1] ? 0 : 1      // reuse the older slot
    tearR[j] = random(1); tearH[j] = 0.04 + random(0.14)
    tearA[j] = (random(1) < 0.5 ? -1 : 1) * (0.1 + random(0.25)); tearT[j] = 0
  }
  lastBeat = b
  tearT[0] += dt; tearT[1] += dt
  hsv2rgb(accent / 360, 0.9, 1, tmp)
  pal[13] = tmp[0]; pal[14] = tmp[1]; pal[15] = tmp[2]
}

function layout() {
  strip = !has2DMap()
  var gw = gridWidth(), gh = gridHeight()
  if (strip) { cw = min(pixelCount, 4096); ch = 1 }
  else {
    if (gw == 0) { gw = 32; gh = 32 }
    var B = max(max(1, round(gw / cells)), ceil(max(gw, gh) / 64))
    cw = ceil(gw / B); ch = ceil(gh / B)
  }
  sw = sym ? ceil(cw / 2) : cw
  sh = sym == 2 ? ceil(ch / 2) : ch
  if (strip) return
  var ox = sym ? 0 : cw / 2, oy = sym == 2 ? 0 : ch / 2, cx = cw / 2 - 0.5
  for (var u = 0; u < sw; u++) du2[u] = (u + 0.5 - ox) * (u + 0.5 - ox)
  for (var v = 0; v < sh; v++) dv2[v] = (v + 0.5 - oy) * (v + 0.5 - oy)
  // column lookup with 48 cells of slack either side for tears and sway
  for (var k = 0; k < cw + 96; k++) mu[k] = sym ? min(sw - 1, floor(abs(k - 48 - cx))) : mod(k - 48, cw)
}

// Streams: lanes a few cells wide, each a train of chunks at a hashed
// spacing; mass fades from the head so the dither thins the tail out.
// Rain falls down the rows; the kaleidoscope (and a strip) flows along u.
function deposit() {
  var along = sym == 2 || strip
  var flen = along ? sw : sh, wide = along ? sh : sw
  var lw = strip ? 1 : max(2, floor(cw / 16)), lanes = strip ? 2 : min(64, ceil(wide / lw))
  for (var l = 0; l < lanes; l++) {
    var hl = hash(l * 1.37 + 0.5)
    var len = 3 + floor(hash(l * 2.91 + 7) * (strip ? 12 : flen * 0.35))
    var cyc = len + 4 + (strip ? 40 : flen * 0.9) * (0.3 + hl), a0 = strip ? 0 : l * lw
    lph[l] = mod(lph[l] + speed * (0.6 + hl * 0.8) * dt, cyc)
    for (var hp = lph[l]; hp - len < flen; hp += cyc) {
      var h0 = floor(hp)
      for (var q = max(0, h0 - flen + 1); q < len && q <= h0; q++) {
        var f = h0 - q, w = q ? 1 - 0.85 * q / len : 100   // 100 marks a head
        for (var a = a0; a < a0 + lw && a < wide; a++) src[along ? a * sw + f : f * sw + a] += w
      }
      if (sym == 2 && h0 < len && h0 >= 0) {     // emitting: light the axis
        for (a = a0; a < a0 + lw && a < wide; a++) src[a * sw] += 10
      }
    }
  }
}

function dither() {
  var m = mask, halo = sym ? 2 : 0, tw = t * 4
  for (var k = 0; k < 256; k++) { var w = sin(rq[k] * 0.55 - tw); rip[k] = w > 0 ? w * w : 0 }
  for (var v = 0; v < sh; v++) {
    var base = v * sw, dv = dv2[v], by = (v & 3) * 4
    for (var u = 0; u < sw; u++) {
      var i = base + u, s = src[i]
      if (s >= 10) { src[i] = s >= 100 ? 0.75 : 1; continue }   // head / beam
      var r = rip[min(255, (du2[u] + dv) * 0.125)]
      s += m && ((u ^ v) & m) == 0 ? 0.1 + 0.6 * r : 0.1 * r
      var th = bay[(u & 3) + by]
      src[i] = s > th ? 0.5 : (u < halo && s > th * 0.3 ? 0.25 : 0)
    }
  }
  // the horizontal axis lane glows as a beam in the kaleidoscope
  if (sym == 2) for (u = 0; u < sw; u++) if (src[u] == 0.5) src[u] = 1
  // a few lattice nodes flash in the accent colour
  for (k = 0; k < 8; k++) {
    u = floor(random(sw)); v = floor(random(sh))
    if (m && ((u ^ v) & m) == 0 && src[v * sw + u] == 0.5) src[v * sw + u] = 0.75
  }
}

// mirror / sway / tear into `out`; plain rain tears its rows in place
function compose() {
  var r, c, torn = 0
  for (r = 0; r < ch; r++) tr[r] = 0
  for (var j = 0; j < 2; j++) {
    if (tearT[j] > 0.7) continue
    var off = round(tearA[j] * cw * (1 - easeInOutCubic(tearT[j] / 0.7)))
    for (r = floor(tearR[j] * ch); r < min(ch, (tearR[j] + tearH[j]) * ch); r++) { tr[r] += off; torn = 1 }
  }
  if (sym == 0) {
    if (!torn) return src
    for (r = 0; r < ch; r++) if (tr[r]) {
      var o = r * cw, sft = 48 - tr[r]
      for (c = 0; c < cw; c++) out[c] = src[o + c]     // row scratch in out
      for (c = 0; c < cw; c++) src[o + c] = out[mu[c + sft]]
    }
    return src
  }
  var amp = (0.6 + 1.6 * pulse) * cw / 32, cy = ch / 2 - 0.5
  for (r = 0; r < ch; r++) {
    var so = (sym == 2 ? min(sh - 1, floor(abs(r - cy))) : r) * sw
    var s0 = 48 - tr[r] + (sym == 2 ? round(amp * sin(r * 0.3 + t * 1.3)) : 0), oo = r * cw
    for (c = 0; c < cw; c++) out[oo + c] = src[so + mu[c + s0]]
  }
  return out
}

function stripOut() {
  var n = cw, bs = pixelCount / n, tw = t * 4
  clear()
  for (var k = 0; k < n; k++) {
    var d = abs(k - n / 2), u = sym ? floor(d) : k
    var w = sin(d * 0.35 - tw), s = src[u] + (w > 0 ? 0.25 * w * w : 0), c = 0
    if (s >= 100) c = 0.75
    else if (s > bay[u & 15]) c = 0.5
    if (c > 0) { paint(c); fillRange(floor(k * bs), floor((k + 1) * bs)) }
  }
}

export function renderBulk() {
  layout()
  feedback(src, 0)
  deposit()
  if (strip) { stripOut(); return }
  dither()
  paintCanvas(compose(), cw, ch)
}
