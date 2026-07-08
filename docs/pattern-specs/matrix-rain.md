# matrix rain
kind: 2D
sensors: no

## What it looks like
Single bright dots "rain" down the columns of a wide, short LED matrix, Matrix-movie style but minimalist: each drop is one lit pixel (no trailing streak). Drops are cool colors — greens and cyans through blue — each drop with its own randomly picked hue and fall speed. Drops start bright at the top and dim as they approach the bottom, fading to nearly nothing before they vanish and the column becomes free for a new drop. At any moment a scattering of columns are active; new drops appear at random columns at a steady trickle.

## Algorithm
This is a simple pattern. State kept between frames, per column of the matrix:
- the drop's current vertical position in row units (a sentinel value marks "no drop in this column"),
- the drop's speed,
- the drop's hue.

Per frame:
1. Every active drop's position advances by its speed. Note: the advance is **per frame, not time-scaled** — the original does not multiply by elapsed time, so fall speed depends on frame rate. An obvious improvement is to scale the step by the frame delta so speed is consistent across devices.
2. If a drop's position passes the bottom row, the column is marked empty.
3. With a chance a bit under half per frame, one random column is picked; if it's empty, a new drop spawns at the top with a random speed (spanning roughly a two-to-one ratio between slowest and fastest, each step covering a fraction of a row per frame) and a random hue drawn from a band of the color wheel covering green-cyan through blue.

Per pixel: convert the pixel's normalized 2D coordinates to integer column and row. The pixel lights only if its row equals the integer part of its column's drop position; otherwise it is black. When lit, hue and full saturation come from the column's drop, and brightness is computed from the pixel's normalized vertical position: a linear ramp that is full near the top and reaches zero somewhat above full travel... more precisely, brightness falls from full at the top to zero as the drop descends, then that ramp is raised to a high power (roughly fourth power), making the fade perceptually smooth and leaving the bottom third quite dim.

## Layout assumptions
The matrix width and height are **hardcoded** (a wide 4:1-ish matrix, a few dozen columns by a handful of rows). The renderer itself uses normalized coordinates, so the obvious fix is to derive width/height from the actual mapped display dimensions (or expose them as controls) instead of constants; everything else already works in normalized space.

## Colors
Black background. Drops: fully saturated cool hues from spring-green/cyan through azure blue, one hue per drop. No reds/yellows ever appear.

## Controls
None.

## Timing feel
A drop takes on the order of a second or a few to cross the matrix (frame-rate dependent, see above). Spawns happen every few frames, so the field stays sparsely but continuously populated.

## Notes
Trivial-to-moderate complexity; the only subtlety is the per-column single-slot drop pool and the position-based brightness falloff standing in for a trail.
