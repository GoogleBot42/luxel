# millipede

kind: 1D
sensors: no

## What it looks like

A full-spectrum rainbow chopped into repeating segments crawls along the strip while ripples of brightness sweep through it, so the lit regions undulate like the legs of a millipede in motion. Bands of color slide continuously in one direction; brightness waves travel through them so each "segment" appears to flex bright and dark as it moves. The motion cycle is on the order of a few seconds; the whole look repeats every several seconds.

## Algorithm

This is a short, stateless pattern — no arrays, no persistence beyond two clock phases.

Per frame: sample two free-running sawtooth clocks from the engine's global timebase — one cycling in roughly three to four seconds, the other about twice as slow.

Per pixel, compute a hue as the sum of three terms:

1. A **scrolling segmented ramp**: take the pixel's normalized position along the strip, add an offset that advances by one full strip length per slower-clock cycle (this makes the bands travel), scale the result up by a small integer factor (around five), and wrap it at one-half rather than at one. Wrapping the scaled ramp at a half creates the repeating half-spectrum "segments."
2. A **static linear gradient**: the pixel's normalized position, unscaled, so hue also drifts smoothly from one end of the strip to the other.
3. A **slow sinusoidal wobble**: a triangle/sine-shaped oscillation of the faster clock, shifting all hues together back and forth over time.

Brightness is a sine-shaped wave of (that same hue value plus the slower clock's phase), then squared to deepen the dark troughs and sharpen the bright crests. Because brightness is a function of the hue value itself, the dark/bright ripples are locked to the color bands and travel with them — that coupling is what produces the leg-like articulation.

Saturation is always full. Hue is passed to the standard HSV call, which wraps hue modulo one, so the summed terms exceeding one is fine.

One quirk: the scrolling offset is sampled from the clock **inside the per-pixel function** rather than once per frame. Functionally this is the same as sampling per frame (all pixels in a frame see nearly the same value); an implementer may hoist it to the per-frame step.

No layout assumptions beyond a 1D strip; everything is normalized by pixel count, so it scales to any strip length.

## Colors

Full rainbow — the entire hue wheel, fully saturated, at whatever brightness the wave allows. Dark gaps between bright multi-colored segments.

## Controls

None.
