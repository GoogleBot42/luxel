// name: Fireworks
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/2d-fireworks-fade.md, 4th.md, firework-nova.md, firework-rocket-sparks.md — reimagined, not a replica.
// A fireworks show on a panel or a strip: rockets climb with a sparkling fuse
// trail, stall at apogee, flash, and break into peony, ring, willow or crackle
// shells whose stars drag against the air, droop and gutter out. Shells overlap,
// so the sky is never dark for long. Drawn straight onto the persistent frame:
// fade() for the afterglow, one drawLine (panel) or fillRangeAA (strip) per star.
// Bunting adds dim red/white/blue flags (a garland on a panel, bands on a strip).

var NSH = 8                 // shells in flight at once
var PER = 32                // star slots per shell
var G = 1.1                 // reference gravity, sky heights per s^2

var launchRate = 30
//# min=5 max=120 step=1 default=30
export function sliderLaunchRate(v) { launchRate = clamp(v, 1, 200) }

// 0 mix, 1 peony, 2 ring, 3 willow, 4 crackle
var shellMode = 0
//# min=0 max=4 step=1 default=0
export function sliderShell(v) { shellMode = clamp(floor(v), 0, 4) }

// 0 mixed, 1 gold, 2 red-white-blue, 3 pastel
var palette = 0
//# min=0 max=3 step=1 default=0
export function sliderPalette(v) { palette = clamp(floor(v), 0, 3) }

// percent of normal gravity: low = high, floating bursts; high = low, drooping
var gravityPct = 100
//# min=0 max=200 step=5 default=100
export function sliderGravity(v) { gravityPct = clamp(v, 0, 200) }

// seconds a streak takes to fade out behind a moving star
var trailS = 1
//# min=0.2 max=3 step=0.1 default=1
export function sliderTrail(v) { trailS = clamp(v, 0.2, 3) }

// percent: how much the climbing fuse sputters white sparkles
var rocketSparks = 50
//# min=0 max=100 step=5 default=50
export function sliderRocketSparks(v) { rocketSparks = clamp(v, 0, 100) }

var bunting = 0
//# min=0 max=1 step=1 default=0
export function toggleBunting(v) { bunting = v > 0.5 }

// a hand-fired volley for a button or a companion app
export function triggerFinale() { volley = 8; launchT = 0 }

// five colours per palette: hue, saturation (entry 1 is the pale/willow one)
var PH = array(20)
var PS = array(20)
function setP(i, h, s) { PH[i] = h; PS[i] = s }
setP(0, 0.0, 1); setP(1, 0.1, 0.8); setP(2, 0.36, 0.9); setP(3, 0.6, 1); setP(4, 0.8, 0.85)
setP(5, 0.07, 1); setP(6, 0.11, 0.65); setP(7, 0.05, 0.95); setP(8, 0.13, 0.45); setP(9, 0.09, 0.85)
setP(10, 0.0, 1); setP(11, 0.1, 0.06); setP(12, 0.63, 1); setP(13, 0.99, 0.95); setP(14, 0.6, 0.85)
setP(15, 0.94, 0.45); setP(16, 0.45, 0.4); setP(17, 0.74, 0.42); setP(18, 0.09, 0.45); setP(19, 0.55, 0.45)

var stage = array(NSH)      // 0 idle, 1 climbing, 2 broken
var sx = array(NSH)
var sy = array(NSH)         // sky height, 0 = ground, 1 = top
var svx = array(NSH)
var svy = array(NSH)
var fuse = array(NSH)       // seconds of climb left
var kind = array(NSH)       // 1 peony, 2 ring, 3 willow, 4 crackle
var cr = array(NSH)
var cg = array(NSH)
var cb = array(NSH)
var drag = array(NSH)
var droop = array(NSH)
var burn = array(NSH)       // star lifetime, seconds
var flash = array(NSH)      // burst flash timer
var rebreak = array(NSH)    // secondary break countdown, 0 = none
var used = array(NSH)

var NSP = NSH * PER
var px = array(NSP)
var py = array(NSP)
var pvx = array(NSP)
var pvy = array(NSP)
var life = array(NSP)

var col = array(3)
var dt = 0.05
var launchT = 0
var started = 0
var volley = 0
var showT = 0
var buntOff = 0

// Mapless strips stay strips: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { hsv(0, 0, 0) }

function pick(i) {
  var k = palette * 5 + i
  hsv2rgb(PH[k], PS[k], 1, col)
}

