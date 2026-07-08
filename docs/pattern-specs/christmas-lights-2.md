# Christmas Lights
kind: 1D
sensors: no

## What it looks like
A string of old-fashioned Christmas bulbs: every other pixel is dark, and the lit pixels cycle through red, green, and warm white in a fixed repeating order (red, gap, green, gap, white, gap). The whole arrangement steps — in discrete hops of one pixel, not smoothly — along the strip, advancing toward higher pixel numbers. Steps are slow and stately: from a couple of seconds up to around ten seconds per hop depending on the slider, so the motion reads as an occasional shuffle rather than a chase.

## Algorithm
State between frames: an accumulated-elapsed-time counter and an integer shift amount.

Per frame: accumulate elapsed time; when it passes a threshold set by the speed slider, reset the counter to zero and increment the shift, wrapping the shift back to zero after six steps (six is the period of the bulb layout).

Per pixel: subtract the shift from the pixel index, then classify the shifted index by remainder:
- even shifted positions: black;
- shifted positions congruent to one (mod six): pure red;
- congruent to three (mod six): pure green;
- congruent to five (mod six): white.

Randomness: none. Layout: works on any strip length (index-remainder based, no pixel-count hardcoding). It is inherently 1D.

Known quirk worth fixing in a reimplementation: the shifted index goes negative for the first few pixels once the shift is nonzero, and with sign-preserving remainder semantics those negative odd values match none of the color cases — so the first handful of pixels are simply never written that frame and can hold stale colors. The obvious fix is to add the layout period (or a multiple of it) to the shifted index before taking remainders, so the pattern wraps cleanly at the start of the strip. Also note the counter resets to zero rather than carrying the overshoot, so step timing is only approximately even — carrying the remainder is a harmless improvement.

## Colors
Fixed palette, full intensity: pure red, pure green, and plain white bulbs on a black background. No fading or blending — every pixel is fully one of the four values.

## Controls
- Slider ("ticks" / step interval): sets how long each hop takes, scaling linearly from nearly instant at the bottom of the range up to roughly ten seconds per hop at the top; default sits mid-range (several seconds).

## Timing feel
Discrete single-pixel hops every few seconds; the full six-step cycle (each bulb color having occupied each position) takes several times that.
