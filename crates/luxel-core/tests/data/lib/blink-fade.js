// name: Blink Fade
// Curated example (hand-written showcase of the Luxel language/builtins).
// Frame-buffer idiom: random pixels flare up and decay each frame.
values = array(pixelCount)
hues = array(pixelCount)
fade = 0.02

hueClock = .05   // time() period argument; .05 -> ~3.28 s per hue revolution
hueSpread = .2   // per-blink hue jitter, as a fraction of the colour wheel
sat = 1          // saturation of every blink

// Fade speed: how fast a lit pixel decays back to black and re-fires.
export var speed = 0.5
//# min=0 max=4 step=0.01 default=0.5
export function sliderSpeed(v) { speed = v }

// Seconds for the base hue to walk once around the colour wheel.
//# min=0.5 max=60 step=0.01 default=3.28
export function sliderHueDriftSeconds(v) { hueClock = max(v, 0.1) / 65.536 }

// How far apart neighbouring blinks land on the wheel (0 = one flat colour).
//# min=0 max=360 step=1 default=72
export function sliderHueSpreadDegrees(v) { hueSpread = clamp(v, 0, 360) / 360 }

// Pull saturation down for pastel / white twinkle.
//# min=0 max=100 step=1 default=100
export function sliderSaturationPercent(v) { sat = clamp(v, 0, 100) / 100 }

export function beforeRender(delta) {
  t1 = time(hueClock)
  for (var i = 0; i < pixelCount; i++) {
    values[i] -= fade * delta * (0.05 + speed)
    if (values[i] <= 0) {
      values[i] = random(1)
      hues[i] = t1 + random(hueSpread)
    }
  }
}

export function render(index) {
  v = values[index]
  hsv(hues[index], sat, v * v)
}
