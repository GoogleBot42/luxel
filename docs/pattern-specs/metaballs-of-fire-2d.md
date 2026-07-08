# Metaballs of Fire 2D
kind: 2D
sensors: no

## What it looks like
A black field on which a handful of glowing fiery blobs drift around. As blobs approach each other their edges bulge toward one another, merge into a single larger blob, then split apart again as they separate — the classic "metaballs" look. Blobs bounce off the edges of the display like billiard balls. Motion is smooth and continuous; at default settings a blob crosses the display in very roughly ten seconds or so, adjustable by a speed slider down to a dead stop.

## Algorithm
State kept between frames: a small fixed-capacity pool of control points (capacity of about eight), each holding a 2D position and a 2D velocity, both in normalized display coordinates (the display is treated as a unit square, so nothing is hardcoded to pixel count — no fix needed). Only the first N of the pool are active, where N is set by a UI slider.

Initialization: each active point gets a uniformly random position inside the unit square and a random velocity whose components are uniformly distributed and can be positive or negative (centered on zero).

Per frame (before rendering): each active point advances by its velocity scaled by the speed setting. If a coordinate crosses a wall of the unit square it is clamped to that wall and the corresponding velocity component's sign is flipped. Deliberate shortcut: after handling the first wall hit for a point, the code skips checking the remaining walls that frame (corner cases self-correct within a frame or two). Note the frame-delta is not used, so motion speed is frame-rate dependent — the original accepts this.

Per pixel: compute a "metaball field" value. Start an accumulator at a ceiling of one. For each active point, multiply the accumulator by that point's Euclidean distance to the pixel and by a spread coefficient, then keep the minimum of that product and the previous accumulator. This is like a Voronoi nearest-distance field, except distances to successive points multiply into a running product instead of being compared independently — that product is what makes nearby blobs' fields combine so they visually fuse. (This is the clever bit of the pattern.)

Thresholding: if the resulting field value is at or above a small cutoff (roughly a twelfth of the unit scale, order-of-magnitude), the pixel is black. Below the cutoff the pixel is lit.

The spread coefficient is not user-set directly: it is recomputed whenever the point count changes, growing modestly with more points (from roughly one-and-a-half up toward two) so that blob size stays sensible as more field sources multiply in.

## Color
Lit pixels use a fiery ramp driven by how far inside the threshold the pixel is:
- Hue: proportional to (cutoff minus field value), so it sits in the red-to-orange band — deep red at blob rims, shading toward orange at blob cores. Never leaves the fire range.
- Saturation: always full.
- Brightness: a constant slightly above full minus a periodic wave of the field value (the wave's period is a small multiple of the field scale). This carves subtle concentric brightness banding inside each blob, giving a molten, shimmering interior rather than a flat disc. Background is pure black.

## UI controls
- Slider, "number of points": maps to an integer count from four up to the pool capacity (about eight). Changing the count also recomputes the spread coefficient and fully re-randomizes all point positions/velocities.
- Slider, "speed": scales the per-frame motion step from zero up to a modest maximum (blobs never move fast — the feel is lava-lamp-like drifting).

## Notes for implementer
- Requires a 2D mapped display; there is no 1D renderer.
- The distance-product accumulation must respect active-point order only in the sense that min() is taken at each step; final result is order-insensitive in practice.
- The early-exit wall bounce and the frame-rate-dependent speed are faithful quirks; reproducing exact physics is unimportant, the visual (soft blobs merging/splitting, wall bounces) is what matters.
