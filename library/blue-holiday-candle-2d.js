// name: Blue Holiday Candle 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Blue Holiday Candle 2D"; original source never consulted.

// A stylized candle on a 2D matrix: blue-cored flame with an orange/yellow
// rim swaying gently above a blue candle body, over a dim deep-purple night
// in which three warm-white stars twinkle at random pixel locations.

var NUM_STARS = 3

var t = 0            // running seconds clock (wrapped hourly)
var sway = 0         // multi-octave side-to-side signal, ~ -1..1
var flickPhase = 0   // fast internal-flicker phase

var twIdx = array(NUM_STARS)    // target pixel index per twinkler
var twLife = array(NUM_STARS)   // 1 -> 0 pulse life
var twRate = array(NUM_STARS)   // per-twinkle decay speed (1/seconds)

// stagger the initial pulses so the stars don't all pop at once
for (var i = 0; i < NUM_STARS; i++) {
  twIdx[i] = floor(random(pixelCount))
  twLife[i] = random(1)
  twRate[i] = 0.5 + random(1)   // one pulse lasts ~0.7..2 s
}

export function beforeRender(delta) {
  var dt = delta / 1000
  t += dt
  if (t > 3600) t -= 3600   // avoid fixed-point precision loss

  // three octave-related sines: each next one twice as fast, half as strong
  sway = (sin(t * PI2 / 7) + 0.5 * sin(t * PI2 / 3.5) + 0.25 * sin(t * PI2 / 1.75)) / 1.75

  flickPhase = t * 3   // internal shimmer runs a few times real time

  for (var i = 0; i < NUM_STARS; i++) {
    twLife[i] -= dt * twRate[i]
    if (twLife[i] <= 0) {
      twIdx[i] = floor(random(pixelCount))
      twLife[i] = 1
      twRate[i] = 0.5 + random(1)
    }
  }
}

export function render2D(index, x, y) {
  // recenter, flip vertical so the candle sits at the bottom of the display
  var cx = x - 0.5
  var cy = 0.5 - y   // cy > 0 is up

  // candle body: lower band only, tapering toward the sides; feeds blue only
  var body = 0
  if (cy < -0.12) {
    var edge = 1 - abs(cx) / 0.3
    if (edge > 0) body = 0.7 * edge
  }

  // aspect correction: tall narrow flame instead of a round blob
  var fx = cx * 1.9

  // sway: displacement grows with height, phase rides the y coordinate
  var lift = max(0, cy + 0.3)
  fx += sin(cy * 5 + sway * 3) * sway * lift * 0.4

  // inner core: soft radial blob stretched upward
  var dy = cy > 0 ? cy * 0.6 : cy
  var core = clamp((0.22 - hypot(fx, dy)) / 0.12, 0, 1)

  // shimmering burn texture: triangle flicker mixing time + fine space
  var fl = triangle(frac(flickPhase + fx * 3.7 + cy * 2.3))
  core *= 0.55 + 0.45 * fl

  // outer shell: soft annulus around a circle of radius ~1/3
  var ring = abs(hypot(fx, cy) - 0.33)
  var shell = 1 - smoothstep(0.02, 0.09, ring)

  // compose: rim is red/orange, yellower toward the flame base; core is blue
  var r = 0.25 * core + 0.9 * shell
  var g = 0.35 * core + shell * 0.6 * clamp(0.5 - cy, 0, 1)
  var b = core + body

  if (r + g + b > 0.02) {
    rgb(clamp(r, 0, 1), clamp(g, 0, 1), clamp(b, 0, 1))
  } else {
    // starfield: only NUM_STARS twinklers, so a tiny scan per pixel is fine
    var star = 0
    for (var i = 0; i < NUM_STARS; i++) {
      if (twIdx[i] == index) star = sin(twLife[i] * PI)   // fade in, peak, out
    }
    if (star > 0) {
      rgb(0.6 * star, 0.52 * star, 0.42 * star)   // warm, faintly peach white
    } else {
      hsv(0.78, 1, 0.02)   // deep dim purple night, just above black
    }
  }
}