function launch(strip) {
  var s = -1
  for (var i = 0; i < NSH; i++) if (stage[i] == 0) { s = i; break }
  if (s < 0) return 0
  var t = shellMode
  if (t == 0) {
    var r = random(1)
    t = r < 0.4 ? 1 : r < 0.6 ? 2 : r < 0.8 ? 3 : 4
  }
  kind[s] = t
  var h = strip ? 0.42 + random(0.42) : 0.42 + random(0.36)
  var v0 = sqrt(2 * G * h)          // the lift charge is fixed…
  fuse[s] = v0 / G                  // …and so is the fuse: gravity moves the burst
  sx[s] = 0.22 + random(0.56)
  sy[s] = 0
  svx[s] = (random(2) - 1) * 0.05
  svy[s] = v0
  stage[s] = 1
  flash[s] = 0
  rebreak[s] = 0
  used[s] = 0
  pick(t == 3 ? 1 : floor(random(5)))
  if (t == 3 && palette == 0) hsv2rgb(0.1, 0.75, 1, col)   // willows burn gold
  if (t == 4) { col[0] = 0.4 + col[0] * 0.6; col[1] = 0.4 + col[1] * 0.6; col[2] = 0.4 + col[2] * 0.6 }
  cr[s] = col[0]; cg[s] = col[1]; cb[s] = col[2]
  return 1
}

function burst(s, strip) {
  stage[s] = 2
  flash[s] = 0.12
  var t = kind[s]
  var n = 30, base = 1.25, ragged = 1
  drag[s] = 3; droop[s] = 0.3; burn[s] = 1.7
  if (t == 2) { n = 28; base = 0.75; ragged = 0; drag[s] = 2; droop[s] = 0.12; burn[s] = 1.8 }
  else if (t == 3) { n = 24; base = 1.1; drag[s] = 3.6; droop[s] = 0.9; burn[s] = 2.8 }
  else if (t == 4) { n = 32; base = 1.5; drag[s] = 4; droop[s] = 0.25; burn[s] = 1.1; rebreak[s] = 0.25 + random(0.1) }
  else if (random(1) < 0.25) rebreak[s] = 0.35 + random(0.15)   // a two-colour peony
  if (strip) n = 18
  used[s] = n
  for (var k = 0; k < n; k++) {
    var i = s * PER + k
    var sp = ragged ? base * (0.45 + random(0.65)) : base
    px[i] = sx[s]; py[i] = sy[s]
    if (strip) {
      pvx[i] = 0
      pvy[i] = ragged ? sp * (random(2) - 1) : (k % 2 ? sp : -sp)
    } else {
      var a = (k + 0.5) / n * PI2 + (ragged ? (random(1) - 0.5) * PI2 / n : 0)
      pvx[i] = cos(a) * sp + svx[s] * 0.5
      pvy[i] = sin(a) * sp + svy[s] * 0.3
    }
    life[i] = 0.8 + random(0.2)
  }
}

function rebreakShell(s) {
  flash[s] = 0.07
  var kick = kind[s] == 4 ? 0.45 : 0.3
  for (var k = 0; k < used[s]; k++) {
    var i = s * PER + k
    if (life[i] <= 0) continue
    pvy[i] += (random(2) - 1) * kick
    if (pvx[i] != 0 || kind[s] != 4) pvx[i] += (random(2) - 1) * kick
    life[i] = max(life[i], 0.75)
  }
  if (kind[s] == 1) { pick(floor(random(5))); cr[s] = col[0]; cg[s] = col[1]; cb[s] = col[2] }
  rebreak[s] = kind[s] == 4 && random(1) < 0.5 ? 0.18 + random(0.1) : 0
}

export function beforeRender(delta) {
  dt = min(delta, 80) * 0.001
  showT += dt
  buntOff = mod(buntOff + dt * 2, 3000)
}

// ---- drawing helpers: the same calls serve both geometries ----------------

var strip = 0
var N1 = 1
var rad = 0.04

// a star's path this frame in colour (r, g, b); on a strip the swept range is
// dimmer than the head, so a burst stays a spray of points, not a smear
function streak(x0, y0, x1, y1, w, r, g, b, tail) {
  if (strip) {
    var a = y0 * N1, e = y1 * N1
    if (abs(e - a) > 1) {
      rgb(r * tail, g * tail, b * tail)
      fillRangeAA(min(a, e), max(a, e) + 0.5, 2)
    }
    rgb(r, g, b)
    splatIndex(e + 0.5, w, 2)
  } else {
    rgb(r, g, b)
    drawLine(x0, 1 - y0, x1, 1 - y1, rad * w, 2)
  }
}

function dot(x, y, w) {
  if (strip) splatIndex(y * N1 + 0.5, w, 2)
  else splat(x, 1 - y, rad * w, 2)
}

