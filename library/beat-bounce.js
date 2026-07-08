// name: Beat Bounce
// Curated example (hand-written showcase of the Luxel language/builtins).
// Tempo-locked motion with no audio hardware: beatSin() sweeps a hot core
// back and forth while beat() spikes its width on every beat. A gauge
// readout shows the beat phase.
var c = array(3)

export var bpm = 96
export function inputNumberBpm(v) { bpm = clamp(v, 30, 220) } //# min=30 max=220 step=1 default=96

export function gaugeBeat() { return beatPhase }

export function beforeRender(delta) {
  pos = beatSin(bpm * 0.25, 0.1, 0.9)  // one full sweep every 4 beats
  beatPhase = beat(bpm)
  thump = (1 - beatPhase) * (1 - beatPhase)
  width = 0.05 + 0.09 * thump
}

export function render(index) {
  x = index / (pixelCount - 1)
  d = abs(x - pos)
  core = saturate(1 - d / width)
  halo = saturate(1 - d / 0.3) * 0.25 * thump
  mixColors(1, 0.25, 0, 0.35, 0, 1, saturate(d / 0.12), c)  // ember → violet
  b = core * core + halo
  rgb(c[0] * b, c[1] * b, c[2] * b)
}
