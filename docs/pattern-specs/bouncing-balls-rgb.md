# bouncing balls - rgb
kind: 1D
sensors: no

## What it looks like

Eight single-pixel "balls" bounce along the strip under simulated gravity, as if dropped from the far end onto the ground at the near end. Each ball is a fixed color spanning the rainbow (red, orange, yellow, green, cyan, blue, purple, magenta). Each bounce is lower than the last — the balls lose energy at slightly different rates so they quickly drift out of phase — and when a ball's bounces have decayed to almost nothing it instantly relaunches at full height. The overall feel is continuous, physical, slightly chaotic bouncing; a full drop from the top takes a couple of seconds, with bounces getting quicker and shallower until reset. Where two balls occupy the same pixel their colors add together.

## Algorithm

State per ball (persisting across frames): time of last ground strike, current upward launch speed (the "impact velocity" of the most recent bounce), current height, current pixel position, and a per-ball elasticity (coefficient of restitution). Elasticity starts just below unity for the first ball and decreases very slightly for each subsequent ball — this small spread is what desynchronizes them. There is also a whole-frame RGB accumulation buffer (three parallel arrays, one entry per usable pixel) and a running clock accumulated from frame deltas.

Per frame (before rendering):

1. For each ball, compute elapsed time since its last ground strike and evaluate the standard projectile equation: height = launch-speed x time plus one-half x (negative) gravity x time-squared. Gravity is gentle enough that a full drop from the top takes on the order of a couple of seconds. The initial launch speed is derived from the gravity and the full drop height so that a fresh ball exactly reaches the top of the strip.
2. If the height goes negative, clamp it to zero, multiply the launch speed by the ball's elasticity, and record now as the ground-strike time. If the launch speed has decayed below a small threshold, reset it to the full initial value (the relaunch).
3. Map height linearly onto the usable pixel range (floored to an integer index) and *add* the ball's color into the RGB buffer at that pixel. The eight colors are hardwired to ball indices: pure red, orange (mostly red with some green), yellow (roughly equal red and green), pure green, cyan (equal green and blue), pure blue, purple (mostly blue with some red), magenta (mostly red with some blue).

Per pixel (render): read the buffered RGB for the appropriate source index and emit it, then zero that buffer entry — the render pass doubles as the buffer clear, so no separate clearing loop is needed. All unoccupied pixels are black.

## Direction / symmetry modes

A constant in the code (not a UI control) selects one of four display modes:

- bounce from the head of the strip (identity mapping);
- bounce from the tail (index mirrored end-to-end);
- symmetric from both ends toward the middle: the simulation runs on half the pixel count and the second half mirrors the first;
- symmetric from the middle toward both ends: same half-length simulation, mirrored the other way.

In the two symmetric modes only half the strip's length is simulated and each buffer entry is read twice per frame (cleared only on its second read).

## Hardcoding and suggested fix

The ball count is a hardcoded constant, and the color table is a chain of per-index special cases valid only for exactly that count. Obvious generalization: make ball count a variable and derive each ball's color from a hue evenly spaced around the color wheel by ball index (skipping the last stretch of the wheel so the endpoints don't both look red). The direction mode would also naturally become a UI control instead of an edit-the-source constant.

## Colors

Fixed rainbow assignment, one hue per ball, fully saturated, rendered additively in RGB (overlaps blend toward white). Background is black.

## UI controls

None exposed; ball count, gravity strength, and direction mode are edit-in-source constants.

## Timing

Fresh ball: a couple of seconds per full drop; bounce intervals shrink geometrically with each bounce; a ball typically survives many bounces (tens of seconds) before relaunching, with lower-index balls (higher elasticity) lasting longer.
