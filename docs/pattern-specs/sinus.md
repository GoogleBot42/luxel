# sinus
kind: 2D
sensors: no

This is a simple pattern; the spec is short on purpose.

## What it looks like
A single bright sinusoidal ribbon snaking horizontally across the matrix — a glowing sine curve with a soft colored halo around it, on black. The curve slithers sideways: its horizontal phase oscillates back and forth (it speeds up, slows, and reverses direction smoothly rather than scrolling at constant speed). The ribbon's color, and the halo around it, slowly cycle through the hue wheel.

## Algorithm
No state beyond two per-frame phase values.

Per frame:
- Phase A: take a very slow sawtooth, multiply it up so it wraps on the order of a minute, then pass it through a triangle/sine-shaped oscillator and scale to a few horizontal wavelengths of amplitude. This is the horizontal phase offset — because it is an oscillation of a ramp, the curve drifts back and forth with smoothly varying speed.
- Phase B: a slow sawtooth (period on the order of ten-plus seconds) used as a base hue offset.

Per pixel:
- Slightly stretch the vertical coordinate away from center (by a modest factor, roughly a third extra) so the wave's crests run a bit off the top and bottom edges — makes the curve fill the matrix better.
- Compute the sine-wave height at this column: a triangle/sine oscillator of (horizontal coordinate + phase A), scaled so about three full wave periods span the display width.
- Brightness: one minus the vertical distance from the pixel to that curve, with a steep falloff (distance doubled before subtracting), clamped to non-negative — gives a fairly narrow bright band along the curve.
- Hue: the same distance but with a gentle falloff (distance halved), clamped, plus the cycling base hue — so hue varies with distance from the curve, tinting the halo differently from the core, and the whole scheme drifts around the color wheel.
- Saturation: always full.

## Controls
None.

## Timing
Side-to-side slither: reverses over tens of seconds. Hue cycle: ten-to-twenty-second feel.

## Layout assumptions
Normalized 2D coordinates only; no hardcoding. Degrades fine on any 2D map.
