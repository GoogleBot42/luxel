// name: Performance test framework
// Clean-room reimplementation from a prose functional description of the
// community pattern "Performance test framework"; original source never
// consulted. A developer utility: it times three user functions (shared
// overhead, control, experiment) against each other and publishes a
// speedup ratio via watched vars. The original keeps the LEDs dark; this
// port adds a minimal visible readout so it renders something inspectable.
// That readout is a DELIBERATE deviation, kept because the playground has no
// Vars Watch and a permanently black gallery tile reads as a broken pattern.
// It is why the verification pair is excluded from the fidelity sweep rather
// than scored — see tools/verify/fixups.json `nonVisual` (Gitea #123).

// --- benchmark configuration ---
var ITER = 100          // iterations per timed call (the workload size)
var WINDOW = 1000       // ms per phase before a result is recorded
var scratch = 0         // sink so the workloads are not optimized away

// The three timed workloads. Overhead is the bare loop; control squares
// via the general power builtin; experiment squares by multiplication.
function overhead() {
  var i = 0
  while (i < ITER) { scratch = i; i += 1 }
}
function control() {
  var i = 0
  while (i < ITER) { scratch = pow(i, 2); i += 1 }
}
function experiment() {
  var i = 0
  while (i < ITER) { scratch = i * i; i += 1 }
}

// --- state kept between frames ---
var phase = 0           // 0 overhead, 1 control, 2 experiment
var accum = 0           // accumulated ms in the current phase
var frames = 0          // frames counted in the current phase

// --- watched outputs ---
export var results = array(3)   // ms/exec: [overhead, control, experiment]
export var speedup = 0          // control ms / experiment ms (>1 = faster)

export function beforeRender(delta) {
  accum += delta
  frames += 1

  // run the phase's workload -- this is what we are timing
  if (phase == 0) overhead()
  else if (phase == 1) control()
  else experiment()

  if (accum >= WINDOW) {
    var perExec = accum / frames         // div guarded: frames >= 1 here
    if (phase == 0) {
      results[0] = perExec
    } else if (phase == 1) {
      results[1] = max(0, perExec - results[0])
    } else {
      results[2] = max(0, perExec - results[0])
      // control / experiment; div-by-zero yields 0 per the language spec
      speedup = results[1] / results[2]
    }
    phase = (phase + 1) % 3
    accum = 0
    frames = 0
  }
}

// Visible readout: hue names the active phase, and a fill bar shows how
// far this phase has progressed toward its one-second window.
export function render(index) {
  var pos = index / pixelCount
  var progress = accum / WINDOW
  var hue = phase * 0.28              // red / green / teal by phase
  var v = (pos <= progress) ? 1 : 0.12
  hsv(hue, 0.85, v)
}
