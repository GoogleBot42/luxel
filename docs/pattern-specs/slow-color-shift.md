# slow color shift
kind: 1D
sensors: no

This is a near-trivial ambient pattern. It shows soft, slowly drifting islands of color separated by dark gaps, with the overall hue creeping around the color wheel. Everything moves gently; nothing is abrupt.

## Visual behavior
Bright blobs roughly a dozen pixels wide sit along the strip with dark valleys between them. The blobs slosh back and forth (they don't travel steadily in one direction — their positions oscillate) on a cycle of roughly ten seconds. Meanwhile the whole palette slides continuously through the rainbow over several seconds per hue lap, and at any instant a gentle hue gradient (about a quarter of the color wheel end to end) is spread across the strip, so neighboring blobs are related but not identical colors.

## Algorithm
No state is kept between frames beyond two free-running clocks read each frame:

- Clock A: a phase angle that completes a full turn in about ten seconds.
- Clock B: a sawtooth ramp through the hue wheel with a period of several seconds.

Per pixel:
1. Compute a sinusoid whose argument is the raw pixel index scaled down by about half (i.e., a spatial wavelength of roughly a dozen LEDs), phase-modulated by a few units times the sine of Clock A. This makes the whole standing-wave pattern sweep back and forth rather than scroll.
2. Brightness: map that sinusoid to a 0..1 value and raise it to the fourth power. The strong power sharpening is what turns a plain sine into distinct bright blobs with wide dark gaps.
3. Hue: Clock B, plus a small wobble (roughly one fifth scale) from the same sinusoid, plus a linear spatial term that spreads about a quarter of the hue wheel across the strip. Full saturation.

## Layout notes / hardcoding
The spatial frequency is written against the raw pixel index, not the normalized position, so the blob size is a fixed number of LEDs regardless of strip length: a short strip shows one or two blobs, a long strip shows many. The end-to-end hue spread is normalized to strip length (it divides by a multiple of the pixel count), so that part scales. If you want the blob count rather than blob size to be constant across installations, replace the raw index with normalized position times a chosen blob count.

## Controls
None.

## Colors
Full-saturation rainbow, continuously cycling; adjacent blobs offset slightly along the wheel. Blackish gaps between blobs.
