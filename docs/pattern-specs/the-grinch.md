# The Grinch
kind: 1D
sensors: no

## Overview
A trivial chaser pattern (reads like generated/beginner code). The whole strip is
solid saturated blue; two short blocks — one pure red, then a gap, then one pure
green — march along the strip in single-pixel steps, wrapping at the end. That's
the entire effect.

## Visual behavior
Bright fully saturated blue background. A red segment a few pixels long, then a
gap of background blue of the same length, then a green segment of the same
length. The pair advances one pixel at a time at a
steady rate of roughly ten pixels per second, so a full lap takes on the order of
pixel-count-divided-by-ten seconds. Motion is stepwise (pixel-quantized), not
smooth.

## Algorithm
- State between frames: an integer head position and a millisecond accumulator.
- Per frame: add the frame delta to the accumulator; when it exceeds a fixed
  step interval (about a tenth of a second), advance the head position by one,
  wrapping modulo the pixel count, and reset the accumulator to zero. (Resetting
  to zero rather than subtracting the interval loses remainder time; subtracting
  would be the cleaner reimplementation, though it slightly changes pacing.)
- Per pixel: default the pixel to blue; if the pixel index falls within the red
  block (starting at the head position, a few pixels long, wrapping) color it
  red; if it falls within the green block (starting after the red block plus an
  equal-sized gap, wrapping) color it green. The original does this with a
  per-pixel loop over the block length testing equality against each occupied
  index — O(block length) per pixel; a reimplementation should just use modular
  offset-in-range arithmetic.
- No randomness, no layout assumptions beyond a linear strip; adapts to any
  pixel count.

## Colors
Exactly three: fully saturated, full-brightness primary red, green, and blue
(hue-wheel primaries). Red and green blocks on a blue field — Grinch/Christmas
theming.

## Controls
None. Block length, gap length, and step interval are top-of-file constants; a
speed slider would be the obvious extension.
