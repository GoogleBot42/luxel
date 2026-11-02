// name: Starburst
// tags: strip, ambient, sound
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/sound-starburst-2.md, white-rainbows.md — reimagined, not a replica.
// Comet heads sweep both ways along the strip and pass through each other:
// a white-hot head, a short coloured tail (gradientRange) and a trail left by
// fading the frame, so no per-pixel buffers. Rainbow paints the trail by
// position. With Sound on, rising energy flares the heads and lets trails
// linger while falling energy snuffs them; the demo groove drives it when no
// sensor is attached.

var sound = 0
//# default=0
export function toggleSound(v) { sound = v }

var leaders = 2
//# min=1 max=8 step=1 default=2
export function sliderLeaders(v) { leaders = clamp(floor(v), 1, 8) }

var rainbow = 1
//# default=1
export function toggleRainbow(v) { rainbow = v }

var repeats = 2
//# min=1 max=6 step=1 default=2
export function sliderRainbowRepeats(v) { repeats = clamp(floor(v), 1, 6) }

var sweepS = 5
//# min=1 max=20 step=0.5 default=5
export function sliderSweepSeconds(v) { sweepS = clamp(v, 1, 20) }

// trail half-life while the heads glide (Sound: while energy rises)
var trailMs = 350
//# min=50 max=3000 step=50 default=350
export function sliderTrailMs(v) { trailMs = clamp(v, 50, 3000) }

// ---- sound front-end (the same block in every library sound pattern) ----
// spec[0..31]: the bins, treble-tilted and auto-gained so recent PEAKS read 1.
// sndLevel: loudness 0..1 against its own peak; sndBeat: 1 on a bass-onset
// frame; sndBpm: tempo estimate. No sensor, or 1.5 s of silence, crossfades
// to a built-in 120 BPM demo groove, so the pattern never sits dark.
export var frequencyData = array(32)
var spec = array(32)
var sndPeak = 0.3, sndLvlPk = 1, sndLevel = 0, sndBeat = 0, sndBpm = 120
var sndAvg = 0, sndPrev = 0, sndSince = 0, sndQuiet = 9999, sndDemo = 1, sndClock = 0
function soundFrame(delta) {
  var i, v, raw = 0, pk = 0, sum = 0
  for (i = 0; i < 32; i++) raw = max(raw, frequencyData[i])
  sndQuiet = raw > 0.01 ? 0 : min(sndQuiet + delta, 9999)
  sndDemo = clamp(sndDemo + (sndQuiet > 1500 ? delta / 1000 : -delta / 150), 0, 1)
  sndClock = mod(sndClock + delta / 500, 64)
  var ph = frac(sndClock), kick = pow(1 - ph, 4), hat = pow(1 - frac(sndClock * 2), 6)
  var snare = floor(sndClock) % 2 ? pow(1 - ph, 2) * 0.6 : 0
  var lead = 14 + floor(8 * wave(sndClock / 16))
  for (i = 0; i < 32; i++) {
    v = 0.06 * wave(i * 0.13 - sndClock * 0.02)
    v += i < 6 ? kick * (1 - i / 7) : (i < 18 ? snare * wave(i * 0.37) : hat * 0.35 * wave(i * 0.29))
    if (i == lead || i == lead + 5) v += 0.4 - ph * 0.2
    v = mix(frequencyData[i], v * 0.5, sndDemo) * (1 + i / 16)
    spec[i] = v
    pk = max(pk, v)
    sum += v
  }
  sndPeak = max(pk, max(0.04, sndPeak * pow(0.5, delta / 2500)))
  sndLvlPk = max(sum, max(0.3, sndLvlPk * pow(0.5, delta / 4000)))
  sndLevel = sum / sndLvlPk
  arrayScale(spec, 1 / sndPeak)
  var bass = (spec[0] + spec[1] + spec[2] + spec[3]) / 4
  sndAvg += (bass - sndAvg) * min(1, delta / 300)
  sndSince += delta
  sndBeat = 0
  if (bass > sndAvg * 1.25 + 0.1 && bass > sndPrev && sndSince > 250) {
    v = sndSince
    while (v > 860) v /= 2
    if (sndSince < 2000) sndBpm = mix(sndBpm, 60 / (max(v, 330) / 1000), 0.3)
    sndSince = 0
    sndBeat = 1
  }
  sndPrev = bass
}
// ---- end sound front-end ----

var t = 0, drift = 0, fast = 0, slow = 0, rising = 1, flare = 1, keep = 0.9
var prevF = array(8), prevR = array(8)

export function beforeRender(delta) {
  t = mod(t + delta / (sweepS * 1000), 1)
  drift = mod(drift + delta / 24000, 1)
  keep = pow(0.5, delta / trailMs)
  if (sound) {
    soundFrame(delta)
    fast += (sndLevel - fast) * min(1, delta / 60)
    slow += (sndLevel - slow) * min(1, delta / 700)
    rising = fast > slow * 1.04
    if (rising) flare = max(flare, min(1, 0.25 + fast))
    else {
      flare = max(0.2, flare * pow(0.5, delta / 300))
      keep = pow(0.5, delta / (trailMs * 0.15))
    }
  } else { rising = 1; flare = 1 }
}

function comet(p, prev, dir, b) {
  var n = pixelCount, h = drift + (rainbow ? p / n * repeats : 0)
  // repaint the stretch swept since last frame (old white head included) in
  // full colour, so fast heads leave no gaps and the trail re-saturates
  if (abs(p - prev) < n / 4) gradientRange(dir > 0 ? floor(prev - 1.5) : ceil(prev + 1.5), p, h, 1, b * keep, h, 1, b, 0)
  gradientRange(p - dir * 3, p, h, 1, b * 0.6, h, 0.5, b, 2)
  hsv(h, rising ? 0.1 : 0.35, b)
  splatIndex(p, 1.3, 2)
}

export function renderBulk() {
  fade(keep)
  var n = pixelCount
  for (var i = 0; i < leaders; i++) {
    var off = i / leaders
    var pf = frac(t + off) * n, pr = frac(1 - t + off) * n
    comet(pf, prevF[i], 1, flare)
    comet(pr, prevR[i], -1, flare)
    prevF[i] = pf; prevR[i] = pr
  }
}
