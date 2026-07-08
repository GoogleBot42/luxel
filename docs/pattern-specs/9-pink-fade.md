# 9 Pink Fade
kind: 1D (index-independent — renders identically on any layout)
sensors: no

This pattern is trivial: the entire strip is a single solid color — a fully saturated hue sitting a hair below the top of the hue wheel, i.e. right at the red/magenta boundary, reading as a hot pinkish red — whose brightness pulses smoothly up and down (triangle-wave-shaped fade in and out through black) with a very slow period of roughly half a minute per full pulse.

Details:
- Per frame: sample one slow sawtooth timer and shape it with a triangle wave to get the global brightness.
- Per pixel: emit the fixed hue at full saturation with that shared brightness. Every pixel is identical; the pixel index is ignored.
- The original also computes a second, faster timer each frame that is never used (dead code — omit it).
- No state between frames, no randomness, no controls, no layout assumptions.
