// Falling sand, the classic cellular automaton: grains pour from the
// spout, drop one cell per tick, and slide down the pile's slopes. When
// the pile reaches the spout the panel fades out and pours again. Each
// grain keeps its own shade, so the pile ends up naturally speckled.
gw = 16
rows = 16
grid = array(gw * rows)
tickMs = 0
fading = 0

function step() {
  // bottom-up scan: a grain moves at most one cell per tick
  for (var y = rows - 2; y >= 0; y--) {
    for (var x = 0; x < gw; x++) {
      var i = y * gw + x
      if (grid[i] > 0) {
        var below = i + gw
        if (grid[below] == 0) {
          grid[below] = grid[i]
          grid[i] = 0
        } else {
          var d = random(1) < 0.5 ? -1 : 1  // coin-flip slide direction
          if (x + d >= 0 && x + d < gw && grid[below + d] == 0 && grid[i + d] == 0) {
            grid[below + d] = grid[i]
            grid[i] = 0
          } else if (x - d >= 0 && x - d < gw && grid[below - d] == 0 && grid[i - d] == 0) {
            grid[below - d] = grid[i]
            grid[i] = 0
          }
        }
      }
    }
  }
  spout = floor(gw / 2)
  if (grid[spout] == 0) grid[spout] = 0.5 + random(0.5)
  else if (grid[spout + gw] > 0) fading = 1  // pile reached the spout
}

export function beforeRender(delta) {
  if (fading) {
    feedback(grid, pow(0.85, delta * 0.06))
    if (arraySum(grid) < 0.5) {
      arrayReplace(grid, 0)
      fading = 0
    }
  } else {
    tickMs += min(delta, 100)
    while (tickMs >= 40) {
      tickMs -= 40
      step()
    }
  }
}

export function render2D(index, x, y) {
  v = grid[floor(y * 15.99) * gw + floor(x * 15.99)]
  hsv(0.09 + v * 0.03, 0.85 - v * 0.35, v * 0.9)
}
