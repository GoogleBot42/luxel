# Red-Green XY 2D Sweep
kind: 2D
sensors: no

A simple mapping test pattern; deliberately trivial.

## What it looks like
A narrow bright red band sweeps horizontally (left to right) across the mapped 2D surface,
then a narrow bright green band sweeps vertically (top to bottom), and the two alternate
forever. Each sweep takes about three seconds. Background is black.

## Algorithm
Per frame: read one repeating ramp clock with a period of several seconds. The first half of
the cycle is the "horizontal/red" phase, the second half the "vertical/green" phase; within
its half, the ramp is rescaled to run the full 0-to-1 range so each phase performs exactly one
sweep.

Per pixel: pick the coordinate for the active axis (horizontal position in the red phase,
vertical in the green phase). Brightness is a smooth periodic bump as a function of half that
coordinate minus the rescaled clock (with a small phase offset), raised to a high power
(roughly the twentieth) so the broad sinusoid collapses into one narrow bright band. Hue is
fixed per phase: pure red in one, pure green in the other; full saturation.

No state between frames. No randomness. No layout hardcoding — works on any 2D map. No 1D
renderer; the obvious 1D adaptation is to use the normalized index as both axes.

## Controls
None.

## Purpose note
This is essentially a diagnostic for verifying a 2D pixel map: correct maps show clean
straight bands moving in the two cardinal directions.
