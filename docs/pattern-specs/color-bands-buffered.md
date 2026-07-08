# color bands (buffered)
kind: 1D
sensors: no

This is a small technique demo, not an elaborate effect: it exists to show the "compute everything in the per-frame hook into per-pixel buffers, then have the per-pixel renderer only read buffers" pattern, which keeps the render callback fast enough for timing-sensitive LED protocols.

## What it looks like
Sparse bright bands of rainbow color slide along the strip in opposing directions, brightening and interfering where they cross, over a mostly dark background. Colors run through two full rainbow cycles end-to-end and are mostly vivid, but soft desaturated (whitish) zones drift through slowly. The motion feel is a continuous, watery drift with cycles of a few to several seconds.

## Algorithm
State: three pixel-count-sized buffers (hue, saturation, brightness).

Each frame, three sawtooth phases with different periods (roughly in the several-seconds range, one about twice as fast as another) are sampled, then a loop over all pixels fills the buffers:
- Hue: pixel index divided by half the pixel count — a static two-cycle rainbow ramp (never animated).
- Saturation: take a sine-shaped wave of (negative index scaled down by a few pixels per cycle... i.e., a spatial wave with a period of a handful of pixels) offset by one moving phase; raise it to the fourth power and subtract from one. Result: fully saturated almost everywhere, with narrow drifting dips toward white.
- Brightness: the product of two spatial sine-waves with different short periods (a couple of pixels and about five pixels per cycle) drifting in opposite directions, plus a third wave with a slightly longer period drifting with the first; the sum is raised to the fourth power. The fourth power crushes the mid-range so only the constructive-interference peaks survive as distinct bright bands.

The renderer just emits hue/saturation/brightness from the buffers at the given index.

Note: the spatial wave periods are in absolute pixels (a few to several pixels per cycle), not fractions of strip length, so band width is constant regardless of strip size — fine as-is, arguably a feature.
