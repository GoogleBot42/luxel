// name: Glittering Jewels
// Curated example (hand-written showcase of the Luxel language/builtins).
// Clean-room reimplementation from a prose description of the community
// pattern "Glittering Jewels" (no source consulted). Soft gems bloom at
// random spots — the glow radius breathes with the bloom, a hot core sits
// in a wider half-strength halo, and a slow staggered clock fires narrow
// "glints" confined to each core. Overlaps composite winner-take-all so
// jewels stay crisp. The invisible tail of each lifetime doubles as a
// randomized respawn delay.
maxJewels = 20
jpos = array(maxJewels)
jhue = array(maxJewels)
jphase = array(maxJewels)
jrate = array(maxJewels)
jwid = array(maxJewels)
jvis = array(maxJewels)
jspark = array(maxJewels)

export var speed = 0.35
export function sliderSpeed(v) { speed = 0.05 + v * v * 1.5 } //# min=0 max=1 step=0.01 default=0.45
export var width = 0.09
export function sliderWidth(v) { width = 0.02 + v * 0.2 } //# min=0 max=1 step=0.01 default=0.35
sparkle = 0.6
export function sliderSparkle(v) { sparkle = v } //# min=0 max=1 step=0.01 default=0.6
count = 6
export function sliderJewels(v) { count = 1 + floor(v * (maxJewels - 1)) } //# min=1 max=20 step=1 default=6
rainbow = 0
export function toggleRainbow(v) { rainbow = v }
pickH = 0.78
export function hsvPickerColor(h, s, v) { pickH = h }

function respawn(i) {
  jpos[i] = random(1)
  jhue[i] = random(1)
  jrate[i] = 0.7 + random(0.7)
  jwid[i] = 0.7 + random(0.6)  // per-jewel width factor
  jvis[i] = 0.65 + random(0.2) // visible fraction of the lifetime
  jspark[i] = random(1)
}
for (i = 0; i < maxJewels; i++) {
  respawn(i)
  jphase[i] = random(1)  // stagger so they never pulse in unison
}

export function beforeRender(delta) {
  dt = delta * 0.001
  for (var i = 0; i < count; i++) {
    jphase[i] += dt * speed * jrate[i]
    if (jphase[i] >= 1) {
      jphase[i] -= 1
      respawn(i)
    }
  }
  glintT = time(0.1 + sparkle * 0.3)  // stronger sparkle = slower cycle
}

export function render(index) {
  x = index / (pixelCount - 1)
  bv = 0
  bh = 0
  for (var i = 0; i < count; i++) {
    p = jphase[i] / jvis[i]
    if (p < 1) {
      bloom = sin(p * PI)
      r = width * jwid[i] * (0.33 + 0.67 * bloom)
      d = abs(x - jpos[i])
      core = saturate(1 - (d * d) / (r * r))
      rh = r * 1.3
      g = max(core, saturate(1 - (d * d) / (rh * rh)) * 0.5) * bloom
      if (g > 0.01) {
        sp = 0
        if (sparkle > 0) {
          sp = pow(max(sin((glintT + jspark[i]) * PI2), 0), 24)
          sp *= core * core * core * sparkle * 2
        }
        b = g * (1 + sp)
        if (b > bv) {
          bv = b
          bh = (rainbow ? jhue[i] : pickH) + sp * 0.04
        }
      }
    }
  }
  hsv(bh, 0.85, min(bv, 1))
}
