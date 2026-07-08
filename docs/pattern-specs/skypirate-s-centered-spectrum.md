# SkyPirate's Centered Spectrum
kind: 2D
sensors: yes (audio spectrum from the sensor expansion board)

## Important caveat: install-specific pattern
This is a commissioned pattern hard-tailored to one specific installation: two controllers (a leader with the sensor board and a follower), each driving three tall vertical LED columns of a fixed, large per-column pixel count, with matching custom pixel maps (commented in the source). It bakes in that geometry in several places (see "Layout assumptions"). A clean reimplementation should generalize it as suggested there, keeping the visual design.

## What it looks like
A vertical-bar spectrum analyzer where each column is one frequency bar, but instead of growing up from the bottom, every bar grows symmetrically outward from the vertical center line — up and down at once, like a mirrored VU meter. Bars are solid-colored, whitening toward their tips (saturation falls off toward the outer extremes). Each bar has a falling "peak" marker: a single distinctly-colored (red, full brightness) pixel row that gets pushed out by the bar and then sinks back toward the current level at an adjustable rate, classic spectrum-analyzer style. An automatic gain loop keeps the tallest bar hovering near a target fill regardless of how loud the room is, adapting over several seconds.

## Sensor inputs
- A multi-band audio spectrum array (32 bands, low to high frequency) exported so the sensor board / leader fills it each frame. That is the only sensor input.
- On the follower controller the array arrives via the leader's sync mechanism; the pattern also reads the node's identity (leader vs follower index) to decide which frequency bands and hues that controller displays.

## Per-frame algorithm
State kept between frames: current bar levels, per-bar peak positions, a rolling average of the recent maximum bar level, and the auto-gain integrator.

1. **Auto-gain (PI controller)**: sensitivity = a proportional-plus-integral controller fed the error between a target maximum fill (slider, default near full) and the rolling average of the recent frame maxima; clamped integrator, and sensitivity never drops below unity. The rolling average is an exponential moving average with a time constant of a few seconds, so the meter adapts to level changes over roughly five to fifteen seconds.
2. **Peak fall**: each bar's stored peak height decays toward the bar's current height by a fixed percentage of the remaining gap per frame (percentage set by a slider), then is floored to a whole pixel row.
3. **Bar levels**: each physical column maps to a global bar number = local column index + (node index × columns-per-node), so the leader shows the low-frequency bars and the follower the high-frequency ones. Each bar's power is the average of a contiguous range of spectrum bins; the ranges are hand-chosen and widen toward high frequencies (roughly geometric growth: the lowest bar averages a couple of bins, the highest averages several), approximating a log-frequency layout. Before averaging, a silence threshold (slider) is subtracted from every bin and negatives clipped to zero — this stops the meter twitching on line-in noise floor, and inherently affects the energy-poor high bins most. The averaged power is scaled by the auto-gain sensitivity, clamped to unit range, and converted to a bar height in pixel rows (out of half the column, since the display mirrors both ways). Peaks are kept at least at one row below the new bar top, and the frame's maximum power feeds the rolling average.
4. The coordinate transform is shifted so the map's vertical range is centered on zero (this supported the original center-mirrored math; a reimplementation can just use distance from the vertical center).

## Per-pixel rendering
For each pixel, determine: which column it is in, and its distance in rows from the vertical center line (the mirroring is just "absolute distance from center"). Then:

- Lit if its distance from center is below the bar's current level, else dark.
- Hue: a fixed per-column hue from a small hand-picked table keyed by node: the low-frequency half uses red through pinkish reds; the high-frequency half uses magenta through purple/violet. (A commented-out alternative scrolled hues over time and position; a slow hue-scroll clock is still computed each frame but unused — omit it or wire it to an optional mode.)
- Saturation: precomputed per pixel as a gentle falloff with distance from center — full saturation at the center line, bleaching to white at the extreme tips (a fractional-power curve, so the whitening is concentrated near the very ends).
- Peak marker: if this pixel's row distance equals the bar's peak position, override with the peak hue (red) at full brightness.

## Colors (qualitative)
Bars: a warm-to-cool progression across the frequency axis — red, then pinkish reds on the low side; magenta, then purples/violets on the high side — each bar a single hue, whitening at the tips. Peak dots: red. Background: black.

## UI controls (all sliders)
- **Silence level**: noise-floor threshold subtracted from all bins; raise it to calm the meter during quiet input.
- **Fill**: the target maximum-bar fill the auto-gain aims for.
- **Peak drop speed**: how fast peak markers sink (from quite slow to quite fast, as a per-frame fraction of the gap).

## Layout assumptions and suggested fixes
Hardcoded in the original, all of which should be generalized:
- Columns per controller (three) and the two-controller split; generalize to a configurable column count and drop the node split (or make bands-per-node configurable).
- Column height and the pixel-index→(column, row-from-center) conversion are precomputed lookup tables assuming exact column lengths (including one literal per-column pixel count); generalize by deriving column and vertical position from the pixel map coordinates instead.
- The bin-range table assumes exactly six bars; generalize by computing log-spaced bin ranges from the bar count (the original's commented-out generic version did a log mapping).
- Peak decay is per-frame rather than per-second, so it is frame-rate dependent; a reimplementation should scale it by elapsed time.

## Non-obvious bits
- The PI-controller auto-gain (borrowed from a stock sound pattern) is the key to it looking good at any volume; without it the bars pin or vanish.
- Subtracting the noise floor *before* averaging bins keeps quiet high-frequency bins from accumulating hiss into visible bars.
- Precomputing the per-pixel column/row/saturation tables was a deliberate frame-rate optimization for thousands of pixels; the same effect can be had by caching or by cheap arithmetic on map coordinates.
