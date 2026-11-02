# a big hit

Idea spec written from Patternflow preset preset_a_big_hit.h (CC BY-SA 4.0 per its SPDX header, by Seunghun Lee; the brief said MIT); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A "liquid neon" plasma: thin, intensely bright ridges of saturated colour snaking across a mostly dark panel, like neon tubes bent by heat shimmer or caustic lines on the bottom of a pool. The ridges continuously flow, merge, split and pinch off as the underlying fields drift. Hue sweeps diagonally across the panel (a gentle rainbow gradient from one corner to the other) with small extra shifts along the ridges, and the brightest ridge centres desaturate slightly toward white, giving them a hot core. With the chaos knob at zero the ridges are smooth, almost regular interference lattices; turning it up warps them into tangled, kinked, organic squiggles. The scale knob zooms the whole field from broad slow curves to dense fine lacework that fills the panel.

## The idea

Sum several travelling sine fields, two of which are domain-warped by other sines, and light only the places where the sum crosses zero — the absolute value inverted and sharpened. Zero-crossings of a smooth field are thin, continuous, always-connected curves, which is why it looks like neon tubing.

## Algorithm in words

- State: a time accumulator.
- Per pixel: scaled coordinates (by the scale control). Four fields: a sine of x travelling with time; a cosine of y travelling the other way; a sine of a warped x (x displaced by the chaos control × a sine of y) at a higher frequency; and a cosine of a warped y (y displaced by chaos × a cosine of x).
- Field = absolute value of their sum. Brightness falls off linearly from full at a zero-crossing to nothing once the field is moderately large; that is clamped, cubed to make the ridges thin, then boosted and clamped again so ridge centres saturate.
- Hue = base hue control + a small diagonal gradient from the scaled coordinates + a small offset from the field. Saturation slightly reduced where brightness is highest. HSV output.

## Knobs

- K1 hueBase — rotates the whole colour gradient.
- K2 speed — flow speed.
- K3 scale — zoom; small values give big lazy curves, large values dense lacework.
- K4 chaos — strength of the domain warp, from regular lattice to tangled squiggles.
- Audio: none directly; chaos is the knob that would look best driven by sound.

## Making it better in Luxel

- Use `simplex3` fields (time on the third axis) instead of plain sines so the network never repeats, and warp with `curl2` for swirly rather than wavy distortion — the zero-crossing trick works on any smooth field.
- Control ridge width explicitly (divide the field by its approximate gradient magnitude, or use `smoothstep` around zero) so lines stay a constant pixel width whatever the zoom; add `setGlow` for a real neon halo around the thin cores.
- Colour from a palette or an OKLCH hue sweep along the ridge (by field phase) rather than a fixed diagonal gradient; a "two-tube" mode (ridges of two different fields in two colours) is a cheap, striking variant.
- Cost: four trig calls per pixel plus a cube; fine on 64x64, use a half-res canvas on 128x128 (but ridges are thin, so upscale with care — better to lower the scale so ridges are wider instead).
- Near-duplicate of "Vector Field Flow" (another sine-warped interference plasma); keep this one — the zero-crossing neon look is the stronger idea. Strips: a 1D cut gives moving bright points, not worth it.

## Payoff

3/5 — an attractive neon caustic plasma; with simplex fields and constant-width glowing ridges it would be a 4.
