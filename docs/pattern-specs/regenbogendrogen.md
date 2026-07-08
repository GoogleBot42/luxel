# regenbogendrogen
kind: 1D
sensors: no

This is a trivial pattern; a short spec suffices.

## What it looks like
A psychedelic rainbow on a strip, mirrored around the strip's midpoint. Because the hue is pushed through two stacked waveshaping passes, the rainbow is not a smooth linear gradient: colors compress and stretch into repeating multicolored bands that flow and morph as time advances. Everything is at full saturation and full brightness — pure moving color, no dimming.

## Algorithm
No state between frames beyond one sawtooth phase read each frame: a repeating ramp with a period on the order of ten-odd seconds.

Per pixel: take the pixel's normalized distance from the strip midpoint (so the output is symmetric about the center), negate/offset it slightly so the center sits near one end of the ramp, then apply a smooth 0-to-1 wave shaping function (sine-based triangle-ish wave mapping any input to 0..1). Add the global time phase and apply the same wave shaping a second time. The result is the hue; saturation and brightness are maxed.

The double wave application is the whole trick: it folds the linear center-distance ramp twice, producing multiple mirrored rainbow repetitions whose spacing shifts nonlinearly as the time phase sweeps — the "drug rainbow" wobble the name promises.

Layout: uses the true pixel count, so it adapts to any strip length. 1D only; no 2D renderer.

## Controls
None.

## Timing
One full color cycle takes roughly ten to fifteen seconds; motion is continuous and fluid.
