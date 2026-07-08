# Wavy Bands
kind: 1D+2D (designed for 2D; 1D fallback samples a horizontal slice)
sensors: no

## What it looks like

A handful of vertical rainbow-colored bands fill the display. The band boundaries are not straight: they undulate side-to-side like seaweed (a sinusoidal wiggle traveling through them), and their widths swell and pinch organically over time (noise-driven). Each band is brightest at its center and fades toward darkened, softly antialiased edges, so the bands read as glowing columns separated by dark seams. Motion is slow and fluid — a gentle, lava-lamp-like drift with no abrupt changes.

## Algorithm

State kept between frames: a single time accumulator advanced by the frame delta in seconds, wrapping after about an hour. From it derive two drift clocks: one moving slowly in one direction (used for the horizontal-wiggle phase, at about a quarter of real time) and one moving the other way about twice as fast (used to animate the noise field).

Per pixel (2D renderer), all in the unit square:

1. Warp the vertical coordinate: subtract a modest fraction (roughly a third at full amplitude) of a 3D-ish perlin-noise sample taken at (a doubled-scale x, doubled-scale y, the faster drift clock) — this makes column widths vary organically along their length and over time. (The scale/amplitude constants are hand-tuned; treat them as "noise features a bit larger than a band".)
2. Warp the horizontal coordinate: add a small sideways displacement (about a tenth of the display width) equal to a sine of several times (the slow drift clock plus the warped vertical coordinate) — this is the traveling wave that makes bands snake.
3. Quantize the warped horizontal coordinate into N bins (N = configured column count). The bin index picks the hue.
4. Edge shading: measure how far the pixel sits from its bin's center (fractional position within the bin re-centered to the middle); brightness is one minus twice that distance, so it peaks at the bin center and hits zero at the seams. Raise it to a power slightly above one for a softer falloff. This darkens and antialiases the seams rather than drawing hard black lines (the shader this is based on used hard black edges; soft looks better at LED resolution).

1D renderer: renders each pixel by calling the 2D logic with x = position along the strip and a fixed y about a quarter of the way up — i.e., the strip shows one horizontal scanline of the 2D effect (bands become moving colored segments).

Layout assumptions: none beyond a normalized 2D map; strip length handled via normalization. No randomness beyond the deterministic noise field. Nothing hardcoded to pixel count.

## Colors

Evenly spaced hues around the color wheel, one per band, in wheel order horizontally (with the default band count: reads roughly as red, cyan-leaning green, blue-violet region, etc. — simply the wheel divided evenly). Saturation is high but a touch below full, giving slightly softened, vivid colors. Brightness ramps from full at band centers to black at seams.

## Controls

None exported by default, but the band count is an exported variable deliberately left ready to hook to a slider ("number of columns" concept, small integers; default four). A reimplementation should expose it as a slider.

## Timing

Slow: the sideways wave takes on the order of several seconds to tens of seconds per visible cycle; the width-breathing evolves on a similar slow timescale. Nothing pulses fast.

## Non-obvious details

- The whole effect is just two coordinate warps (noise on y, sine on x) applied before a simple quantize-and-shade step — the trick is warping the coordinates, not the colors.
- Warping y first and then feeding the warped y into the x-wiggle couples the two distortions, which keeps the seams looking continuous instead of shearing.
- Hue comes from the quantized bin, but brightness comes from the continuous pre-quantization position — that mismatch is what produces smooth shading inside crisply-colored bands.
