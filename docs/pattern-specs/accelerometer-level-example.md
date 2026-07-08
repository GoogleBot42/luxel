# Accelerometer level example
kind: 2D
sensors: yes (accelerometer)

## Purpose
A tutorial pattern showing how to use the accelerometer plus the coordinate-transform API to keep a drawn shape level with gravity — a digital spirit level. It draws a rainbow-colored horizontal bar across the middle of a 2D panel; when you tilt the panel, the bar counter-rotates so it stays parallel to the ground.

## Sensor inputs
- The three-axis accelerometer vector from the sensor expansion (only two of the three axes are used — the two in the plane of the display).

## What it looks like
A bright band, roughly a fifth of the panel tall, running through the center of the panel. Along its length the band shows a full rainbow sweep of hues. Brightness peaks at the band's centerline and falls off smoothly to black at its edges. When the fixture is tilted, the band smoothly and somewhat sluggishly re-levels itself over a second or two rather than snapping instantly (deliberate smoothing to hide sensor noise). Background is black.

## State kept between frames
- A smoothed correction angle (radians).

## Per-frame work
1. Compute the instantaneous tilt angle as the two-argument arctangent of the two in-plane accelerometer components (gravity direction in the panel's plane).
2. Negate it (sign depends on how the sensor board is mounted relative to the LEDs — the original carries a comment saying you may need to flip this for your orientation; a reimplementation should expose or document that flip).
3. Low-pass filter it: blend the stored angle a small fraction of the way (a couple percent) toward the new measurement each frame. Note this blend is per-frame, not per-elapsed-time, so responsiveness varies with frame rate — a known simplification; scaling by elapsed time would be the obvious fix.
4. Rebuild the render transform from scratch each frame: reset it, translate coordinates so the panel center becomes the origin (so rotation pivots around the center, not a corner), then rotate by the smoothed correction angle.

## Per-pixel work (2D renderer)
After the transform, pixel coordinates are centered (roughly minus-a-half to plus-a-half in each axis) and rotated. For each pixel:
- Brightness: one minus the absolute vertical distance from the centerline scaled up by several-fold, clamped to the unit range, then **squared**. This yields a bar about a fifth of the panel tall with a soft falloff.
- Hue: the pixel's horizontal coordinate shifted back by half (undoing the centering), so hue runs through the whole wheel from one side of the panel to the other.
- Full saturation.

## Extra output
Exports a "gauge" style value (a function the host UI can display as a meter): one minus the magnitude of the correction angle as a fraction of a half-turn — reads full when perfectly level, lower as tilt grows.

## Colors
Full-saturation rainbow across the bar's length; black elsewhere.

## UI controls
None (beyond the read-only gauge).

## Timing
No animation of its own — all motion comes from physically tilting the device. The smoothing gives roughly a one-to-two-second settle time after a tilt change.

## Layout assumptions
Requires a 2D pixel map with unit-space coordinates. No pixel-count hardcoding.

## Non-obvious details
- The translate-then-rotate order is the key trick: without first moving the origin to the panel center, rotation would swing the bar around a corner.
- The two-argument arctangent of the in-plane gravity components directly gives the roll angle; no trig gymnastics needed.
- Smoothing state persists across frames, so power-on starts with the bar level to the panel and it drifts to true level over the first second or two.
