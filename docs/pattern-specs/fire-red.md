# fire - red
kind: 1D
sensors: no

## What it looks like
A classic simulated flame on a strip (the well-known "Fire2012"-style
cellular-automaton fire). Flames lick upward from the base of the strip:
bright white-yellow near the base, cooling through orange and red to black as
the heat rises, with random flicker and occasional bright sparks surging up.
Motion feels like a steady campfire — continuous, chaotic, never repeating.
The simulation advances at a fixed tick of a few hundredths of a second
regardless of render frame rate, so the flicker speed is consistent.

## Algorithm
State kept between frames: a "heat" array with one value per simulated pixel
(range zero to one), plus a timer accumulating elapsed time.

Each frame, accumulated time is checked against a fixed step (a few tens of
milliseconds). Only when a full step has elapsed does the simulation advance
(subtracting the step from the accumulator, so long frames don't drift):

1. **Cooling**: every cell loses a small random amount of heat — a uniform
   random value between zero and a small cooling constant — then is clamped to
   the zero-to-one range.
2. **Convection**: iterating from the top of the array down to the third cell,
   each cell is replaced by a weighted average of the cells below it: one part
   the cell immediately below plus two parts the cell two below, divided by
   three. This diffuses heat upward and makes flames rise and thin out.
3. **Sparking**: with a chance of roughly one in two per simulation step, a
   spark is injected: a random cell within the bottom tenth of the array gets
   a large heat boost — a guaranteed floor of well over half of full heat plus
   a random extra — clamped to full.

Per pixel (render): the pixel's heat value is scaled into an index into a
precomputed 256-entry heat palette and the corresponding color is emitted
directly as RGB.

**Direction/symmetry mode**: a constant in the code (not a UI control) selects
one of four layouts:
- from the strip head (heat array maps directly to pixels),
- from the tail (mapping reversed),
- symmetric from both ends toward the middle,
- symmetric from the middle toward both ends.
In the two symmetric modes the simulation only covers half the pixel count and
the render mirrors it; the cooling constant is bumped up slightly to
compensate for the shorter flame run.

Randomness: uniform random numbers drive per-cell cooling amounts, the spark
probability, spark position, and spark intensity. That is the entire source of
the organic look.

## Colors
A "heat" palette built at startup as three channel ramps over the palette
index: the red channel ramps from black to full over the first third and stays
full; the green channel ramps up over the middle third; the blue channel ramps
up over the final third. Qualitatively: black, through deep red, red, orange,
yellow, to white — cold cells are black, hot cells white.

## Controls
None exposed. Cooling rate, spark chance, simulation tick, and direction mode
are all constants in the source (the author notes they were tuned on a strip
of about seventy pixels). Obvious improvement for a reimplementation: expose
cooling, sparking, and direction mode as sliders/selector. Note the constants
do not auto-scale with pixel count; long strips will want less cooling.

## Non-obvious bits
- The fixed-timestep gate is what decouples flame speed from frame rate.
- The convection average deliberately weights the cell two-below twice, which
  makes heat rise faster than a plain neighbor average.
- Sparks confined to the bottom tenth of the strip create the flame "base".
- The heat-to-palette index is clamped so full heat doesn't run off the end of
  the lookup table.
