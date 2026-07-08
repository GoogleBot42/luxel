# ChristmasLights
kind: 1D
sensors: no

## What it looks like
The strip is divided into equal contiguous blocks of a couple dozen pixels each. Blocks cycle through three colors in a repeating three-block sequence — red, dimmed white, green — like a string of chunky holiday lights. Every couple of seconds the color assignment rotates by one step, so the colors appear to march block-by-block along the strip in slow, discrete jumps. No fading; transitions are instant.

## Algorithm
State kept between frames: a millisecond accumulator and the current rotation phase (which of the three colors is assigned to the first block-position of the three-slot cycle).

Per frame: add the frame delta to the accumulator. When it exceeds a threshold of about two seconds, reset it and advance the rotation: the three colors cyclically shift one slot (first slot's color moves to the second slot, etc.), so after three steps the assignment repeats.

Per pixel: integer-divide the pixel index by the block size, take that block number modulo three, and look up the color currently assigned to that slot.

Colors are three fixed choices:
- Red: fully saturated, full brightness.
- White: zero saturation, deliberately dimmed to a bit under half brightness so it doesn't overpower the colored blocks.
- Green: fully saturated, full brightness.

(The original's header comment says "red/blue", but the hue value it uses wraps around the color wheel and actually displays green — appropriately for a pattern named ChristmasLights. Implement what it displays: red / white / green.)

Randomness: none. Sensors: none.

Layout: block size is hardcoded (about twenty pixels). Obvious fix: expose block size as a slider, or derive it as a fraction of pixel count. Strips shorter than three blocks won't show all colors.

## Controls
None in the original.

## Timing feel
A discrete color-rotation step every couple of seconds; full cycle back to the starting arrangement in about six seconds.
