# 4 Blue Fade
kind: 1D
sensors: no

This pattern is trivial: the entire strip shows one fixed, fully saturated blue-violet (indigo-leaning blue) hue, uniform across all pixels, whose brightness pulses smoothly up and down following a sine-shaped wave. One full bright-dim-bright cycle takes roughly half a minute — a very slow, calm breathing fade.

No per-pixel variation (the pixel index is ignored), no state beyond the frame clock, no randomness, no layout assumptions, no controls. The original also computes a second, faster clock that it never uses; omit it.
