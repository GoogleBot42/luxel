# Warped Wave

Idea spec written from Patternflow preset preset_0718.h (CC BY-SA 4.0 per its SPDX header, by Seunghun Lee; the brief said MIT); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

Tight concentric rings around the panel centre whose shapes are wobbled by a domain warp, so the rings become wavy, lumpy loops that writhe slowly. The colouring is posterized into four flat levels: black, a very dim grey, one saturated hue (chosen by a knob, pushed bright), and pure white. Each ring therefore reads as a stack of flat bands — a black gap, a dim grey border, a coloured band, a white core — giving a bold, cel-shaded, screen-print look. The rings flow outward or inward depending on the speed knob's sign. With low warp frequency the distortion is a gentle squish; at high warp frequency the ring outlines become finely crinkled, and the overall feel tips from "ripples in a pond" to "contour map gone wrong". On a 128x64 panel the normalisation is per axis, so the rings are ellipses stretched to the panel's shape.

## The idea

A radial sine wave sampled on warped coordinates (each axis displaced by a sine of the other), then posterized through a hard four-step ramp. Domain warping is the cheap trick that makes a boring ripple organic.

## Algorithm in words

- State: a time accumulator that can run in either direction, wrapped.
- Per frame: convert the hue control to an RGB colour once.
- Per pixel: normalised coordinates in −1..1 per axis. Warped x = x + amplitude × sine(y × warp frequency + slow time); warped y = y + amplitude × cosine(x × a slightly different warp frequency + another slow time). Distance from centre of the warped point; wave = sine(distance × a high ring frequency + time).
- Map the wave to a slightly compressed 0..1 range and threshold it into four bands: black, dim grey, the chosen hue at boosted brightness, white.
- The original also maps its four push-buttons to "reset this knob to its default".

## Knobs

- K1 Hue — the colour of the coloured band.
- K2 Speed — ring flow speed, signed (outward or inward).
- K3 Warp Amp — how far the warp displaces the rings.
- K4 Warp Freq — how finely the warp crinkles the ring outlines.
- Audio: none directly.

## Making it better in Luxel

- Swap the sine warp for `simplex3` (or `curl2`) displacement with time on the third axis — the rings then deform organically and never visibly repeat.
- Make the posterization a palette: use `setOutputPalette` with hard-stop ramps (several presets: cel-shaded, thermal, risograph two-tone) instead of the fixed black/grey/hue/white, and let the step count be a control.
- Multiple ring centres that wander and interfere would make it far richer than a single centre.
- Cost: the warp is two sines plus a sqrt and a sine per pixel; render at half resolution to a canvas on large chains (the posterized output tolerates it if you posterize after upscaling, e.g. via the output palette).
- Near-duplicate of "Glitch Rings" (posterized rings with wobble) and shares its four-level black/grey/hue/white ramp with "Origin"; merge with those into one rings pattern. Strips: a 1D posterized pulse from the centre works but is plain.

## Payoff

2/5 — a decent cel-shaded ripple but a thin idea on its own; keep only as a mode of a combined rings pattern.
