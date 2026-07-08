# 2D canvas example
kind: 2D
sensors: no

## Purpose
This is a tutorial pattern demonstrating an "offscreen canvas" technique: draw into a small in-memory pixel buffer once per frame, then sample that buffer per-pixel during rendering. The visible effect is a single bright dot traveling around a circle, leaving behind a fading, slowly color-cycling trail.

## What it looks like
On a 2D matrix, one bright dot orbits clockwise-or-counterclockwise around an invisible circle centered in the panel, with a radius of roughly a third of the panel size. As it moves it leaves a comet trail that decays smoothly to black over roughly a second or so. The dot's hue drifts slowly around the color wheel, so the trail is a gradient of recent hues — the head is the newest hue, the tail shows slightly older hues fading out. One full orbit takes on the order of a second; a full trip around the color wheel takes modestly longer than one orbit, so the orbit's color shifts noticeably lap to lap.

## Data structures / state kept between frames
- Two flat arrays acting as a small square canvas (one entry per canvas cell): one array of brightness values, one array of hues. Cells are packed row-major (column index plus row index times width).
- The canvas is hardcoded to a small square (8 cells on a side in the original). **Obvious fix:** derive the canvas dimensions from the actual mapped matrix size, or expose them as variables/controls, rather than hardcoding.

## Per-frame work (before rendering)
1. Multiply every brightness cell by a decay factor a little below one (around ninety-five percent). This is applied once per frame regardless of frame duration — the original even has a comment admitting the fade should be scaled by elapsed time for frame-rate-independent decay; a reimplementation may fix that.
2. Compute the dot's position: an angle that advances steadily through a full turn (driven by a sawtooth timer with a period around a second), converted with sine/cosine to a point on a circle of radius one-third, centered at the middle of unit coordinate space.
3. Convert that (x, y) in unit coordinates to a canvas cell index (floor of coordinate times dimension, row-major). If the index is in range, set that cell's brightness to full and set its hue cell from a second, slower sawtooth timer (period roughly one-and-a-half times the orbit period).
4. Commented-out (inactive) code shows an optional variant where out-of-range coordinates wrap around to the opposite edge instead of being skipped.

## Per-pixel work (2D renderer)
For each physical LED with unit-space coordinates (x, y): compute the same row-major canvas index from (x, y), look up hue and brightness from the two arrays, and emit HSV color at full saturation with the brightness **squared** (squaring makes the trail decay look perceptually smoother and darkens the dim tail). The renderer ignores the pixel index passed in and relies purely on mapped coordinates, so the canvas can be resampled onto any layout or density.

## Colors
Full-saturation rainbow: the hue slowly cycles through the entire wheel over time. No fixed palette; background is black.

## UI controls
None.

## Randomness
None — fully deterministic.

## Non-obvious details
- The decoupling of "draw resolution" (fixed small canvas) from "display resolution" (whatever the map provides) is the whole point of the example.
- Brightness is squared only at display time; the stored canvas keeps linear values so the geometric per-frame decay behaves predictably.
- Bounds checking before writing the dot prevents out-of-range array writes when the dot's coordinates land outside unit space (they don't in the default motion, but the guard is part of the demonstrated idiom).
