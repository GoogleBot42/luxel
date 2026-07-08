# rainbow fonts
kind: 1D
sensors: no

This is a trivial pattern — a short spec suffices.

## What it looks like
A smoothly animated rainbow that is mirror-symmetric about the middle of the strip: colors sweep continuously through the full hue wheel, and because the underlying value is a folded distance-from-center, the two halves of the strip are mirror images. The sine warping makes the bands non-uniform — hues bunch up and spread out rather than spacing evenly. The whole cycle repeats every several seconds at a relaxed pace.

## Algorithm
No state beyond a per-frame phase taken from a sawtooth timebase with a period of several seconds.

Per pixel: compute a 0..1 "closeness to center" value (one at the strip midpoint, falling linearly to zero at both ends — inherently symmetric). Pass it through a sine-shaped wave (mapping 0..1 through one full sine cycle back to 0..1), then add the animation phase and pass it through the same sine-shaped wave again. Use the result as the hue, at full saturation and modest fixed brightness (dimmed to roughly a third of full).

The double sine-fold is the only mildly clever part: it turns the linear center-distance ramp into smoothly compressing/expanding color bands as the phase slides.

## Colors
Full rainbow (entire hue wheel), fully saturated, at a fixed moderate brightness.

## Controls
None.

## Layout assumptions
Pure 1D by index; midpoint computed from the pixel count, so it adapts to any strip length. No fixes needed.
