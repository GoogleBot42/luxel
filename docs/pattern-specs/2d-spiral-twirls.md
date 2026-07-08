# 2D Spiral Twirls
kind: 2D
sensors: no

## What it looks like

A rotating pinwheel/spiral centered on a 2D display. One to a few "arms" of
color sweep around the center; a twist component periodically winds the arms
into a spiral, relaxes them straight, then winds them the opposite way, so the
figure alternates between a straight pinwheel and left/right-handed spirals.
Each arm is a rainbow-ish gradient along its width with a crisp dark seam on
one edge, fading toward the display's outer corners. The whole figure rotates
continuously, and the palette drifts around the hue wheel over time. With all
speed sliders at their slow end the figure freezes into a static spiral. The
author recommends a matrix of at least moderate resolution (roughly sixteen
pixels per side) for it to read well.

## Algorithm

No persistent state; three values are sampled once per frame from repeating
clocks:
- a **twist** amount: a sinusoidal wave of one clock, remapped to the range
  minus-one..plus-one;
- a **rotation** phase: a linearly repeating clock ramp;
- a **color drift** phase: another linear ramp on its own clock.

Per pixel:
1. Recenter the coordinates on the display middle and scale so the display
   half-width is one (edges reach one, corners a bit past it). Compute the
   polar radius, and the polar angle normalized to the unit range (this needs
   a proper full-quadrant arctangent; the original had to hand-roll one
   because the runtime's builtin was broken at the time — a reimplementation
   should just use a correct atan2).
2. Add to the angle a term proportional to radius times the twist amount
   (scaled down by half). Radius-proportional angular offset is what bends
   straight arms into spirals; the sign flip of the twist wave is what
   alternates the spiral's handedness.
3. Form a base value: angle times the arm count, minus the rotation phase,
   plus a constant offset to keep it positive; keep only the fractional part.
   This fraction is the "position across the arm" and repeats once per arm —
   it serves as both the arm-local hue ramp and the brightness profile.
4. Brightness = (slightly more than one, minus the radius) — a linear radial
   falloff that keeps the very edge just visible — multiplied by a shaping of
   the arm fraction: below the halfway point the fraction is cubed (deeply
   dark), above it it is used linearly (ramping to bright). The result is a
   comet-like profile across each arm: a dark seam, a slow rise, a bright
   trailing half, and a hard bright-to-dark edge where the fraction wraps.
5. Hue = the arm fraction plus a user-set base color, compressed to half the
   hue wheel, plus the continuously advancing color drift. So each arm spans
   about half the rainbow at any instant, and the whole palette slides around
   the wheel over time. Saturation is always full.

## Controls (all sliders)

- **Twist speed:** how fast the arms wind and unwind. Inverse-period mapping:
  fully right oscillates in about a second; toward the left it slows
  drastically, and at the extreme the twist effectively stops. (Note: the
  original's explicit zero-guard on these three speed sliders is buggy — a
  mistyped comparison makes it divide by the slider value even at zero,
  which "works" only because it yields an effectively infinite period.
  Reimplement deliberately: slider at zero = that motion frozen.)
- **Rotation speed:** how fast the whole figure spins; same inverse mapping
  and zero-freezes behavior.
- **Initial color:** sets the palette's base position on the hue wheel; the
  visible starting hue when color drift is frozen.
- **Color speed:** how fast the palette drifts around the wheel; same inverse
  mapping and zero-freezes behavior.
- **Arms:** number of symmetric arms, snapped to an integer, ranging from one
  to a few (about three at maximum).

## Timing feel

At mid slider settings: twist oscillation over several seconds to tens of
seconds, rotation similar, color drift slowest. All three are independent, so
the composite motion rarely repeats exactly.

## Layout assumptions

Requires 2D coordinates in the unit square; no pixel-count hardcoding.
Center pixel sits at radius zero where the angle is undefined — any consistent
convention there is fine (it is the brightest point regardless).
