// name: Typing Heatmap 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// QMK's typing-heatmap idea: keystrokes deposit heat, heat diffuses to
// its neighbors and cools, and a thermal palette maps it black → blue →
// red → white. REAL keystrokes arrive as injected events (click/drag the
// preview, or POST /api/events); a phantom typist fills the idle time
// and pauses whenever real input is flowing. The trigger hammers a few
// keys at once. Diffusion is blur2D + arrayMix; rendering samples
// bilinearly via canvasGet.
gw = 16
heat = array(gw * gw)
scratch = array(gw * gw)
acc = 0
burst = 0
ev = array(4)
quiet = 0  // seconds of phantom silence left after real input

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
  // real input: each event heats the cell under its normalized (x, y)
  while (readEvent(ev)) {
    kx = clamp(floor(ev[1] * gw), 0, gw - 1)
    ky = clamp(floor(ev[2] * gw), 0, gw - 1)
    heat[ky * gw + kx] = min(heat[ky * gw + kx] + 0.85, 1.3)
    quiet = 4
  }
  quiet -= dt
  // phantom typist: ~8 keys/s, clustered toward the middle rows —
  // silenced while real events are flowing
  acc = quiet > 0 ? 0 : acc + dt * 10
  acc += burst
  burst = 0
  while (acc >= 1) {
    acc -= 1
    kx = floor(random(gw))
    ky = floor(clamp(8 + (random(1) - 0.5) * 9, 0, gw - 1))
    heat[ky * gw + kx] = min(heat[ky * gw + kx] + 0.85, 1.3)
  }
  // diffuse: blend the field toward its 3×3 box blur, then cool
  k = min(dt * 4.5, 0.22)
  cool = 1 - min(dt * 0.35, 0.9)
  arrayMix(scratch, heat, 1)     // scratch = heat
  blur2D(scratch, gw, gw, 1)
  arrayMix(heat, scratch, k)     // heat += (blurred − heat)·k
  arrayScale(heat, cool)
}

export function render2D(index, x, y) {
  paint(min(canvasGet(heat, gw, x, y), 1), 1)
}
