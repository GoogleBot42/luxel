# Shimmer Crossfade 2D
kind: 2D
sensors: no

## Overview / what it looks like
A demo/framework pattern for a 2D matrix that cycles through three distinct sub-patterns, spending several seconds on each. Instead of a conventional brightness or color blend between patterns, transitions are done by *stochastic dithering*: during a transition window, each pixel randomly decides — per pixel, per frame — whether to be drawn by the outgoing pattern or the incoming one. The probability of picking the incoming pattern rises smoothly from zero to one across the transition, so the change reads as a sparkling, shimmering dissolve from one scene into the next. Outside transition windows, exactly one sub-pattern is shown cleanly.

The three sub-scenes it cycles between:
1. **Rotating white line** — a bright white bar sweeping around the matrix like a radar arm / rotating diameter.
2. **Rainbow plasma rings** — soft, colorful interference blobs/circles that drift and breathe, cycling through the full rainbow.
3. **Rotating checkerboard** — a hard-edged checkerboard that spins about the matrix center while zooming in and out, tinted with a slowly drifting diagonal rainbow gradient.

## Top-level algorithm
Configuration (constants, easily exposed as controls):
- Dwell time per sub-pattern: on the order of several seconds each.
- Fraction of each dwell slot spent crossfading: roughly the last third of each slot.
- Number of sub-patterns: three, held as two parallel arrays of function references — one array of per-frame setup functions, one array of per-pixel renderers. This is the extensible part: to add a pattern, paste it in, de-conflict its globals, and register its setup and renderer as entries in these arrays.

Per frame:
- Compute a master clock that ramps continuously from zero to the pattern count over the full cycle (dwell time × pattern count), so its integer part is the current mode index and its fractional part is progress within the current slot.
- From the fractional part, compute "progress into the crossfade": zero for most of the slot, then rising linearly from zero to one across the final crossfade fraction of the slot.
- Call *all* sub-patterns' per-frame setup functions every frame (a noted inefficiency; with many patterns you would only call the two that can be visible).

Per pixel:
- Draw a Bernoulli sample: with probability given by an eased (smoothed, S-curve-like) version of the crossfade progress, add one to the master clock before taking its integer part; otherwise add nothing. Take the result modulo the pattern count. This yields either the current mode's index or the next mode's index.
- Dispatch to that mode's per-pixel renderer, which sets the pixel color itself.

The easing means the dissolve starts and ends gently rather than popping. Because the random choice is re-drawn every frame for every pixel, mid-transition frames are a twinkling salt-and-pepper mix of both scenes — that is the "shimmer". State kept between frames is just the sub-patterns' own animation phases; the dither itself is memoryless.

Cleverness worth preserving: this composition technique requires no color-space blending and almost no rewriting of the source patterns.

## Sub-pattern 1: rotating white line
Per frame: derive a rotation angle sweeping a full turn over the dwell time; convert it to a line slope via tangent, clamped to a large-magnitude bound so squaring it later cannot overflow.
Per pixel: compute the perpendicular distance from the pixel to a line of that slope passing near the matrix center (standard point-to-line distance formula). Brightness is full on the line and falls linearly to zero at a small distance (about a fifth of the matrix width), then is squared to sharpen the falloff. Color: pure white (zero saturation). Note: because the slope comes from a tangent, the visual effect is a line rotating through all orientations.

## Sub-pattern 2: rainbow plasma
Per frame: maintain two phase angles advancing at different rates (both cycling over a few seconds, one roughly twice as slow as the other), plus a spatial zoom factor that breathes slowly (over roughly ten-plus seconds) between about unity and several times that.
Per pixel: evaluate a classic plasma field — average of a sine of (x scaled by zoom, offset by phase one) and a cosine of (y scaled by zoom, offset by phase two), normalized to the unit range. That single field value is used **both** as the hue (full rainbow, fully saturated) and, after cubing and halving, as the brightness. Cubing crushes the dim regions to black so the result reads as glowing colored rings/blobs on a dark field rather than a wall of color.

## Sub-pattern 3: rotating checkerboard
This one does everything per pixel (no per-frame state).
- Derive a rotation angle completing a full turn over several (roughly eight) seconds. Translate pixel coordinates so the matrix center is the origin, apply a standard 2D rotation, then shift back (the re-shift is deliberately uneven between axes, which offsets the checker pattern off-center — harmless quirk).
- A second, faster clock (a few seconds per cycle) drives both a zoom and a color drift: the visible number of checker blocks breathes between roughly half a block and a few blocks via a triangle-like wave.
- Cell parity: scale the rotated coordinates by the block count, floor each axis, and take the parity of their sum — pixels in "on" cells are lit at full brightness, others are black.
- Hue for lit cells is a gentle diagonal gradient (proportional to the sum of the rotated coordinates, divided down so only a slice of the rainbow is visible at once), slowly sliding over time.

## Layout assumptions
Requires a 2D mapped layout with coordinates normalized to the unit square (designed on a small square matrix, but any 2D map works). No pixel-count hardcoding.

## Controls
None exported. Natural additions: sliders for dwell time and crossfade fraction.
