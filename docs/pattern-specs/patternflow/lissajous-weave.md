# Lissajous Weave

Idea spec written from Patternflow preset preset_0701.h (CC BY-SA 4.0 per its SPDX header, by Seunghun Lee; the brief said MIT); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A bundle of Lissajous figures drawn as dotted lines over a dim teal background that brightens faintly from left to right. Each figure is the same curve shape, shifted in phase from its neighbours and given its own hue, so together they form a woven rainbow knot — a dozen-odd coloured threads looping over and under each other across most of the panel (they span most of the width and height, leaving a small margin). Brightness pulses along each thread, so the threads look beaded. Everything rotates slowly in phase so the knot continuously morphs, and the hue set creeps around the colour wheel. The frequency knobs snap between integer ratios, so the figure jumps from a simple loop to a figure-eight to dense multi-lobed rosettes. There are no trails — each frame is redrawn from scratch — and the dots are single pixels, so on a 128x64 panel the threads are thin and slightly sparse, which gives a delicate string-art feel.

## The idea

Draw many phase-shifted copies of one parametric Lissajous curve with distinct hues; the overlaps create a woven, three-dimensional-looking knot. It is the classic oscilloscope art made colourful and plural.

## Algorithm in words

- State: time accumulator; derived integer x and y frequencies (from the K1 and K3 controls, snapped to whole numbers) and a phase offset.
- Per frame: fill the background with a dark teal horizontal gradient.
- For each of several curves (evenly spaced phase offsets around a full turn): pick a hue = curve's fraction of the set plus a slow drift. Walk a few hundred parameter steps around several loops of the parameter; at each step position = centre + sine(param × x-frequency + slow time + curve phase) × horizontal radius, and likewise for y with the y-frequency and the extra global phase. Plot one pixel there with HSV at fairly high saturation and a brightness that oscillates along the parameter (beading).
- No anti-aliasing, no accumulation: later curves overwrite earlier ones where they cross.

## Knobs

- K1 X Frequency — the horizontal frequency ratio (stepped integers).
- K2 Speed — how fast the knot morphs.
- K3 Y Frequency — the vertical frequency ratio (stepped integers).
- K4 Phase Offset — shifts the y phase relative to x, morphing a figure from a line through an ellipse-like loop to a full figure.
- Audio: none directly; mapping sound to Phase Offset makes the knot writhe to music.

## Making it better in Luxel

- Draw the threads with `drawLine` segments between successive points (additive blend mode) instead of isolated dots, so threads are continuous and crossings glow brighter; add `fade` persistence for comet trails and `setGlow` for an oscilloscope-phosphor look.
- Ease the frequency changes: instead of snapping, interpolate the ratio with an easing curve so the figure morphs smoothly between integer rosettes (a slow auto-cycle through ratios makes it a great unattended pattern).
- Colour the threads from a palette or OKLCH ramp rather than evenly spaced HSV, and depth-shade them (dim a thread where its z-phase says it is "behind") for a real 3D weave.
- Cost: a few thousand points per frame of interpreted loop is the expensive part; `drawLine` with fewer, longer segments in `renderBulk` is much cheaper and looks better.
- Overlaps the library's `lissajous-curve-tracer` — merge as a "weave" mode (many phase-shifted threads) of that pattern. Strips: not meaningful.

## Payoff

3/5 — lovely string-art concept held back by dotted single-pixel rendering; with lines, glow and eased morphs it becomes a 4.
