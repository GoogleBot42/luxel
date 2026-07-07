// name: Cyclic Cellular Automata 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Cyclic Cellular Automata 2D"; original source never
// consulted.

// Two rule sets on a toroidal grid: an excitable-medium (Greenberg-
// Hastings) automaton — dark field with traveling wave fronts curling
// into spiral cores — and a classic cyclic CA that coarsens from confetti
// into rotating spiral "demons". Discrete clockwork steps; the grid
// re-seeds itself when a run dies out or its lifetime expires.
// Requires a 2D map.

var W = 16
var H = 16
var N = 256               // W * H

var bufA = array(N)
var bufB = array(N)
var cells = bufA          // displayed / current generation
var back = bufB           // scratch for the next generation

// mode 0 = excitable medium, 1 = cyclic; sliderMode restores each mode's
// known-good defaults for the sensitive parameters
var mode = 0
var threshold = 1
var states = 24
var excitedFrac = 0.03    // seeding: fraction of cells set excited
var refractFrac = 0.66    // seeding: fraction set to random refractory levels

var stepMs = 90
var lifetimeMs = 15000    // 0 = run forever (re-seed only on death)
// timers start saturated so the first frame seeds and steps immediately
var stepAcc = 30000
var lifeAcc = 30000
var activity = 0          // per-generation life tally; 0 means dead

//# min=0 max=1 step=0.01 default=0.3
export function sliderSpeed(v) {
  stepMs = 1000 * v * v   // squared response: fine control at the fast end
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderLifetime(v) {
  lifetimeMs = 30000 * v  // up to ~half a minute; 0 = forever
}

//# min=0 max=1 step=1 default=0
export function sliderMode(v) {
  if (v < 0.5) {          // excitable medium
    mode = 0
    threshold = 1
    states = 24
    excitedFrac = 0.03
    refractFrac = 0.66
  } else {                // cyclic
    mode = 1
    threshold = 3
    states = 5
  }
  lifeAcc = 30000         // force a re-seed under the new rules
}

/* Advanced sliders (uncomment to override the mode defaults):
//# min=0 max=1 step=0.01 default=0.2
export function sliderThreshold(v) { threshold = 1 + floor(v * 4) }
//# min=0 max=1 step=0.01 default=0.8
export function sliderStates(v) { states = 3 + floor(v * 29) }
//# min=0 max=1 step=0.01 default=0.4
export function sliderExcitedFraction(v) { excitedFrac = 0.2 * v * v }
//# min=0 max=1 step=0.01 default=0.9
export function sliderRefractoryFraction(v) { refractFrac = 0.8 * v * v }
*/

function seed() {
  var i, n
  if (mode == 0) {
    // clear to resting, scatter a few excited cells and many random
    // refractory obstacles that break symmetry and nucleate spirals
    for (i = 0; i < N; i++) cells[i] = 0
    n = floor(N * excitedFrac)
    for (i = 0; i < n; i++) cells[floor(random(N))] = 1
    n = floor(N * refractFrac)
    for (i = 0; i < n; i++) cells[floor(random(N))] = 2 + floor(random(states - 2))
  } else {
    // cyclic mode: uniform random confetti
    for (i = 0; i < N; i++) cells[i] = floor(random(states))
  }
  lifeAcc = 0
  activity = 1
}

function step() {
  var x, y, s, cnt, i
  activity = 0
  for (y = 0; y < H; y++) {
    var ym = ((y + H - 1) % H) * W    // toroidal wrap at every edge
    var yp = ((y + 1) % H) * W
    var y0 = y * W
    for (x = 0; x < W; x++) {
      var xm = (x + W - 1) % W
      var xp = (x + 1) % W
      i = y0 + x
      s = cells[i]
      if (mode == 0) {
        // excitable medium, von Neumann 4-neighborhood
        if (s == 0) {
          cnt = (cells[y0 + xm] == 1) + (cells[y0 + xp] == 1) +
                (cells[ym + x] == 1) + (cells[yp + x] == 1)
          back[i] = cnt >= threshold ? 1 : 0
        } else {
          back[i] = (s + 1) % states  // fixed recovery ladder back to rest
        }
        activity += back[i]           // dead = everything at rest
      } else {
        // cyclic rule, Moore 8-neighborhood: eaten by the successor state
        var succ = (s + 1) % states
        cnt = (cells[ym + xm] == succ) + (cells[ym + x] == succ) +
              (cells[ym + xp] == succ) + (cells[y0 + xm] == succ) +
              (cells[y0 + xp] == succ) + (cells[yp + xm] == succ) +
              (cells[yp + x] == succ) + (cells[yp + xp] == succ)
        if (cnt >= threshold) {
          back[i] = succ
          activity += 1               // dead = nothing changed
        } else {
          back[i] = s
        }
      }
    }
  }
  var tmp = cells   // swap the double buffers
  cells = back
  back = tmp
}

export function beforeRender(delta) {
  stepAcc = min(stepAcc + delta, 30000)
  lifeAcc = min(lifeAcc + delta, 30000)
  if (activity == 0 || (lifetimeMs > 0 && lifeAcc >= lifetimeMs)) seed()
  if (stepAcc >= stepMs) {
    stepAcc = 0
    step()
  }
}

export function render2D(index, x, y) {
  var s = cells[floor(y * 15.99) * 16 + floor(x * 15.99)]
  var f = s / states
  // state index spread evenly around the hue wheel; brightness breathes
  // once across the state ladder, with resting/early states at mid level
  hsv(f, 1, wave(f))
}
