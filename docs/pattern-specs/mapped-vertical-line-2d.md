# mapped vertical line 2D
kind: 2D (requires a pixel map; a commented-out 1D fallback exists in the original but is not active)
sensors: no

## What it looks like
A single solid vertical bar sweeps horizontally across the mapped surface at a steady rate, jumping back to the left edge when it reaches the right (sawtooth motion, no bounce). Everything behind it is a dim, saturated background color. Both the bar color and the background color are user-picked hues.

This is a trivial pattern; the spec is short on purpose.

## Algorithm
- Per frame: one sawtooth clock in 0..1 gives the bar's center x-position. Period is set by the speed slider.
- Per pixel (given mapped x, y in 0..1): if the pixel's x lies within half the bar width on either side of the center, draw the bar color at full saturation and full brightness; otherwise draw the background hue at full saturation and low brightness (around a tenth). y is ignored, which is what makes the line vertical.
- No state between frames beyond the clock; no randomness. No layout hardcoding — works on any mapped geometry.
- Edge behavior: the bar is simply clipped at the edges (no wrap-around drawing), so as it re-enters at the left it appears to slide in rather than wrap seamlessly. (Wrap support was sketched but disabled in the original.)

## Colors
Two flat colors, both fully saturated: the bar hue and the background hue, each chosen by slider across the whole hue wheel. Background is much dimmer than the bar.

## Controls (all sliders)
- Line speed: sweep rate. Mapped so that even at zero the bar still moves slowly (a small offset prevents a stall); full range spans from a leisurely multi-second sweep to a fast sub-second-ish sweep.
- Line width: bar thickness as a fraction of the surface width, again with a small floor so it never vanishes entirely; at maximum the bar covers about half the surface.
- Line color: hue of the bar.
- Background color: hue of the dim backdrop.

## Timing
Continuous linear sweep; at default-ish settings expect a crossing every few seconds.
