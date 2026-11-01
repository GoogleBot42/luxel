// name: Day Ribbon
// Curated original (hand-written showcase of the Luxel language/builtins).
// The strip is today, midnight to midnight: deep night at the ends, dawn pinks
// and gold around sunrise, sky blue at noon, a dusk glow at sunset. A bright
// marker (a sun by day, a cool moon by night) sits at the current time, hours
// already gone are dimmed so the day visibly fills, faint notches mark the
// hours and stars twinkle in the night. Or centre it on now as a rolling day.
// Index space only: the ribbon is repainted once a minute, the marker every frame.

var sunrise = 6.5, sunset = 19.5
var centred = 0
var reverse = 0

//# min=3 max=10 step=0.25 default=6.5
export function sliderSunriseHour(v) { sunrise = clamp(v, 3, 10); dirty = 1 }

//# min=14 max=23 step=0.25 default=19.5
export function sliderSunsetHour(v) { sunset = clamp(v, 14, 23); dirty = 1 }

// Off: midnight to midnight. On: a rolling day with now in the middle.
//# default=0
export function toggleCentreOnNow(v) { centred = v; dirty = 1 }

//# default=0
export function toggleReverse(v) { reverse = v; dirty = 1 }

// sky keyframes (hours set in beforeRender from sunrise/sunset): night, blue hour,
// pink, sunrise, gold, day, noon, day, gold, sunset, dusk, violet, blue hour, night
var KR = [0.02, 0.05, 0.5, 1.0, 0.9, 0.18, 0.32, 0.18, 0.95, 1.0, 0.55, 0.25, 0.06, 0.02]
var KG = [0.025, 0.03, 0.12, 0.42, 0.65, 0.42, 0.62, 0.42, 0.58, 0.3, 0.08, 0.04, 0.03, 0.025]
var KB = [0.09, 0.18, 0.32, 0.12, 0.35, 0.85, 1.0, 0.85, 0.25, 0.07, 0.28, 0.3, 0.18, 0.09]
var NK = 14
var KH = array(14), col = array(3)
var dirty = 1, now = 0, lastMin = -1, secFrac = 0, lastSec = -1, t = 0
var mark = -10, start = 0                    // marker pixel, hour at pixel 0

function sky(hr) {                            // colour of the sky at hour hr -> col
  hr = mod(hr - KH[0], 24) + KH[0]
  var i = 0
  while (i < NK - 1 && KH[i + 1] <= hr) i++
  if (hr <= KH[0] || i >= NK - 1) { col[0] = KR[0]; col[1] = KG[0]; col[2] = KB[0]; return }
  var f = smoothstep(KH[i], KH[i + 1], hr)
  mixColors(KR[i], KG[i], KB[i], KR[i + 1], KG[i + 1], KB[i + 1], f, col)
}

function hourAt(i) { return mod(start + (reverse ? pixelCount - 1 - i + 0.5 : i + 0.5) / pixelCount * 24, 24) }

function base(i) {                            // ribbon colour of pixel i -> col
  var hr = hourAt(i)
  sky(hr)
  var k = 1, ago = mod(now - hr, 24)
  if (ago < (centred ? 12 : now)) k = 0.42    // hours already gone
  var pph = pixelCount / 24
  if (pph >= 4 && floor(hourAt(i - 1)) != floor(hr)) k *= 0.45
  else if (pph < 4 && floor(hourAt(i - 1) / 3) != floor(hr / 3)) k *= 0.45
  col[0] *= k; col[1] *= k; col[2] *= k
}

function paintPixel(i) { base(i); rgb(col[0], col[1], col[2]); setPixel(i) }

export function beforeRender(delta) {
  var s = clockSecond(), dt = min(delta, 200) / 1000
  if (s != lastSec) { lastSec = s; secFrac = 0 } else secFrac = min(secFrac + dt, 0.999)
  now = clockHour() + clockMinute() / 60 + (s + secFrac) / 3600
  t = mod(t + dt, 3600)
  if (clockMinute() != lastMin) { lastMin = clockMinute(); dirty = 1 }
  start = centred ? now - 12 : 0
  var sr = sunrise, ss = sunset, noon = (sr + ss) / 2, dA = min(2.2, (noon - sr) * 0.6)
  KH[0] = sr - 1.6; KH[1] = sr - 0.8; KH[2] = sr - 0.3; KH[3] = sr; KH[4] = sr + 0.6
  KH[5] = sr + dA; KH[6] = noon; KH[7] = ss - dA; KH[8] = ss - 0.8; KH[9] = ss - 0.2
  KH[10] = ss + 0.15; KH[11] = ss + 0.5; KH[12] = ss + 1.0; KH[13] = ss + 1.6
}

export function renderBulk() {
  var n = pixelCount, i
  if (dirty) { for (i = 0; i < n; i++) paintPixel(i); dirty = 0 }
  else for (i = floor(mark) - 4; i <= floor(mark) + 5; i++) if (i >= 0 && i < n) paintPixel(i)

  var nStars = max(2, floor(n / 25))          // stars twinkle in the night stretches
  for (var j = 0; j < nStars; j++) {
    var cyc = t / (2.5 + j % 3) + j * 0.37
    var p = floor(hash2(j, floor(cyc)) * n)
    var night = hourAt(p)
    if (night > sunrise - 1.2 && night < sunset + 1.2) continue
    var tw = triangle(mod(cyc, 1)), prev = floor(hash2(j, floor(cyc) - 1) * n)
    paintPixel(prev)
    base(p)
    rgb(col[0] + 0.55 * tw, col[1] + 0.55 * tw, col[2] + 0.6 * tw)
    setPixel(p)
  }

  var pos = mod(now - start, 24) / 24 * n - 0.5
  if (reverse) pos = n - 1 - pos
  mark = pos
  var day = now > sunrise && now < sunset, pulse = 0.85 + 0.15 * wave(t / 2.2)
  for (i = floor(pos) - 3; i <= floor(pos) + 4; i++) {
    var d = abs(i - pos), glow = clamp(1 - d / 4, 0, 1)
    if (glow <= 0 || i < 0 || i >= n) continue
    var core = clamp(1.2 - d, 0, 1)
    base(i)
    var g = glow * glow * 0.8 * pulse
    if (day) rgb(col[0] + g + core, col[1] + g * 0.8 + core * 0.85, col[2] + g * 0.3 + core * 0.5)
    else rgb(col[0] + g * 0.55 + core * 0.7, col[1] + g * 0.6 + core * 0.8, col[2] + g + core)
    setPixel(i)
  }
}
