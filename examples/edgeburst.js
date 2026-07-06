// Clean-room reimplementation from a prose description of the community
// pattern "Edgeburst" (no source consulted). A rainbow front bursts from
// the center, sweeps to the ends, and collapses back. One clamped blend
// of a spatial tent and a time bounce yields position, brightness, and
// hue all at once.
export function beforeRender(delta) {
  t1 = triangle(time(0.09))  // bounce, ~6 s round trip
}

export function render(index) {
  tent = triangle(index / pixelCount)
  e = clamp(tent + t1 * 4 - 2, 0, 1)
  hsv(e * e - 0.2, 1, triangle(e))
}
