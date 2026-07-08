# Bouncing Balls 2D
kind: 2D (only a 2D renderer; strips are handled by treating the layout as a one-column matrix)
sensors: yes (3-axis accelerometer from the sensor expansion board)

## What it looks like
One ball per column of a rectangular matrix, each a single white pixel,
bouncing under realistic gravity: dropped from near the top, each bounce
lower than the last, settling to rest at the bottom over several seconds.
Balls start slightly out of phase (random per-ball delays), so the wall of
balls ripples rather than moving in lockstep. Optionally a rainbow column is
drawn beneath each ball, rising and falling with it — like each ball dragging
a colored column of light. Shaking the device relaunches all balls to full
height. It reads as genuinely physical because it uses real projectile math
in real seconds.

## Algorithm
Layout: the matrix width is a hardcoded constant (a typical panel width,
sixteen); height is total pixels divided by width. For a bare strip, width is
meant to be set to one. Obvious fix: derive width/height from the pixel map
instead of hardcoding, or expose width as a control.

Per-ball state (arrays of length width): current height (in "meters", the
panel representing roughly one meter), current launch/impact velocity,
elapsed time since the last bounce, and a remaining start-delay.

Global state: gravity (negative acceleration), a bounce damping factor
(each bounce retains around nine tenths of the impact velocity), the launch
velocity needed to reach a nominal one-unit apex (computed from gravity via
the standard kinematic relation, i.e. square root of twice gravity times the
drop height), previous accelerometer sample, and a shake-debounce timer.

Reset (at startup, when gravity changes, and on shake): recompute the launch
velocity from gravity, then for every ball zero its height and bounce clock,
set its velocity to the full launch velocity, and give it a fresh random
start delay drawn uniformly up to a maximum set by the Randomness control
(up to about half a second at maximum).

Per frame:
- Read the accelerometer (a 3-element x/y/z array). Compute the vector
  magnitude. If the magnitude exceeds the sensitivity threshold and at least
  about a second has passed since the last trigger (a debounce accumulator,
  clamped so it cannot overflow), relaunch all balls.
- For each ball: if its start delay has not expired, count it down and skip.
  Otherwise advance its bounce clock by the frame delta (converted to
  seconds) and set its height by projectile motion: half gravity times time
  squared, plus launch velocity times time. If height went below zero, clamp
  to zero, multiply the launch velocity by the damping factor, and zero the
  bounce clock (a bounce). The source contains a disabled option: when a
  ball's velocity decays to nearly nothing, automatically relaunch that ball
  — recommended for installations without a sensor board (otherwise balls
  simply come to rest until shaken).

Per pixel: quantize the normalized x to a column and normalized y to a row,
flipping y so rows count up from the physical bottom. Convert the column's
ball height to a row number. If this pixel's row equals the ball's row, draw
pure white. Else if the rainbow option is on and the pixel is below the ball,
draw a fully saturated, full-brightness hue that varies with the pixel's
distance below the ball (scaled by panel height) plus a per-column offset
from a triangle wave of the normalized x — giving diagonal rainbow banding
across columns. Otherwise the pixel is dark.

Randomness: only the per-ball start delays.

## Colors
Balls: pure white. Beneath each ball (optional): a full rainbow, hue sliding
with height below the ball and staggered across columns. Background: black.

## Sensor inputs
A 3-axis accelerometer vector. Only the overall magnitude is compared against
a user-set threshold to detect a shake; a shake relaunches every ball, rate
limited to about once per second. (The frame code also computes per-axis
deltas from the previous sample, but they do not feed the trigger — the
magnitude test is what matters.)

## Controls
- Slider "Gravity": strength of gravity, from gentle (floaty, slow bounces)
  to roughly thirty times stronger (fast, snappy bounces). Changing it
  relaunches all balls.
- Slider "Motion sensitivity": the shake threshold; response is squared so
  the low end gives fine control. Small offset so it can never be zero.
- Slider "Show rainbow": acts as a toggle — on when past the midpoint.
- Slider "Randomness": maximum random start delay per ball (squared
  response), from perfectly synchronized to up to about half a second of
  stagger.

## Timing
Real-time physics in seconds: at default gravity (about Earth strength) a
full drop takes under half a second and the bounce train decays over a few
seconds. Shake retrigger limited to roughly once per second.

## Non-obvious points
- Heights are simulated in physical units against a one-unit ceiling, then
  scaled to rows only at render time, so panel resolution never affects the
  physics.
- Computing the launch velocity from gravity keeps the initial apex at the
  top of the panel no matter where the gravity slider is set.
- Per-frame ball simulation happens once per column, not per pixel; the
  per-pixel path is just quantize-and-compare, keeping it fast on large
  panels.
