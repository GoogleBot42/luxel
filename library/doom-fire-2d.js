// name: Doom Fire 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// Clean-room reimplementation from a prose description of the community
// pattern "Doom Fire (v2.0) 2D" (no source consulted) — itself the PSX
// Doom fire: pure cellular propagation, no noise fields. Heat rises from
// a hidden source row, cooled by a random draw that's harshest near the
// bottom (survivors get carried high). Guard columns skip bounds checks;
// the sim is double-buffered and runs on its own clock, decoupled from
// the render rate. Dragon mode makes the whole bed exhale in surges.
gw = 16
rows = 16
sw = gw + 2           // guard column each side
cells = sw * (rows + 1)  // +1 hidden source row at the bottom
A = array(cells)
B = array(cells)
wind = 0
stepT = 0

baseH = 0.01
bright = 1
export function hsvPickerColor(h, s, v) { baseH = h; bright = v }
flame = 0.75
export function sliderFlameHeight(v) { flame = v } //# min=0 max=1 step=0.01 default=0.75
windAmt = 0.3
export function sliderWind(v) { windAmt = v } //# min=0 max=1 step=0.01 default=0.3
stepMs = 40
export function sliderSpeed(v) { stepMs = 15 + (1 - v) * 120 } //# min=0 max=1 step=0.01 default=0.8
dragon = 0
export function toggleDragonBreath(v) { dragon = v }

function step() {
  tmp = A
  A = B
  B = tmp
  if (random(1) < windAmt * windAmt) wind = floor(random(3)) - 1
  coolMax = 0.035 + (1 - flame) * (1 - flame) * 0.45
  for (var y = 0; y < rows; y++) {
    bend = 1 - abs(y / rows - 0.5) * 2  // wind bites hardest mid-flame
    cool = coolMax * (0.35 + 0.65 * (y + 1) / rows)  // harsh low, gentle high
    for (var x = 1; x <= gw; x++) {
      var sx = x
      if (wind != 0 && random(1) < bend * 0.7) sx = x + wind
      v = B[(y + 1) * sw + sx] - random(cool)
      A[y * sw + x] = max(v, 0)
    }
  }
  // refresh the hidden source row
  base = (rows) * sw
  if (dragon) {
    pulse = pow(wave(time(0.06)), 2)  // the exhale, ~4 s cycle
    for (var x = 1; x <= gw; x++) {
      A[base + x] = pulse * (0.65 + 0.35 * wave(x / gw * 2))
    }
  } else {
    ph = triangle(time(0.3)) * 3  // bed shimmer drifts over ~20 s
    for (var x = 1; x <= gw; x++) {
      A[base + x] = 0.82 + 0.18 * wave(x * 0.13 + ph)
    }
  }
}

export function beforeRender(delta) {
  stepT += min(delta, 100)
  while (stepT >= stepMs) {
    stepT -= stepMs
    step()
  }
}

export function render2D(index, x, y) {
  heat = A[floor(y * 15.99) * sw + 1 + floor(x * 15.99)]
  v = heat * heat * heat * bright
  hsv(baseH + heat * 0.06, 1 - saturate(heat - 0.75) * 1.2, min(v, 1))
}
