# snake
kind: 1D
sensors: no

This is a trivial pattern: a comet/chaser over a static rainbow.

## What it looks like
The strip carries a fixed rainbow gradient (hue proportional to pixel position, first pixel through last spanning the full hue wheel, fully saturated). A bright "head" travels along the strip at constant speed, completing a lap in several seconds, with a short tail (about ten pixels) fading linearly to black behind it. Everything outside the tail is dark, so you see a rainbow-colored snake whose body color changes as it moves through the gradient. It wraps around from the end back to the start seamlessly.

## Algorithm
Per frame: one sawtooth clock in the unit range (several seconds per cycle) marks the head's normalized position. Per pixel: hue = pixel index divided by pixel count; compute the pixel's distance *behind* the head in pixels, wrapping around the strip; brightness = one minus that distance divided by the tail length, clamped to the unit range (head brightest, linear ramp to zero over the tail). No state between frames, no randomness.

Layout note: the tail length is hardcoded at about ten pixels, so on long strips the snake looks proportionally tiny and on very short strips it dominates. Obvious fix: make the tail a fraction of the pixel count (or expose it as a slider).

## Colors
Full rainbow positional gradient, fully saturated; brightness-only tail envelope.

## Controls
None.
