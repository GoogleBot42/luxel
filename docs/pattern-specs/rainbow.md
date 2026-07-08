# rainbow
kind: 1D
sensors: no

Trivial pattern — the canonical scrolling rainbow.

## Behavior
Exactly one full hue cycle is spread across the strip (pixel's hue = its position as a fraction of the pixel count), at full saturation and full brightness. A global hue offset advances as a repeating sawtooth so the whole rainbow scrolls smoothly along the strip, completing one full revolution every several seconds.

## Algorithm
Per frame: sample a sawtooth time base (the engine's standard normalized 0→1 repeating timer at a fairly quick setting) into a global offset. Per pixel: hue = offset + position fraction (hue wraps naturally), full saturation and brightness. No state beyond the timer, no randomness, no layout assumptions, no controls, no sensors.
