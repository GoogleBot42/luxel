# Bouncer3D
kind: 2D+3D (no 1D renderer; requires a mapped 2D or 3D display)
sensors: no

## What it looks like
A handful of small colored "balls" fly around inside the unit square (2D) or unit cube (3D), bouncing off the walls like a screensaver. Each ball has its own random hue; ball centers render white-hot, fringing out to the ball's saturated color at the edges, over a black background. Motion is straight lines with elastic wall bounces. At typical settings the balls cross the display in a second or two.

## UI controls (sliders)
1. **Ball count** — from one up to a fixed cap of about twenty.
2. **Ball size** — scales the ball radius from zero up to a modest fraction (roughly a fifth) of the display width. The 3D radius is kept several times (about four times) larger than the 2D radius to compensate for the sparser hit volume.
3. **Speed** — scales maximum ball speed; note that moving this slider also **re-randomizes all balls** (positions, velocities, hues), which reads as a "reshuffle" side effect. Preserve or fix as desired; preserving matches original behavior.

## Algorithm
State: a fixed-capacity pool of ball records, each holding a 3D position, a 3D velocity, and a hue. On startup (and on speed-slider changes) every active ball gets a uniformly random position in the unit cube, a random velocity in each axis, and a random hue.

Quirk to preserve or knowingly fix: initial velocity components are drawn from 0..max only — all balls initially drift toward the same corner until their first bounces decorrelate them.

Per frame (setup phase): each active ball's position advances by its velocity — note this is **per frame, not time-scaled**, so speed depends on frame rate; a faithful port may keep this, a better one scales by elapsed time. Wall handling: if any coordinate leaves the 0..1 box, clamp it to the wall and negate that axis's velocity; the original checks the axes in order and stops after the first wall hit per ball per frame (corner hits resolve over successive frames — acceptable imprecision for speed).

Per pixel: scan the active balls; for each, compare the pixel's coordinates to the ball center one axis at a time, bailing out as soon as any single axis distance exceeds the ball radius (cheap box rejection). If all axes are within the radius, the pixel belongs to this ball: take the **sum of the per-axis distances** (Manhattan distance) normalized by the radius, square it, and use that as the falloff parameter. Brightness is one minus that parameter (bright at center, dark at edge); saturation is the parameter scaled up several-fold (so the very center is desaturated/white and saturation reaches full within the inner fraction of the radius, giving a hot white core with a colored halo). Use the ball's hue; stop scanning after the first matching ball (no blending of overlapping balls — first in list wins). Pixels matching no ball are black.

The diamond-shaped (Manhattan) falloff inside a square/cube bounding test is what makes it cheap; visually it still reads as round dots at LED resolutions.

No pixel-count hardcoding; coordinates come normalized from the mapper. There is no 1D fallback — adding one (e.g. projecting onto x only) would be an easy extension but is not in the original.
