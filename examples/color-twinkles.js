// Clean-room reimplementation from a prose description of the community
// pattern "color twinkles" (no source consulted). Stateless, random-free
// twinkles: two incommensurate spatial sines phase-modulate each other
// into a noise-like field; a fourth power plus a hard floor carves it
// into crisp sparks. The same field on a slower clock picks the hues.
export function beforeRender(delta) {
  tf = time(0.16) * PI2  // brightness clock, ~10 s
  ts = time(0.44) * PI2  // color clock, ~29 s
}

export function render(index) {
  b = (1 + sin(index * 0.31 + sin(index * 0.171 + tf) * PI2)) * 0.5
  b = b * b
  b = b * b  // ^4: crush the midtones, keep sparse peaks
  if (b < 0.22) b = 0  // hard floor: clean black between twinkles
  h = sin(index * 0.31 + sin(index * 0.171 + ts) * PI2)
  hsv(h, 1, b * 0.5)
}
