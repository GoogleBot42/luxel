# 2 Purple Fade
kind: 1D
sensors: no

This pattern is trivial and is byte-for-byte the same idea as the "yellow fade" sibling pattern, with only the hue changed: the entire strip glows a single purple/violet and breathes in brightness, all pixels in unison.

## Visual behavior
Every pixel shows the same color at the same time. Brightness rises smoothly from off to full and back in a continuous sinusoidal-feeling pulse, taking on the order of half a minute per full cycle. Hue and saturation never change.

## Algorithm
- Per frame: sample one slow global sawtooth timebase and shape it with a smooth periodic wave function to get a shared brightness between zero and full.
- Per pixel: ignore the index; emit a fixed fully-saturated purple hue at the frame's shared brightness.
- No persistent state, no randomness, no layout assumptions — uniform on any geometry.
- Dead code note: a second, faster timebase is computed each frame but never used; omit it.

## Colors
Fully saturated purple/violet (a hue roughly three quarters of the way around the color wheel — between blue and magenta, closer to violet). Only brightness varies: black through dim violet to bright purple.

## Controls
None.
