// Stable sparkle without a frame buffer: hash2(index, slot) re-rolls each
// pixel once per slot instead of every frame, and hash(index) staggers the
// slot boundaries so the sparks don't blink in unison.
tick = 0

export var density = 0.25
export function sliderDensity(v) { density = v } //# min=0 max=1 step=0.01 default=0.25

export function beforeRender(delta) {
  tick = (tick + delta * 0.003) % 64  // 3 sparkle slots per second
  hueBase = time(0.15)
}

export function render(index) {
  // per-pixel local clock: same speed, offset boundaries
  t = (tick + hash(index) * 64) % 64
  slot = floor(t)
  phase = t - slot
  spark = hash2(index, slot) < density * 0.4
  if (spark) {
    v = 1 - easeOutQuad(phase)  // pop on, fade over the slot
    hsv(hueBase, 0.25, v * v)   // near-white glint
  } else {
    hsv(hueBase + 0.5, 0.9, 0.05)  // dim complementary backdrop
  }
}
