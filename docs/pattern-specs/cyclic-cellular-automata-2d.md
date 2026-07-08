# Cyclic Cellular Automata 2D
kind: 2D
sensors: no

## Overview
A cellular-automaton simulator for 2D matrices with two related rule sets:
(a) a Greenberg–Hastings-style "excitable medium" automaton (the forest-fire /
nerve-impulse model), and (b) a classic cyclic cellular automaton. It produces
self-organizing spirals, expanding wave rings, and swirling rainbow fronts that
would be very hard to author by hand. Patterns evolve step by step; when a run
dies out or its lifetime expires, the grid is re-seeded randomly and a new run
begins. Requires a 2D map; there is no 1D renderer.

## Visual behavior
- **Excitable-medium mode (default):** mostly-dark field in which traveling wave
  fronts sweep across the grid, curling into rotating spiral cores that emit
  repeating colored rings. Each cell flashes through a rainbow-ordered sequence
  of states as a wave passes, then rests dark until the next wave.
- **Cyclic mode:** starts as random confetti, coarsens into blobs, then develops
  turbulent boundaries and finally locked rotating spiral "demons" cycling through
  a small set of colors.
- Steps are discrete: the whole grid updates at once on a tick, giving a
  deliberate, clockwork motion whose rate is set by the speed slider (from many
  steps per second down to around a step per second).
- Every so often (per the lifetime slider, up to about half a minute, or
  immediately if the automaton has frozen/died) the whole display re-randomizes
  and a new evolution starts. Some random seeds fizzle quickly; that is expected
  and auto-recovered.

## Grid and layout assumptions
The grid dimensions are hardcoded as a square matrix of modest size (16-ish per
side) in two constants at the top; the user is told to edit them to match their
matrix. Obvious fix: derive them from the mapped resolution or expose as config.
Additional quirk: the two framebuffers are allocated as rows-by-height but then
indexed column-first everywhere, which only works because the grid is square; a
reimplementation should index consistently so non-square matrices work.

## State kept between frames
- Two full-grid integer buffers (current and previous generation) with a pair of
  swappable references — classic double buffering. Cell values are integers in
  [0, number-of-states).
- A frame-timer accumulator (milliseconds since last simulation step) and a
  pattern-lifetime accumulator.
- An "activity" tally recomputed each generation; if it comes out zero the
  automaton is considered dead and gets re-seeded on the next frame. (In cyclic
  mode it counts cells that changed; in excitable mode it sums cell values, so
  death there means literally everything returned to the resting state.)
- Mode, threshold, state count, and the two seeding fractions.

## Per-frame work (in the pre-render hook)
Accumulate elapsed time. If lifetime is enabled and exceeded, or the automaton
died, re-seed. If the step interval has elapsed, compute one new generation into
the back buffer (after swapping), resetting the activity tally first.

### Excitable-medium rule (von Neumann 4-neighborhood, toroidal wrap)
- State 0 is "resting". A resting cell becomes excited (state 1) if at least
  `threshold` of its four orthogonal neighbors are currently excited.
- Any non-resting cell simply advances to the next state, wrapping back to
  resting after the last ("refractory") state. So excitation is followed by a
  fixed recovery ladder during which the cell cannot re-fire.

### Cyclic rule (Moore 8-neighborhood, toroidal wrap)
- Each cell looks for neighbors whose state equals its own state plus one
  (mod state count). If at least `threshold` neighbors hold that successor
  state, the cell is "eaten" and advances to that successor state; otherwise it
  stays put.

Both rules wrap at all edges (torus).

## Seeding
- **Excitable mode:** clear the grid to resting, then scatter a small fraction of
  cells (a few percent) set to the excited state, and a larger fraction (roughly
  two-thirds) set to random refractory levels, placing each at distinct random
  coordinates. The random refractory cells act as obstacles/nuclei that break
  symmetry and spawn spirals.
- **Cyclic mode:** every cell gets a uniformly random state.

## Rendering (per pixel)
Map the pixel's normalized coordinates to a grid cell (scale by width/height and
truncate). Let f = cell state divided by the state count. Hue = f (full trip
around the color wheel across the state range), full saturation, and brightness
= a smooth sine-like wave of f, so brightness rises and falls once across the
state ladder — resting/early states are at mid brightness, and the sequence
reads as a rainbow that also breathes in luminance. No numeric colors to copy:
it is simply "state index spread evenly around the hue wheel".

## Controls
- **Speed slider:** milliseconds per simulation step; response is squared so the
  low end has fine resolution. Full range spans from effectively-every-frame to
  around a second per step.
- **Lifetime slider:** how long a run lasts before forced re-seed, scaling up to
  roughly half a minute; zero means run forever (only re-seed on death).
- **Mode slider used as a toggle:** below half selects the excitable-medium rule,
  above half the cyclic rule. Switching mode also restores that mode's known-good
  defaults for the sensitive parameters: excitable mode gets threshold of one, a
  couple dozen states, and the few-percent/two-thirds seeding fractions; cyclic
  mode gets a threshold of three and only a handful of states.
- Four more sliders exist but are shipped commented out (advanced use):
  threshold (small integer range), state count (up to a few dozen), initial
  excited fraction (up to a fifth, squared response), initial refractory
  fraction (up to four-fifths, squared response). If enabled they override the
  mode toggle's defaults.

## Clever / non-obvious bits
- Death detection via the per-generation activity tally lets bad random seeds
  self-heal without user action.
- Buffers are described as intentionally oversized to dodge edge clipping,
  though the update loops actually handle edges by explicit modular wrap.
- The timers start saturated (huge initial values) so the first frame immediately
  seeds and steps rather than waiting a full interval.
