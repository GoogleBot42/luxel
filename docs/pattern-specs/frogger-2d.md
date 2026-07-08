# Frogger 2D
kind: 2D
sensors: no

## What it looks like
On a square LED matrix, the display is divided into horizontal "lanes" — one lane per pixel row (for the default configuration). In each lane a short, glowing colored bar slides horizontally back and forth, like traffic lanes in the arcade game Frogger. Each lane's bar moves at its own speed, so the lanes quickly fall out of step with each other. Bars slide fully off one edge before reversing, so at any moment some lanes are empty. The bars are rainbow-colored — hue varies steadily from lane to lane so the full set spans the color wheel — and the whole rainbow slowly drifts/cycles over time. Bars have soft edges: brightest along their center line, fading to black at their fringes. Background is black. With the width control turned low the bars shrink to sparse particle-like dots; turned very high they fatten until neighboring lanes merge into a wash.

## Layout assumptions
The pattern assumes a square matrix: it derives the matrix side length as the square root of the total pixel count, and uses that both as the default number of lanes and in the line-width math. Obvious fix: accept the actual mapped width/height (or a lane-count control) instead of assuming a perfect square. Rendering itself is resolution-independent (it works in normalized 2D world coordinates), so only the defaults depend on this assumption.

## State kept between frames
- A fixed number of lanes (default = matrix side length). There is a cap equal to that default.
- Per lane: a random speed multiplier, chosen once at startup, roughly uniform between about one-half and two-and-a-half times the base speed. (The original also generates a per-lane random phase offset at startup but never uses it — it's dead state; a reimplementation may drop it or, better, actually use it to de-synchronize lanes at t=0.)
- Per lane, recomputed every frame: the segment's two endpoints and its current hue.

## Per-frame work (before rendering)
1. Sample a global sawtooth clock whose period is set by the color-speed control; this is the base hue for this frame.
2. For each lane:
   - Hue = base hue plus the lane's index as a fraction of the lane count, wrapped into the unit hue circle. This yields the rainbow spread across lanes, all drifting together.
   - Horizontal position: take a triangle wave (0→1→0) of a per-lane clock whose period is the base movement period times that lane's random speed multiplier, then scale and shift it so the bar's center sweeps well past both edges of the unit square (from roughly one-and-a-half widths off the left edge to the same distance off the right). This makes each bar exit completely, pause off-screen conceptually, and glide back.
   - The bar is a horizontal segment of fixed length (a large fraction of the display width — a bit under half the sweep amplitude, roughly 0.8 of the unit width from end to end) centered at that position.
   - The lane's vertical position is the center of its row band: lane index divided by lane count, plus half a band.

## Per-pixel work
For each pixel (given its normalized 2D coordinates), loop over the lanes in order. Compute the pixel's distance to the lane's line segment — true point-to-segment distance: if the point projects beyond either endpoint use the distance to that endpoint, otherwise the perpendicular distance to the segment's line (standard dot-product/cross-product segment-distance formulation). If that distance is under the current line-width threshold, light the pixel with the lane's hue at full saturation, with brightness falling linearly from full at the segment to zero at the threshold, and stop (first matching lane wins — lower-numbered lanes occlude higher ones). If no lane matches, the pixel is black.

## Controls
- Slider, "line width": sets the glow radius around each bar. Mapped so small values give thin, sparse, particle-like traces and large values give fat bars that can fill lanes; the mapping also folds in the lane count and matrix size so the default lands around a thin-but-visible line (about a tenth of one row's height).
- Slider, "movement speed": sets the base back-and-forth period. Inverted and quadratically eased, so the top of the slider is fast and the low end very slow; full range runs from a couple of seconds per sweep to on the order of a minute (each lane then multiplies this by its own random factor).
- Slider, "color speed": sets the hue-cycle period, same inverted quadratic feel; ranges from a fast few-second rainbow churn to a several-minute slow drift.

## Timing feel
Defaults: a bar takes several seconds to tens of seconds to cross the display (varying per lane), and the rainbow completes a cycle over tens of seconds.

## Notes
Nothing algorithmically exotic; the character comes from (a) per-lane random speed multipliers, (b) sweeping beyond the edges so bars vanish entirely, and (c) the soft anti-aliased segment rendering via exact point-to-segment distance.
