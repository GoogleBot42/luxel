# 7 Yelllow Fade
kind: 1D
sensors: no

This pattern is trivial: the entire strip glows a single warm yellow and breathes in brightness, all pixels in unison.

## Visual behavior
Every pixel shows the same color at the same time. Brightness swells smoothly from fully off up to full brightness and back down again in a continuous, sinusoidal-feeling pulse. The cycle is slow and meditative — on the order of half a minute for one full dim-bright-dim round trip. Hue and saturation never change.

## Algorithm
- Per frame: sample one global sawtooth timebase (a slow one) and shape it with a smooth periodic wave function to get a brightness value that oscillates between zero and full.
- Per pixel: ignore the pixel index entirely; emit a fixed fully-saturated yellow hue at the frame's shared brightness.
- No state is kept between frames beyond the engine's own time counters. No randomness. No layout assumptions — works on any pixel count in any geometry (it's uniform).
- Dead code note: the original also derives a second, faster timebase each frame but never uses it. A reimplementation can omit it.

## Colors
Fully saturated yellow (a warm yellow, slightly toward golden). Only brightness varies, so visually it runs from black through dim amber up to bright yellow.

## Controls
None.
