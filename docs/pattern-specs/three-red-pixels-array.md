# Three Red Pixels (array)
kind: 1D
sensors: no

## Summary
This is a simple tutorial-style chase pattern. It is trivial by design; this spec is short on purpose.

## What it looks like
The entire strip is solid saturated blue. Three single saturated red pixels, evenly spaced one-third of the strip apart from each other, march along the strip at a slow, steady pace and wrap around from the end back to the start. The motion is smooth and constant — on a typical strip a red dot takes on the order of ten seconds or more to make a full lap (the speed is fixed in pixels-per-second, roughly ten, so lap time scales with strip length).

## Algorithm
- Persistent state: a working buffer with one entry per pixel, holding a hue value for that pixel; and a fractional "head position" (a real number in units of pixels) that persists between frames.
- Per frame (all work happens in the frame-setup step, not per pixel):
  1. Fill the whole buffer with the blue hue.
  2. Advance the head position by (fixed speed in pixels per second) x (elapsed frame time), then wrap it modulo the pixel count so it stays on the strip.
  3. For each of the three dots: take the head position plus zero, one-third, or two-thirds of the pixel count; floor it to an integer index; wrap modulo pixel count; write the red hue into the buffer at that index.
- Per pixel (render): read the hue from the buffer at that pixel's index and emit it at full saturation and full brightness.

No randomness. Frame-rate independent movement (uses elapsed time per frame).

## Colors
Exactly two hues, both fully saturated and full brightness: blue for the background, red for the three moving dots. Only the hue is stored in the buffer; saturation and brightness are constant.

## Layout assumptions
None hardcoded — it uses the runtime pixel count everywhere, so it works on any strip length. (Because positions are floored to integers, on very short strips the three dots could momentarily land on adjacent or identical indices; that is inherent, not a bug.)

## UI controls
None. Speed and the two hues are internal constants; the obvious enhancement is exposing speed as a slider, but the original has no controls.