function drawBunting() {
  if (strip) {
    var bw = max(3, round(pixelCount / 30))
    var off = mod(buntOff, bw * 3)
    var k = 0
    for (var p = -off; p < pixelCount; p += bw) {
      var c = k % 3
      k++
      if (c == 0) rgb(0.16, 0.01, 0.015)
      else if (c == 1) rgb(0.08, 0.08, 0.08)
      else rgb(0.015, 0.02, 0.18)
      fillRangeAA(p + 0.5, p + bw - 0.5, 2)
    }
    return
  }
  // a garland of pennants swagged between three hooks along the top edge
  var n = 15
  for (var k = 0; k < n; k++) {
    var x = (k + 0.5) / n
    var y = 0.03 + 0.07 * sin(frac(x * 2) * PI)
    var c = k % 3
    if (c == 0) rgb(0.3, 0.01, 0.02)
    else if (c == 1) rgb(0.16, 0.16, 0.16)
    else rgb(0.02, 0.04, 0.32)
    drawLine(x, y, x, y + 0.035, rad * 1.6, 2)
  }
}

export function renderBulk() {
  strip = !has2DMap()
  N1 = pixelCount - 1
  if (!strip) {
    var gw = gridWidth()
    if (gw < 1) gw = 32
    rad = (1.1 + gw / 96) / gw
  }
  fade(pow(0.5, dt * 4.3 / trailS))
  if (!started) {
    // open on a shell already halfway up, so the first second is not empty
    started = 1
    launch(strip)
    sy[0] = 0.3; svy[0] = sqrt(2 * G * 0.25); fuse[0] = svy[0] / G
  }
  if (bunting) drawBunting()

  // schedule: jittered gaps; never leave the sky empty for long
  var live = 0
  for (var s = 0; s < NSH; s++) if (stage[s]) live++
  launchT -= dt
  if (live == 0) launchT = min(launchT, 0.25)
  if (showT > 24) {
    showT = 0
    if (shellMode == 0 && volley == 0) volley = 4   // every so often, a volley
  }
  if (launchT <= 0) {
    var fired = launch(strip)
    if (fired && random(1) < 0.2) launch(strip)        // a pair, now and then
    if (volley > 0) {
      if (fired) volley--
      launchT = fired ? 0.3 + random(0.25) : 0.1
    } else launchT = fired ? (60 / launchRate) * (0.5 + random(1)) : 0.1
  }

  var g = G * gravityPct / 100
  for (var s = 0; s < NSH; s++) {
    if (stage[s] == 0) continue
    if (stage[s] == 1) {
      // climbing: a guttering warm comet, drawn along the whole step
      var x0 = sx[s], y0 = sy[s]
      svy[s] -= g * dt
      sx[s] += svx[s] * dt
      sy[s] += svy[s] * dt
      fuse[s] -= dt
      var b = 0.55 + random(0.45)
      streak(x0, y0, sx[s], sy[s], 1, b, 0.55 * b, 0.18 * b, 0.25)
      if (random(1) < rocketSparks * 0.25 * dt) {
        var w = 0.5 + random(0.5)
        rgb(w, w, w * 0.9)
        dot(sx[s] + (random(2) - 1) * 0.02, sy[s] - random(strip ? 0.06 : 0.12), 0.8)
      }
      if (fuse[s] <= 0 || svy[s] <= 0.03 || sy[s] > (strip ? 0.9 : 0.8)) burst(s, strip)
      continue
    }

    if (flash[s] > 0) {
      flash[s] -= dt
      var f = flash[s] > 0 ? 1 : 0.5
      rgb(f, f, f * 0.92)
      dot(sx[s], sy[s], 2.4)
    }
    if (rebreak[s] > 0) {
      rebreak[s] -= dt
      if (rebreak[s] <= 0) rebreakShell(s)
    }

    var dk = min(drag[s] * dt, 0.9)
    var drop = g * droop[s] * dt
    var step = dt / burn[s]
    var crackle = kind[s] == 4
    var alive = 0
    for (var k = 0; k < used[s]; k++) {
      var i = s * PER + k
      var L = life[i]
      if (L <= 0) continue
      var vx = pvx[i], vy = pvy[i] - drop
      vx -= vx * dk
      vy -= vy * dk
      var ox = px[i], oy = py[i]
      var nx = ox + vx * dt, ny = oy + vy * dt
      if (ny <= 0) { ny = 0; vx = 0; vy = 0; L -= step * 4 }
      pvx[i] = vx; pvy[i] = vy; px[i] = nx; py[i] = ny
      L -= step
      life[i] = L
      if (L <= 0) continue
      alive++
      var bri = L * (0.5 + L * 0.5)
      if (crackle) bri *= random(1) < 0.45 ? 0.1 : 1.3
      else if (L < 0.35 && random(1) < 0.35) bri *= 0.3        // guttering embers
      // hot stars burn whiter, cooling ones show the shell colour
      var hot = max(0, L - 0.7) * 2
      streak(ox, oy, nx, ny, 1.5, (cr[s] + hot) * bri, (cg[s] + hot) * bri, (cb[s] + hot) * bri, 0.3)
    }
    if (alive == 0 && flash[s] <= 0) stage[s] = 0
  }
}
