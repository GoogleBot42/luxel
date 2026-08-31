// name: Continuous Cellular Automata
// Clean-room reimplementation from a prose functional description of the
// community pattern "Continuous Cellular Automata"; original source never
// consulted. Concept: Wolfram-style continuous-valued cellular automata.

// The rule is the classic one: every cell is the average of its three
// parents plus an offset, fractional part kept — the wrap is what makes
// the fractal structure. Everything around it is rebuilt for Luxel.
// The original derives ONE spacetime diagram and then sits frozen until a
// dial moves; this runs the automaton live. A generation is born at the
// bottom of the display every tick and the whole diagram rises, so the
// familiar nested cones grow and drift upward instead of standing still.
// The rule offset wanders slowly (Rule Drift), stray sparks nucleate new
// cones, and a watchdog reseeds the front row whenever the ring goes flat
// or saturates — the pattern can never freeze, black out or white out.
// Requires a 2D map.

var VIEW = 16             // displayed rows/columns
var COLS = 16             // automaton width — a ring, so no edge artifacts
var ROWS = 20             // ring buffer of generations (17 are on screen)
var gen = array(ROWS * COLS)
var head = 0              // ring slot holding the newest generation

var baseOffset = 0.09     // rule offset setpoint, in rule units
var driftAmt = 0.06       // peak-to-peak wander of the offset, rule units
var offset = 0.09         // the offset actually used this tick
var gensPerSec = 6
var sparksPerMin = 24
var baseHue = 0.02
var sat = 1

var tickMs = 1000 / 6
var acc = 0               // ms accumulated toward the next generation
var phase = 0             // 0..1 progress through the current tick
var flatCount = 0         // consecutive generations judged converged
var started = 0
var ringBase = 2 * ROWS   // per-frame render helpers
var hueNow = 0.02

//# min=0.005 max=0.995 step=0.005 default=0.09
export function sliderRuleOffset(v) {
  baseOffset = clamp(v, 0.005, 0.995)   // 0 and 1 render a dead field
}

//# min=0 max=0.4 step=0.005 default=0.06
export function sliderRuleDrift(v) {
  driftAmt = v            // 0 = a single fixed rule, like the original
}

//# min=1 max=30 step=1 default=6
export function sliderGenerationsPerSecond(v) {
  gensPerSec = max(1, floor(v))
  tickMs = 1000 / gensPerSec
}

//# min=0 max=120 step=1 default=24
export function sliderSparksPerMinute(v) {
  sparksPerMin = max(0, floor(v))
}

export function hsvPickerColor(h, s, v) {
  baseHue = h
  sat = s                 // v is ignored: brightness is a device setting
}

// Fresh life for a converged ring. Only the newest generation is touched,
// so there is no flash: the new values ride in from the bottom edge and
// organise themselves as the diagram scrolls.
function reseed(dst) {
  for (var c = 0; c < COLS; c++) {
    gen[dst + c] = random(1)
  }
  gen[dst + floor(random(COLS))] = 1
  flatCount = 0
}

// One generation: three-parent average + offset, fractional part kept,
// wrapped around the ring so no boundary rule is needed at all.
function step() {
  var src = head * COLS
  head = (head + 1) % ROWS
  var dst = head * COLS
  var c, v
  for (c = 0; c < COLS; c++) {
    v = (gen[src + (c + COLS - 1) % COLS] + gen[src + c] +
         gen[src + (c + 1) % COLS]) / 3
    gen[dst + c] = frac(v + offset)
  }

  // A spark is a maximal cell dropped into the front row; it opens a new
  // cone that widens one cell per generation as it rises.
  if (sparksPerMin > 0 &&
      random(1) < sparksPerMin / (60 * gensPerSec)) {
    gen[dst + floor(random(COLS))] = 1
  }

  // Convergence watchdog. Three ways the automaton can stop being worth
  // looking at: no lateral texture left (a flat ring), no change from the
  // parent generation (the picture scrolls but never differs), or the whole
  // ring pinned dark or pinned bright. Three such generations in a row and
  // the front row is reseeded, so it can never settle for good.
  var energy = 0
  var churn = 0
  var mean = 0
  for (c = 0; c < COLS; c++) {
    energy += abs(gen[dst + c] - gen[dst + (c + COLS - 1) % COLS])
    churn += abs(gen[dst + c] - gen[src + c])
    mean += gen[dst + c]
  }
  energy /= COLS
  churn /= COLS
  mean /= COLS
  if (energy < 0.02 || churn < 0.01 || mean < 0.02 || mean > 0.98) flatCount += 1
  else flatCount = 0
  if (flatCount >= 3) reseed(dst)
}

export function beforeRender(delta) {
  if (!started) {
    started = 1
    // classic seed: one maximal cell in an otherwise empty generation, so
    // the first cone grows into view instead of appearing all at once
    gen[head * COLS + floor(COLS / 2)] = 1
  }

  // Slow two-rate wander of the rule offset: the automaton family morphs
  // (cones -> lattice -> bands -> chaos) over minutes, on its own.
  var wander = (wave(time(1.7)) - 0.5) + 0.6 * (wave(time(0.61)) - 0.5)
  offset = clamp(baseOffset + driftAmt * wander / 1.6, 0.005, 0.995)

  acc += delta
  var guard = 0
  while (acc >= tickMs && guard < 4) {   // catch up, but never spiral
    step()
    acc -= tickMs
    guard += 1
  }
  if (acc > tickMs) acc = tickMs
  phase = acc / tickMs
  ringBase = head + 2 * ROWS   // keeps the render's row lookup branch-free
  // +1 keeps the render's frac() argument positive for any picked hue
  hueNow = 1 + baseHue + 0.06 * (wave(time(1.1)) - 0.5)
}

export function render2D(index, x, y) {
  var col = floor(x * (COLS - 0.01))
  var dr = floor(y * (VIEW - 0.01))

  // Generations rise one row per tick; sample between the two nearest
  // generations (age 0 = newest) so the scroll is continuous, not stepped.
  var a = VIEW - phase - dr
  var i0 = floor(a)
  var f = a - i0
  var v = (1 - f) * gen[((ringBase - i0) % ROWS) * COLS + col] +
          f * gen[((ringBase - i0 - 1) % ROWS) * COLS + col]

  // The picked colour is where the bright cells sit; dimmer cells ride up
  // to a third of the wheel above it, with a sinusoidal easing that
  // compresses the ends. The whole arc breathes slowly around the anchor.
  var eased = (1 - cos(PI * clamp(v, 0, 1))) / 2
  var h = frac(hueNow + (1 - eased) / 3)

  hsv(h, sat, v * v * (0.3 + 0.7 * v))
}
