# firework dust
kind: 1D
sensors: no

This pattern is trivial.

## Visual behavior
The strip is almost entirely dark, with tiny multicolored sparks popping on and off at random positions every frame — like drifting firework embers or glitter. Sparks last only a single frame, so at typical frame rates the flicker is fast and restless. At any instant only a small fraction of pixels (well under one in a hundred) is lit.

## Algorithm
Completely stateless; no pre-render work, no state between frames, no layout assumptions (any pixel count works).

Per pixel, per frame:
- Draw a fresh uniform random hue spanning the whole color wheel.
- Draw a second independent uniform random number; the pixel is lit only if it exceeds a threshold very close to the top of the range (a fraction-of-a-percent chance). The comparison yields a hard on/off — lit pixels are at full brightness, everything else is fully off.
- Lit pixels are fully saturated.

Note the spark density is per-frame, so perceived sparkle rate scales with frame rate; a faithful port may want to keep that quirk rather than normalize by time.

## Colors
Fully saturated random hues — every spark can be any color of the wheel — against black.

## Controls
None.
