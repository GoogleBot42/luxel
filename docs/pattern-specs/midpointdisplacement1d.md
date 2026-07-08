# MidpointDisplacement1D
kind: 1D
sensors: no

## Visual behavior
A fractal "mountain range" silhouette rendered as brightness along the strip: peaks glow bright, valleys fade to black, with the jaggedness of real terrain. The terrain itself is static, but color flows across it — a compressed slice of the hue wheel scrolls through the height field over a couple of seconds, so ridgelines ripple with moving color. Every several seconds (adjustable) the terrain is thrown away and a fresh random mountain range appears instantly. With the roughness control low it looks like smooth rolling hills; high, like a spiky seismograph.

## Algorithm
### Terrain generation (at startup and on each regeneration)
Classic recursive midpoint displacement over a height array with one entry per pixel:
1. Give the two endpoints small random heights and draw a straight line between them.
2. Recursively: find the midpoint of the current segment, add a random vertical displacement, redraw straight lines from each end to the displaced midpoint, then recurse into both halves.
3. The random displacement at each recursion level is drawn uniformly from a symmetric range around zero whose half-width shrinks geometrically with depth, divided by a roughness factor raised to the level — larger roughness factor means displacements die off faster with depth (smoother terrain); a factor below one makes deeper levels wilder (very jagged).
4. Recursion stops at a maximum depth: the binary log of the pixel count, hard-capped at about seven levels for performance on the device.
5. Finally the whole height array is rescaled (min-max normalized) to the zero-to-one range.

### Per frame
- Advance a scroll phase (sawtooth; default full cycle around a couple of seconds).
- Accumulate elapsed time; when it exceeds the configured map lifetime (default several seconds), reset the timer and regenerate the terrain from scratch. A lifetime of zero means the map lives forever.

### Per pixel
- Brightness = the pixel's normalized height (valleys dark, peaks full).
- Hue = take (height + scroll phase) wrapped to one cycle, compress it by the palette-width fraction, then add the palette-offset base hue. Saturation is always full.

## Colors
Fully saturated hues from a user-chosen slice of the rainbow: offset picks where on the wheel the slice starts, width picks how much of the wheel it spans (narrow slice = near-monochrome shimmer; full width = whole rainbow draped over the terrain). Because hue is keyed to height, contour bands of equal altitude share a color, and the scroll makes those bands flow uphill/downhill.

## UI controls (six sliders)
- **Detail level**: fraction of the maximum recursion depth actually used; low = a few big triangular slopes, high = full fractal detail. Regenerates the terrain immediately.
- **Map lifetime**: how long each terrain lasts before regeneration, from instant-churn up to about half a minute (and the code path where it's zero means "forever"). Regenerates immediately when moved.
- **Speed**: color-scroll rate; inverted so sliding right speeds the flow up.
- **Palette width**: how much of the hue wheel the terrain spans.
- **Palette offset**: base hue of the palette slice.
- **Roughness**: the per-level displacement falloff described above, from very smooth to very jagged. Regenerates immediately.

Defaults are also exported as watchable variables for tinkering.

## Layout / scaling notes
1D; height array sized from pixel count, so it scales automatically. The depth cap of about seven levels is a device-performance guess; on a faster implementation it could simply be the log of pixel count.

## Non-obvious points
- All the expensive fractal work happens only at (re)generation; per-frame cost is just the timer and a cheap per-pixel add/wrap, which is why it can afford full recursion on-device.
- Reusing the height value for both brightness and hue index is what makes color track the terrain contours rather than scrolling independently of them.
- Regeneration is abrupt (no crossfade); a crossfade between old and new maps is an easy enhancement.
