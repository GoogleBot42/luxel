# 2D Wandering Fireball
kind: 2D
sensors: no

A simple pattern; short spec.

## What it looks like

A soft glowing ball, several pixels across, drifting around a 2D display in a
smooth wandering path that never exactly repeats the same loop. The ball's
color slowly cycles through the whole hue wheel (tens of seconds per lap),
with a slightly hue-shifted hot core at its center. The rest of the display
is not black but a very dim, moderately saturated wash of the same cycling
hue.

## Algorithm

No persistent state; everything derives from three free-running repeating
clocks read once per frame, each with a different period of a handful of
seconds to tens of seconds:
- The ball's x position follows a triangle wave of the first clock (linear
  back-and-forth).
- The ball's y position follows a sinusoidal wave of the second clock
  (eases at the edges).
- The third, slowest clock drives the hue.

Because the x and y periods differ (roughly a two-to-three ratio), the path
is a Lissajous-style wander that takes a long time to repeat.

Per pixel: compute a triangular "closeness" profile independently in each
axis — one minus the normalized distance from the pixel's coordinate to the
ball's coordinate on that axis, clamped to zero beyond a tolerance of a
little under half the display width. Multiply the two profiles together to
get the ball intensity (the product makes a rounded diamond-ish blob rather
than a plus-shape).

Where the product exceeds a small threshold, draw the ball: brightness rises
with the product (offset down slightly so the fringe is dim), saturation
rises with it too (offset up so it is always fairly saturated), hue is the
slow clock — except in the innermost core (product above a high threshold)
where the hue is nudged forward a small fraction, giving a subtly different-
colored hot center. Everywhere else, draw the background: same hue, medium
saturation, very low fixed brightness.

## Controls

None.

## Layout / notes

Requires 2D coordinates in the unit square; no pixel-count assumptions. The
tolerance constant makes the ball's size a fixed fraction of the display, so
it scales with resolution automatically.
