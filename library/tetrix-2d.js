// name: Tetrix 2D
// Curated example (hand-written showcase of the Luxel language/builtins).
// Stacker (WLED's "Tetrix", reimagined per-column): every column drops
// blocks onto its pile at its own pace; a full column flashes and dumps.
// All state — no canvas — render2D reads the arrays directly.
gw = 16
rows = 16
stack = array(gw)  // settled height per column
fy = array(gw)     // falling block's bottom edge, in rows from the top
fh = array(gw)     // falling block height
fhue = array(gw)
flash = array(gw)  // > 0 while a full column celebrates + clears

function startBlock(c) {
  fh[c] = 1 + floor(random(3))
  fy[c] = 0
  fhue[c] = random(1)
}
for (i = 0; i < gw; i++) startBlock(i)

export function beforeRender(delta) {
  dt = min(delta, 50) * 0.001
  for (var c = 0; c < gw; c++) {
    if (flash[c] > 0) {
      flash[c] -= dt * 1.5
      if (flash[c] <= 0) {
        stack[c] = 0
        startBlock(c)
      }
    } else {
      fy[c] += dt * (4 + hash(c * 5.1) * 5)
      if (fy[c] >= rows - stack[c]) {  // landed
        stack[c] += fh[c]
        if (stack[c] >= rows) flash[c] = 1
        else startBlock(c)
      }
    }
  }
}

export function render2D(index, x, y) {
  c = floor(x * 15.99)
  row = y * rows  // 0 = top edge
  if (flash[c] > 0) {
    hsv(0.14, 0.3, square(flash[c] * 5, 0.5))
  } else if (row >= rows - stack[c]) {
    hsv(0.02 + (rows - row) * 0.04, 1, 0.75)  // the pile, banded by depth
  } else if (row >= fy[c] - fh[c] && row < fy[c]) {
    hsv(fhue[c], 1, 1)  // the falling block
  } else {
    hsv(0, 0, 0)
  }
}
