// name: Spirograph
// tags: panel, ambient
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/lissajous-curve-tracer.md,
// swirlpool-2d.md — reimagined, not a replica. Weave mode: original work from the prose idea spec
// docs/pattern-specs/patternflow/lissajous-weave.md (Patternflow preset, CC BY-SA 4.0 — prose-described
// idea only, no code). Seven curve tracers on one pen engine: gears, Lissajous, rose, a damped
// harmonograph that restarts with fresh ratios, a depth-shaded weave of phase-shifted threads,
// epicycle arms, and a comet swarm on a tumbling Lissajous. renderBulk: the frame is the paper —
// a delta-timed fade() plus one drawLine per pen sub-step at native resolution, OKLCH hue along
// the path. Wander steps the gear ratio through classic pairs (eased); off holds Ratio A/B.

const MAXP = 16
var mode = 0, ratA = 5, ratB = 3, phaseDeg = 90, trailS = 8, circuitS = 1.5
var threads = 8, arms = 5, wander = 1
var curA = 5, curB = 3, th = 0, hueT = 0, fadeAcc = 0, fadeK = 1, wanderT = 0, wi = 0
var stepTh = 0, rot = 0, pen = 0.8, lw = 0.04, oldMode = -1, gw = 32
var px = array(MAXP), py = array(MAXP), has = array(MAXP)
var wA = [5, 7, 3, 8, 5, 7, 4, 6], wB = [3, 4, 2, 3, 2, 5, 3, 5]
var hf = array(4), hp = array(4), hT = 0, hDecay = 0.025, pathHue = 0

//# min=0 max=6 step=1 default=0
export function sliderMode(v) { mode = clamp(floor(v), 0, 6) }
//# min=1 max=8 step=1 default=5
export function sliderRatioA(v) { ratA = clamp(floor(v), 1, 8) }
//# min=1 max=8 step=1 default=3
export function sliderRatioB(v) { ratB = clamp(floor(v), 1, 8) }
//# min=0 max=180 step=5 default=90
export function sliderPhaseDeg(v) { phaseDeg = clamp(v, 0, 180) }
//# min=0.5 max=30 step=0.5 default=8
export function sliderTrailSeconds(v) { trailS = clamp(v, 0.5, 30) }
//# min=0.5 max=20 step=0.5 default=1.5
export function sliderCircuitSeconds(v) { circuitS = clamp(v, 0.5, 20) }
//# min=1 max=12 step=1 default=8
export function sliderThreads(v) { threads = clamp(floor(v), 1, 12) }
//# min=2 max=8 step=1 default=5
export function sliderArms(v) { arms = clamp(floor(v), 2, 8) }
//# min=0 max=1 step=1 default=1
export function toggleWander(on) { wander = on > 0.5 ? 1 : 0 }

function newHarmonograph() {
  var p = floor(random(4)) + 1
  var q = p + floor(random(3)) + 1
  if (random(1) < 0.5) { var s = p; p = q; q = s }
  hf[0] = p; hf[1] = q + random(0.04) - 0.02
  hf[2] = q; hf[3] = p + random(0.04) - 0.02
  for (var i = 0; i < 4; i++) hp[i] = random(PI2)
  hT = 0
  has[0] = 0
}
newHarmonograph()

export function beforeRender(delta) {
  var dt = min(delta, 100) * 0.001
  stepTh = dt * PI2 / circuitS
  hueT = mod(hueT + dt * 0.02, 1)
  rot = mod(rot + dt * PI2 / 90, PI2)
  pen = 0.75 + 0.2 * sin(rot * 3)
  if (wander) {
    wanderT += dt
    if (wanderT > 6 * circuitS + 6) { wanderT = 0; wi = (wi + 1) % 8 }
    ratA = wA[wi]; ratB = wB[wi]
  }
  var e = 1 - exp(-dt / 0.8)            // eased morph between integer ratios
  curA += (ratA - curA) * e
  curB += (ratB - curB) * e
  // Trail: fade in coarse delta-timed steps so 8-bit flooring never cuts it short.
  var tr = mode == 6 ? min(trailS, 0.7) : (mode == 4 ? min(trailS, circuitS * 0.6) : trailS)
  var tau = tr * 434                     // ms; 10% left after the trail time
  fadeAcc += delta
  fadeK = 1
  if (fadeAcc >= tau / 40) { fadeK = exp(-fadeAcc / tau); fadeAcc = 0 }
}

