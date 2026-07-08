# KITT
kind: 1D
sensors: no

## What it looks like
The classic Knight Rider / Cylon scanner: a single bright red "eye" sweeps from one end of the strip to the other, bounces off the end, and sweeps back, forever. Behind the moving head is a red comet tail that fades to black. One full end-to-end sweep takes a bit under a second regardless of strip length; the tail takes on the order of a second to fully extinguish, so on most strips the tail stretches a good fraction of the strip behind the head.

## State kept between frames
- A per-pixel brightness array, one entry per LED (persistent trail buffer).
- A fractional head position (can be between pixels).
- A direction flag (+/-).

## Per-frame work
1. Advance the head position by elapsed-time times a speed. The speed is **proportional to the pixel count** divided by a constant, which makes the sweep take a fixed wall-clock time (just under a second end-to-end) no matter how long the strip is. This is elapsed-time-scaled, so it is frame-rate independent.
2. If the head passes either end, clamp it to that end and reverse direction.
3. Set the brightness of the pixel under the head (integer floor of the fractional position) to full.
4. Decay every pixel's brightness by a small constant times the elapsed milliseconds (linear decay, clamped at zero). The decay rate is such that a pixel fades from full to black in roughly one-and-a-half seconds.

## Per-pixel work (1D renderer)
Look up the pixel's brightness from the trail buffer, **cube it**, and emit HSV at the red end of the hue wheel, full saturation, with the cubed value as brightness. Cubing turns the linear decay into a perceptually sharp comet: a hot head with a rapidly dimming tail.

## Colors
Pure saturated red only, fading through dim red to black. No other hues.

## UI controls
None.

## Timing
Sweep period: just under a second per one-way pass. Tail persistence: on the order of a second to a second and a half.

## Layout assumptions
Uses only the runtime-provided pixel count; no hardcoding. Works on any 1D strip length. Trail buffer must be sized to the pixel count.

## Non-obvious details
- The constant wall-clock sweep time (speed scaled by strip length) is the main design decision — short and long strips both feel the same.
- Only one pixel per frame is stamped to full brightness; at very high sweep speeds relative to frame rate the head can visually skip pixels (acceptable in the original; a reimplementation could stamp the span crossed since last frame if it wants to be fancier, but matching the original does not require it).
