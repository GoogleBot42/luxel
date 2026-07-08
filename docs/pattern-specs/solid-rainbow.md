# Solid Rainbow
kind: 1D
sensors: no

A near-trivial pattern. The entire strip shows a single hue at a time, cycling
smoothly around the full color wheel at full saturation. Despite the name, it
is not uniform in brightness: brightness ramps linearly along the strip from
completely dark at the first pixel to full at the last, so it looks like a
one-color gradient whose color rotates through the rainbow.

State: one hue phase accumulated between frames. Each frame it advances by the
elapsed time multiplied by the speed setting and wraps around at one (modulo),
so the cycle rate is frame-rate independent.

Per pixel: hue = the shared phase; saturation = full; brightness = the pixel's
index divided by the pixel count.

Control: a single "speed" slider. Its value is squared before use, giving
finer resolution at the slow end; at maximum the hue completes roughly one
full cycle per second, and near zero it can take arbitrarily long (at exactly
zero it freezes).

Nothing else — no randomness, no layout assumptions beyond a 1D index.
