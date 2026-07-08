# Complements 3D
kind: 1D+2D+3D (3D is canonical; 2D maps its second axis to the blend axis; 1D maps strip position to the blend axis)
sensors: no

## What it looks like
A static spatial gradient between two complementary colors along one axis (the vertical/z axis in 3D, the y axis in 2D, strip position in 1D), with the whole gradient slowly rotating around the color wheel. One end of the space is a fully saturated hue; the opposite end is that hue's complement (directly across the wheel). The middle, where the two blend, is deliberately dimmed — because complementary colors mixed in RGB wash out to gray, the pattern darkens the crossover zone instead, so you see two rich color fields meeting in a dark waistline. Over roughly ten seconds the pair of hues completes one full rotation around the wheel, continuously and smoothly, then repeats.

## Algorithm
State between frames: a single phase value in 0..1, advanced each frame proportionally to elapsed time so a full cycle takes a fixed duration (order of ten seconds); it wraps back to zero.

Per pixel:
1. Convert the current phase (as a hue, full saturation, full brightness) to an RGB triple — color A.
2. Convert the phase plus a fixed offset of half the wheel to RGB — color B. (The offset is a named fraction: one half gives complements; the comment notes a third/two-thirds would give triadic pairs. Expose it as a tweakable if desired.)
3. Linearly interpolate between B and A in RGB space using the pixel's blend-axis coordinate (0..1).
4. Apply a dimming envelope keyed to the same coordinate: from the low end toward the middle, brightness ramps linearly down; past the middle it climbs back up along a slightly convex (quadratic-flavored) curve, so recovery is a bit different in shape from the descent. The dimming depth is a constant of roughly two-thirds to three-quarters attenuation at the midpoint.
5. Emit the result as RGB.

Hue-to-RGB conversion is done with a hand-rolled standard HSV-to-RGB routine (with hue wrap handling); an implementation can use any builtin equivalent.

Notes for the implementer:
- The two endpoint colors depend only on the frame's phase, not on the pixel, so compute them once per frame rather than per pixel (the original wastefully redoes both conversions for every pixel).
- No UI controls at all — cycle speed, wheel offset and dimming depth are code constants. Suggest exposing them as sliders.
- In 3D the first two coordinates are ignored (every horizontal slice is uniform). In 1D this reduces to a strip-long gradient between complements, dark in the middle, hues rotating.

This is a simple pattern; the only subtlety is the darkened midpoint to hide the muddy complement mix, and the asymmetric shape of that darkening.
