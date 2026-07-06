// Falling code: 16 virtual columns, each with its own speed and phase
// accumulated in beforeRender (frac() keeps them continuous forever).
// hash2 flickers the "glyphs" so cells shimmer as the trail passes.
cols = 16
phases = array(cols)
speeds = array(cols)

for (i = 0; i < cols; i++) {
  speeds[i] = 0.4 + hash(i * 13.7) * 0.9
  phases[i] = hash(i * 7.3)
}

glyphTick = 0

export function beforeRender(delta) {
  for (var i = 0; i < cols; i++) {
    phases[i] = frac(phases[i] + delta * 0.0004 * speeds[i])
  }
  glyphTick = (glyphTick + delta * 0.008) % 64  // ~8 glyph re-rolls per second
}

export function render2D(index, x, y) {
  col = floor(x * 15.999)
  behind = mod(phases[col] - y, 1)  // how far the head has passed this cell
  trail = saturate(1 - behind / 0.45)
  glyph = 0.4 + 0.6 * hash2(col * 16 + floor(y * 15.999), floor(glyphTick))
  head = saturate((0.06 - behind) * 25)  // ≈1 at the head, 0 in the trail
  v = trail * trail * glyph
  hsv(0.36, 1 - head * 0.8, max(v, head))
}
