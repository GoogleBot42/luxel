# 6 Green Fade
kind: 1D
sensors: no

This pattern is trivial.

## Visual behavior
The entire strip shows a single uniform green. Its brightness smoothly rises and falls in a repeating fade — a slow, gentle breathe. One complete bright-to-dark-to-bright cycle takes on the order of half a minute.

## Algorithm
- No per-pixel variation: every pixel gets the identical color each frame.
- Per frame: sample a sawtooth phase from a slow global clock, then shape it with a triangle wave so brightness ramps linearly up and back down (no discontinuous jump).
- Per pixel: emit a fixed green hue at full saturation, with the frame's brightness value.
- Stateless apart from the global clock. No randomness. No layout assumptions; works at any pixel count.
- The original also computes a second, faster clock phase each frame but never uses it — omit it.

## Colors
A single pure, fully saturated green (a "classic green" hue, not teal or lime-shifted), fading between off and full brightness.

## Controls
None.
