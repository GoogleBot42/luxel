# 3 Violet Fade
kind: 1D
sensors: no

Trivial pattern — one short spec.

## What it looks like
The entire strip is a single fixed violet (a fully saturated hue in the violet/magenta-leaning purple region). All pixels are identical. Brightness smoothly fades up and down together in a continuous wave — a slow breathe, one full bright-dark-bright cycle taking on the order of half a minute.

## Algorithm
- Per frame: sample the global clock as a sawtooth phase with a long period (several tens of seconds), and map it through a smooth 0→1→0 wave to get brightness. (A second, faster clock phase is also computed but never used — dead code; drop it.)
- Per pixel: output constant violet hue, full saturation, the shared brightness. Pixel index is ignored entirely.
- No state, no randomness, no layout assumptions — works on any pixel count and any mapping (uniform color everywhere).

## Controls
None.
