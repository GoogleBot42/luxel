# Crosstown Traffic 2D
kind: 2D
sensors: no

## What it looks like
On a square matrix, many thin colored line segments — like headlights of cars on a grid of streets — glide across the display. Roughly half travel horizontally and half vertically, each in its own lane, all moving in the same direction along their axis at different speeds. Each "car" is a soft-edged bar of a single vivid color; hues are spread evenly across all the lines and the whole color assignment slowly rotates through the rainbow. Cars slide fully off one edge and re-enter from the other. Each crossing takes on the order of a couple of seconds (per-line speeds vary a few-fold). With the width control turned low the bars shrink to sparkling particle-like dashes; turned high they fatten until they overlap and flood the panel.

## Algorithm
Layout assumption: the matrix is assumed square — its width is taken as the square root of the pixel count. **This is hardcoded; the obvious fix is to expose width as a setting or derive it from the pixel map.** The number of lines defaults to twice the matrix width, and lanes are evenly spaced fractions of the display height at half-pixel pitch, each lane centered within its slot.

Setup (once per line): pick a random half-length between roughly a quarter and a half of the display span; pick a random speed multiplier from about half to two-and-a-half times the base speed; orientation alternates by index (even lines one axis, odd lines the other). (A random per-line phase offset is also generated but never used — an apparent leftover.)

Per frame, for every line:
- Hue = a global sawtooth color clock plus the line's index fraction, wrapped — so hues are fanned evenly across lines and all rotate together.
- Position: a per-line sawtooth clock (period = base line period times the line's speed multiplier) is stretched to sweep the segment's center from beyond one edge to beyond the opposite edge (the sweep range is about four times the unit span, centered), so the line fully exits before wrapping. All lines travel the same direction; variety comes only from speed and length.
- Endpoints are the center ± the half-length along the travel axis; the cross-axis coordinate is the lane position (segments are axis-aligned).

Per pixel: iterate over the lines in order. For a line flagged as "other orientation", the pixel's coordinates are swapped (transpose trick) so the same horizontal-segment math renders vertical traffic. Compute the true Euclidean distance from the point to the finite segment (three cases via dot products: nearest to one endpoint, nearest to the other, or perpendicular distance to the interior). If that distance is under the line-width threshold, output that line's hue at full saturation with brightness falling linearly from full at the centerline to zero at the width edge, and stop (first hit wins; earlier lines occlude later ones). If no line is hit, the pixel is black.

**Quirk to be aware of:** in the original, the coordinate swap is applied in place inside the per-pixel line loop and never undone, so after an odd-indexed line the swapped coordinates leak into the next even-indexed line; the effective orientation pattern alternates in pairs rather than strictly even/odd. Visually it still reads as crisscross traffic. A reimplementation should just cleanly transpose per line (or store per-line orientation and swap into temporaries); exact preservation of the leak is not important to the look.

State kept between frames: per-line arrays of endpoints, hue, half-length, speed multiplier, orientation. Randomness is used only at startup (lengths and speeds); motion afterward is deterministic clocks.

## Colors
Full rainbow: every line gets a fully saturated hue, evenly distributed across the set of lines, and the whole palette cycles continuously. Background is black. Bar edges fade to black linearly.

## Controls
- Slider "line width": thickness of the bars, scaled relative to matrix width and line count; low values give tiny particle streaks, high values give broad overlapping bands.
- Slider "line speed": sweep speed; mapped so moving the slider up increases speed, with a squared curve giving fine control at the slow end.
- Slider "color speed": how fast the global hue rotation cycles, same kind of inverse-squared mapping.

## Timing
Default feel: each car crosses the panel in a couple of seconds (varying per car by its random speed factor); the rainbow assignment drifts noticeably over several seconds to tens of seconds depending on the color-speed slider.

## Non-obvious details
- Vertical traffic is obtained for free by transposing the query point rather than writing separate vertical-segment math.
- Extending the sweep well past both edges (rather than 0–1) is what makes cars enter and exit smoothly instead of popping.
- The point-to-segment distance uses the standard two-dot-product endpoint test, so the bars have properly rounded ends.
