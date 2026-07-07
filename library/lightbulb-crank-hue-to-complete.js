// name: Lightbulb - Crank Hue to Complete
// Clean-room reimplementation from a prose functional description of the
// community pattern "Lightbulb - Crank Hue to Complete"; original source
// never consulted.
//
// Interactive exhibit: a hand crank closes a switch once per revolution.
// Resting, the bulb's two filament coils breathe in warm resting colors;
// cranking snaps to red and every few turns steps through the rainbow;
// finishing on purple earns a strobing flash in the resting colors.
//
// The original ran on two controllers (upper/lower coil) selected by node
// ID; this single-device version maps the two color-table rows onto the
// two halves of the strip instead.

// --- states: 0 = resting, 1..6 = rainbow steps, 7 = success -----------
var numStates = 8
var REST = 0
var SUCCESS = 7

var cranksPerStep = 5      // crank turns needed per color
var timeoutSecs = 10       // walk-away reset
var flashLen = 0.35        // seconds per success flash
var flashCount = 5

// color tables: row 0 = upper coil, row 1 = lower coil
var huesUpper = array(numStates)
var satsUpper = array(numStates)
var huesLower = array(numStates)
var satsLower = array(numStates)

function setColors(state, hUp, sUp, hLo, sLo) {
  huesUpper[state] = hUp
  satsUpper[state] = sUp
  huesLower[state] = hLo
  satsLower[state] = sLo
}
setColors(REST, 0.13, 1, 0.1, 0.4)      // golden yellow / soft warm white
setColors(1, 0, 1, 0, 1)                // red
setColors(2, 0.08, 1, 0.08, 1)          // orange
setColors(3, 0.16, 1, 0.16, 1)          // yellow
setColors(4, 0.33, 1, 0.33, 1)          // green
setColors(5, 0.66, 1, 0.66, 1)          // blue
setColors(6, 0.78, 1, 0.78, 1)          // purple
setColors(SUCCESS, 0.13, 1, 0.1, 0.4)   // success flashes the resting colors

// load-time sanity check: crash immediately if the tables ever disagree
if (arrayLength(huesUpper) != numStates || arrayLength(satsUpper) != numStates ||
    arrayLength(huesLower) != numStates || arrayLength(satsLower) != numStates) {
  crashOnBadTables = array(0)[0]
}

// --- sensor: one digital GPIO, a switch closing once per revolution ---
// Pulled down, active HIGH — so an unwired pin (stubbed to 0) reads idle.
var sensorPin = 25

// Debug-only controls (remove for production):
var simulate = 0
export function toggleSensorSimulation(v) {
  simulate = v
}
var simCrank = 0
export function toggleSimulatedSensor(v) {
  simCrank = v
}

// --- state machine ----------------------------------------------------
var state = REST
var stateTime = 0
var cranks = 0
var lastSensor = 0
var bri = 1

export function beforeRender(delta) {
  stateTime += delta / 1000
  if (stateTime > 3600) stateTime -= 3600   // numeric-range guard

  var sensed
  if (simulate) {
    sensed = simCrank
  } else {
    pinMode(sensorPin, INPUT_PULLDOWN)
    sensed = digitalRead(sensorPin) == HIGH
  }

  // rising edge = one crank turn; any turn counts as activity
  if (sensed && !lastSensor) {
    cranks += 1
    stateTime = 0
  }

  // first crank leaves resting, carrying the turn as progress
  if (state == REST && cranks > 0) {
    state = 1
  }

  // enough turns on a color advances to the next; past purple = success
  if (state >= 1 && state < SUCCESS && cranks >= cranksPerStep) {
    state += 1
    cranks = 0
    stateTime = 0
  }

  // success runs its full flash sequence then rests
  if (state == SUCCESS && stateTime > flashCount * flashLen) {
    state = REST
    cranks = 0
    stateTime = 0
  }

  // walk-away: inactivity in any non-resting state resets
  if (state != REST && stateTime > timeoutSecs) {
    state = REST
    cranks = 0
    stateTime = 0
  }

  lastSensor = sensed

  // per-state brightness
  if (state == REST) {
    bri = 0.75 + 0.25 * sin(stateTime * PI2 / 4)   // few-second breathing, half..full
  } else if (state == SUCCESS) {
    var ramp = 1 - frac(stateTime / flashLen)      // sharp pop, fast decaying trail
    bri = ramp * ramp
  } else {
    bri = 1                                        // steady while counting cranks
  }
}

export function render(index) {
  if (index < pixelCount / 2) {
    hsv(huesUpper[state], satsUpper[state], bri)   // upper coil
  } else {
    hsv(huesLower[state], satsLower[state], bri)   // lower coil
  }
}
