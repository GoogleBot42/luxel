// name: Day Ribbon
// Curated original (hand-written showcase of the Luxel language/builtins).
// The strip is today, midnight to midnight: deep night at the ends, dawn pinks
// and gold around sunrise, sky blue at noon, a dusk glow at sunset. A bright
// marker (a sun by day, a cool moon by night) sits at the current time, hours
// already gone are dimmed so the day visibly fills, faint notches mark the
// hours and stars twinkle in the night. Or centre it on now as a rolling day.
// Index space only: once a minute the sky is sampled (per pixel, at most 144 times) and the
// ribbon repainted as one gradientRange per sample span; the marker is a
// splatIndex glow and only the pixels it and the stars touch are redrawn.

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
var MS = 144                                 // most sky samples across the day
var SH = array(145), SS = array(145), SV = array(145), hc = array(3)
var M = 24                                   // sample spans in use

function sky(hr) {                            // colour of the sky at hour hr -> col
  hr = mod(hr - KH[0], 24) + KH[0]
  var i = 0
  while (i < NK - 1 && KH[i + 1] <= hr) i++
  if (hr <= KH[0] || i >= NK - 1) { col[0] = KR[0]; col[1] = KG[0]; col[2] = KB[0]; return }
  var f = smoothstep(KH[i], KH[i + 1], hr)
  mixColors(KR[i], KG[i], KB[i], KR[i + 1], KG[i + 1], KB[i + 1], f, col)
}

// the sky at M + 1 even steps from pixel 0's hour, as HSV with the hue unwrapped
// so neighbouring samples never lerp the long way round
function sampleSky() {
  M = clamp(pixelCount, 24, MS)
  for (var k = 0; k <= M; k++) {
    sky(start + k * 24 / M)
    rgb2hsv(col[0], col[1], col[2], hc)
    SH[k] = k == 0 ? hc[0] : SH[k - 1] + mod(hc[0] - SH[k - 1] + 0.5, 1) - 0.5
    SS[k] = hc[1]; SV[k] = hc[2]
  }
}

function hourAt(i) { return mod(start + (reverse ? pixelCount - 1 - i + 0.5 : i + 0.5) / pixelCount * 24, 24) }

function notch(i, hr) {                       // first pixel of an hour (or of 3 h on short strips)
  var q = pixelCount / 24 >= 4 ? 1 : 3
  return floor(hourAt(i - 1) / q) != floor(hr / q)
}

function base(i) {                            // ribbon colour of pixel i -> col, as HSV
  var j = reverse ? pixelCount - 1 - i : i
  var f = (j + 0.5) / pixelCount * M, k = clamp(floor(f), 0, M - 1), u = f - k
  col[0] = mix(SH[k], SH[k + 1], u); col[1] = mix(SS[k], SS[k + 1], u); col[2] = mix(SV[k], SV[k + 1], u)
  var hr = hourAt(i), ago = mod(now - hr, 24)
  if (ago < (centred ? 12 : now)) col[2] *= 0.42    // hours already gone
  if (notch(i, hr)) col[2] *= 0.45
}

function paintPixel(i) { base(i); hsv(col[0], col[1], col[2]); setPixel(i) }

// logical span [a, b) (pixel 0 = the ribbon's start) from sample position fa to fb
function span(a, b, fa, fb, dim) {
  var ka = clamp(floor(fa), 0, M - 1), ua = fa - ka, kb = clamp(ceil(fb) - 1, 0, M - 1), ub = fb - kb
  var h0 = mix(SH[ka], SH[ka + 1], ua), s0 = mix(SS[ka], SS[ka + 1], ua), v0 = mix(SV[ka], SV[ka + 1], ua) * dim
  var h1 = mix(SH[kb], SH[kb + 1], ub), s1 = mix(SS[kb], SS[kb + 1], ub), v1 = mix(SV[kb], SV[kb + 1], ub) * dim
  var n = pixelCount
  if (reverse) gradientRange(n - a, n - b, h0, s0, v0, h1, s1, v1, 1)
  else gradientRange(a, b, h0, s0, v0, h1, s1, v1, 1)
}

function repaint() {
  sampleSky()
  var n = pixelCount, w = n / M
  clear()
  // hours already gone are the first B pixels of the ribbon: split there
  var B = clamp(floor((centred ? 12 : mod(now - start, 24)) * n / 24 - 0.5) + 1, 0, n)
  for (var k = 0; k < M; k++) {
    var a = k * w, b = (k + 1) * w
    if (b <= B) span(a, b, k, k + 1, 0.42)
    else if (a >= B) span(a, b, k, k + 1, 1)
    else { span(a, B, k, B / w, 0.42); span(B, b, B / w, k + 1, 1) }
  }
  var q = n / 24 >= 4 ? 1 : 3                 // the hour notches, redrawn per pixel
  for (var hh = 0; hh < 24; hh += q) {
    var x = mod(hh - start, 24) / 24 * n, c = floor(reverse ? n - x : x)
    for (var i = max(c - 1, 0); i <= min(c + 1, n - 1); i++) if (notch(i, hourAt(i))) paintPixel(i)
  }
}

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
  if (dirty) { repaint(); dirty = 0 }
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
    hsv2rgb(col[0], col[1], col[2], col)
    rgb(col[0] + 0.55 * tw, col[1] + 0.55 * tw, col[2] + 0.6 * tw)
    setPixel(p)
  }

  // the marker: a soft (1 - d/4)^2 glow (two splats) and a bright core, added
  var pos = mod(now - start, 24) / 24 * n - 0.5
  if (reverse) pos = n - 1 - pos
  mark = pos
  var day = now > sunrise && now < sunset, g = 0.4 * (0.85 + 0.15 * wave(t / 2.2))
  if (day) rgb(g, g * 0.8, g * 0.3)
  else rgb(g * 0.55, g * 0.6, g)
  splatIndex(pos, 4, 1)
  splatIndex(pos, 2, 1)
  if (day) rgb(1, 0.85, 0.5)
  else rgb(0.7, 0.8, 1)
  splatIndex(pos, 1.2, 1)
}
