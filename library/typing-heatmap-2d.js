// name: Typing Heatmap 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// QMK's typing-heatmap idea: phantom keystrokes deposit heat, heat
// diffuses to its four neighbors and cools, and a thermal palette maps
// it black → blue → red → white. The trigger hammers a few keys at once.
// Diffusion is the hand-rolled 2D blur — see docs/ideas.md for blur2D.
gw = 16
heat = array(gw * gw)
scratch = array(gw * gw)
acc = 0
burst = 0

setPalette([
  0.0,  0,    0,    0,
  0.2,  0,    0.05, 0.3,
  0.45, 0.25, 0,    0.5,
  0.7,  0.9,  0.1,  0,
  0.88, 1,    0.55, 0,
  1.0,  1,    1,    0.9
])

export function triggerKeys() { burst = 6 }

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  // phantom typist: ~8 keys/s, clustered toward the middle rows
  acc += dt * 10 + burst
  burst = 0
  while (acc >= 1) {
    acc -= 1
    kx = floor(random(gw))
    ky = floor(clamp(8 + (random(1) - 0.5) * 9, 0, gw - 1))
    heat[ky * gw + kx] = min(heat[ky * gw + kx] + 0.85, 1.3)
  }
  // diffuse to 4-neighbors (edges clamp), then cool
  k = min(dt * 3.5, 0.18)
  cool = 1 - min(dt * 0.35, 0.9)
  last = gw * gw - 1
  for (var i = 0; i <= last; i++) {
    x = i % gw
    up = i >= gw ? heat[i - gw] : heat[i]
    dn = i <= last - gw ? heat[i + gw] : heat[i]
    lf = x > 0 ? heat[i - 1] : heat[i]
    rt = x < gw - 1 ? heat[i + 1] : heat[i]
    scratch[i] = (heat[i] * (1 - k) + k * (up + dn + lf + rt) / 4) * cool
  }
  tmp = heat
  heat = scratch
  scratch = tmp
}

export function render2D(index, x, y) {
  h = heat[floor(y * 15.99) * gw + floor(x * 15.99)]
  paint(min(h, 1), 1)
}
