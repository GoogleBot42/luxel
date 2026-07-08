# glitch bands
kind: 1D
sensors: no

## What it looks like
The strip fills with bands of rainbow color that continuously shear, stretch, and abruptly wrap — like a color gradient being smeared across the strip and then "glitching" with hard sawtooth resets at irregular-looking intervals. On top of the color bands, whitish desaturated flashes race along the strip where two invisible traveling waves collide, and the strip is simultaneously carved into hard-edged brighter and dimmer segments whose boundaries sweep back and forth. The overall vibe is busy, digital, and deliberately discontinuous. Base color drifts through the spectrum every several seconds; the band shearing evolves over tens of seconds; the sparkle/segment motion churns on a roughly one-second scale.

## Algorithm
Layout: fully pixel-count relative (positions normalized by strip length, centered on the strip's midpoint) — scales to any strip with no changes.

Per frame, sample six free-running time ramps at different rates (no other state):
- a slow phase (several-second cycle) whose sine sets the global base hue — so the base color swings back and forth through part of the color wheel rather than rotating uniformly;
- a several-second triangle that sets the wrap modulus for the hue bands (a smallish value, varying by roughly half its own size);
- a slow (tens of seconds) triangle plus a moderate (ten-ish seconds) sinusoid that together set a spatial "slope" factor — how steeply hue changes per unit distance along the strip. The combination ranges from mildly negative through zero to strongly positive, so the banding direction and density both evolve;
- two faster ramps driving the saturation/brightness waves described below.

Per pixel:
1. Hue: take the pixel's signed distance from the strip center (as a fraction of strip length), multiply by the current slope factor, then wrap it modulo the current band modulus. This wrapped value is added to the base hue. The modulo is the whole trick: instead of one smooth gradient across the strip, you get repeating gradient segments with hard discontinuities where the wrap occurs — the "glitch bands." As the slope factor sweeps through zero the bands widen to infinity (uniform color), then re-form leaning the other way. (Note: the remainder should keep the sign of its left operand, so the two halves of the strip wrap in mirrored directions.)
2. Two traveling triangle waves are computed:
   - wave one: several repetitions across the strip length, drifting in one direction with a cycle of a few seconds, sharpened by squaring;
   - wave two: about one repetition across the strip, moving the opposite direction faster (roughly a second per cycle), sharpened much harder (raised to about the fourth power).
3. Saturation: take the product of the two waves and feed it through a triangle shaper, then invert. Where the product is near zero or near full the pixel stays fully saturated; where the product lands mid-range the saturation dips toward zero, producing white-hot flashes exactly where the two waves overlap at moderate strength.
4. Brightness: a hard comparison — where wave one exceeds wave two the pixel renders at (over-)full brightness, otherwise at about half brightness. This binary split creates the crisp bright/dim segmentation whose edges travel as the waves move.

No randomness anywhere — everything is deterministic interference between incommensurate periods, which is why it looks random-ish but never exactly repeats on a short timescale.

## Colors
Full-spectrum rainbow hues (the whole wheel is reachable via the wrapping offsets), fully saturated by default, punctuated by desaturated near-white flashes. Two brightness levels: full and about half. No black.

## Controls
None.

## Non-obvious notes
- The signature look comes from applying the modulo to the hue *offset* (position times slope) rather than to position itself: band width is controlled independently (by the modulus) from band density (by the slope), and both breathe on different periods.
- Brightness intentionally exceeds the displayable maximum on the bright side of the comparison; it just clamps, guaranteeing a solid-full band rather than a value that dips with the waves.
