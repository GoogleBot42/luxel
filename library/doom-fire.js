// name: Doom Fire
// Clean-room reimplementation from a prose functional description of the
// community pattern "Doom Fire"; original source never consulted.
//
// PSX-DOOM-style fire: no noise functions, just neighbor propagation with
// random cooling. Simulated on a 16x16 virtual canvas; the heat buffers get
// one pad column on each side (no horizontal clip checks) and one extra
// row beyond the display that permanently holds the source heat.

var SIZE = 16              // virtual canvas is SIZE x SIZE
var W = SIZE + 2           // buffer width: display + a pad column each side
var H = SIZE + 1           // buffer height: display rows + the source row
var SRC = SIZE             // buffer row index of the source row

var bufA = array(W * H)
var bufB = array(W * H)
var cur = bufA
var prev = bufB

// light the source row at load
var i0
for (i0 = 0; i0 < W; i0++) {
  bufA[SRC * W + i0] = 1
  bufB[SRC * W + i0] = 1
}

// --- controls ---------------------------------------------------------
var baseHue = 0            // red fire by default
var bright = 1
export function hsvPickerColor(h, s, v) {
  // picker saturation is ignored: hue re-bases the fire, value scales it
  baseHue = h
  bright = v
}

var maxCool = 0.25
//# min=0 max=1 step=0.05 default=0.5
export function sliderFlameHeight(v) {
  // inverse: taller flames = gentler maximum cooling
  maxCool = 0.4 - 0.3 * v
}

var dragon = 0
//# min=0 max=1 step=1 default=0
export function sliderDragonMode(v) {
  dragon = v > 0.5
}

var minStepMs = 30
//# min=0 max=1 step=0.05 default=0.1
export function sliderSpeed(v) {
  // minimum ms between simulation steps: 0 = as fast as possible,
  // 1 = a chunky retro ~300 ms per step
  minStepMs = v * 300
}

// --- simulation -------------------------------------------------------
var wind = 0
var accum = 0

function simStep() {
  // swap buffers
  var tmp = prev
  prev = cur
  cur = tmp

  // wind: mostly calm; occasionally either calm down or gust sideways.
  // Every change passes through a zero-wind interlude before a new
  // random direction is picked.
  if (random(1) < 0.05) {
    if (wind != 0) {
      wind = 0
    } else {
      wind = random(2.4) - 1.2   // roughly one column-width either way
    }
  }

  // propagate heat upward. Row 0 is never written, so it stays dark;
  // each cell pulls from the cell below it in the previous buffer.
  var center = (SIZE - 1) / 2
  var yy, xx
  for (yy = 1; yy < SRC; yy++) {
    // cooling is harshest near the source and gentle near the top, so
    // surviving parcels get carried high as distinct tongues
    var coolScale = maxCool * yy / SRC
    for (xx = 1; xx <= SIZE; xx++) {
      // wind bends the flanks more than the core: edge-weighted offset
      var lean = wind * (xx - 1 - center) / center
      var sx = clamp(round(xx + lean), 0, W - 1)
      var heat = prev[(yy + 1) * W + sx] - random(coolScale)
      cur[yy * W + xx] = max(0, heat)
    }
  }

  // re-perturb the source row
  if (dragon) {
    // periodic die-back and eruption, several seconds per exhale
    var breath = 1.3 * wave(time(0.09))
    for (xx = 1; xx <= SIZE; xx++) {
      cur[SRC * W + xx] = breath * (0.75 + 0.25 * wave((xx - 1) / SIZE))
    }
  } else {
    // high base heat with a slowly sliding spatial shimmer
    var phase = triangle(time(0.3)) * 2
    for (xx = 1; xx <= SIZE; xx++) {
      cur[SRC * W + xx] = 0.88 + 0.12 * wave((xx - 1) / SIZE + phase)
    }
  }
}

export function beforeRender(delta) {
  // simulation decoupled from rendering: slow steps look like chunky
  // retro fire, not a slow frame rate
  accum += delta
  if (accum >= minStepMs) {
    accum = 0
    simStep()
  }
}

export function render2D(index, x, y) {
  var cell = floor(y * 15.99) * W + floor(x * 15.99) + 1  // +1 skips the pad
  var heat = cur[cell]
  var v = heat * heat * heat   // cubed: only genuinely hot cells glow

  // PSX-DOOM fire ramp: black -> dark red -> red -> orange -> yellow ->
  // white-hot. Hue and saturation track the RAW heat, not the cubed value:
  // driving them off the cubed value kept the whole flame pinned at red
  // while the saturation fell away, which painted it a washed-out salmon
  // instead of fire. Everything below a quarter heat stays pure red (Doom's
  // palette spends its bottom third on dark reds); the top of the ramp runs
  // out to yellow and then bleaches to white only in the hottest cells.
  var h = clamp((heat - 0.25) / 0.75, 0, 1)
  h = h * h * h                // cubed: the ramp lingers in the reds and only
                               // runs out to yellow in the hottest tenth
  hsv(baseHue + 0.16 * h,
      1 - 0.85 * clamp((heat - 0.92) / 0.08, 0, 1),
      v * bright)
}
