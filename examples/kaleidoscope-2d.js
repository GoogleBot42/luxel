// Clean-room reimplementation from a prose description of the community
// pattern "Perlin Kaleidoscope 2D" (no source consulted). Three
// noise-displaced lines — one per additive primary — get folded into
// mirrored pie wedges (the abs() against the wedge bisector is what makes
// true mirrors instead of rotated copies) and the whole thing spins.
zt = 0
rot = 0

slices = 5
export function sliderSlices(v) { slices = 1 + floor(v * 6) } //# min=1 max=7 step=1 default=5
spd = 0.5
export function sliderSpeed(v) { spd = 0.1 + v * v * 2 } //# min=0 max=1 step=0.01 default=0.5
lineW = 0.14
export function sliderLineWidth(v) { lineW = 0.05 + v * 0.3 } //# min=0 max=1 step=0.01 default=0.3

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  zt += dt * 0.1 * spd
  if (zt > 512) zt -= 512
  rot = mod(rot + dt * 0.4 * spd, PI2)
}

export function render2D(index, x, y) {
  dx = x - 0.5
  dy = y - 0.5
  if (slices > 1) {
    r = hypot(dx, dy)
    seg = PI2 / slices
    a = abs(mod(atan2(dy, dx), seg) - seg / 2) + rot  // mirror fold + spin
    dx = cos(a) * r
    dy = sin(a) * r
  }
  u = dx + 0.5
  w = dy + 0.5
  // three independently wandering lines, one per channel
  ry = 0.5 + simplex3(u * 1.2, zt, 0, 31) * 0.35
  gy = 0.5 + simplex3(zt, u * 1.7, 1, 32) * 0.35
  by = 0.5 + simplex3(u * 0.9, 3, zt * 1.4, 33) * 0.3
  rgb(
    saturate((lineW - abs(w - ry)) / lineW * 1.4),
    saturate((lineW - abs(w - gy)) / lineW * 1.4),
    saturate((lineW - abs(w - by)) / lineW * 1.4)
  )
}
