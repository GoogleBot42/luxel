# rainbow pinwheel
kind: 1D
sensors: no

This pattern is trivial.

## What it looks like
A fully saturated, full-brightness rainbow laid along the strip, continuously scrolling. Because the hue is driven by a sinusoid of position rather than a linear ramp, the rainbow is "folded": hue sweeps up and back down across the strip, so colors mirror rather than hard-wrap. One full color cycle takes a few seconds. On a circular/radial layout it reads as a spinning pinwheel.

## Algorithm
Per frame: sample a sawtooth time base with a period of a few seconds.
Per pixel: hue = a triangle/sine wave function of (time phase + pixel position as a fraction of the strip length). Saturation is deliberately overdriven well past full (it clamps, guaranteeing maximum saturation); brightness is full. No state, no randomness, layout-independent (scales with pixel count).

## Colors
The entire rainbow wheel, fully saturated.

## Controls
None. Obvious extensions: sliders for speed and spatial repeat count.