// Pen position for pen j at parameter t → (ox, oy) in 0..1.
var ox, oy, oz
function curve(j, t) {
  var a = curA, b = curB, x = 0, y = 0, ph = phaseDeg * PI / 180
  oz = 1
  if (mode == 0) {                       // gears: B-tooth wheel inside/outside an A-tooth ring
    var rho = b / a
    var k, a1, a2
    if (b < a) { k = 1 - a / b; a1 = 1 - rho; a2 = pen * rho }
    else { k = 1 + a / b; a1 = 1 + rho; a2 = pen * rho }
    var n = a1 + a2
    x = (a1 * cos(t) + a2 * cos(k * t)) / n
    y = (a1 * sin(t) + a2 * sin(k * t)) / n
  } else if (mode == 1) {                // Lissajous, precessing slowly
    x = sin(a * t + ph + rot * 2); y = sin(b * t)
  } else if (mode == 2) {                // rose r = cos(A/B · t)
    var r = cos(a / b * t)
    x = r * cos(t); y = r * sin(t)
  } else if (mode == 3) {                // harmonograph: two damped pendulums per axis
    var env = exp(-hDecay * t)
    x = env * (sin(hf[0] * t + hp[0]) + sin(hf[1] * t + hp[1])) * 0.5
    y = env * (sin(hf[2] * t + hp[2]) + sin(hf[3] * t + hp[3])) * 0.5
    oz = env
  } else if (mode == 4) {                // weave: thread j phase-shifted, depth-shaded
    var d = j * PI / threads + ph + rot * 2
    x = sin(a * t + d); y = sin(b * t + d * 0.5)
    oz = 0.55 + 0.45 * cos(a * t + d)
  } else if (mode == 5) {                // epicycles: arm j orbits its own breathing centre
    var al = j * PI2 / arms + t * 0.25
    var rc = 0.32 + 0.16 * sin(rot * 5)
    x = rc * cos(al) + 0.45 * cos(t * a / b + j * PI2 / arms)
    y = rc * sin(al) + 0.45 * sin(t * a / b + j * PI2 / arms)
  } else {                               // swarm: dots on a tumbling Lissajous
    var off = j * 0.16 + hash(j) * 0.3
    var t2 = (1 + sin(t * 0.25)) * 1.6
    x = sin(t * 0.5 + off); y = sin(t2 * a / b + off)
  }
  var rr = mode == 0 || mode == 2 ? rot : 0
  var c = cos(rr), s = sin(rr)
  ox = 0.5 + 0.46 * (x * c - y * s)
  oy = 0.5 + 0.46 * (x * s + y * c)
}

export function renderBulk() {
  if (mode != oldMode) { clear(); oldMode = mode; for (var i = 0; i < MAXP; i++) has[i] = 0; th = 0 }
  if (fadeK < 1) fade(fadeK)
  gw = gridWidth() > 0 ? gridWidth() : 32
  lw = 1.8 / gw
  var pens = mode == 4 ? threads : (mode == 5 ? arms : (mode == 6 ? 16 : 1))
  var rate = mode == 0 ? 1 + abs(curA / curB) : max(curA, curB) / (mode == 2 ? curB : 1) + 1
  if (mode == 3) { rate = 6; hT += stepTh; if (exp(-hDecay * hT) < 0.15) newHarmonograph() }
  var steps = clamp(ceil(stepTh * rate * gw / 6), 1, max(1, floor(48 / pens)))
  var hs = stepTh / steps
  for (var s = 1; s <= steps; s++) {
    var t = th + hs * s
    var tt = mode == 3 ? hT - stepTh + hs * s : t
    for (var j = 0; j < pens; j++) {
      curve(j, tt)
      var h = mode == 4 || mode == 5 ? j / pens + hueT : pathHue + hueT
      if (mode == 6) h = hueT + j * 0.04
      oklch(0.47 + 0.38 * oz, 0.16, h)
      if (has[j] && abs(ox - px[j]) + abs(oy - py[j]) < 0.3) drawLine(px[j], py[j], ox, oy, lw, 2)
      if (mode == 6 && s == steps) splat(ox, oy, lw * 1.6, 2)
      px[j] = ox; py[j] = oy; has[j] = 1
    }
    pathHue = mod(pathHue + hs * 0.03, 1)
  }
  th += stepTh
  // Wrap the parameter only on a closed figure (integer B: every term is periodic in 8πB).
  var nb = round(curB)
  if (abs(curB - nb) < 0.001 && th > PI2 * nb * 4) th -= PI2 * nb * 4
}
