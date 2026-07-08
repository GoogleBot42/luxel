# Fast Palette Blending
kind: 1D
sensors: no

## What it looks like
A smooth multi-color gradient washes back and forth along the strip. Every
several seconds the entire color scheme cross-fades over a couple of seconds
into the next of three preset palettes, cycling round-robin forever. The
visible demo effect is simple; the pattern's real point is the palette
manager underneath (it is written as a reusable demo/library).

## Algorithm
Data: three gradient palettes, each stored as an ordered list of rows of
(position, red, green, blue) where positions ascend across the full unit
range. Values are normalized to the unit range at startup (the source data is
in byte range and divided down once).

Palette manager state kept between frames:
- index of the current palette and of the next one (cyclic successor);
- a flag: holding vs. transitioning;
- elapsed time within the current phase (accumulated from frame deltas, in
  seconds, wrapped to avoid overflow after about an hour);
- the current blend fraction;
- a working "active" palette: a fixed-size table of roughly sixteen evenly
  spaced entries in the same (position, r, g, b) row format. This table is
  registered with the engine as the palette that per-pixel palette-paint
  calls draw from.

Per frame:
- While holding: do nothing until the hold time (several seconds) elapses,
  then enter the transition phase and reset the phase clock.
- While transitioning: set the blend fraction to phase-elapsed divided by the
  transition duration (a couple of seconds); rebuild the active palette; when
  the transition duration elapses, advance both palette indices cyclically,
  reset the blend to zero, and return to holding.
- The active palette is rebuilt ONLY during transitions — during the hold
  phase no per-frame palette work happens at all, which is the "fast" in the
  name.

Rebuilding the active palette: for each of the ~sixteen evenly spaced sample
positions, look up the interpolated color at that position in the current
palette and in the next palette, then linearly mix the two colors by the
blend fraction, and store position plus mixed color as one row.

Palette lookup (a user-space reimplementation of the engine's palette-paint,
provided because two palettes must be sampled simultaneously): scan rows for
the first whose position is at or above the query; if the query falls at or
outside the ends, or hits a row exactly, return that row's color directly;
otherwise linearly interpolate each channel between the bounding rows in
proportion to where the query sits between their positions.

Also included (as library surface, unused by the demo render): a helper that
samples both palettes at a color position and emits the blended result
directly to the current pixel — for renderers that want per-pixel dual-palette
mixing without the intermediate table.

Per pixel (demo renderer): compute the pixel's normalized position along the
strip, add a triangle-wave oscillation driven by a cyclic timer with a period
of several seconds, take the fractional part, and paint that value from the
active palette. This produces the gradient sliding back and forth with wrap.

Randomness: none. Layout: any 1D strip; no hardcoding.

## Colors (qualitative palette stop lists)
1. Black through deep blue to vivid blue, then blue-violet to magenta,
   softening through pink-magenta to white.
2. A "landscape": near-black green, dark olive/earth, rich green, bright
   orange-gold (a sunlit band placed high in the range), a cooler medium
   green, then back to near-black green at the top.
3. Classic heatmap: black through red (red dominating the lower half),
   red to yellow, yellow to white at the very top.

## Controls
None. Hold time and transition time are code-level constants (several seconds
and a couple of seconds respectively), as is the active-palette resolution.

## Timing
Hold each palette around five-ish seconds; cross-fade over roughly two;
the back-and-forth wash cycles over several seconds.

## Non-obvious points
- The key optimization: blending cost is paid once per frame into a small
  fixed-size table (and only during transitions), while per-pixel work is a
  single engine palette lookup. Naive per-pixel dual-palette blending would
  be far slower.
- The end-of-range fast path in the lookup must clamp the row index so a
  query beyond the last row returns the last row's color.
