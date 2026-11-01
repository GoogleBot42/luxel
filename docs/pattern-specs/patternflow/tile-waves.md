# Tile Waves

Idea spec written from Patternflow preset preset_0710.h (CC BY-SA 4.0 per its SPDX header, by Seunghun Lee; the brief said MIT); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

The panel is a grid of small square tiles (a few pixels on a side) separated by one-pixel black grout lines, like a mosaic floor or an LED video wall seen up close. Each tile is a single flat colour. A concentric wave radiates from the panel centre across the tiles, so rings of bright tiles expand outward, coloured through a ramp of black → deep violet → electric blue → magenta-red → solid red (the upper half of the brightness range is all red). The motion is optionally time-stepped: tiles update in discrete jumps, and the step timing is delayed with distance from the centre, so the ripple reads as a stuttering "wave of tiles flipping" rather than smooth flow. A sharpness control narrows the bright crests into thin rings with wide dark gaps between them.

## The idea

A radial ripple sampled at tile resolution with grout lines, plus phase-delayed time quantization so each ring of tiles changes in discrete, staggered steps — a mechanical "stadium card stunt" feel.

## Algorithm in words

- Setup: compute the distance from each tile's centre to the panel centre.
- Per pixel: look up the tile's distance. Local time = global time minus distance × phase-shift control (so outer tiles lag). If the quantize control is above its minimum, floor local time to a step grid whose resolution rises with the control.
- Wave = sine of (distance × a fixed ring frequency − local time × a speed factor), mapped to 0..1, raised to the sharpness power.
- Pixels on a tile's top or left edge are forced to zero (grout). Map the value through the colour ramp.

## Knobs

- K1 Quantize — time-step resolution. At its very bottom there is no stepping at all; just above it the steps are coarsest (very choppy), and they get finer as the control rises toward smooth motion.
- K2 Speed — wave speed.
- K3 PhaseShift — how much outer tiles lag the centre, which bends the wave's apparent speed and tightens or loosens rings.
- K4 Sharpness — crest width, from broad soft rings to thin bright rings.
- Audio: none directly.

## Making it better in Luxel

- This is a tile-resolution pattern: compute one value per tile into a tiny canvas and draw it with `fillRect` per tile (or `blit` of an upscaled canvas plus a grout pass) in `renderBulk` — orders of magnitude cheaper than per-pixel trig on a 128x128 chain.
- Generalise the wave source: radial, linear sweep, multiple wandering emitters, or a `simplex3` field — the tile mosaic is the interesting part, the plain ripple is not.
- Give each tile a short eased transition (`easeOutBack` pop) when its quantized value changes, so stepping reads as tiles flipping rather than a choppy framerate.
- Near-duplicate of "Origin" (tiled radial ripples) and related to "Glitch Rings"; merge into one tiled-ripple pattern. Strips: a 1D "segmented" version (groups of LEDs with dark gaps) works on long strips.

## Payoff

2/5 — a pleasant mosaic ripple but thin on ideas; worth keeping only as a mode of a combined tiled-waves pattern.
