# index walk
kind: 1D
sensors: no

This is a deliberately minimal teaching pattern — a single lit pixel walking along the
strip.

## What it looks like
One pixel is lit at a time; everything else is black. The lit dot marches steadily
from the start of the strip to the end, then jumps back to the start and repeats. Its
color is a rainbow hue that shifts rapidly (a full hue cycle in under a second) and
also depends on the dot's position along the strip.

## Algorithm
State: a fractional cursor position, advanced every frame by a small fixed step (a
fraction of a pixel per frame) and reset to zero once it passes the last pixel. Per
pixel: if the pixel's index equals the cursor rounded down, draw it at full saturation
and brightness with hue = (fast-cycling global phase + normalized position); otherwise
draw black (the source comments note that explicitly writing black matters because
unwritten pixels would retain their previous color).

Caveats worth fixing in a re-implementation:
- The step is per-frame, not time-based, so walk speed depends on frame rate; scale
  the step by the frame delta instead.
- A step larger than one pixel per frame skips LEDs (acknowledged in the source's own
  comments).

## Colors
Full rainbow, fully saturated, single pixel; background black.

## Controls
None (walk speed is a constant in the source; an obvious slider candidate).

## Timing feel
At the default step the dot crosses a few pixels per second-ish (frame-rate
dependent); the hue cycles very fast, under a second per rainbow.
