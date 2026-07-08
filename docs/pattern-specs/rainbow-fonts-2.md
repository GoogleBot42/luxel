# rainbow fonts 2
kind: 1D
sensors: no

This is a simple pattern; short spec.

## What it looks like
Concentric rainbow bands mirrored around the middle of the strip, continuously
rippling as hues cycle. The whole mirrored structure also sways slowly side to side,
so the center of symmetry drifts back and forth around the strip's midpoint. Rendered
at full saturation but deliberately dim (roughly a fifth of full brightness).

## Algorithm
No state persists between frames beyond the global clock.

Per frame:
- A fast sawtooth phase from the clock (several seconds per full hue cycle).
- A slow sinusoidal offset (on the order of ten seconds per sway) whose amplitude is
  about a tenth of the strip length, computed both in pixel units and as a fraction of
  the strip.

Per pixel:
1. Compute a symmetric ramp: one minus the normalized distance from the (offset)
   strip center, so it peaks at the swaying center and falls to zero at both ends.
2. Fold that ramp through a sine-shaped zero-to-one wave, then through a second such
   wave after adding the time phase plus the normalized sway offset. The double
   folding turns the single ramp into multiple mirrored bands that compress and
   expand nonlinearly.
3. Use the result directly as hue, full saturation, fixed dim brightness.

## Colors
Full rainbow; every hue appears. No blacks — the strip is always fully lit, just dim.

## Controls
None.

## Layout assumptions
Pure 1D, works for any strip length (everything is scaled by pixel count).

## Timing feel
Hue ripple: a handful of seconds per cycle. Sway: slower, around ten seconds per
back-and-forth.
