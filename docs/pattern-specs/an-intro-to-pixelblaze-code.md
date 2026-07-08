# An Intro to Pixelblaze Code
kind: 1D
sensors: no

Note: despite being catalogued as sensor-reactive, this pattern uses no sound or sensor input at all. It is the stock interactive language tutorial: the file is overwhelmingly explanatory comments plus a series of commented-out lesson examples. Only one small piece of code is active, and that is what should be reimplemented. The commented-out lessons are inert and out of scope.

## Active behavior (the wiring test pattern)

Three single-pixel colored dots chase along the strip in a fixed formation: a red dot in the lead, a green dot trailing it by a few pixels, and a blue dot trailing by twice that gap. Everything else is black. The formation sweeps the whole strip once every several seconds (around five), then wraps and repeats. It reads as "red pixel, then green pixel, then blue pixel run through all the LEDs" — a quick visual check that wiring, color order, and pixel count are configured correctly.

## Algorithm

Purely per-pixel, no state between frames. A sawtooth master clock ramps from zero to one over roughly five seconds; multiplying it by the pixel count gives a lead position that sweeps the strip. For each pixel, each of the three color channels is switched fully on when the pixel sits within about one pixel of the lead position offset backward by that channel's fixed lag (no lag for red, a few pixels for green, double that for blue). The comparison is a plain distance test producing an on/off value per channel, emitted directly as RGB. Because the test is a distance of less than about one around a continuously-moving point, each dot lights one or occasionally two adjacent pixels.

Note the trailing dots do not wrap: when the lead is near the start of the strip the green and blue dots have negative positions and simply don't appear until the sweep advances far enough. A reimplementation may keep this quirk (it is what the original does) or wrap the offsets modulo the strip length.

## Colors

Pure full-brightness red, green, and blue on black. Deliberately primary so the user can verify their LED color-order setting.

## UI controls

None.

## Timing

One full sweep of the strip in roughly five seconds, constant speed, independent of frame rate (driven by a wall-clock sawtooth).

This is a trivial pattern; the file's value in the original product is the tutorial prose, not the effect.
