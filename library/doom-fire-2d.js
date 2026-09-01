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
  // Wind is a SMOOTH sway (~10 s out and back) whose AMPLITUDE is the slider.
  // The old code re-rolled a full-strength +-1 cell shift no matter where the
  // slider sat and applied it to 70% of cells per row, which sheared the whole
  // column ~11 cells across the panel and pushed it into the dead guard
  // columns — the flame ended up as a diagonal smear with a black wedge beside
  // it. Now the per-row nudge probability is proportional to the wind and
  // tapers to nothing at the source, so the total lean is at most ~4 cells
  // over the full height and the base stays put.
  wind = windAmt * (wave(time(0.15)) * 2 - 1)
  windDir = wind < 0 ? -1 : 1
  windP = abs(wind) * 0.5
  // Cooling is sized so the column burns out after `reach` rows: FlameHeight
  // buys height directly instead of only thinning an ever-full panel, which
  // is what made the port a uniform orange wash from floor to ceiling.
  reach = 0.5 + rows * flame * flame * flame
  coolMax = 1.9 / reach
  for (var y = 0; y < rows; y++) {
    lift = 1 - (y + 0.5) / rows          // 0 at the source, 1 at the ceiling
    cool = coolMax * (0.35 + 0.65 * (y + 1) / rows)  // harsh low, gentle high
    for (var x = 1; x <= gw; x++) {
      var sx = x
      if (random(1) < windP * lift) sx = x + windDir
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
    lean = wind * 0.5             // the bed burns hotter downwind
    for (var x = 1; x <= gw; x++) {
      A[base + x] = (0.82 + 0.18 * wave(x * 0.13 + ph)) *
                    (1 + lean * ((x - 0.5) / gw * 2 - 1))
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
