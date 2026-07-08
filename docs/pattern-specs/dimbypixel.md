# dimbypixel
kind: 1D
sensors: no

This pattern is trivial: it is a "how much of the strip is lit" utility, not an animation.

## Behavior
A single slider sets what fraction of the strip is on. Pixels from the start of the strip up to that fraction are lit; the rest are off. There is no dimming gradient despite the name — each pixel is either fully on or fully off, so the slider effectively moves a hard cutoff point along the strip. Default is the whole strip lit.

## Algorithm
No state between frames beyond the slider value. Per pixel: normalize the pixel index to 0..1 of the strip length; the pixel is on if the slider fraction exceeds that position, off otherwise (the on/off comparison result is used directly as the brightness). Fully proportional to pixel count; no hardcoding.

## Colors
All lit pixels are one fixed, fully saturated yellow-green (chartreuse). Unlit pixels are black.

## UI controls
- Slider, "lights on" (fraction of strip lit): 0 = all off, max = all on.

## Timing
Static; changes only when the slider moves.
